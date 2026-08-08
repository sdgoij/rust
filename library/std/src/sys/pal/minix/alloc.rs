//! Mmap-backed free-list allocator for the Minix std PAL.
//!
//! Replaces the original bump allocator, which never freed memory: rustc is
//! an arena-heavy program that allocates and frees constantly, and a no-free
//! allocator OOMs it in seconds. The allocator maps page-aligned chunks via
//! the VM server's `mmap` syscall and carves them into variable-size blocks
//! served from a first-fit free list; a chunk whose blocks are all free is
//! returned to the kernel with `munmap`, so the heap can shrink as well as
//! grow.
//!
//! Thread-safe: a futex-backed lock serializes the heap metadata, because the
//! Minix target has 1:1 kernel threads (see THREADS.md).

use crate::alloc::Layout;
use crate::sys::sync::Mutex;

/// The lock guarding the heap metadata (free list + chunk table). Futex-backed
/// on Minix, so a thread waiting for it sleeps instead of spinning.
static LOCK: Mutex = Mutex::new();

/// Header size (two `usize` words: `size` at +0, `flags` at +8). The header
/// stays at the block start for the block's whole lifetime so free-list walks
/// and coalescing can always read it.
const HDR: usize = 16;

/// Block flag: the block is allocated.
const IN_USE: usize = 1;
/// Block flag: first block of an mmap chunk (its bounds live in the chunk
/// table below).
const CHUNK_START: usize = 2;

/// Minimum payload offset from the block start. The payload is aligned up and
/// the block base is recorded at `payload - HDR`; forcing at least 32 bytes of
/// header+slack keeps that back-pointer clear of the block header even for
/// 16-byte-aligned payloads.
const MIN_PAYLOAD_OFF: usize = 32;

/// Smallest block we ever split off (header + minimal payload).
const MIN_BLOCK: usize = 48;

/// Size of an mmap chunk: 1 MiB (256 pages). Large chunks keep the region
/// count low (the VM server tracks at most `MAX_REGIONS` per process).
const CHUNK_SIZE: usize = 1024 * 1024;

const PAGE_SIZE: usize = 4096;

/// Maximum tracked chunks. The VM server caps live regions at
/// `MAX_REGIONS` (16) per process, so this is generous.
const MAX_CHUNKS: usize = 32;

/// A live mmap chunk, for returning fully-free chunks to the kernel.
#[repr(C)]
#[derive(Clone, Copy)]
struct Chunk {
    base: usize,
    len: usize,
}

/// The heap's mutable state, guarded by [`LOCK`].
struct HeapState {
    /// Head of the free list (0 = empty). A free block stores the next
    /// block's address at `block + HDR` (its payload area).
    free_head: usize,
    /// Live mmap chunks; `len == 0` marks a free slot.
    chunks: [Chunk; MAX_CHUNKS],
}

static mut HEAP: HeapState =
    HeapState { free_head: 0, chunks: [Chunk { base: 0, len: 0 }; MAX_CHUNKS] };

/// RAII guard for [`LOCK`].
struct HeapGuard;

impl HeapGuard {
    fn acquire() -> HeapGuard {
        LOCK.lock();
        HeapGuard
    }
}

impl Drop for HeapGuard {
    fn drop(&mut self) {
        // SAFETY: the guard is the only holder of `LOCK` and drops exactly
        // once.
        unsafe { LOCK.unlock() }
    }
}

// ---- block helpers ----

#[inline]
fn hdr_size(b: usize) -> usize {
    unsafe { *core::ptr::with_exposed_provenance::<usize>(b) }
}

#[inline]
fn hdr_flags(b: usize) -> usize {
    unsafe { *core::ptr::with_exposed_provenance::<usize>(b + 8) }
}

#[inline]
unsafe fn set_hdr(b: usize, size: usize, flags: usize) {
    unsafe {
        *core::ptr::with_exposed_provenance_mut::<usize>(b) = size;
        *core::ptr::with_exposed_provenance_mut::<usize>(b + 8) = flags;
    }
}

#[inline]
fn align_up(n: usize, a: usize) -> usize {
    (n + a - 1) & !(a - 1)
}

// ---- free list ----

