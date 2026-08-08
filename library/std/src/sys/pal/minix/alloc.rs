//! Free-list allocator for the Minix std PAL, backed by the program break.
//!
//! Replaces the original bump allocator, which never freed memory: rustc is
//! an arena-heavy program that allocates and frees constantly, and a no-free
//! allocator OOMs it in seconds. This allocator carves the program-break
//! region into variable-size blocks served from a first-fit free list;
//! deallocation returns blocks to the list and coalesces with the next
//! block, so freed memory is reused. The heap grows monotonically — an
//! mmap-backed variant that can also shrink is the follow-up once the VM
//! server's mmap path is fixed (`crates/servers/src/vm/mod.rs` `do_mmap`).
//!
//! Thread-safe: a futex-backed lock serializes the heap metadata, because the
//! Minix target has 1:1 kernel threads (see THREADS.md).

use crate::alloc::Layout;
use crate::sys::sync::Mutex;

/// The lock guarding the heap metadata (free list + heap end). Futex-backed
/// on Minix, so a thread waiting for it sleeps instead of spinning.
static LOCK: Mutex = Mutex::new();

/// Header size (two `usize` words: `size` at +0, `flags` at +8). The header
/// stays at the block start for the block's whole lifetime so free-list walks
/// and coalescing can always read it.
const HDR: usize = 16;

/// Block flag: the block is allocated.
const IN_USE: usize = 1;

/// Minimum payload offset from the block start. The payload is aligned up and
/// the block base is recorded at `payload - HDR`; forcing at least 32 bytes of
/// header+slack keeps that back-pointer clear of the block header even for
/// 16-byte-aligned payloads.
const MIN_PAYLOAD_OFF: usize = 32;

/// Smallest block we ever split off (header + minimal payload).
const MIN_BLOCK: usize = 48;

/// Size of a heap chunk (1 MiB).
const CHUNK_SIZE: usize = 1024 * 1024;

const PAGE_SIZE: usize = 4096;

/// The heap's mutable state, guarded by [`LOCK`].
struct HeapState {
    /// Head of the free list (0 = empty). A free block stores the next
    /// block's address at `block + HDR` (its payload area).
    free_head: usize,
    /// Exact end of the highest block handed out. The program break may be
    /// higher (the VM server rounds it to pages, and TLS blocks / thread
    /// stacks are sbrk'd beyond it), so the free-list walk and the coalesce
    /// check must not read past this into the zeroed/rounded gap.
    heap_end: usize,
}

static mut HEAP: HeapState = HeapState { free_head: 0, heap_end: 0 };

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

/// Unlink `b` from the free list (used when coalescing).
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

// ---- allocation ----

/// Extend the heap with a chunk of at least `size` bytes via `sbrk` and
/// record its exact end (see `HeapState::heap_end`).
unsafe fn grow_chunk(size: usize) -> usize {
    // SAFETY: `size` is a positive chunk size; `sbrk` returns the previous
    // break (the start of the new region) or a negative errno.
    let r = unsafe { minix_rt::sbrk(size as isize) };
    if r < 0 {
        return 0;
    }
    let base = r as usize;
    // SAFETY: guarded by `LOCK` (the caller holds it).
    unsafe { HEAP.heap_end = base + size };
    base
}

unsafe fn alloc_impl(layout: Layout, zero: u8) -> *mut u8 {
    let size = layout.size().max(1);
    let align = layout.align().max(16);
    // Header + payload + up to `align` alignment padding (+ slack).
    let need = align_up(size + align.max(MIN_PAYLOAD_OFF), 16);

    let _guard = HeapGuard::acquire();

    let mut block = unsafe { free_pop_fit(need) };
    if block == 0 {
        // No free block big enough: extend the heap with a new chunk. The
        // sbrk'd pages are uninitialized, so stamp the chunk's first block
        // header before it is sized/split below.
        let chunk_size = CHUNK_SIZE.max(align_up(need, PAGE_SIZE));
        let base = unsafe { grow_chunk(chunk_size) };
        if base == 0 {
            return core::ptr::null_mut();
        }
        block = base;
        unsafe { set_hdr(block, chunk_size, 0) };
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

    // Coalesce with the next block when it is free and inside the heap.
    let next = block + size;
    if next < unsafe { HEAP.heap_end } && hdr_flags(next) & IN_USE == 0 {
        let nsize = hdr_size(next);
        unsafe {
            free_unlink(next);
            set_hdr(block, size + nsize, 0);
        }
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
