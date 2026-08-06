//! Raw Minix syscall wrappers, IPC message helpers, and the constants used by
//! the std platform abstraction layer.
//!
//! The syscall numbers and calling conventions mirror `crates/minix-rt` in the
//! minixrs repository. Message layouts match what the PM/VFS servers actually
//! read: `m_type` at byte offset 4, call-specific payload after that.

// Syscall numbers (kernel-side dispatch, see `crates/kernel/src/system/`).
pub const NR_EXIT: u64 = 0;
pub const NR_READ: u64 = 2;
pub const NR_WRITE: u64 = 3;
pub const NR_CLOSE: u64 = 5;
pub const NR_GETPID: u64 = 20;
pub const NR_MKDIR: u64 = 40;
pub const NR_UNLINK: u64 = 41;
pub const NR_RMDIR: u64 = 42;
pub const NR_SETFDVFS: u64 = 53;

// IPC syscall number used for message passing to the servers.
pub const SENDREC_CALL: u64 = 48;

// Well-known server endpoints.
pub const PM_PROC_NR: i32 = 0;
pub const VFS_PROC_NR: i32 = 1;
pub const VM_PROC_NR: i32 = 8;

// PM call numbers (from `minix/include/minix/callnr.h`).
pub const PM_BASE: i32 = 0x000;
pub const PM_FORK: i32 = PM_BASE + 2; // 0x002
pub const PM_WAITPID: i32 = PM_BASE + 3; // 0x003
pub const PM_KILL: i32 = PM_BASE + 11; // 0x00B
pub const PM_EXEC: i32 = PM_BASE + 14; // 0x00E
pub const PM_CLOCK_GETTIME: i32 = PM_BASE + 34; // 0x022

// VFS call numbers (from `minix/include/minix/callnr.h`).
pub const VFS_BASE: i32 = 0x100;
pub const VFS_READ: i32 = VFS_BASE + 0;
pub const VFS_WRITE: i32 = VFS_BASE + 1;
pub const VFS_LSEEK: i32 = VFS_BASE + 2;
pub const VFS_OPEN: i32 = VFS_BASE + 3;
pub const VFS_CREAT: i32 = VFS_BASE + 4;
pub const VFS_CLOSE: i32 = VFS_BASE + 5;
pub const VFS_CHDIR: i32 = 0x108;
pub const VFS_FCNTL: i32 = VFS_BASE + 25;
pub const VFS_IOCTL: i32 = VFS_BASE + 24; // 0x118
pub const VFS_PIPE2: i32 = VFS_BASE + 26; // 0x11A
pub const VFS_GETDENTS: i32 = VFS_BASE + 29;
pub const VFS_FSTAT: i32 = VFS_BASE + 22;
pub const VFS_FSYNC: i32 = VFS_BASE + 32;
pub const VFS_TRUNCATE: i32 = VFS_BASE + 33;
pub const VFS_DUP2: i32 = VFS_BASE + 49; // 0x131

// VM call numbers.
pub const VM_BRK: i32 = 0xC02;

// Open flags (from `minix/include/fcntl.h`).
pub const O_RDONLY: i32 = 0o00;
pub const O_WRONLY: i32 = 0o01;
pub const O_RDWR: i32 = 0o02;
pub const O_CREAT: i32 = 0o100;
pub const O_EXCL: i32 = 0o200;
pub const O_TRUNC: i32 = 0o1000;
pub const O_APPEND: i32 = 0o2000;

// Seek whence values (from `minix/include/unistd.h`).
pub const SEEK_SET: i32 = 0;
pub const SEEK_CUR: i32 = 1;
pub const SEEK_END: i32 = 2;

// fcntl commands (from `minix/include/fcntl.h`).
pub const F_DUPFD: i32 = 0;

// waitpid options (from `minix/include/sys/wait.h`).
pub const WNOHANG: i32 = 1;

// Signals (from `minix/include/signal.h`).
pub const SIGKILL: i32 = 9;

// Clock ids (from `minix/include/time.h`).
pub const CLOCK_REALTIME: i32 = 0;
pub const CLOCK_MONOTONIC: i32 = 1;

// Standard file descriptors.
pub const STDIN_FILENO: i32 = 0;
pub const STDOUT_FILENO: i32 = 1;
pub const STDERR_FILENO: i32 = 2;