/// Push a free block onto the free list (head insert).
unsafe fn free_push(b: usize) {
    unsafe {
        let head = HEAP.free_head;
        *core::ptr::with_exposed_provenance_mut::<usize>(b + HDR) = head;
        HEAP.free_head = b;
    }
}

/// Walk the free list for the first block with `hdr_size >= need`, unlinking
/// it. Returns the block, or 0.
unsafe fn free_pop_fit(need: usize) -> usize {
    unsafe {
        let mut prev = 0usize;
        let mut cur = HEAP.free_head;
        while cur != 0 {
            if hdr_size(cur) >= need {
                let next = *core::ptr::with_exposed_provenance::<usize>(cur + HDR);
                if prev == 0 {
                    HEAP.free_head = next;
                } else {
                    *core::ptr::with_exposed_provenance_mut::<usize>(prev + HDR) = next;
                }
                return cur;
            }
            prev = cur;
            cur = *core::ptr::with_exposed_provenance::<usize>(cur + HDR);
        }
        0
    }
}

/// Unlink `b` from the free list (used when coalescing or munmapping a chunk).
unsafe fn free_unlink(b: usize) {
    unsafe {
        let mut prev = 0usize;
        let mut cur = HEAP.free_head;
        while cur != 0 && cur != b {
            prev = cur;
            cur = *core::ptr::with_exposed_provenance::<usize>(cur + HDR);
        }
        if cur == b {
            let next = *core::ptr::with_exposed_provenance::<usize>(b + HDR);
            if prev == 0 {
                HEAP.free_head = next;
            } else {
                *core::ptr::with_exposed_provenance_mut::<usize>(prev + HDR) = next;
            }
        }
    }
}

// ---- chunk table ----

/// Record a new mmap chunk. Returns false when the table is full (the chunk
/// is then simply never returned to the kernel).
unsafe fn chunk_add(base: usize, len: usize) -> bool {
    unsafe {
        for i in 0..MAX_CHUNKS {
            if HEAP.chunks[i].len == 0 {
                HEAP.chunks[i] = Chunk { base, len };
                return true;
            }
        }
        false
    }
}

/// Find the chunk containing `addr`.
unsafe fn chunk_find(addr: usize) -> Option<(usize, usize)> {
    unsafe {
        for i in 0..MAX_CHUNKS {
            let c = HEAP.chunks[i];
            if c.len != 0 && addr >= c.base && addr < c.base + c.len {
                return Some((c.base, c.len));
            }
        }
        None
    }
}

/// Drop the chunk at `base` from the table.
unsafe fn chunk_remove(base: usize) {
    unsafe {
        for i in 0..MAX_CHUNKS {
            if HEAP.chunks[i].base == base {
                HEAP.chunks[i].len = 0;
                return;
            }
        }
    }
}

// ---- allocation ----

/// Map an anonymous, private, read/write chunk of `size` bytes via the VM
/// server. Returns the page-aligned base, or 0 on failure.
unsafe fn mmap_chunk(size: usize) -> usize {
    unsafe {
        let r = minix_std::vmem::mmap(
            core::ptr::null_mut(),
            size,
            minix_std::vmem::PROT_READ | minix_std::vmem::PROT_WRITE,
            minix_std::vmem::MAP_PRIVATE | minix_std::vmem::MAP_ANONYMOUS,
            -1,
            0,
        );
        let base = r.addr();
        if base == usize::MAX || base == 0 {
            return 0;
        }
        base
    }
}

/// True when every block in `[cbase, cend)` is free (so the chunk can be
/// returned to the kernel).
unsafe fn chunk_fully_free(cbase: usize, cend: usize) -> bool {
    let mut b = cbase;
    while b < cend {
        if hdr_flags(b) & IN_USE != 0 {
            return false;
        }
        let size = hdr_size(b);
        // A zero or out-of-range header means the walk left the chunk.
        if size == 0 || b + size > cend {
            return false;
        }
        b += size;
    }
    true
}

