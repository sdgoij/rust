//! File system operations over the VFS server protocol.
//!
//! The VFS server handles `open`/`read`/`write`/`lseek`/`close`/`getdents`/
//! `fstat` etc.; the kernel handles `mkdir`/`unlink`/`rmdir` directly.
//!
//! Note: the VFS `stat` path does not yet copy the `struct stat` payload back
//! to user space, so `FileAttr` synthesizes the file size from `lseek` until
//! that is wired up.

use crate::ffi::OsString;
use crate::fmt;
use crate::fs::TryLockError;
use crate::hash::Hasher;
use crate::io::{self, BorrowedCursor, IoSlice, IoSliceMut, SeekFrom};
use crate::os::fd::{AsFd, AsRawFd, BorrowedFd, FromRawFd, IntoRawFd, OwnedFd, RawFd};
use crate::path::{Path, PathBuf};
use crate::sys::fd::FileDesc;
pub use crate::sys::fs::common::Dir;
use crate::sys::time::SystemTime;
use crate::sys::{AsInner, FromInner, IntoInner, syscall, unsupported};

// File type bits for st_mode (from `minix/include/stat.h`).
const S_IFMT: u32 = 0o170000;
const S_IFDIR: u32 = 0o040000;
const S_IFREG: u32 = 0o100000;
const S_IFLNK: u32 = 0o120000;

// getdents entry type codes.
const DT_UNKNOWN: u8 = 0;
const DT_REG: u8 = 1;
const DT_DIR: u8 = 2;
const DT_LNK: u8 = 3;

pub struct File {
    inner: FileDesc,
}

impl File {
    fn fd(&self) -> RawFd {
        self.inner.as_raw_fd()
    }
}

impl AsInner<FileDesc> for File {
    #[inline]
    fn as_inner(&self) -> &FileDesc {
        &self.inner
    }
}

impl IntoInner<FileDesc> for File {
    #[inline]
    fn into_inner(self) -> FileDesc {
        self.inner
    }
}

impl FromInner<FileDesc> for File {
    #[inline]
    fn from_inner(inner: FileDesc) -> Self {
        Self { inner }
    }
}

impl AsFd for File {
    #[inline]
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.inner.as_fd()
    }
}

impl AsRawFd for File {
    #[inline]
    fn as_raw_fd(&self) -> RawFd {
        self.inner.as_raw_fd()
    }
}

impl IntoRawFd for File {
    #[inline]
    fn into_raw_fd(self) -> RawFd {
        self.into_inner().into_raw_fd()
    }
}

impl FromRawFd for File {
    #[inline]
    unsafe fn from_raw_fd(fd: RawFd) -> Self {
        // SAFETY: the caller must pass an owned file descriptor.
        Self::from_inner(unsafe { FileDesc::from_raw_fd(fd) })
    }
}

impl From<OwnedFd> for File {
    #[inline]
    fn from(owned_fd: OwnedFd) -> Self {
        Self::from_inner(FromInner::from_inner(owned_fd))
    }
}

#[derive(Clone)]
pub struct FileAttr {
    stat: syscall::Stat,
}

impl FileAttr {
    fn from_stat(stat: syscall::Stat) -> Self {
        Self { stat }
    }
}

// all DirEntry's will have a reference to this struct
struct InnerReadDir {
    root: PathBuf,
}

pub struct ReadDir {
    inner: crate::sync::Arc<InnerReadDir>,
    fd: i32,
    buf: [u8; 8192],
    pos: usize,
    filled: usize,
}

/// A directory entry as returned by `getdents`:
/// `d_ino (u32), d_rec_len (u16), d_name_len (u8), d_file_type (u8), d_name`.
#[repr(C)]
struct MinixDirent {
    d_ino: u32,
    d_rec_len: u16,
    d_name_len: u8,
    d_file_type: u8,
}

pub struct DirEntry {
    dir: crate::sync::Arc<InnerReadDir>,
    type_: u8,
    name: OsString,
}

#[derive(Clone, Debug)]
pub struct OpenOptions {
    // generic
    read: bool,
    write: bool,
    append: bool,
    truncate: bool,
    create: bool,
    create_new: bool,
    // system-specific
    mode: u32,
}

#[derive(Copy, Clone, Debug, Default)]
pub struct FileTimes {}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct FilePermissions {
    mode: u32,
}

#[derive(Copy, Clone, Eq, Debug)]
pub struct FileType {
    mode: u8,
}