// User stack top — must match `crates/minix-rt` and the kernel's
// `user_stack_base() + user_stack_size()` (the exec frame's pointer
// arithmetic depends on this value).
#[cfg(target_arch = "x86_64")]
pub(crate) const USER_STACK_TOP: u64 = 0x0FE1_0000;
#[cfg(target_arch = "riscv64")]
pub(crate) const USER_STACK_TOP: u64 = 0x8FE1_0000;
#[cfg(target_arch = "aarch64")]
pub(crate) const USER_STACK_TOP: u64 = 0x3FD0_0000;

// errno values (from `minix/include/errno.h`). The kernel returns these
// negated; `MinixErr` unwraps them.
pub const EPERM: i32 = 1;
pub const ENOENT: i32 = 2;
pub const ESRCH: i32 = 3;
pub const EINTR: i32 = 4;
pub const EIO: i32 = 5;
pub const ENXIO: i32 = 6;
pub const EBADF: i32 = 9;
pub const EAGAIN: i32 = 11;
pub const ENOMEM: i32 = 12;
pub const EACCES: i32 = 13;
pub const EFAULT: i32 = 14;
pub const EBUSY: i32 = 16;
pub const EEXIST: i32 = 17;
pub const ENODEV: i32 = 19;
pub const ENOTDIR: i32 = 20;
pub const EISDIR: i32 = 21;
pub const EINVAL: i32 = 22;
pub const ENOSPC: i32 = 28;
pub const EDOM: i32 = 33;
pub const ERANGE: i32 = 34;
pub const ENOSYS: i32 = 71;

/// POSIX-style file status, matching the layout the FS servers fill for
/// `VFS_STAT`/`VFS_FSTAT` (88 bytes).
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Stat {
    pub st_dev: u64,
    pub st_ino: u64,
    pub st_mode: u32,
    pub st_nlink: u32,
    pub st_uid: u32,
    pub st_gid: u32,
    pub st_rdev: u64,
    pub st_size: i64,
    pub st_blksize: i64,
    pub st_blocks: i64,
    pub st_atime: i64,
    pub st_mtime: i64,
    pub st_ctime: i64,
}

// x86_64 syscalls: number in rax, args in rdi/rsi/rdx/r10/r8/r9, return in rax.

#[cfg(target_arch = "x86_64")]
#[inline]
pub unsafe fn syscall0(nr: u64) -> i64 {
    let ret: i64;
    unsafe {
        core::arch::asm!(
            "syscall",
            in("rax") nr,
            lateout("rcx") _,
            lateout("r11") _,
            lateout("rax") ret,
            options(nostack),
        );
    }
    ret
}

#[cfg(target_arch = "x86_64")]
#[inline]
pub unsafe fn syscall1(nr: u64, a1: u64) -> i64 {
    let ret: i64;
    unsafe {
        core::arch::asm!(
            "syscall",
            in("rax") nr,
            in("rdi") a1,
            lateout("rcx") _,
            lateout("r11") _,
            lateout("rax") ret,
            options(nostack),
        );
    }
    ret
}

#[cfg(target_arch = "x86_64")]
#[inline]
pub unsafe fn syscall2(nr: u64, a1: u64, a2: u64) -> i64 {
    let ret: i64;
    unsafe {
        core::arch::asm!(
            "syscall",
            in("rax") nr,
            in("rdi") a1,
            in("rsi") a2,
            lateout("rcx") _,
            lateout("r11") _,
            lateout("rax") ret,
            options(nostack),
        );
    }
    ret
}

#[cfg(target_arch = "x86_64")]
#[inline]
pub unsafe fn syscall3(nr: u64, a1: u64, a2: u64, a3: u64) -> i64 {
    let ret: i64;
    unsafe {
        core::arch::asm!(
            "syscall",
            in("rax") nr,
            in("rdi") a1,
            in("rsi") a2,
            in("rdx") a3,
            lateout("rcx") _,
            lateout("r11") _,
            lateout("rax") ret,
            options(nostack),
        );
    }
    ret
}

// RISC-V syscalls: number in a7, args in a0-a5, return in a0.

#[cfg(target_arch = "riscv64")]
#[inline]
pub unsafe fn riscv_syscall(nr: u64, a0: u64, a1: u64, a2: u64, a3: u64, a4: u64, a5: u64) -> i64 {
    let ret: i64;
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") nr,
            in("a0") a0,
            in("a1") a1,
            in("a2") a2,
            in("a3") a3,
            in("a4") a4,
            in("a5") a5,
            lateout("a0") ret,
            options(nostack),
        );
    }
    ret
}

