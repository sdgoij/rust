use crate::fs::{DirBuilder, File, FileType, Metadata, OpenOptions, Permissions};
use crate::io::{self, BorrowedCursor, IoSlice, IoSliceMut};
use crate::path::Path;
use crate::sys::{AsInner, AsInnerMut, FromInner};

/// Minix-specific extensions to [`OpenOptions`].
#[stable(feature = "minix_os", since = "1.100.0")]
pub trait OpenOptionsExt {
    /// Sets the mode bits, which is `open(2)`'s third argument when the file
    /// is created.
    #[stable(feature = "minix_os", since = "1.100.0")]
    fn mode(&mut self, mode: u32) -> &mut Self;

    /// Sets the `O_*` flags that go to `open(2)` beyond the ones the builder
    /// derives from read, write, append, truncate and create.
    #[stable(feature = "minix_os", since = "1.100.0")]
    fn custom_flags(&mut self, flags: i32) -> &mut Self;
}

#[stable(feature = "minix_os", since = "1.100.0")]
impl OpenOptionsExt for OpenOptions {
    fn mode(&mut self, mode: u32) -> &mut Self {
        self.as_inner_mut().mode(mode);
        self
    }

    fn custom_flags(&mut self, flags: i32) -> &mut Self {
        self.as_inner_mut().custom_flags(flags);
        self
    }
}

/// Minix-specific extensions to [`FileType`].
#[stable(feature = "minix_os", since = "1.100.0")]
pub trait FileTypeExt {
    /// Is this a block device?
    #[stable(feature = "minix_os", since = "1.100.0")]
    fn is_block_device(&self) -> bool;

    /// Is this a character device?
    #[stable(feature = "minix_os", since = "1.100.0")]
    fn is_char_device(&self) -> bool;

    /// Is this a FIFO (named pipe)?
    #[stable(feature = "minix_os", since = "1.100.0")]
    fn is_fifo(&self) -> bool;

    /// Is this a socket?
    #[stable(feature = "minix_os", since = "1.100.0")]
    fn is_socket(&self) -> bool;
}

#[stable(feature = "minix_os", since = "1.100.0")]
impl FileTypeExt for FileType {
    fn is_block_device(&self) -> bool {
        self.as_inner().is_block_device()
    }

    fn is_char_device(&self) -> bool {
        self.as_inner().is_char_device()
    }

    fn is_fifo(&self) -> bool {
        self.as_inner().is_fifo()
    }

    fn is_socket(&self) -> bool {
        self.as_inner().is_socket()
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

    /// The raw `st_mode`: the file type bits and the permission bits.
    #[stable(feature = "minix_os", since = "1.100.0")]
    fn mode(&self) -> u32;

    /// The owner's user id.
    #[stable(feature = "minix_os", since = "1.100.0")]
    fn uid(&self) -> u32;

    /// The group's id.
    #[stable(feature = "minix_os", since = "1.100.0")]
    fn gid(&self) -> u32;

    /// The number of hard links to the file.
    #[stable(feature = "minix_os", since = "1.100.0")]
    fn nlink(&self) -> u64;

    /// The device id, for a character or block special file.
    #[stable(feature = "minix_os", since = "1.100.0")]
    fn rdev(&self) -> u64;

    /// The size of the file in bytes.
    #[stable(feature = "minix_os", since = "1.100.0")]
    fn size(&self) -> u64;

    /// The preferred I/O block size for the file.
    #[stable(feature = "minix_os", since = "1.100.0")]
    fn blksize(&self) -> u64;

    /// The number of blocks the file occupies, in 512-byte units.
    #[stable(feature = "minix_os", since = "1.100.0")]
    fn blocks(&self) -> u64;
}

#[stable(feature = "minix_os", since = "1.100.0")]
impl MetadataExt for Metadata {
    fn dev(&self) -> u64 {
        self.as_inner().dev()
    }

    fn ino(&self) -> u64 {
        self.as_inner().ino()
    }

    fn mode(&self) -> u32 {
        self.as_inner().mode()
    }

    fn uid(&self) -> u32 {
        self.as_inner().uid()
    }

    fn gid(&self) -> u32 {
        self.as_inner().gid()
    }

    fn nlink(&self) -> u64 {
        self.as_inner().nlink()
    }

    fn rdev(&self) -> u64 {
        self.as_inner().rdev()
    }

    fn size(&self) -> u64 {
        self.as_inner().size()
    }

    fn blksize(&self) -> u64 {
        self.as_inner().blksize()
    }

    fn blocks(&self) -> u64 {
        self.as_inner().blocks()
    }
}

/// Minix-specific extensions to [`Permissions`].
#[stable(feature = "minix_os", since = "1.100.0")]
pub trait PermissionsExt {
    /// The raw `st_mode`, which includes the file type bits when the value came
    /// from [`Metadata::permissions`].
    #[stable(feature = "minix_os", since = "1.100.0")]
    fn mode(&self) -> u32;

    /// Sets the permission bits from a raw mode.
    #[stable(feature = "minix_os", since = "1.100.0")]
    fn set_mode(&mut self, mode: u32);

    /// Builds a [`Permissions`] from a raw mode.
    #[stable(feature = "minix_os", since = "1.100.0")]
    fn from_mode(mode: u32) -> Self;
}

#[stable(feature = "minix_os", since = "1.100.0")]
impl PermissionsExt for Permissions {
    fn mode(&self) -> u32 {
        self.as_inner().mode()
    }

    fn set_mode(&mut self, mode: u32) {
        *self = Self::from_mode(mode);
    }

