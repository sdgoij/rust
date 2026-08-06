#![unstable(reason = "not public", issue = "none", feature = "fd")]

use crate::os::fd::{AsFd, AsRawFd, BorrowedFd, FromRawFd, IntoRawFd, OwnedFd, RawFd};
use crate::sys::{AsInner, FromInner, IntoInner};

/// An owned file descriptor for the Minix kernel's integer-fd interface.
///
/// Wraps an [`OwnedFd`]; dropping the descriptor closes it via the kernel's
/// `close` syscall.
#[derive(Debug)]
pub struct FileDesc(OwnedFd);

impl FileDesc {
    #[inline]
    pub fn as_raw_fd(&self) -> RawFd {
        self.0.as_raw_fd()
    }
}

impl AsInner<OwnedFd> for FileDesc {
    #[inline]
    fn as_inner(&self) -> &OwnedFd {
        &self.0
    }
}

impl IntoInner<OwnedFd> for FileDesc {
    #[inline]
    fn into_inner(self) -> OwnedFd {
        self.0
    }
}

impl FromInner<OwnedFd> for FileDesc {
    #[inline]
    fn from_inner(owned_fd: OwnedFd) -> Self {
        Self(owned_fd)
    }
}

impl AsFd for FileDesc {
    #[inline]
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.0.as_fd()
    }
}

impl AsRawFd for FileDesc {
    #[inline]
    fn as_raw_fd(&self) -> RawFd {
        self.0.as_raw_fd()
    }
}

impl IntoRawFd for FileDesc {
    #[inline]
    fn into_raw_fd(self) -> RawFd {
        self.0.into_raw_fd()
    }
}

impl FromRawFd for FileDesc {
    #[inline]
    unsafe fn from_raw_fd(fd: RawFd) -> Self {
        // SAFETY: the caller must pass an owned file descriptor.
        Self(unsafe { OwnedFd::from_raw_fd(fd) })
    }
}