#[cfg(target_arch = "riscv64")]
#[inline]
pub unsafe fn syscall0(nr: u64) -> i64 {
    unsafe { riscv_syscall(nr, 0, 0, 0, 0, 0, 0) }
}
#[cfg(target_arch = "riscv64")]
#[inline]
pub unsafe fn syscall1(nr: u64, a1: u64) -> i64 {
    unsafe { riscv_syscall(nr, a1, 0, 0, 0, 0, 0) }
}
#[cfg(target_arch = "riscv64")]
#[inline]
pub unsafe fn syscall2(nr: u64, a1: u64, a2: u64) -> i64 {
    unsafe { riscv_syscall(nr, a1, a2, 0, 0, 0, 0) }
}
#[cfg(target_arch = "riscv64")]
#[inline]
pub unsafe fn syscall3(nr: u64, a1: u64, a2: u64, a3: u64) -> i64 {
    unsafe { riscv_syscall(nr, a1, a2, a3, 0, 0, 0) }
}

// AArch64 syscalls: `svc #0`, number in x8, args in x0-x5, return in x0.

#[cfg(target_arch = "aarch64")]
#[inline]
pub unsafe fn aarch64_syscall(
    nr: u64,
    a0: u64,
    a1: u64,
    a2: u64,
    a3: u64,
    a4: u64,
    a5: u64,
) -> i64 {
    let ret: i64;
    unsafe {
        core::arch::asm!(
            "svc #0",
            inlateout("x0") a0 => ret,
            in("x1") a1,
            in("x2") a2,
            in("x3") a3,
            in("x4") a4,
            in("x5") a5,
            in("x8") nr,
            options(nostack),
        );
    }
    ret
}

#[cfg(target_arch = "aarch64")]
#[inline]
pub unsafe fn syscall0(nr: u64) -> i64 {
    unsafe { aarch64_syscall(nr, 0, 0, 0, 0, 0, 0) }
}
#[cfg(target_arch = "aarch64")]
#[inline]
pub unsafe fn syscall1(nr: u64, a1: u64) -> i64 {
    unsafe { aarch64_syscall(nr, a1, 0, 0, 0, 0, 0) }
}
#[cfg(target_arch = "aarch64")]
#[inline]
pub unsafe fn syscall2(nr: u64, a1: u64, a2: u64) -> i64 {
    unsafe { aarch64_syscall(nr, a1, a2, 0, 0, 0, 0) }
}
#[cfg(target_arch = "aarch64")]
#[inline]
pub unsafe fn syscall3(nr: u64, a1: u64, a2: u64, a3: u64) -> i64 {
    unsafe { aarch64_syscall(nr, a1, a2, a3, 0, 0, 0) }
}

/// `sendrec(dest, msg)`: send `msg` to `dest` and block until a reply arrives.
///
/// The caller must place the call number in `msg[4..8]`. On success the reply
/// `m_type` is written back there and the source endpoint is returned.
///
/// # Safety
///
/// `msg` must point to 64 valid bytes.
#[inline]
pub unsafe fn sendrec(dest: i32, msg: &mut [u8; 64]) -> i32 {
    unsafe { syscall2(SENDREC_CALL, dest as u64, msg.as_mut_ptr().addr() as u64) as i32 }
}

/// Terminate the current process with `status`.
pub fn exit(status: i32) -> ! {
    unsafe {
        syscall1(NR_EXIT, status as u64);
    }
    // The syscall should never return; spin in case it does.
    loop {
        core::hint::spin_loop();
    }
}

/// Read up to `buf.len()` bytes from `fd`. Returns the byte count or a
/// negative errno.
pub fn read(fd: i32, buf: &mut [u8]) -> i64 {
    unsafe { syscall3(NR_READ, fd as u64, buf.as_mut_ptr().addr() as u64, buf.len() as u64) }
}

/// Write `buf` to `fd`. Returns the byte count or a negative errno.
pub fn write(fd: i32, buf: &[u8]) -> i64 {
    unsafe { syscall3(NR_WRITE, fd as u64, buf.as_ptr().addr() as u64, buf.len() as u64) }
}

/// Close `fd`. Returns 0 or a negative errno.
pub fn close(fd: i32) -> i64 {
    unsafe { syscall1(NR_CLOSE, fd as u64) }
}

