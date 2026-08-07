//! Threads — 1:1 kernel threads on Minix (see THREADS.md in the OS repo).
//!
//! Threads are kernel-scheduled `Proc` slots that share the process's
//! address space. `Thread::new` allocates a user stack from the VM-backed
//! heap and calls `thread_create` with a trampoline (`thread_start`) that
//! first sets up the thread's TLS block and thread pointer, then runs the
//! user's closure. `join` waits through the kernel's `thread_join`.

use crate::ffi::CStr;
use crate::io;
use crate::num::NonZero;
use crate::thread::ThreadInit;
use crate::time::Duration;

pub struct Thread {
    tid: i32,
}

unsafe impl Send for Thread {}
unsafe impl Sync for Thread {}

pub const DEFAULT_MIN_STACK_SIZE: usize = 256 * 1024;

impl Thread {
    // unsafe: see thread::Builder::spawn_unchecked for safety requirements
    pub unsafe fn new(stack: usize, init: Box<ThreadInit>) -> io::Result<Thread> {
        let stack_size = core::cmp::max(stack, DEFAULT_MIN_STACK_SIZE);

        // Allocate the thread's user stack from the VM-backed heap. The
        // kernel only records the stack top; the pages are mapped in the
        // shared address space.
        let base = unsafe { minix_rt::sbrk(stack_size as isize + 16) };
        if base < 0 {
            return Err(io::Error::from_raw_os_error((-base) as i32));
        }

        // The kernel restores the thread directly to `entry` with no call,
        // so the entry RSP must match the ABI convention: ≡ 8 (mod 16) on
        // x86_64 (as if a return address had been pushed), 16-aligned on
        // RISC-V/AArch64 (return address lives in a register).
        #[cfg(target_arch = "x86_64")]
        let stack_top = (((base as usize) + stack_size) & !0xF) - 8;
        #[cfg(not(target_arch = "x86_64"))]
        let stack_top = ((base as usize) + stack_size) & !0xF;

        // Leak the init box to the new thread (reclaimed in thread_start).
        let data = Box::into_raw(init).expose_provenance();
        let entry: usize = thread_start as extern "C" fn(usize) -> ! as usize;
        let tid = minix_rt::thread_create(entry, stack_top, data);
        if tid <= 0 {
            // The thread did not start: reclaim the box and report the error.
            drop(unsafe {
                Box::from_raw(core::ptr::with_exposed_provenance_mut::<ThreadInit>(data))
            });
            return Err(io::Error::from_raw_os_error((-tid) as i32));
        }
        Ok(Thread { tid })
    }

    pub fn join(self) {
        minix_rt::thread_join(self.tid);
    }
}

/// The spawned-thread entry: called by the kernel with the leaked `ThreadInit`
/// box in the first argument register on a fresh stack. Sets up TLS, runs the
/// closure, runs destructors + runtime cleanup, and exits the thread.
extern "C" fn thread_start(data: usize) -> ! {
    // Set up this thread's TLS block + thread pointer before any
    // `thread_local!` access.
    crate::sys::pal::minix::init_tls();

    // SAFETY: `data` is the box leaked in `Thread::new`.
    let init = unsafe { Box::from_raw(core::ptr::with_exposed_provenance_mut::<ThreadInit>(data)) };
    let rust_start = init.init();
    rust_start();

    // Run the TLS destructor list and the runtime cleanup for this thread.
    unsafe {
        crate::sys::thread_local::destructors::run();
    }
    crate::rt::thread_cleanup();

    minix_rt::thread_exit(0)
}

pub fn available_parallelism() -> io::Result<NonZero<usize>> {
    // Single-CPU kernel for now.
    Ok(unsafe { NonZero::new_unchecked(1) })
}

pub fn current_os_id() -> Option<u64> {
    // No gettid syscall; std's ThreadId uses a global counter instead.
    None
}

pub fn set_name(_name: &CStr) {}

pub fn sleep(dur: Duration) {
    // No kernel sleep syscall yet: busy-wait, yielding the CPU to other
    // threads between polls. A proper sleep needs a kernel timer.
    let start = crate::time::Instant::now();
    while start.elapsed() < dur {
        minix_rt::thread_yield();
    }
}

pub fn yield_now() {
    minix_rt::thread_yield();
}
