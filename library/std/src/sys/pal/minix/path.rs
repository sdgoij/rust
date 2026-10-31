//! Working directory and temp dir.

use crate::io;
use crate::os::minix::ffi::OsStringExt as _;
use crate::path::{Path, PathBuf};
use crate::ffi::OsString;

pub fn chdir(path: &Path) -> io::Result<()> {
    let bytes = path.as_os_str().as_encoded_bytes();
    let r = minix_rt::chdir(bytes);
    if r < 0 {
        Err(io::Error::from_raw_os_error(-r as i32))
    } else {
        Ok(())
    }
}

/// The current working directory, recovered by walking up to the root and back
/// down (see `minix_std::fs::getcwd`).
///
/// The walk reports `ERANGE` when the buffer cannot hold the path, which is the
/// only reason to retry; it starts small because most paths are short and stops
/// at the port's own ceiling.
pub fn getcwd() -> io::Result<PathBuf> {
    let mut buf = vec![0u8; 256];
    loop {
        match minix_std::fs::getcwd(&mut buf) {
            Ok(len) => {
                let bytes = buf[..len].to_vec();
                return Ok(PathBuf::from(OsString::from_vec(bytes)));
            }
            Err(e)
                if e == minix_std::MinixErr::from_i32(minix_std::ERANGE)
                    && buf.len() < minix_std::fs::PATH_MAX =>
            {
                let next = (buf.len() * 2).min(minix_std::fs::PATH_MAX);
                buf.resize(next, 0);
            }
            Err(e) => return Err(io::Error::from_raw_os_error(e.0)),
        }
    }
}

pub fn temp_dir() -> PathBuf {
    PathBuf::from("/tmp")
}