impl PartialEq for FileType {
    fn eq(&self, other: &Self) -> bool {
        self.mode == other.mode
    }
}

impl core::hash::Hash for FileType {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.mode.hash(state);
    }
}

#[derive(Debug)]
pub struct DirBuilder {
    mode: u32,
}

impl FileAttr {
    pub fn size(&self) -> u64 {
        self.stat.st_size.max(0) as u64
    }

    pub fn perm(&self) -> FilePermissions {
        FilePermissions { mode: self.stat.st_mode }
    }

    pub fn file_type(&self) -> FileType {
        let masked_mode = self.stat.st_mode & S_IFMT;
        let mode = match masked_mode {
            S_IFDIR => DT_DIR,
            S_IFLNK => DT_LNK,
            S_IFREG => DT_REG,
            // The VFS server does not copy the stat payload yet, so treat
            // zeroed stat data as a regular file.
            0 => DT_REG,
            _ => DT_UNKNOWN,
        };
        FileType { mode }
    }

    pub fn modified(&self) -> io::Result<SystemTime> {
        Ok(SystemTime::from_secs(self.stat.st_mtime.max(0) as u64))
    }

    pub fn accessed(&self) -> io::Result<SystemTime> {
        Ok(SystemTime::from_secs(self.stat.st_atime.max(0) as u64))
    }

    pub fn created(&self) -> io::Result<SystemTime> {
        Ok(SystemTime::from_secs(self.stat.st_ctime.max(0) as u64))
    }
}

impl FilePermissions {
    pub fn readonly(&self) -> bool {
        // Check if any class (owner, group, others) has write permission.
        self.mode & 0o222 == 0
    }

    pub fn set_readonly(&mut self, readonly: bool) {
        if readonly {
            self.mode &= !0o222;
        } else {
            self.mode |= 0o222;
        }
    }
}

impl FileTimes {
    pub fn set_accessed(&mut self, _t: SystemTime) {}
    pub fn set_modified(&mut self, _t: SystemTime) {}
}

impl FileType {
    pub fn is_dir(&self) -> bool {
        self.mode == DT_DIR
    }
    pub fn is_file(&self) -> bool {
        self.mode == DT_REG
    }
    pub fn is_symlink(&self) -> bool {
        self.mode == DT_LNK
    }
}

impl fmt::Debug for ReadDir {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ReadDir").field("root", &self.inner.root).finish()
    }
}

impl Iterator for ReadDir {
    type Item = io::Result<DirEntry>;

    fn next(&mut self) -> Option<io::Result<DirEntry>> {
        loop {
            if self.pos >= self.filled {
                self.pos = 0;
                self.filled = 0;
                match getdents(self.fd, &mut self.buf) {
                    Ok(0) => return None,
                    Ok(n) => self.filled = n,
                    Err(e) => return Some(Err(e)),
                }
            }

            let entry = &self.buf[self.pos..];
            if entry.len() < size_of::<MinixDirent>() {
                // Truncated entry; ask for more data.
                self.pos = self.filled;
                continue;
            }
            let dirent = MinixDirent {
                d_ino: u32::from_ne_bytes(entry[0..4].try_into().unwrap()),
                d_rec_len: u16::from_ne_bytes(entry[4..6].try_into().unwrap()),
                d_name_len: entry[6],
                d_file_type: entry[7],
            };
            let rec_len = dirent.d_rec_len as usize;
            if dirent.d_rec_len == 0 || rec_len < size_of::<MinixDirent>() {
                self.pos = self.filled;
                continue;
            }
            let name_len = dirent.d_name_len as usize;
            if rec_len < size_of::<MinixDirent>() + name_len {
                self.pos = self.filled;
                continue;
            }
            self.pos += rec_len;
            if dirent.d_ino == 0 {
                continue;
            }
            let name = &entry[size_of::<MinixDirent>()..size_of::<MinixDirent>() + name_len];
            // SAFETY: directory entry names are byte strings without interior NULs.
            let name = unsafe { OsString::from_encoded_bytes_unchecked(name.to_vec()) };
            return Some(Ok(DirEntry {
                dir: crate::sync::Arc::clone(&self.inner),
                type_: dirent.d_file_type,
                name,
            }));
        }
    }
}

impl DirEntry {
    pub fn path(&self) -> PathBuf {
        self.dir.root.join(&self.name)
    }

    pub fn file_name(&self) -> OsString {
        self.name.clone()
    }

    pub fn metadata(&self) -> io::Result<FileAttr> {
        stat(&self.path())
    }

