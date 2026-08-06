//! Global initialization and retrieval of command line arguments.
//!
//! The kernel exec loader places `argc`/`argv` on the initial stack; the
//! entry point (`sys::pal::minix::_start`) hands them to `init` before
//! `main` runs, and `args()` parses them lazily.

pub use crate::sys::args::common::Args;
use crate::ffi::{CStr, OsString};
use crate::ptr;
use crate::sync::atomic::{Atomic, AtomicIsize, AtomicPtr, Ordering};

static ARGC: Atomic<isize> = AtomicIsize::new(0);
static ARGV: Atomic<*mut *const u8> = AtomicPtr::new(ptr::null_mut());

/// One-time global initialization.
pub unsafe fn init(argc: isize, argv: *const *const u8) {
    // These only hold the unmodified kernel-provided argc/argv, which stay
    // valid for the whole process lifetime.
    ARGC.store(argc, Ordering::Relaxed);
    ARGV.store(argv as *mut _, Ordering::Relaxed);
}

/// Returns the command line arguments.
pub fn args() -> Args {
    let argv = ARGV.load(Ordering::Relaxed);
    let argc = if argv.is_null() { 0 } else { ARGC.load(Ordering::Relaxed) };

    let mut vec = Vec::with_capacity(argc as usize);
    for i in 0..argc {
        // SAFETY: `argv` is non-null if `argc` is positive and is guaranteed
        // to be at least `argc` pointers long; stop at the first NULL like
        // other platforms do.
        let ptr = unsafe { argv.add(i as usize).read() };
        if ptr.is_null() {
            break;
        }
        // SAFETY: arguments are guaranteed to be valid C strings.
        let cstr = unsafe { CStr::from_ptr(ptr.cast()) };
        // SAFETY: Minix arguments are byte strings without interior NULs.
        unsafe {
            vec.push(OsString::from_encoded_bytes_unchecked(cstr.to_bytes().to_vec()));
        }
    }

    Args::new(vec)
}
