use crate::fs::{File, Metadata, OpenOptions};
use crate::io::{self, BorrowedCursor, IoSlice, IoSliceMut};
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