    pub fn file_type(&self) -> io::Result<FileType> {
        Ok(FileType { mode: self.type_ })
    }
}

impl OpenOptions {
    pub fn new() -> OpenOptions {
        OpenOptions {
            // generic
            read: false,
            write: false,
            append: false,
            truncate: false,
            create: false,
            create_new: false,
            // system-specific
            mode: 0o666,
        }
    }

    pub fn read(&mut self, read: bool) -> &mut OpenOptions {
        self.read = read;
        self
    }
    pub fn write(&mut self, write: bool) -> &mut OpenOptions {
        self.write = write;
        self
    }
    pub fn append(&mut self, append: bool) -> &mut OpenOptions {
        self.append = append;
        self
    }
    pub fn truncate(&mut self, truncate: bool) -> &mut OpenOptions {
        self.truncate = truncate;
        self
    }
    pub fn create(&mut self, create: bool) -> &mut OpenOptions {
        self.create = create;
        self
    }
    pub fn create_new(&mut self, create_new: bool) -> &mut OpenOptions {
        self.create_new = create_new;
        self
    }

    fn get_access_mode(&self) -> io::Result<i32> {
        match (self.read, self.write, self.append) {
            (true, false, false) => Ok(syscall::O_RDONLY),
            (false, true, false) => Ok(syscall::O_WRONLY),
            (true, true, false) => Ok(syscall::O_RDWR),
            (false, _, true) => Ok(syscall::O_WRONLY | syscall::O_APPEND),
            (true, _, true) => Ok(syscall::O_RDWR | syscall::O_APPEND),
            (false, false, false) => Err(io::const_error!(
                io::ErrorKind::InvalidInput,
                "cannot access a file with no read or write access"
            )),
        }
    }

    fn get_creation_mode(&self) -> io::Result<i32> {
        match (self.write, self.append) {
            (true, false) => {}
            _ => return Ok(0),
        }
        Ok(match (self.create, self.truncate, self.create_new) {
            (false, false, false) => 0,
            (true, false, false) => syscall::O_CREAT,
            (false, true, false) => syscall::O_TRUNC,
            (true, true, false) => syscall::O_CREAT | syscall::O_TRUNC,
            (_, _, true) => syscall::O_CREAT | syscall::O_EXCL,
        })
    }
}

fn errno_of(ret: i32) -> io::Error {
    io::Error::from_raw_os_error(-ret)
}

fn cvt(ret: i64) -> io::Result<i64> {
    if ret < 0 { Err(errno_of(ret as i32)) } else { Ok(ret) }
}

// Raw VFS helpers (message layouts match `servers/vfs/call.rs`).

unsafe fn vfs_call(msg: &mut [u8; 64]) -> io::Result<i32> {
    // SAFETY: `msg` is a valid 64-byte buffer owned by the caller.
    unsafe { syscall::vfs_call(msg).map_err(errno_of) }
}

pub(crate) fn open(path: &Path, flags: i32, mode: u32) -> io::Result<i32> {
    let bytes = path.as_os_str().as_encoded_bytes();
    let mut msg = [0u8; 64];
    if flags & syscall::O_CREAT != 0 {
        // VFS_CREAT: path@8, len@16, flags@24, mode@28.
        syscall::msg_set_i32(&mut msg, 4, syscall::VFS_CREAT);
        syscall::msg_set_u64(&mut msg, 8, bytes.as_ptr().addr() as u64);
        syscall::msg_set_i32(&mut msg, 16, bytes.len() as i32);
        syscall::msg_set_i32(&mut msg, 24, flags);
        syscall::msg_set_i32(&mut msg, 28, mode as i32);
    } else {
        // VFS_OPEN: flags@8, path@16, len@24.
        syscall::msg_set_i32(&mut msg, 4, syscall::VFS_OPEN);
        syscall::msg_set_i32(&mut msg, 8, flags);
        syscall::msg_set_u64(&mut msg, 16, bytes.as_ptr().addr() as u64);
        syscall::msg_set_i32(&mut msg, 24, bytes.len() as i32);
    }
    // SAFETY: `msg` is a valid message buffer.
    let fd = unsafe { vfs_call(&mut msg) }?;
    Ok(fd)
}

pub(crate) fn close(fd: i32) -> io::Result<()> {
    let mut msg = [0u8; 64];
    syscall::msg_set_i32(&mut msg, 4, syscall::VFS_CLOSE);
    syscall::msg_set_i32(&mut msg, 8, fd);
    // SAFETY: `msg` is a valid message buffer.
    unsafe { vfs_call(&mut msg) }?;
    Ok(())
}

