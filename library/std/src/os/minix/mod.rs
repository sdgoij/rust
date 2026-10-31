//! Minix-specific extensions to `std`.
//!
//! MINIX was not a `cfg(unix)` target when these modules were written, so
//! `std::os::unix` was not available for it. `std::os::fd` already covers the
//! descriptor traits; [`net`] supplies the local-domain socket layer over the
//! port's `/dev/uds` server, which is what the Wayland stack needs; [`fs`] adds
//! the `open`/`stat` extensions and the offset read and write those clients also
//! name; [`thread`] names the kernel's thread id.

#![stable(feature = "minix_os", since = "1.100.0")]
#![deny(unsafe_op_in_unsafe_fn)]

#[stable(feature = "minix_os", since = "1.100.0")]
pub mod ffi;
#[stable(feature = "minix_os", since = "1.100.0")]
pub mod fs;
#[stable(feature = "minix_os", since = "1.100.0")]
pub mod net;
#[stable(feature = "minix_os", since = "1.100.0")]
pub mod process;
#[stable(feature = "minix_os", since = "1.100.0")]
pub mod thread;
