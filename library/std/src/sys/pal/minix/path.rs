//! Working directory and temp dir.

use crate::io;
use crate::path::{Path, PathBuf};
use crate::sys::syscall;

pub fn chdir(path: &Path) -> io::Result<()> {
    let bytes = path.as_os_str().as_encoded_bytes();
    let mut msg = [0u8; 64];
    // VFS_CHDIR: path@8, len@16.
    syscall::msg_set_i32(&mut msg, 4, syscall::VFS_CHDIR);
    syscall::msg_set_u64(&mut msg, 8, bytes.as_ptr().addr() as u64);
    syscall::msg_set_i32(&mut msg, 16, bytes.len() as i32);
    // SAFETY: `msg` is a valid message buffer.
    match unsafe { syscall::vfs_call(&mut msg) } {
        Ok(_) => Ok(()),
        Err(e) => Err(io::Error::from_raw_os_error(-e)),
    }
}

pub fn temp_dir() -> PathBuf {
    PathBuf::from("/tmp")
}