unsafe fn alloc_impl(layout: Layout, zero: u8) -> *mut u8 {
    let size = layout.size().max(1);
    let align = layout.align().max(16);
    // Header + payload + up to `align` alignment padding (+ slack).
    let need = align_up(size + align.max(MIN_PAYLOAD_OFF), 16);

    let _guard = HeapGuard::acquire();

    let mut block = unsafe { free_pop_fit(need) };
    if block == 0 {
        // No free block big enough: map a new chunk.
        let chunk_size = CHUNK_SIZE.max(align_up(need, PAGE_SIZE));
        let base = unsafe { mmap_chunk(chunk_size) };
        if base == 0 {
            return core::ptr::null_mut();
        }
        block = base;
        unsafe {
            set_hdr(block, chunk_size, IN_USE | CHUNK_START);
            chunk_add(block, chunk_size);
        }
    }

    // Split off a tail free block when the leftover is large enough.
    let bsize = hdr_size(block);
    let flags = hdr_flags(block);
    let tail = block + need;
    if bsize - need >= MIN_BLOCK {
        unsafe {
            set_hdr(tail, bsize - need, 0);
            free_push(tail);
            set_hdr(block, need, flags | IN_USE);
        }
    } else {
        unsafe { set_hdr(block, bsize, flags | IN_USE) };
    }

    // Compute the aligned payload and record the block back-pointer at
    // `payload - HDR` (in the padding, never the block header).
    let mut payload = align_up(block + HDR, align);
    if payload < block + MIN_PAYLOAD_OFF {
        payload = block + MIN_PAYLOAD_OFF;
    }
    unsafe {
        *core::ptr::with_exposed_provenance_mut::<usize>(payload - HDR) = block;
        if zero != 0 {
            core::ptr::write_bytes(core::ptr::with_exposed_provenance_mut::<u8>(payload), 0, size);
        }
    }
    core::ptr::with_exposed_provenance_mut::<u8>(payload)
}

#[inline]
pub unsafe fn alloc(layout: Layout) -> *mut u8 {
    unsafe { alloc_impl(layout, 0) }
}

#[inline]
pub unsafe fn alloc_zeroed(layout: Layout) -> *mut u8 {
    unsafe { alloc_impl(layout, 1) }
}

#[inline]
pub unsafe fn dealloc(ptr: *mut u8, _layout: Layout) {
    if ptr.is_null() {
        return;
    }
    let _guard = HeapGuard::acquire();
    let payload = ptr.addr();
    // SAFETY: `ptr` was returned by `alloc`; the back-pointer was written at
    // `payload - HDR` by `alloc_impl`.
    let block = unsafe { *core::ptr::with_exposed_provenance::<usize>(payload - HDR) };
    let size = hdr_size(block);
    let flags = hdr_flags(block);
    debug_assert!(flags & IN_USE != 0);
    unsafe { set_hdr(block, size, flags & !IN_USE) };

    let (cbase, clen) = match unsafe { chunk_find(block) } {
        Some(c) => c,
        None => return,
    };
    let cend = cbase + clen;

    // Coalesce with the next block when it is free and inside the chunk.
    let next = block + size;
    if next < cend && hdr_flags(next) & IN_USE == 0 {
        let nsize = hdr_size(next);
        unsafe {
            free_unlink(next);
            set_hdr(block, size + nsize, 0);
        }
    }

    // A fully-free chunk goes back to the kernel.
    if unsafe { chunk_fully_free(cbase, cend) } {
        unsafe {
            let mut b = cbase;
            while b < cend {
                free_unlink(b);
                let bsize = hdr_size(b);
                if bsize == 0 {
                    break;
                }
                b += bsize;
            }
            chunk_remove(cbase);
            minix_std::vmem::munmap(core::ptr::with_exposed_provenance_mut::<u8>(cbase), clen);
        }
        return;
    }

    unsafe { free_push(block) };
}

#[inline]
pub unsafe fn realloc(ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
    // SAFETY: per the GlobalAlloc contract, `ptr`/`layout` describe a live
    // allocation and `new_size` is non-zero.
    unsafe {
        let payload = ptr.addr();
        let block = *core::ptr::with_exposed_provenance::<usize>(payload - HDR);
        let bsize = hdr_size(block);
        let padding = payload - block - HDR;
        let capacity = bsize - HDR - padding;
        if new_size <= capacity {
            // Fits in place with the same alignment.
            return ptr;
        }
        let new_ptr = alloc(Layout::from_size_align_unchecked(new_size, layout.align()));
        if !new_ptr.is_null() {
            core::ptr::copy_nonoverlapping(ptr, new_ptr, core::cmp::min(layout.size(), new_size));
            dealloc(ptr, layout);
        }
        new_ptr
    }
}
