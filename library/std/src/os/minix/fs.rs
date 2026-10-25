use crate::fs::{Metadata, OpenOptions};
use crate::sys::{AsInner, AsInnerMut};

/// Minix-specific extensions to [`OpenOptions`].
#[stable(feature = "minix_os", since = "1.100.0")]
pub trait OpenOptionsExt {
    /// Sets the mode bits, which is `open(2)`'s third argument when the file
    /// is created.
    #[stable(feature = "minix_os", since = "1.100.0")]
    fn mode(&mut self, mode: u32) -> &mut Self;
}

#[stable(feature = "minix_os", since = "1.100.0")]
impl OpenOptionsExt for OpenOptions {
    fn mode(&mut self, mode: u32) -> &mut Self {
        self.as_inner_mut().mode(mode);
        self
    }
}

/// Minix-specific extensions to [`Metadata`].
#[stable(feature = "minix_os", since = "1.100.0")]
pub trait MetadataExt {
    /// The device the file resides on.
    #[stable(feature = "minix_os", since = "1.100.0")]
    fn dev(&self) -> u64;

    /// The file's inode number.
    #[stable(feature = "minix_os", since = "1.100.0")]
    fn ino(&self) -> u64;
}

#[stable(feature = "minix_os", since = "1.100.0")]
impl MetadataExt for Metadata {
    fn dev(&self) -> u64 {
        self.as_inner().dev()
    }

    fn ino(&self) -> u64 {
        self.as_inner().ino()
    }
}
