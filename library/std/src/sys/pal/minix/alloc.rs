//! Bump allocator backed by the VM server's `brk` call.
//!
//! Minix does not support threads yet, so a simple cursor suffices. Memory is
//! never returned to the kernel; `dealloc` is a no-op and `realloc` falls
//! back to allocate-and-copy.

use crate::alloc::Layout;
use crate::ptr;
use crate::sync::atomic::{AtomicUsize, Ordering};
use crate::sys::syscall;

// The brk allocator guarantees 16-byte alignment.
const MIN_ALIGN: usize = 16;

// Current allocation cursor; 0 means "not yet initialized".
static CURSOR: AtomicUsize = AtomicUsize::new(0);

#[inline]
fn align_up(n: usize, align: usize) -> usize {
    (n + align - 1) & !(align - 1)
}

/// Initialize the cursor to the current program break.
fn ensure_initialized() -> usize {
    let cursor = CURSOR.load(Ordering::Relaxed);
    if cursor != 0 {
        return cursor;
    }
    // `brk(0)` returns the current break.
    let cur = syscall::brk(0);
    let cur = if cur < 0 { 0 } else { cur as usize };
    CURSOR.store(cur, Ordering::Relaxed);
    cur
}

fn grow(need: usize) -> *mut u8 {
    let old = ensure_initialized();
    let new_brk = old.checked_add(need).unwrap_or(usize::MAX);
    // The VM server protocol passes the break as a 32-bit value for now.
    let r = syscall::brk(new_brk as u32);
    if r < 0 {
        return ptr::null_mut();
    }
    let actual = r as usize;
    CURSOR.store(actual, Ordering::Relaxed);
    // The break address is a plain address returned by the kernel; reattach
    // provenance to it.
    ptr::with_exposed_provenance_mut(old)
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
