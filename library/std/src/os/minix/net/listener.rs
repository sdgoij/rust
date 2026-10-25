use crate::io;
use crate::os::fd::{AsFd, AsRawFd, BorrowedFd, FromRawFd, OwnedFd, RawFd};
use crate::path::Path;
use crate::sys::fd::FileDesc;
use crate::sys::FromInner;

use super::addr::SocketAddr;
use super::err;
use super::stream::UnixStream;

/// The backlog `bind` asks the `/dev/uds` server for; it caps it at its own
/// limit.
const LISTEN_BACKLOG: i32 = 128;

/// A local-domain listening socket, backed by the port's `/dev/uds` server.
///
/// The port has no `cfg(unix)` family, so `std::os::unix::net::UnixListener`
/// does not exist for the target; this is its counterpart. Dropping the
/// listener closes the descriptor but does not unlink the path the caller gave
/// to [`bind`](UnixListener::bind).
#[stable(feature = "minix_os", since = "1.100.0")]
#[derive(Debug)]
pub struct UnixListener {
    inner: FileDesc,
}

impl UnixListener {
    /// Binds to `path` and starts listening.
    #[stable(feature = "minix_os", since = "1.100.0")]
    pub fn bind<P: AsRef<Path>>(path: P) -> io::Result<UnixListener> {
        let fd = minix_std::uds::socket().map_err(err)?;
        let name = path.as_ref().as_os_str().as_encoded_bytes();
        let listening = minix_std::uds::bind(fd, name)
            .and_then(|()| minix_std::uds::listen(fd, LISTEN_BACKLOG));
        match listening {
            Ok(()) => Ok(UnixListener { inner: unsafe { FileDesc::from_raw_fd(fd) } }),
            Err(e) => {
                // Adopt the descriptor so dropping it closes it.
                drop(unsafe { FileDesc::from_raw_fd(fd) });
                Err(err(e))
            }
        }
    }

    /// Accepts a pending connection, or reports [`WouldBlock`] if none is
    /// queued.
    ///
    /// The address is the one the accepted socket is bound to, which the
    /// `/dev/uds` server reports by name.
    ///
    /// [`WouldBlock`]: io::ErrorKind::WouldBlock
    #[stable(feature = "minix_os", since = "1.100.0")]
    pub fn accept(&self) -> io::Result<(UnixStream, SocketAddr)> {
        let fd = minix_std::uds::accept(self.inner.as_raw_fd()).map_err(err)?;
        // SAFETY: `accept` returns a freshly opened, owned descriptor.
        let stream = UnixStream { inner: unsafe { FileDesc::from_raw_fd(fd) } };
        let addr = minix_std::uds::getsockname(fd).map_err(err)?;
        Ok((stream, SocketAddr::from_wire(&addr)))
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
impl AsFd for UnixListener {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.inner.as_fd()
    }
}

#[stable(feature = "minix_os", since = "1.100.0")]
impl AsRawFd for UnixListener {
    fn as_raw_fd(&self) -> RawFd {
        self.inner.as_raw_fd()
    }
}

#[stable(feature = "minix_os", since = "1.100.0")]
impl FromRawFd for UnixListener {
    unsafe fn from_raw_fd(fd: RawFd) -> UnixListener {
        // SAFETY: the caller guarantees ownership of `fd`.
        UnixListener { inner: unsafe { FileDesc::from_raw_fd(fd) } }
    }
}

#[stable(feature = "minix_os", since = "1.100.0")]
impl From<OwnedFd> for UnixListener {
    fn from(fd: OwnedFd) -> UnixListener {
        UnixListener { inner: FileDesc::from_inner(fd) }
    }
}
