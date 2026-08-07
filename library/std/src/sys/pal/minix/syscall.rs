//! Thin facade over the Minix userspace crates (`minix_rt`, `minix_std`).
//!
//! The raw syscall wrappers, IPC message helpers and PM/VFS/VM protocol
//! constants live in `crates/minix-rt` and `crates/minix-std` in the minixrs
//! repository; this module re-exports what `std` needs and adapts the
//! crates' `Result<_, MinixErr>` returns to the `i32`/`i64` errno conventions
//! the rest of the PAL uses (negative return = -errno, `MinixErr` stores a
//! positive errno).

pub use minix_rt::{exit, getpid, read};
pub use minix_std::fs::Stat;

/// Open flags (from `minix/include/fcntl.h`).
pub const O_RDONLY: i32 = 0o00;
pub const O_WRONLY: i32 = 0o01;
pub const O_RDWR: i32 = 0o02;
pub const O_CREAT: i32 = 0o100;
pub const O_EXCL: i32 = 0o200;
pub const O_TRUNC: i32 = 0o1000;
pub const O_APPEND: i32 = 0o2000;

/// Seek whence values (from `minix/include/unistd.h`).
pub const SEEK_SET: i32 = 0;
pub const SEEK_CUR: i32 = 1;
pub const SEEK_END: i32 = 2;

/// Standard file descriptors. Used by `os/fd/raw.rs`, which aliases `libc`
/// to this module for minix.
pub const STDIN_FILENO: i32 = 0;
pub const STDOUT_FILENO: i32 = 1;
pub const STDERR_FILENO: i32 = 2;

/// fcntl commands (from `minix/include/fcntl.h`).
pub const F_DUPFD: i32 = 0;

/// waitpid options (from `minix/include/sys/wait.h`).
pub const WNOHANG: i32 = 1;

/// Signals (from `minix/include/signal.h`).
pub const SIGKILL: i32 = 9;

// errno values (from `minix/include/errno.h`). The crates return these
// negated; `MinixErr` unwraps them to positive values.
pub const EPERM: i32 = 1;
pub const ENOENT: i32 = 2;
pub const ESRCH: i32 = 3;
pub const EINTR: i32 = 4;
pub const EIO: i32 = 5;
pub const ENXIO: i32 = 6;
pub const EBADF: i32 = 9;
pub const EAGAIN: i32 = 11;
pub const ENOMEM: i32 = 12;
pub const EACCES: i32 = 13;
pub const EFAULT: i32 = 14;
pub const EBUSY: i32 = 16;
pub const EEXIST: i32 = 17;
pub const ENODEV: i32 = 19;
pub const ENOTDIR: i32 = 20;
pub const EISDIR: i32 = 21;
pub const EINVAL: i32 = 22;
pub const ENOSPC: i32 = 28;
pub const EDOM: i32 = 33;
pub const ERANGE: i32 = 34;
pub const ENOSYS: i32 = 71;

/// Fork the current process. Returns the child PID in the parent, 0 in the
/// child, or a negative errno.
pub fn fork() -> i32 {
    // SAFETY: `minix_std::process::fork` requires PM to be reachable; std
    // only runs on a live Minix system.
    match unsafe { minix_std::process::fork() } {
        Ok(pid) => pid,
        Err(e) => -e.0,
    }
}

/// Wait for a child process. Returns `(pid, status)`; a negative `pid` is a
/// negative errno (`EAGAIN` for `WNOHANG` with no exited child).
///
/// `minix_std::process::waitpid` checks the reply `m_type`, so a WNOHANG
/// miss is reported as `Err(EAGAIN)` rather than a bogus status.
pub fn waitpid(pid: i32, options: i32) -> (i32, i32) {
    match minix_std::process::waitpid(pid, options) {
        Ok((pid, status)) => (pid, status),
        Err(e) => (-e.0, 0),
    }
}

/// Send a signal to a process (PM_KILL). Returns 0 on success or a negative
/// errno.
pub fn kill(pid: i32, sig: i32) -> i32 {
    match minix_std::time::kill(pid, sig) {
        Ok(()) => 0,
        Err(e) => -e.0,
    }
}

/// Duplicate `fd` onto `newfd` (POSIX `dup2`). Returns `newfd` or a
/// negative errno.
pub fn dup2(fd: i32, newfd: i32) -> i32 {
    match minix_std::fs::dup2(fd, newfd) {
        Ok(v) => v,
        Err(e) => -e.0,
    }
}

/// Create a pipe. Returns `(read_fd, write_fd)` or a pair of the same
/// negative errno.
pub fn pipe() -> (i32, i32) {
    match minix_std::fs::pipe() {
        Ok((r, w)) => (r, w),
        Err(e) => (-e.0, -e.0),
    }
}

/// Close `fd`. Returns 0 or a negative errno.
pub fn close(fd: i32) -> i64 {
    match minix_std::fs::close(fd) {
        Ok(()) => 0,
        Err(e) => -(e.0 as i64),
    }
}

/// Perform `fcntl(fd, cmd, arg)` (e.g. `F_DUPFD`). Returns the result
/// (for `F_DUPFD`, the new descriptor) or a negative errno.
pub fn fcntl(fd: i32, cmd: i32, arg: i32) -> i64 {
    match minix_std::fs::fcntl(fd, cmd, arg) {
        Ok(v) => v as i64,
        Err(e) => -(e.0 as i64),
    }
}

/// Write `buf` to `fd`. Returns the byte count or a negative errno.
pub fn write(fd: i32, buf: &[u8]) -> i64 {
    // SAFETY: `buf` is valid for `buf.len()` bytes for the duration of the
    // call.
    unsafe { minix_rt::write(fd, buf.as_ptr(), buf.len()) }
}

/// Mark fd 0..2 as VFS-owned (1) or serial (0) so the kernel routes
/// reads/writes through VFS instead of the serial shortcut.
pub fn set_fd_vfs(fd: i32, on: i32) -> i64 {
    // SAFETY: `fd` must be in 0..=2, which the kernel validates (EBADF
    // otherwise).
    unsafe { minix_rt::set_fd_vfs(fd, on) }
}
