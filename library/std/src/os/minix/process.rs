//! Minix-specific extensions to `std::process`.
//!
//! The standard library's unix `CommandExt` carries uid/gid, process-group,
//! chroot and `pre_exec` setters as well as `exec`. The port's process model
//! has no equivalent for those (PM owns identity and the kernel owns the
//! image), so this trait is the exec alone.

use crate::io;
use crate::process;
use crate::sys::AsInnerMut;

/// Minix-specific extensions to the [`process::Command`] builder.
#[stable(feature = "minix_os", since = "1.100.0")]
pub trait CommandExt {
    /// Replaces the current process with the command's program.
    ///
    /// The program is resolved the same way a spawn resolves it (the path as
    /// given when it contains a slash, otherwise `/bin` and then `/sbin`), the
    /// command's environment and arguments are passed, and a configured
    /// working directory is entered first. On success this never returns.
    ///
    /// # Errors
    ///
    /// The error the exec failed with, or `InvalidInput` when the command was
    /// configured with a pipe: a pipe cannot survive the replacement.
    #[stable(feature = "minix_os", since = "1.100.0")]
    fn exec(&mut self) -> io::Error;
}

#[stable(feature = "minix_os", since = "1.100.0")]
impl CommandExt for process::Command {
    fn exec(&mut self) -> io::Error {
        self.as_inner_mut().exec(crate::sys::process::Stdio::Inherit)
    }
}
