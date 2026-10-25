use crate::ffi::{OsStr, OsString};

/// Minix-specific extensions to [`OsStr`].
#[stable(feature = "minix_os", since = "1.100.0")]
pub trait OsStrExt {
    /// Makes an [`OsStr`] from a byte slice.
    #[stable(feature = "minix_os", since = "1.100.0")]
    fn from_bytes(slice: &[u8]) -> &Self;

    /// The bytes this `OsStr` is made of.
    #[stable(feature = "minix_os", since = "1.100.0")]
    fn as_bytes(&self) -> &[u8];
}

#[stable(feature = "minix_os", since = "1.100.0")]
impl OsStrExt for OsStr {
    fn from_bytes(slice: &[u8]) -> &OsStr {
        // SAFETY: an `OsStr` on this target is a byte string; the encoding is
        // not interpreted.
        unsafe { OsStr::from_encoded_bytes_unchecked(slice) }
    }

    fn as_bytes(&self) -> &[u8] {
        self.as_encoded_bytes()
    }
}

/// Minix-specific extensions to [`OsString`].
#[stable(feature = "minix_os", since = "1.100.0")]
pub trait OsStringExt {
    /// Makes an [`OsString`] from a byte vector.
    #[stable(feature = "minix_os", since = "1.100.0")]
    fn from_vec(vec: Vec<u8>) -> Self;

    /// Takes this `OsString`'s bytes.
    #[stable(feature = "minix_os", since = "1.100.0")]
    fn into_vec(self) -> Vec<u8>;
}

#[stable(feature = "minix_os", since = "1.100.0")]
impl OsStringExt for OsString {
    fn from_vec(vec: Vec<u8>) -> OsString {
        // SAFETY: as for `OsStrExt::from_bytes`.
        unsafe { OsString::from_encoded_bytes_unchecked(vec) }
    }

    fn into_vec(self) -> Vec<u8> {
        self.into_encoded_bytes()
    }
}
