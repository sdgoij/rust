//! Bump allocator backed by the VM server's `brk` call.
//!
//! Minix has no free-list allocator yet, so a simple cursor suffices. Memory
//! is never returned to the kernel; `dealloc` is a no-op and `realloc` falls
//! back to allocate-and-copy.

use crate::alloc::Layout;
use crate::ptr;
use crate::sync::atomic::{AtomicUsize, Ordering};

// The brk allocator guarantees 16-byte alignment.
const MIN_ALIGN: usize = 16;

// High-water mark of the heap; informational (the real break is the
// authority, see `grow`).
static CURSOR: AtomicUsize = AtomicUsize::new(0);

#[inline]
fn align_up(n: usize, align: usize) -> usize {
    (n + align - 1) & !(align - 1)
}

fn grow(need: usize) -> *mut u8 {
    // Route through `minix_rt::sbrk`, which serializes the query+reserve
    // against the other break users (thread stacks, TLS blocks). Doing the
    // two `brk` calls here directly would race: another thread's `sbrk`
    // could land between them, making this allocation overlap a live TLS
    // block or thread stack (the VM treats a `brk` below the real break as
    // a heap shrink and unmaps the pages).
    let r = unsafe { minix_rt::sbrk(need as isize) };
    if r < 0 {
        return ptr::null_mut();
    }
    let base = r as usize;
    CURSOR.store(base + need, Ordering::Relaxed);
    // The break address is a plain address returned by the kernel; reattach
    // provenance to it.
    ptr::with_exposed_provenance_mut(base)
}

#[inline]
pub unsafe fn alloc(layout: Layout) -> *mut u8 {
    unsafe { alloc_impl(layout, 0) }
}

#[inline]
pub unsafe fn alloc_zeroed(layout: Layout) -> *mut u8 {
    unsafe { alloc_impl(layout, 1) }
}

unsafe fn alloc_impl(layout: Layout, zero: u8) -> *mut u8 {
    let size = align_up(layout.size().max(1), layout.align().max(MIN_ALIGN));
    let ptr = grow(size);
    if ptr.is_null() {
        return ptr;
    }
    if zero != 0 {
        // SAFETY: `ptr` is valid for `size` bytes, freshly allocated.
        unsafe { ptr.write_bytes(0, size) };
    }
    ptr
}

#[inline]
pub unsafe fn dealloc(_ptr: *mut u8, _layout: Layout) {
    // This allocator never deallocates memory.
}

#[inline]
pub unsafe fn realloc(ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
    // SAFETY: this is just a `pub` wrapper.
    unsafe { crate::sys::alloc::realloc_fallback(ptr, layout, new_size) }
}
