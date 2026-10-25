//! Minix-specific extensions to `std`.
//!
//! MINIX is not a `cfg(unix)` target in this toolchain, so `std::os::unix`
//! does not exist for it. `std::os::fd` already covers the descriptor traits;
//! [`net`] supplies the local-domain socket layer over the port's `/dev/uds`
//! server, which is what the Wayland stack needs; [`fs`] adds the two
//! `open`/`stat` extensions those clients also name.

#![stable(feature = "minix_os", since = "1.100.0")]
#![deny(unsafe_op_in_unsafe_fn)]

#[stable(feature = "minix_os", since = "1.100.0")]
pub mod ffi;
#[stable(feature = "minix_os", since = "1.100.0")]
pub mod fs;
#[stable(feature = "minix_os", since = "1.100.0")]
pub mod net;