/// Perform the `fcntl(fd, cmd, arg)` system call (e.g. `F_DUPFD`).
///
/// Returns the result status: a non-negative value on success (for `F_DUPFD`,
/// the new file descriptor), or a negative errno.
pub fn fcntl(fd: i32, cmd: i32, arg: i32) -> i64 {
    let mut msg = [0u8; 64];
    msg_set_i32(&mut msg, 4, VFS_FCNTL);
    msg_set_i32(&mut msg, 8, fd);
    msg_set_i32(&mut msg, 12, cmd);
    msg_set_i32(&mut msg, 16, arg);
    match unsafe { vfs_call(&mut msg) } {
        Ok(v) => v as i64,
        Err(e) => e as i64,
    }
}

/// Get the current process id.
pub fn getpid() -> i32 {
    unsafe { syscall0(NR_GETPID) as i32 }
}

/// Query or set the program break. `addr == 0` queries the current break.
///
/// Returns the new break on success or a negative errno.
pub fn brk(addr: u32) -> i64 {
    let mut msg = [0u8; 64];
    // m_type at offset 4, new break address in m1i1 at offset 8.
    msg[4..8].copy_from_slice(&VM_BRK.to_le_bytes());
    msg[8..12].copy_from_slice(&addr.to_le_bytes());
    let r = unsafe { sendrec(VM_PROC_NR, &mut msg) };
    if r < 0 {
        return r as i64;
    }
    // The VM server writes the new break into m1i1 (offset 8) on success.
    i32::from_le_bytes(msg[8..12].try_into().unwrap_or([0; 4])) as i64
}

/// Send a VFS request and return the reply status (`m_type` at offset 4).
///
/// # Safety
///
/// `msg` must contain a valid VFS call; the reply `m_type` is written back
/// into `msg[4..8]`.
#[inline]
pub unsafe fn vfs_call(msg: &mut [u8; 64]) -> Result<i32, i32> {
    let r = unsafe { sendrec(VFS_PROC_NR, msg) };
    if r < 0 {
        return Err(r);
    }
    let mtype = i32::from_le_bytes(msg[4..8].try_into().unwrap_or([0; 4]));
    if mtype < 0 { Err(mtype) } else { Ok(mtype) }
}

/// Send a PM request and return the reply status (`m_type` at offset 4).
///
/// # Safety
///
/// `msg` must contain a valid PM call; the reply `m_type` is written back
/// into `msg[4..8]`.
#[inline]
pub unsafe fn pm_call(msg: &mut [u8; 64]) -> Result<i32, i32> {
    let r = unsafe { sendrec(PM_PROC_NR, msg) };
    if r < 0 {
        return Err(r);
    }
    let mtype = i32::from_le_bytes(msg[4..8].try_into().unwrap_or([0; 4]));
    if mtype < 0 { Err(mtype) } else { Ok(mtype) }
}

// ---- Process lifecycle (PM protocol, mirroring `crates/minix-rt`) ----

/// Fork the current process. Returns the child PID in the parent and 0 in
/// the child, or a negative errno.
pub fn fork() -> i32 {
    let mut msg = [0u8; 64];
    msg_set_i32(&mut msg, 4, PM_FORK);
    // SAFETY: `msg` is a valid message buffer.
    let reply =
        unsafe { syscall2(SENDREC_CALL, PM_PROC_NR as u64, msg.as_mut_ptr().addr() as u64) };
    if reply < 0 {
        return reply as i32;
    }
    let mtype = msg_i32(&msg, 4);
    if mtype < 0 {
        return mtype;
    }
    // PM replies to the parent with the child PID at offset 8 and leaves
    // it zero in the child's copy, so zero means we are the child.
    let child_pid = msg_i32(&msg, 8);
    if child_pid == 0 { 0 } else { child_pid }
}

/// Wait for a child process. Returns `(pid, status)`; a negative `pid` is a
/// negative errno (`EAGAIN` for `WNOHANG` with no exited child).
pub fn waitpid(pid: i32, options: i32) -> (i32, i32) {
    let mut msg = [0u8; 64];
    msg_set_i32(&mut msg, 4, PM_WAITPID);
    msg_set_i32(&mut msg, 8, pid);
    msg_set_i32(&mut msg, 12, options);
    // SAFETY: `msg` is a valid message buffer.
    let reply =
        unsafe { syscall2(SENDREC_CALL, PM_PROC_NR as u64, msg.as_mut_ptr().addr() as u64) };
    if reply < 0 {
        return (reply as i32, 0);
    }
    (msg_i32(&msg, 8), msg_i32(&msg, 12))
}

