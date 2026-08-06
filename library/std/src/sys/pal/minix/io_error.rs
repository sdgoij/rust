//! Map Minix errno values to `io::ErrorKind`.

use crate::io;
use crate::sys::syscall;

pub fn errno() -> io::RawOsError {
    0
}

pub fn is_interrupted(_code: io::RawOsError) -> bool {
    // Minix does not deliver signals to userspace yet.
    false
}

pub fn decode_error_kind(code: io::RawOsError) -> io::ErrorKind {
    use io::ErrorKind::*;

    match code {
        syscall::EPERM | syscall::EACCES => PermissionDenied,
        syscall::ENOENT => NotFound,
        syscall::EINTR => Interrupted,
        syscall::EAGAIN => WouldBlock,
        syscall::ENOMEM => OutOfMemory,
        syscall::EEXIST => AlreadyExists,
        syscall::ENOTDIR => NotADirectory,
        syscall::EISDIR => IsADirectory,
        syscall::EINVAL => InvalidInput,
        syscall::ENOSPC => StorageFull,
        syscall::ENOSYS => Unsupported,
        _ => Uncategorized,
    }
}

pub fn error_string(errno: io::RawOsError) -> String {
    let name = match errno {
        syscall::EPERM => "Operation not permitted",
        syscall::ENOENT => "No such file or directory",
        syscall::ESRCH => "No such process",
        syscall::EINTR => "Interrupted system call",
        syscall::EIO => "Input/output error",
        syscall::ENXIO => "No such device or address",
        syscall::EBADF => "Bad file descriptor",
        syscall::EAGAIN => "Resource temporarily unavailable",
        syscall::ENOMEM => "Cannot allocate memory",
        syscall::EACCES => "Permission denied",
        syscall::EFAULT => "Bad address",
        syscall::EBUSY => "Device or resource busy",
        syscall::EEXIST => "File exists",
        syscall::ENODEV => "No such device",
        syscall::ENOTDIR => "Not a directory",
        syscall::EISDIR => "Is a directory",
        syscall::EINVAL => "Invalid argument",
        syscall::ENOSPC => "No space left on device",
        syscall::EDOM => "Numerical argument out of domain",
        syscall::ERANGE => "Numerical result out of range",
        syscall::ENOSYS => "Function not implemented",
        _ => return format!("Unknown error {errno}"),
    };
    name.to_string()
}
