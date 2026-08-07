//! Working directory and temp dir.

use crate::io;
use crate::path::{Path, PathBuf};

pub fn chdir(path: &Path) -> io::Result<()> {
    let bytes = path.as_os_str().as_encoded_bytes();
    let r = minix_rt::chdir(bytes);
    if r < 0 {
        Err(io::Error::from_raw_os_error(-r as i32))
    } else {
        Ok(())
    }
}

pub fn temp_dir() -> PathBuf {
    PathBuf::from("/tmp")
}
