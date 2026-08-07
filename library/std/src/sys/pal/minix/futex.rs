//! Futex support — the `sys::futex` API over the kernel's futex syscalls
//! (`SYS_futex_wait`/`SYS_futex_wake`; see `crates/kernel/src/thread.rs`).

use crate::sync::atomic::Atomic;
use crate::time::Duration;

/// An atomic for use as a futex that is at least 32-bits but may be larger.
pub type Futex = Atomic<Primitive>;
/// Must be the underlying type of `Futex`.
pub type Primitive = u32;

/// An atomic for use as a futex that is at least 8-bits but may be larger.
pub type SmallFutex = Atomic<SmallPrimitive>;
/// Must be the underlying type of `SmallFutex`.
pub type SmallPrimitive = u32;

pub fn futex_wait(futex: &Atomic<u32>, expected: u32, _timeout: Option<Duration>) -> bool {
    // No kernel timeout support yet: wait indefinitely (the timeout is
    // ignored). Returns true when woken by `futex_wake`.
    let r = unsafe { minix_rt::futex_wait(futex.as_ptr(), expected) };
    r == 0
}

#[inline]
pub fn futex_wake(futex: &Atomic<u32>) -> bool {
    minix_rt::futex_wake(futex.as_ptr(), 1) > 0
}

#[inline]
pub fn futex_wake_all(futex: &Atomic<u32>) {
    minix_rt::futex_wake(futex.as_ptr(), i32::MAX as u32);
}