pub(crate) fn read(fd: i32, buf: &mut [u8]) -> io::Result<usize> {
    let mut msg = [0u8; 64];
    syscall::msg_set_i32(&mut msg, 4, syscall::VFS_READ);
    syscall::msg_set_i32(&mut msg, 8, fd);
    syscall::msg_set_u64(&mut msg, 16, buf.as_mut_ptr().addr() as u64);
    syscall::msg_set_u64(&mut msg, 24, buf.len() as u64);
    // SAFETY: `msg` is a valid message buffer.
    let n = unsafe { vfs_call(&mut msg) }?;
    Ok(n as usize)
}

pub(crate) fn write(fd: i32, buf: &[u8]) -> io::Result<usize> {
    let mut msg = [0u8; 64];
    syscall::msg_set_i32(&mut msg, 4, syscall::VFS_WRITE);
    syscall::msg_set_i32(&mut msg, 8, fd);
    syscall::msg_set_u64(&mut msg, 16, buf.as_ptr().addr() as u64);
    syscall::msg_set_u64(&mut msg, 24, buf.len() as u64);
    // SAFETY: `msg` is a valid message buffer.
    let n = unsafe { vfs_call(&mut msg) }?;
    Ok(n as usize)
}

fn lseek(fd: i32, offset: i64, whence: i32) -> io::Result<u64> {
    let mut msg = [0u8; 64];
    syscall::msg_set_i32(&mut msg, 4, syscall::VFS_LSEEK);
    syscall::msg_set_i32(&mut msg, 8, fd);
    syscall::msg_set_i64(&mut msg, 12, offset);
    syscall::msg_set_i32(&mut msg, 20, whence);
    // SAFETY: `msg` is a valid message buffer.
    let pos = unsafe { vfs_call(&mut msg) }?;
    Ok(pos as u64)
}

fn getdents(fd: i32, buf: &mut [u8]) -> io::Result<usize> {
    let mut msg = [0u8; 64];
    syscall::msg_set_i32(&mut msg, 4, syscall::VFS_GETDENTS);
    syscall::msg_set_i32(&mut msg, 8, fd);
    syscall::msg_set_u64(&mut msg, 16, buf.as_mut_ptr().addr() as u64);
    syscall::msg_set_u64(&mut msg, 24, buf.len() as u64);
    // SAFETY: `msg` is a valid message buffer.
    let n = unsafe { vfs_call(&mut msg) }?;
    Ok(n as usize)
}

fn fstat(fd: i32) -> io::Result<syscall::Stat> {
    let mut stat_buf = core::mem::MaybeUninit::<syscall::Stat>::zeroed();
    let mut msg = [0u8; 64];
    syscall::msg_set_i32(&mut msg, 4, syscall::VFS_FSTAT);
    syscall::msg_set_i32(&mut msg, 8, fd);
    syscall::msg_set_u64(&mut msg, 12, stat_buf.as_mut_ptr().addr() as u64);
    // SAFETY: `msg` is a valid message buffer.
    unsafe { vfs_call(&mut msg) }?;
    // SAFETY: zero-initialized, so `assume_init` is safe even if the server
    // did not fill the buffer.
    Ok(unsafe { stat_buf.assume_init() })
}

/// Perform the `ioctl(fd, request, arg)` system call. `request` is a
/// NetBSD-style `_IOW`/`_IOR` code (see `crates/net` in the minixrs
/// repository); VFS copies `ioc_size(request)` bytes between `arg` and the
/// target device driver.
///
/// Returns the ioctl result (0 for the NWIO* socket ioctls) or an error.
///
/// # Safety
///
/// `arg` must point to a buffer of at least `ioc_size(request)` bytes.
pub(crate) unsafe fn ioctl(fd: i32, request: u32, arg: *mut u8) -> io::Result<i32> {
    let mut msg = [0u8; 64];
    syscall::msg_set_i32(&mut msg, 4, syscall::VFS_IOCTL);
    syscall::msg_set_i32(&mut msg, 8, fd);
    syscall::msg_set_u32(&mut msg, 12, request);
    syscall::msg_set_u64(&mut msg, 16, arg.addr() as u64);
    // SAFETY: `msg` is a valid message buffer.
    unsafe { vfs_call(&mut msg) }
}