    fn from_mode(mode: u32) -> Self {
        Self::from_inner(crate::sys::fs::FilePermissions::from_mode(mode))
    }
}

/// Minix-specific extensions to [`DirBuilder`].
#[stable(feature = "minix_os", since = "1.100.0")]
pub trait DirBuilderExt {
    /// Sets the mode bits the builder gives the directory it creates.
    #[stable(feature = "minix_os", since = "1.100.0")]
    fn mode(&mut self, mode: u32) -> &mut Self;
}

#[stable(feature = "minix_os", since = "1.100.0")]
impl DirBuilderExt for DirBuilder {
    fn mode(&mut self, mode: u32) -> &mut Self {
        self.as_inner_mut().mode(mode);
        self
    }
}

/// Creates a symbolic link at `link` that points to `original`.
#[stable(feature = "minix_os", since = "1.100.0")]
pub fn symlink<P: AsRef<Path>, Q: AsRef<Path>>(original: P, link: Q) -> io::Result<()> {
    crate::sys::fs::symlink(original.as_ref(), link.as_ref())
}

/// Minix-specific extensions to [`File`] for reading and writing at an offset,
/// which the port's VFS answers with `pread`/`pwrite`.
#[stable(feature = "minix_os", since = "1.100.0")]
pub trait FileExt {
    /// Reads a number of bytes starting from a given offset, leaving the file's
    /// own cursor where it is.
    ///
    /// Returns the number of bytes read, which may be short.
    #[stable(feature = "minix_os", since = "1.100.0")]
    fn read_at(&self, buf: &mut [u8], offset: u64) -> io::Result<usize>;

    /// Like [`read_at`](FileExt::read_at), for a slice of buffers.
    #[unstable(feature = "unix_file_vectored_at", issue = "89517")]
    fn read_vectored_at(&self, bufs: &mut [IoSliceMut<'_>], offset: u64) -> io::Result<usize> {
        io::default_read_vectored(|b| self.read_at(b, offset), bufs)
    }

    /// Reads exactly `buf.len()` bytes from a given offset.
    #[stable(feature = "minix_os", since = "1.100.0")]
    fn read_exact_at(&self, mut buf: &mut [u8], mut offset: u64) -> io::Result<()> {
        while !buf.is_empty() {
            match self.read_at(buf, offset) {
                Ok(0) => break,
                Ok(n) => {
                    let tmp = buf;
                    buf = &mut tmp[n..];
                    offset += n as u64;
                }
                Err(ref e) if e.is_interrupted() => {}
                Err(e) => return Err(e),
            }
        }
        if !buf.is_empty() { Err(io::Error::READ_EXACT_EOF) } else { Ok(()) }
    }

    /// Reads some bytes starting from a given offset into the buffer, through a
    /// [`BorrowedCursor`] so uninitialised buffers can be used.
    #[unstable(feature = "read_buf_at", issue = "140771")]
    fn read_buf_at(&self, buf: BorrowedCursor<'_, u8>, offset: u64) -> io::Result<()> {
        io::default_read_buf(|b| self.read_at(b, offset), buf)
    }

    /// Like [`read_exact_at`](FileExt::read_exact_at), through a
    /// [`BorrowedCursor`].
    #[unstable(feature = "read_buf_at", issue = "140771")]
    #[doc(alias("read_exact_buf_at"))]
    fn read_buf_exact_at(
        &self,
        mut buf: BorrowedCursor<'_, u8>,
        mut offset: u64,
    ) -> io::Result<()> {
        while buf.capacity() > 0 {
            let prev_written = buf.written();
            match self.read_buf_at(buf.reborrow(), offset) {
                Ok(()) => {}
                Err(e) if e.is_interrupted() => {}
                Err(e) => return Err(e),
            }
            let n = buf.written() - prev_written;
            offset += n as u64;
            if n == 0 {
                return Err(io::Error::READ_EXACT_EOF);
            }
        }
        Ok(())
    }

    /// Writes a number of bytes starting from a given offset, leaving the
    /// file's own cursor where it is.
    ///
    /// Returns the number of bytes written, which may be short.
    #[stable(feature = "minix_os", since = "1.100.0")]
    fn write_at(&self, buf: &[u8], offset: u64) -> io::Result<usize>;

    /// Like [`write_at`](FileExt::write_at), for a slice of buffers.
    #[unstable(feature = "unix_file_vectored_at", issue = "89517")]
    fn write_vectored_at(&self, bufs: &[IoSlice<'_>], offset: u64) -> io::Result<usize> {
        io::default_write_vectored(|b| self.write_at(b, offset), bufs)
    }

    /// Writes all of `buf` at a given offset.
    #[stable(feature = "minix_os", since = "1.100.0")]
    fn write_all_at(&self, mut buf: &[u8], mut offset: u64) -> io::Result<()> {
        while !buf.is_empty() {
            match self.write_at(buf, offset) {
                Ok(0) => return Err(io::Error::WRITE_ALL_EOF),
                Ok(n) => {
                    buf = &buf[n..];
                    offset += n as u64;
                }
                Err(ref e) if e.is_interrupted() => {}
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }
}

#[stable(feature = "minix_os", since = "1.100.0")]
impl FileExt for File {
    fn read_at(&self, buf: &mut [u8], offset: u64) -> io::Result<usize> {
        self.as_inner().read_at(buf, offset)
    }

    fn write_at(&self, buf: &[u8], offset: u64) -> io::Result<usize> {
        self.as_inner().write_at(buf, offset)
    }
}
