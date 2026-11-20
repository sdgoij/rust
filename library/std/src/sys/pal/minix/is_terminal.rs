use crate::os::fd::{AsFd, AsRawFd};

/// Whether the descriptor is a terminal.
///
/// The answer comes from the descriptor's file type, which is what the C
/// library's `isatty` reads: VFS replies to an ioctl it does not route with
/// success, so a request aimed at the terminal itself would call a pipe a
/// terminal. Both console streams are character devices, so they answer true,
/// and a redirect to a file or a pipe answers false.
pub fn is_terminal(fd: &impl AsFd) -> bool {
    match minix_std::fs::fstat(fd.as_fd().as_raw_fd()) {
        Ok(stat) => stat.st_mode & minix_std::fs::S_IFMT == minix_std::fs::S_IFCHR,
        Err(_) => false,
    }
}
