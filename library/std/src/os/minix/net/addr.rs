use crate::ffi::OsString;
use crate::path::{Path, PathBuf};

/// The address of a local-domain socket.
///
/// The port's `/dev/uds` server names a socket by a path, so this is the
/// pathname form or nothing.
#[stable(feature = "minix_os", since = "1.100.0")]
#[derive(Clone, Debug)]
pub struct SocketAddr {
    path: Option<PathBuf>,
}

impl SocketAddr {
    /// The path this socket is bound to, if it has one.
    #[stable(feature = "minix_os", since = "1.100.0")]
    pub fn as_pathname(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Whether the socket is unnamed.
    #[stable(feature = "minix_os", since = "1.100.0")]
    pub fn is_unnamed(&self) -> bool {
        self.path.is_none()
    }

    /// Decodes a `sockaddr_un` as the `/dev/uds` ioctls return it: the family in
    /// the first two bytes, then a NUL-terminated path.
    pub(crate) fn from_wire(bytes: &[u8]) -> SocketAddr {
        let path = match bytes.get(2..) {
            Some(path) => path,
            None => return SocketAddr { path: None },
        };
        let end = path.iter().position(|&b| b == 0).unwrap_or(path.len());
        if end == 0 {
            return SocketAddr { path: None };
        }
        // SAFETY: the bytes are the path a caller bound, copied verbatim by the
        // server into the reply.
        let name = unsafe { OsString::from_encoded_bytes_unchecked(path[..end].to_vec()) };
        SocketAddr { path: Some(PathBuf::from(name)) }
    }
}