impl File {
    pub fn open(path: &Path, opts: &OpenOptions) -> io::Result<File> {
        let flags = opts.get_access_mode()? | opts.get_creation_mode()?;
        let mode = opts.mode;
        let fd = open(path, flags, mode)?;
        // SAFETY: `fd` is a freshly opened, owned descriptor.
        Ok(File { inner: unsafe { FileDesc::from_raw_fd(fd) } })
    }

    pub fn file_attr(&self) -> io::Result<FileAttr> {
        let mut stat = fstat(self.fd())?;
        // The VFS server does not copy the stat payload yet; synthesize the
        // size via `lseek` so `Metadata::len()` works.
        if stat.st_mode == 0 && stat.st_size == 0 {
            let cur = lseek(self.fd(), 0, syscall::SEEK_CUR).ok();
            if let Ok(size) = lseek(self.fd(), 0, syscall::SEEK_END) {
                stat.st_size = size as i64;
                stat.st_mode = S_IFREG;
                if let Some(cur) = cur {
                    let _ = lseek(self.fd(), cur as i64, syscall::SEEK_SET);
                }
            }
        }
        Ok(FileAttr::from_stat(stat))
    }

    pub fn fsync(&self) -> io::Result<()> {
        let mut msg = [0u8; 64];
        syscall::msg_set_i32(&mut msg, 4, syscall::VFS_FSYNC);
        syscall::msg_set_i32(&mut msg, 8, self.fd());
        // SAFETY: `msg` is a valid message buffer.
        unsafe { vfs_call(&mut msg) }?;
        Ok(())
    }

    pub fn datasync(&self) -> io::Result<()> {
        self.fsync()
    }

    pub fn lock(&self) -> io::Result<()> {
        unsupported()
    }

    pub fn lock_shared(&self) -> io::Result<()> {
        unsupported()
    }

    pub fn try_lock(&self) -> Result<(), TryLockError> {
        Err(TryLockError::Error(crate::sys::unsupported_err()))
    }

    pub fn try_lock_shared(&self) -> Result<(), TryLockError> {
        Err(TryLockError::Error(crate::sys::unsupported_err()))
    }

    pub fn unlock(&self) -> io::Result<()> {
        unsupported()
    }

    pub fn truncate(&self, size: u64) -> io::Result<()> {
        let mut msg = [0u8; 64];
        syscall::msg_set_i32(&mut msg, 4, syscall::VFS_TRUNCATE);
        syscall::msg_set_i32(&mut msg, 8, self.fd());
        syscall::msg_set_i64(&mut msg, 12, size as i64);
        // SAFETY: `msg` is a valid message buffer.
        unsafe { vfs_call(&mut msg) }?;
        Ok(())
    }

    pub fn read(&self, buf: &mut [u8]) -> io::Result<usize> {
        read(self.fd(), buf)
    }

