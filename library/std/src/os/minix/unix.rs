//! `std::os::unix`, for minix.
//!
//! MINIX is not a `cfg(unix)` target in this toolchain -- declaring the family
//! turns on the `libc` crate's Unix tree and std's Unix PAL, neither of which
//! the port has -- but it is a Unix, and crates name `std::os::unix::io`,
//! `::net`, `::fs` and `::prelude` without guarding them. This module aliases
//! those names onto the port's own implementations so such crates build
//! unmodified; it adds no API of its own.

#![stable(feature = "minix_os", since = "1.100.0")]
#![deny(unsafe_op_in_unsafe_fn)]

#[stable(feature = "minix_os", since = "1.100.0")]
pub mod ffi {
    //! Unix-specific conversions for `OsStr`/`OsString`.
    #[stable(feature = "minix_os", since = "1.100.0")]
    pub use crate::os::minix::ffi::*;
}

#[stable(feature = "minix_os", since = "1.100.0")]
pub mod io {
    //! Unix-specific I/O extensions.
    #[stable(feature = "minix_os", since = "1.100.0")]
    pub use crate::os::fd::*;
}

#[stable(feature = "minix_os", since = "1.100.0")]
pub mod net {
    //! Unix-specific networking.
    #[stable(feature = "minix_os", since = "1.100.0")]
    pub use crate::os::minix::net::*;
}

#[stable(feature = "minix_os", since = "1.100.0")]
pub mod fs {
    //! Unix-specific file extensions.
    #[stable(feature = "minix_os", since = "1.100.0")]
    pub use crate::os::minix::fs::*;
}

#[stable(feature = "minix_os", since = "1.100.0")]
pub mod prelude {
    //! A prelude for platform-specific code.
    #[stable(feature = "minix_os", since = "1.100.0")]
    pub use crate::os::fd::{AsFd, AsRawFd, BorrowedFd, FromRawFd, IntoRawFd, OwnedFd, RawFd};
    #[stable(feature = "minix_os", since = "1.100.0")]
    pub use crate::os::minix::ffi::{OsStrExt, OsStringExt};
    #[stable(feature = "minix_os", since = "1.100.0")]
    pub use crate::os::minix::fs::{MetadataExt, OpenOptionsExt};
}
