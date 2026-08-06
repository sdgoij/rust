//! Standard input/output over the kernel's `read`/`write` syscalls on the
//! fixed fds 0/1/2 (VFS-owned or serial console, see `set_fd_vfs`).

use crate::io;
use crate::sys::syscall;

pub const STDIN_BUF_SIZE: usize = crate::sys::io::DEFAULT_BUF_SIZE;

const STDIN_FD: i32 = 0;
const STDOUT_FD: i32 = 1;
const STDERR_FD: i32 = 2;

fn errno_of(ret: i64) -> io::Error {
    io::Error::from_raw_os_error((-ret) as i32)
}

pub struct Stdin {}

impl Stdin {
    pub const fn new() -> Self {
        Self {}
    }
}

pub struct Stdout {}

impl Stdout {
    pub const fn new() -> Self {
        Self {}
    }
}

pub struct Stderr {}

impl Stderr {
    pub const fn new() -> Self {
        Self {}
    }
}

impl io::Read for Stdin {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let r = syscall::read(STDIN_FD, buf);
        if r < 0 { Err(errno_of(r)) } else { Ok(r as usize) }
    }
}

impl io::Write for Stdout {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let r = syscall::write(STDOUT_FD, buf);
        if r < 0 { Err(errno_of(r)) } else { Ok(r as usize) }
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl io::Write for Stderr {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let r = syscall::write(STDERR_FD, buf);
        if r < 0 { Err(errno_of(r)) } else { Ok(r as usize) }
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub fn panic_output() -> Option<impl io::Write> {
    Some(Stderr::new())
}

pub fn is_ebadf(err: &io::Error) -> bool {
    err.raw_os_error() == Some(syscall::EBADF)
}
