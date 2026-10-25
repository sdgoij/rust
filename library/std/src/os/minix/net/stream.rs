use crate::io;
use crate::os::fd::{AsFd, AsRawFd, BorrowedFd, FromRawFd, IntoRawFd, OwnedFd, RawFd};
use crate::path::Path;
use crate::sys::fd::FileDesc;
use crate::sys::FromInner;

use super::err;

/// A local-domain stream socket, backed by the port's `/dev/uds` server.
///
/// The port has no `cfg(unix)` family, so `std::os::unix::net::UnixStream` does
/// not exist for the target; this is its counterpart. The data path is plain
/// `read`/`write` on the descriptor, which is what `rustix::net::send` and
/// `recvmsg` use.
///
/// The server is non-blocking by construction: `connect` queues the connection,
/// and reads and accepts report [`WouldBlock`] instead of waiting.
///
/// [`WouldBlock`]: io::ErrorKind::WouldBlock
#[stable(feature = "minix_os", since = "1.100.0")]
#[derive(Debug)]
pub struct UnixStream {
    pub(super) inner: FileDesc,
}

impl UnixStream {
    /// Connects to the listening socket bound to `path`.
    #[stable(feature = "minix_os", since = "1.100.0")]
    pub fn connect<P: AsRef<Path>>(path: P) -> io::Result<UnixStream> {
        let fd = minix_std::uds::socket().map_err(err)?;
        let name = path.as_ref().as_os_str().as_encoded_bytes();
        match minix_std::uds::connect(fd, name) {
            Ok(()) => Ok(UnixStream { inner: unsafe { FileDesc::from_raw_fd(fd) } }),
            Err(e) => {
                // Adopt the descriptor so dropping it closes it.
                drop(unsafe { FileDesc::from_raw_fd(fd) });
                Err(err(e))
            }
        }
    }

    /// Creates a pair of connected sockets.
    #[stable(feature = "minix_os", since = "1.100.0")]
    pub fn pair() -> io::Result<(UnixStream, UnixStream)> {
        let (a, b) = minix_std::uds::socketpair().map_err(err)?;
        // SAFETY: both descriptors are freshly opened and owned by the pair.
        Ok((
            UnixStream { inner: unsafe { FileDesc::from_raw_fd(a) } },
            UnixStream { inner: unsafe { FileDesc::from_raw_fd(b) } },
        ))
    }

    /// The `/dev/uds` server is non-blocking by construction, so enabling it is
    /// a no-op; blocking is not available.
    #[stable(feature = "minix_os", since = "1.100.0")]
    pub fn set_nonblocking(&self, nonblocking: bool) -> io::Result<()> {
        if nonblocking {
            Ok(())
        } else {
            Err(io::const_error!(
                io::ErrorKind::Unsupported,
                "blocking /dev/uds sockets are not supported"
            ))
        }
    }
}

#[stable(feature = "minix_os", since = "1.100.0")]
impl AsFd for UnixStream {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.inner.as_fd()
    }
}

#[stable(feature = "minix_os", since = "1.100.0")]
impl AsRawFd for UnixStream {
    fn as_raw_fd(&self) -> RawFd {
        self.inner.as_raw_fd()
    }
}

#[stable(feature = "minix_os", since = "1.100.0")]
impl FromRawFd for UnixStream {
    unsafe fn from_raw_fd(fd: RawFd) -> UnixStream {
        // SAFETY: the caller guarantees ownership of `fd`.
        UnixStream { inner: unsafe { FileDesc::from_raw_fd(fd) } }
    }
}

#[stable(feature = "minix_os", since = "1.100.0")]
impl IntoRawFd for UnixStream {
    fn into_raw_fd(self) -> RawFd {
        self.inner.into_raw_fd()
    }
}

#[stable(feature = "minix_os", since = "1.100.0")]
impl From<OwnedFd> for UnixStream {
    fn from(fd: OwnedFd) -> UnixStream {
        UnixStream { inner: FileDesc::from_inner(fd) }
    }
}