    pub fn read_vectored(&self, bufs: &mut [IoSliceMut<'_>]) -> io::Result<usize> {
        crate::io::default_read_vectored(|buf| self.read(buf), bufs)
    }

    pub fn is_read_vectored(&self) -> bool {
        false
    }

    pub fn read_buf(&self, cursor: BorrowedCursor<'_, u8>) -> io::Result<()> {
        crate::io::default_read_buf(|buf| self.read(buf), cursor)
    }

    pub fn write(&self, buf: &[u8]) -> io::Result<usize> {
        write(self.fd(), buf)
    }

    pub fn write_vectored(&self, bufs: &[IoSlice<'_>]) -> io::Result<usize> {
        crate::io::default_write_vectored(|buf| self.write(buf), bufs)
    }

    pub fn is_write_vectored(&self) -> bool {
        false
    }

    pub fn flush(&self) -> io::Result<()> {
        Ok(())
    }

    pub fn seek(&self, pos: SeekFrom) -> io::Result<u64> {
        let (whence, offset) = match pos {
            SeekFrom::Start(off) => (syscall::SEEK_SET, off as i64),
            SeekFrom::Current(off) => (syscall::SEEK_CUR, off),
            SeekFrom::End(off) => (syscall::SEEK_END, off),
        };
        lseek(self.fd(), offset, whence)
    }

    pub fn size(&self) -> Option<io::Result<u64>> {
        Some(self.seek(SeekFrom::End(0)))
    }

    pub fn tell(&self) -> io::Result<u64> {
        self.seek(SeekFrom::Current(0))
    }

    pub fn duplicate(&self) -> io::Result<File> {
        unsupported()
    }

    pub fn set_permissions(&self, _perm: FilePermissions) -> io::Result<()> {
        unsupported()
    }

    pub fn set_times(&self, _times: FileTimes) -> io::Result<()> {
        unsupported()
    }
}

impl DirBuilder {
    pub fn new() -> DirBuilder {
        DirBuilder { mode: 0o777 }
    }

    pub fn mkdir(&self, p: &Path) -> io::Result<()> {
        let bytes = p.as_os_str().as_encoded_bytes();
        // SAFETY: `bytes` is a valid byte slice in the caller's address space.
        let r = unsafe {
            syscall::syscall2(syscall::NR_MKDIR, bytes.as_ptr().addr() as u64, self.mode as u64)
        };
        cvt(r).map(|_| ())
    }
}

impl fmt::Debug for File {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("File").field("fd", &self.fd()).finish()
    }
}

pub fn readdir(p: &Path) -> io::Result<ReadDir> {
    let fd = open(p, syscall::O_RDONLY, 0)?;
    Ok(ReadDir {
        inner: crate::sync::Arc::new(InnerReadDir { root: p.to_path_buf() }),
        fd,
        buf: [0; 8192],
        pos: 0,
        filled: 0,
    })
}

impl Drop for ReadDir {
    fn drop(&mut self) {
        let _ = close(self.fd);
    }
}

pub fn unlink(p: &Path) -> io::Result<()> {
    let bytes = p.as_os_str().as_encoded_bytes();
    // SAFETY: `bytes` is a valid byte slice in the caller's address space.
    let r = unsafe { syscall::syscall1(syscall::NR_UNLINK, bytes.as_ptr().addr() as u64) };
    cvt(r).map(|_| ())
}

pub fn rename(_old: &Path, _new: &Path) -> io::Result<()> {
    unsupported()
}

pub fn set_perm(_p: &Path, _perm: FilePermissions) -> io::Result<()> {
    unsupported()
}

pub fn set_perm_nofollow(_p: &Path, _perm: FilePermissions) -> io::Result<()> {
    unsupported()
}

pub fn set_times(_p: &Path, _times: FileTimes) -> io::Result<()> {
    unsupported()
}

pub fn set_times_nofollow(_p: &Path, _times: FileTimes) -> io::Result<()> {
    unsupported()
}

pub fn rmdir(p: &Path) -> io::Result<()> {
    let bytes = p.as_os_str().as_encoded_bytes();
    // SAFETY: `bytes` is a valid byte slice in the caller's address space.
    let r = unsafe { syscall::syscall1(syscall::NR_RMDIR, bytes.as_ptr().addr() as u64) };
    cvt(r).map(|_| ())
}

pub fn remove_dir_all(path: &Path) -> io::Result<()> {
    let mut stack = vec![path.to_path_buf()];
    while let Some(dir) = stack.pop() {
        if let Ok(rd) = readdir(&dir) {
            for entry in rd {
                let entry = entry?;
                if entry.file_type()?.is_dir() {
                    stack.push(entry.path());
                } else {
                    unlink(&entry.path())?;
                }
            }
        }
        rmdir(&dir)?;
    }
    Ok(())
}

pub fn exists(path: &Path) -> io::Result<bool> {
    match stat(path) {
        Ok(_) => Ok(true),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e),
    }
}

pub fn readlink(_p: &Path) -> io::Result<PathBuf> {
    unsupported()
}

pub fn symlink(_original: &Path, _link: &Path) -> io::Result<()> {
    unsupported()
}

pub fn link(_src: &Path, _dst: &Path) -> io::Result<()> {
    unsupported()
}

pub fn stat(p: &Path) -> io::Result<FileAttr> {
    let mut opts = OpenOptions::new();
    opts.read(true);
    let file = File::open(p, &opts)?;
    file.file_attr()
}

pub fn lstat(p: &Path) -> io::Result<FileAttr> {
    stat(p)
}

pub fn canonicalize(_p: &Path) -> io::Result<PathBuf> {
    unsupported()
}

pub fn copy(from: &Path, to: &Path) -> io::Result<u64> {
    let mut opts = OpenOptions::new();
    opts.read(true);
    let reader = File::open(from, &opts)?;

    let mut wopts = OpenOptions::new();
    wopts.write(true).create(true).truncate(true);
    let writer = File::open(to, &wopts)?;
    let mut written = 0u64;
    let mut buf = [0u8; 8192];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        writer.write(&buf[..n])?;
        written += n as u64;
    }
    Ok(written)
}
