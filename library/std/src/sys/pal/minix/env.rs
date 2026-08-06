//! Environment variables, snapshotted from the `envp` block on the initial
//! stack by the entry point.

use core::slice::memchr;

pub use crate::sys::env::common::Env;
use crate::collections::HashMap;
use crate::ffi::{CStr, OsStr, OsString, c_char};
use crate::io;
use crate::sync::Mutex;

static ENV: Mutex<Option<HashMap<OsString, OsString>>> = Mutex::new(None);

pub fn init(envp: *const *const c_char) {
    let mut guard = ENV.lock().unwrap();
    let map = guard.insert(HashMap::new());

    if envp.is_null() {
        return;
    }

    // SAFETY: `envp` points into the kernel-provided initial stack, laid out
    // as a NULL-terminated array of NUL-terminated strings.
    unsafe {
        let mut environ = envp;
        while !(*environ).is_null() {
            if let Some((key, value)) = parse(CStr::from_ptr(*environ).to_bytes()) {
                map.insert(key, value);
            }
            environ = environ.add(1);
        }
    }

    fn parse(input: &[u8]) -> Option<(OsString, OsString)> {
        // Variable name and value are separated by an ASCII '='. A variable
        // name must not be empty; skip malformed lines.
        if input.is_empty() {
            return None;
        }
        let pos = memchr::memchr(b'=', &input[1..]).map(|p| p + 1);
        pos.map(|p| {
            // SAFETY: the segments come from a valid C string and have no
            // interior NULs.
            unsafe {
                (
                    OsString::from_encoded_bytes_unchecked(input[..p].to_vec()),
                    OsString::from_encoded_bytes_unchecked(input[p + 1..].to_vec()),
                )
            }
        })
    }
}

/// Returns an iterator over all environment variables.
pub fn env() -> Env {
    let guard = ENV.lock().unwrap();
    let env = guard.as_ref().unwrap();

    let result = env.iter().map(|(key, value)| (key.clone(), value.clone())).collect();

    Env::new(result)
}

pub fn getenv(k: &OsStr) -> Option<OsString> {
    ENV.lock().unwrap().as_ref().unwrap().get(k).cloned()
}

pub unsafe fn setenv(k: &OsStr, v: &OsStr) -> io::Result<()> {
    let (k, v) = (k.to_owned(), v.to_owned());
    ENV.lock().unwrap().as_mut().unwrap().insert(k, v);
    Ok(())
}

pub unsafe fn unsetenv(k: &OsStr) -> io::Result<()> {
    ENV.lock().unwrap().as_mut().unwrap().remove(k);
    Ok(())
}
