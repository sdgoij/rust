//! Minix-specific extensions to [`std::thread`].
//!
//! The port has no pthreads: a thread is a kernel `Proc` slot and
//! `thread_create` hands back its id, so [`RawPthread`] is that id rather than a
//! `pthread_t` from a C library.
//!
//! [`std::thread`]: crate::thread

use crate::sys::{AsInner, IntoInner};
use crate::thread::JoinHandle;

/// The kernel's id for a thread, as `thread_create` returns it.
#[stable(feature = "minix_os", since = "1.100.0")]
pub type RawPthread = u64;

/// Minix-specific extensions to [`JoinHandle`].
#[stable(feature = "minix_os", since = "1.100.0")]
pub trait JoinHandleExt {
    /// Extracts the raw thread id without taking ownership.
    #[stable(feature = "minix_os", since = "1.100.0")]
    fn as_pthread_t(&self) -> RawPthread;

    /// Consumes the handle, returning the raw thread id, so that the caller
    /// becomes its only owner.
    #[stable(feature = "minix_os", since = "1.100.0")]
    fn into_pthread_t(self) -> RawPthread;
}

#[stable(feature = "minix_os", since = "1.100.0")]
impl<T> JoinHandleExt for JoinHandle<T> {
    fn as_pthread_t(&self) -> RawPthread {
        self.as_inner().id()
    }

    fn into_pthread_t(self) -> RawPthread {
        self.into_inner().into_id()
    }
}
