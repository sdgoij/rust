//! Minix-specific networking.

mod addr;
mod listener;
mod stream;

use crate::io;

/// Turns a `minix-std` return value into an [`io::Error`]. `MinixErr` carries
/// the positive errno, which is the form `from_raw_os_error` expects.
pub(crate) fn err(e: minix_std::MinixErr) -> io::Error {
    io::Error::from_raw_os_error(e.0)
}

#[stable(feature = "minix_os", since = "1.100.0")]
pub use self::addr::SocketAddr;
#[stable(feature = "minix_os", since = "1.100.0")]
pub use self::listener::UnixListener;
#[stable(feature = "minix_os", since = "1.100.0")]
pub use self::stream::UnixStream;