/// Send a signal to a process (PM_KILL: `m_type@4, sig@8, pid@12`).
/// Returns 0 on success or a negative errno.
pub fn kill(pid: i32, sig: i32) -> i32 {
    let mut msg = [0u8; 64];
    msg_set_i32(&mut msg, 4, PM_KILL);
    msg_set_i32(&mut msg, 8, sig);
    msg_set_i32(&mut msg, 12, pid);
    // SAFETY: `msg` is a valid message buffer.
    match unsafe { pm_call(&mut msg) } {
        Ok(_) => 0,
        Err(e) => e,
    }
}

/// Execute a new program via the PM->VFS chain (`PM_EXEC`). `path` must
/// point to a NUL-terminated path of `path_len` bytes; `frame` is the exec
/// stack frame (argc, argv/envp pointers, strings) of `frame_len` bytes.
/// On success the process image is replaced and this never returns.
///
/// # Safety
///
/// `path` and `frame` must point to valid memory for the given lengths.
pub unsafe fn execve(path: *const u8, path_len: usize, frame: *const u8, frame_len: usize) -> i32 {
    let mut msg = [0u8; 64];
    msg_set_i32(&mut msg, 4, PM_EXEC);
    msg_set_u64(&mut msg, 8, path.addr() as u64);
    msg_set_u64(&mut msg, 16, path_len as u64);
    msg_set_u64(&mut msg, 24, frame.addr() as u64);
    msg_set_u64(&mut msg, 32, frame_len as u64);
    msg_set_u64(&mut msg, 40, 0); // ps_str
    // SAFETY: `msg` is a valid message buffer; the frame stays alive for
    // the duration of the call (VFS vircopies it before the reply).
    unsafe { syscall2(SENDREC_CALL, PM_PROC_NR as u64, msg.as_mut_ptr().addr() as u64) as i32 }
}

// ---- VFS process helpers ----

/// Duplicate `fd` onto `newfd` (POSIX `dup2`). Returns `newfd` or a
/// negative errno.
pub fn dup2(fd: i32, newfd: i32) -> i32 {
    let mut msg = [0u8; 64];
    msg_set_i32(&mut msg, 4, VFS_DUP2);
    msg_set_i32(&mut msg, 8, fd);
    msg_set_i32(&mut msg, 12, newfd);
    // SAFETY: `msg` is a valid message buffer.
    match unsafe { vfs_call(&mut msg) } {
        Ok(v) => v,
        Err(e) => e,
    }
}

/// Create a pipe. Returns `(read_fd, write_fd)` or a pair of the same
/// negative errno.
pub fn pipe() -> (i32, i32) {
    let mut msg = [0u8; 64];
    msg_set_i32(&mut msg, 4, VFS_PIPE2);
    msg_set_i32(&mut msg, 8, 0); // flags: no O_CLOEXEC on Minix yet
    // SAFETY: `msg` is a valid message buffer.
    match unsafe { vfs_call(&mut msg) } {
        Ok(_) => (msg_i32(&msg, 8), msg_i32(&msg, 12)),
        Err(e) => (e, e),
    }
}

/// Mark fd 0..2 as VFS-owned (1) or serial (0) so the kernel routes
/// reads/writes through VFS instead of the serial shortcut.
pub fn set_fd_vfs(fd: i32, on: i32) -> i64 {
    // SAFETY: this is a plain syscall.
    unsafe { syscall2(NR_SETFDVFS, fd as u64, on as u64) }
}

#[inline]
pub fn msg_set_i32(msg: &mut [u8; 64], off: usize, val: i32) {
    msg[off..off + 4].copy_from_slice(&val.to_ne_bytes());
}

#[inline]
pub fn msg_i32(msg: &[u8; 64], off: usize) -> i32 {
    i32::from_ne_bytes(msg[off..off + 4].try_into().unwrap_or([0; 4]))
}

#[inline]
pub fn msg_set_u32(msg: &mut [u8; 64], off: usize, val: u32) {
    msg[off..off + 4].copy_from_slice(&val.to_ne_bytes());
}

#[inline]
pub fn msg_set_u64(msg: &mut [u8; 64], off: usize, val: u64) {
    msg[off..off + 8].copy_from_slice(&val.to_ne_bytes());
}

#[inline]
pub fn msg_i64(msg: &[u8; 64], off: usize) -> i64 {
    i64::from_ne_bytes(msg[off..off + 8].try_into().unwrap_or([0; 8]))
}

#[inline]
pub fn msg_set_i64(msg: &mut [u8; 64], off: usize, val: i64) {
    msg[off..off + 8].copy_from_slice(&val.to_ne_bytes());
}
