use crate::fmt;
use crate::io::{self, BorrowedCursor, IoSlice, IoSliceMut};
use crate::os::fd::{FromRawFd, RawFd};
use crate::sys::fd::FileDesc;
use crate::sys::pal::minix::{fs, syscall};

/// A pipe end, owned (closes the fd on drop).
pub struct Pipe(FileDesc);

impl Pipe {
    pub(crate) fn from_raw_fd(fd: RawFd) -> Pipe {
        // SAFETY: the caller hands over ownership of an open descriptor.
        Pipe(unsafe { FileDesc::from_raw_fd(fd) })
    }

    pub(crate) fn into_inner(self) -> FileDesc {
        self.0
    }

    pub fn try_clone(&self) -> io::Result<Pipe> {
        let ret = syscall::fcntl(self.0.as_raw_fd(), syscall::F_DUPFD, 3);
        if ret < 0 {
            Err(io::Error::from_raw_os_error(-ret as i32))
        } else {
            Ok(Pipe::from_raw_fd(ret as i32))
        }
    }

    pub fn read(&self, buf: &mut [u8]) -> io::Result<usize> {
        // PFS pipe reads return EAGAIN while a writer is open and 0 at EOF
        // (this port's pipes don't suspend readers yet); retry the transient
        // empty case so reads behave like blocking reads.
        loop {
            match fs::read(self.0.as_raw_fd(), buf) {
                Err(e) if e.raw_os_error() == Some(syscall::EAGAIN) => continue,
                r => return r,
            }
        }
    }

    pub fn read_buf(&self, cursor: BorrowedCursor<'_, u8>) -> io::Result<()> {
        crate::io::default_read_buf(|buf| self.read(buf), cursor)
    }

    pub fn read_vectored(&self, bufs: &mut [IoSliceMut<'_>]) -> io::Result<usize> {
        // Single-buffer I/O: read into the first non-empty slice.
        for buf in bufs {
            if !buf.is_empty() {
                return self.read(buf);
            }
        }
        Ok(0)
    }

    pub fn is_read_vectored(&self) -> bool {
        false
    }

    pub fn read_to_end(&self, buf: &mut Vec<u8>) -> io::Result<usize> {
        let mut tmp = [0u8; 1024];
        let mut total = 0usize;
        loop {
            match self.read(&mut tmp) {
                Ok(0) => return Ok(total),
                Ok(n) => {
                    buf.extend_from_slice(&tmp[..n]);
                    total += n;
                }
                Err(e) => return Err(e),
            }
        }
    }

    pub fn write(&self, buf: &[u8]) -> io::Result<usize> {
        // A full pipe buffer also returns EAGAIN; retry until it drains.
        loop {
            match fs::write(self.0.as_raw_fd(), buf) {
                Err(e) if e.raw_os_error() == Some(syscall::EAGAIN) => continue,
                r => return r,
            }
        }
    }

    pub fn write_vectored(&self, bufs: &[IoSlice<'_>]) -> io::Result<usize> {
        let mut written = 0usize;
        for buf in bufs {
            written += self.write(buf)?;
        }
        Ok(written)
    }

    pub fn is_write_vectored(&self) -> bool {
        false
    }
}

impl fmt::Debug for Pipe {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Pipe").field(&self.0).finish()
    }
}

pub fn pipe() -> io::Result<(Pipe, Pipe)> {
    let (r, w) = syscall::pipe();
    if r < 0 {
        return Err(io::Error::from_raw_os_error(-r));
    }
    Ok((Pipe::from_raw_fd(r), Pipe::from_raw_fd(w)))
}
