//! Process lifecycle: fork/exec/wait over the PM protocol.
//!
//! `Command::spawn` forks and execs like the reference shell does: PM_FORK,
//! child-side `dup2`/`chdir`, then a PM_EXEC frame built with the program,
//! argv and the captured environment. `wait`/`try_wait` go through
//! PM_WAITPID; `kill` through PM_KILL (SIGKILL).

pub use crate::ffi::OsString as EnvKey;
use crate::ffi::{OsStr, OsString};
use crate::num::NonZero;
use crate::os::fd::AsRawFd;
use crate::path::Path;
use crate::process::StdioPipes;
use crate::sys::fd::FileDesc;
use crate::sys::fs::File;
use crate::sys::pal::minix::fs::open;
use crate::sys::pipe::Pipe;
use crate::sys::process::env::{CommandEnv, CommandEnvs, CommandResolvedEnvs};
use crate::sys::syscall;
use crate::{fmt, io};

////////////////////////////////////////////////////////////////////////////////
// Command
////////////////////////////////////////////////////////////////////////////////

pub struct Command {
    program: OsString,
    args: Vec<OsString>,
    env: CommandEnv,

    cwd: Option<OsString>,
    stdin: Option<Stdio>,
    stdout: Option<Stdio>,
    stderr: Option<Stdio>,
}

#[derive(Debug)]
pub enum Stdio {
    Inherit,
    Null,
    MakePipe,
    ParentStdout,
    ParentStderr,
    #[allow(dead_code)] // This variant exists only for the Debug impl
    InheritFile(File),
    Fd(FileDesc),
}

impl Command {
    pub fn new(program: &OsStr) -> Command {
        Command {
            program: program.to_owned(),
            args: vec![program.to_owned()],
            env: Default::default(),
            cwd: None,
            stdin: None,
            stdout: None,
            stderr: None,
        }
    }

    pub fn arg(&mut self, arg: &OsStr) {
        self.args.push(arg.to_owned());
    }

    pub fn env_mut(&mut self) -> &mut CommandEnv {
        &mut self.env
    }

    pub fn cwd(&mut self, dir: &OsStr) {
        self.cwd = Some(dir.to_owned());
    }

    pub fn stdin(&mut self, stdin: Stdio) {
        self.stdin = Some(stdin);
    }

    pub fn stdout(&mut self, stdout: Stdio) {
        self.stdout = Some(stdout);
    }

    pub fn stderr(&mut self, stderr: Stdio) {
        self.stderr = Some(stderr);
    }

    pub fn get_program(&self) -> &OsStr {
        &self.program
    }

    pub fn get_args(&self) -> CommandArgs<'_> {
        let mut iter = self.args.iter();
        iter.next();
        CommandArgs { iter }
    }

    pub fn get_envs(&self) -> CommandEnvs<'_> {
        self.env.iter()
    }

    pub fn get_env_clear(&self) -> bool {
        self.env.does_clear()
    }

    pub fn get_resolved_envs(&self) -> CommandResolvedEnvs {
        CommandResolvedEnvs::new(self.env.capture())
    }

    pub fn get_current_dir(&self) -> Option<&Path> {
        self.cwd.as_ref().map(|cs| Path::new(cs))
    }

    pub fn spawn(
        &mut self,
        default: Stdio,
        needs_stdin: bool,
    ) -> io::Result<(Process, StdioPipes)> {
        let stdin = match &self.stdin {
            Some(stdin) => stdin,
            None if needs_stdin => &default,
            None => &Stdio::Null,
        };
        let stdout = self.stdout.as_ref().unwrap_or(&default);
        let stderr = self.stderr.as_ref().unwrap_or(&default);

        let stdin = prepare(stdin, true)?;
        let stdout = prepare(stdout, false)?;
        let stderr = prepare(stderr, false)?;

        // Resolve the program path and build the exec frame before forking,
        // so the child only runs syscalls.
        let program = self.program.as_encoded_bytes();
        let has_slash = program.contains(&b'/');
        let mut path = Vec::new();
        if has_slash {
            path.extend_from_slice(program);
        } else {
            path.extend_from_slice(b"/bin/");
            path.extend_from_slice(program);
        }
        path.push(0);
        let mut alt_path = Vec::new();
        if !has_slash {
            alt_path.extend_from_slice(b"/sbin/");
            alt_path.extend_from_slice(program);
            alt_path.push(0);
        }

        let mut argv: Vec<&[u8]> = Vec::new();
        argv.push(&path[..path.len() - 1]);
        for arg in &self.args[1..] {
            argv.push(arg.as_encoded_bytes());
        }

        let env = self.env.capture();
        let mut envp: Vec<Vec<u8>> = Vec::new();
        for (key, value) in &env {
            let mut entry = key.as_encoded_bytes().to_vec();
            entry.push(b'=');
            entry.extend_from_slice(value.as_encoded_bytes());
            envp.push(entry);
        }
        let envp: Vec<&[u8]> = envp.iter().map(|entry| entry.as_slice()).collect();
        let frame = build_exec_frame(&argv, &envp)?;

        let pid = syscall::fork();
        if pid < 0 {
            cleanup_prepared(&stdin, &stdout, &stderr);
            return Err(errno_of(pid));
        }
        if pid == 0 {
            child_exec(&stdin, &stdout, &stderr, &path, &alt_path, &frame, self.cwd.as_deref());
        }

        // Parent: close the child's ends first (the parent keeps only its
        // own pipe ends), then hand the pipes to the caller.
        for prep in [&stdin, &stdout, &stderr] {
            if prep.parent.is_some() {
                close_fd(prep.dup.unwrap_or(-1));
            } else {
                close_fd(prep.parent_close.unwrap_or(-1));
            }
        }
        let pipes =
            StdioPipes { stdin: stdin.parent, stdout: stdout.parent, stderr: stderr.parent };
        Ok((Process { pid, status: None }, pipes))
    }
}

// ---- stdio preparation (before forking) ----

/// One stdio slot, resolved into the child-side fd wiring.
struct Prepared {
    /// fd to `dup2` onto the stdio slot in the child.
    dup: Option<i32>,
    /// pipe fds the child must close after `dup2` (both ends, or -1).
    close: [i32; 2],
    /// fd the parent must close after forking (the child's pipe end or the
    /// `/dev/null` open); `None` for caller-owned fds.
    parent_close: Option<i32>,
    /// the parent's end of a created pipe (kept in `StdioPipes`).
    parent: Option<Pipe>,
}

fn prepare(stdio: &Stdio, is_stdin: bool) -> io::Result<Prepared> {
    match stdio {
        Stdio::Inherit | Stdio::ParentStdout | Stdio::ParentStderr => {
            Ok(Prepared { dup: None, close: [-1, -1], parent_close: None, parent: None })
        }
        Stdio::Null => {
            let fd = open(Path::new("/dev/null"), syscall::O_RDWR, 0)?;
            Ok(Prepared { dup: Some(fd), close: [-1, -1], parent_close: Some(fd), parent: None })
        }
        Stdio::MakePipe => {
            let (r, w) = syscall::pipe();
            if r < 0 {
                return Err(errno_of(r));
            }
            if is_stdin {
                // Child reads from r; the parent keeps w.
                Ok(Prepared {
                    dup: Some(r),
                    close: [r, w],
                    parent_close: Some(r),
                    parent: Some(Pipe::from_raw_fd(w)),
                })
            } else {
                // Child writes to w; the parent keeps r.
                Ok(Prepared {
                    dup: Some(w),
                    close: [r, w],
                    parent_close: Some(w),
                    parent: Some(Pipe::from_raw_fd(r)),
                })
            }
        }
        Stdio::InheritFile(file) => Ok(Prepared {
            dup: Some(file.as_raw_fd()),
            close: [-1, -1],
            parent_close: None,
            parent: None,
        }),
        Stdio::Fd(fd) => Ok(Prepared {
            dup: Some(fd.as_raw_fd()),
            close: [-1, -1],
            parent_close: None,
            parent: None,
        }),
    }
}

fn close_fd(fd: i32) {
    if fd >= 0 {
        let _ = syscall::close(fd);
    }
}

/// Close every fd `prepare` created (pipe ends and `/dev/null` opens) after
/// a failed fork; caller-owned fds are left alone.
fn cleanup_prepared(stdin: &Prepared, stdout: &Prepared, stderr: &Prepared) {
    for prep in [stdin, stdout, stderr] {
        close_fd(prep.close[0]);
        close_fd(prep.close[1]);
        if prep.close[0] < 0 && prep.close[1] < 0 {
            close_fd(prep.parent_close.unwrap_or(-1));
        }
    }
}

/// Build the exec stack frame: argc, argv pointers, NULL, envp pointers,
/// NULL, then the NUL-terminated strings (the layout the kernel's
/// `parse_exec_frame` expects, mirroring `minix-rt::execve`).
fn build_exec_frame(argv: &[&[u8]], envp: &[&[u8]]) -> io::Result<Vec<u8>> {
    const EXEC_FRAME_MAX: usize = 16384;
    const MAX_STRINGS: usize = 63;

    let argc = argv.len().min(MAX_STRINGS);
    let envc = envp.len().min(MAX_STRINGS);
    let mut str_bytes = 0usize;
    for arg in argv.iter().take(argc) {
        str_bytes += arg.len() + 1;
    }
    for entry in envp.iter().take(envc) {
        str_bytes += entry.len() + 1;
    }
    let header = 8 + (argc + envc + 2) * 8;
    let frame_size = (header + str_bytes + 7) & !7;
    if frame_size > EXEC_FRAME_MAX {
        return Err(io::const_error!(io::ErrorKind::InvalidInput, "arg list too long"));
    }

    let vsp = syscall::USER_STACK_TOP - frame_size as u64;
    let mut frame = vec![0u8; frame_size];

    // Strings at the end of the frame, going down: argv then envp.
    let mut str_pos = frame_size;
    let mut offsets = [0usize; 128];
    let mut oi = 0usize;
    for arg in argv.iter().take(argc) {
        str_pos -= arg.len() + 1;
        frame[str_pos..str_pos + arg.len()].copy_from_slice(arg);
        offsets[oi] = str_pos;
        oi += 1;
    }
    for entry in envp.iter().take(envc) {
        str_pos -= entry.len() + 1;
        frame[str_pos..str_pos + entry.len()].copy_from_slice(entry);
        offsets[oi] = str_pos;
        oi += 1;
    }

    // Pointer array: argc, argv ptrs, NULL, envp ptrs, NULL.
    let mut pos = 0usize;
    let put = |frame: &mut [u8], pos: usize, value: u64| {
        frame[pos..pos + 8].copy_from_slice(&value.to_le_bytes());
    };
    put(&mut frame, pos, argc as u64);
    pos += 8;
    for i in 0..argc {
        put(&mut frame, pos, vsp + offsets[i] as u64);
        pos += 8;
    }
    put(&mut frame, pos, 0);
    pos += 8;
    for i in 0..envc {
        put(&mut frame, pos, vsp + offsets[argc + i] as u64);
        pos += 8;
    }
    put(&mut frame, pos, 0);
    Ok(frame)
}

/// Child-side exec: wire the prepared fds onto 0/1/2, apply the cwd, then
/// exec the resolved path (with an `/sbin/` fallback). Never returns.
fn child_exec(
    stdin: &Prepared,
    stdout: &Prepared,
    stderr: &Prepared,
    path: &[u8],
    alt_path: &[u8],
    frame: &[u8],
    cwd: Option<&OsStr>,
) -> ! {
    for (prep, slot) in [(stdin, 0), (stdout, 1), (stderr, 2)] {
        if let Some(fd) = prep.dup {
            if syscall::dup2(fd, slot) < 0 {
                syscall::exit(1);
            }
            // Route the stdio slot through VFS instead of the serial
            // shortcut (the fd table and this flag survive the exec).
            syscall::set_fd_vfs(slot, 1);
        }
        close_fd(prep.close[0]);
        close_fd(prep.close[1]);
    }

    if let Some(cwd) = cwd {
        if crate::sys::paths::chdir(Path::new(cwd)).is_err() {
            syscall::exit(1);
        }
    }

    // SAFETY: `path` is NUL-terminated and `frame` is a valid exec frame;
    // both stay alive for the duration of the PM_EXEC call.
    let r = unsafe { syscall::execve(path.as_ptr(), path.len(), frame.as_ptr(), frame.len()) };
    if r < 0 && !alt_path.is_empty() {
        // Try /sbin/<prog> as fallback (matching the shell).
        let _ = unsafe {
            syscall::execve(alt_path.as_ptr(), alt_path.len(), frame.as_ptr(), frame.len())
        };
    }
    syscall::exit(1);
}

pub fn output(cmd: &mut Command) -> io::Result<(ExitStatus, Vec<u8>, Vec<u8>)> {
    cmd.stdin(Stdio::Null);
    cmd.stdout(Stdio::MakePipe);
    cmd.stderr(Stdio::MakePipe);
    let (mut process, pipes) = cmd.spawn(Stdio::Null, false)?;
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    if let (Some(out), Some(err)) = (pipes.stdout, pipes.stderr) {
        read_output(out, &mut stdout, err, &mut stderr)?;
    }
    let status = process.wait()?;
    Ok((status, stdout, stderr))
}

impl From<ChildPipe> for Stdio {
    fn from(pipe: ChildPipe) -> Stdio {
        Stdio::Fd(pipe.into_inner())
    }
}

impl From<io::Stdout> for Stdio {
    fn from(_: io::Stdout) -> Stdio {
        Stdio::ParentStdout
    }
}

impl From<io::Stderr> for Stdio {
    fn from(_: io::Stderr) -> Stdio {
        Stdio::ParentStderr
    }
}

impl From<File> for Stdio {
    fn from(file: File) -> Stdio {
        Stdio::InheritFile(file)
    }
}

impl fmt::Debug for Command {
    // show all attributes
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if f.alternate() {
            let mut debug_command = f.debug_struct("Command");
            debug_command.field("program", &self.program).field("args", &self.args);
            if !self.env.is_unchanged() {
                debug_command.field("env", &self.env);
            }

            if self.cwd.is_some() {
                debug_command.field("cwd", &self.cwd);
            }

            if self.stdin.is_some() {
                debug_command.field("stdin", &self.stdin);
            }
            if self.stdout.is_some() {
                debug_command.field("stdout", &self.stdout);
            }
            if self.stderr.is_some() {
                debug_command.field("stderr", &self.stderr);
            }

            debug_command.finish()
        } else {
            if let Some(ref cwd) = self.cwd {
                write!(f, "cd {cwd:?} && ")?;
            }
            if self.env.does_clear() {
                write!(f, "env -i ")?;
                // Altered env vars will be printed next, that should exactly work as expected.
            } else {
                // Removed env vars need the command to be wrapped in `env`.
                let mut any_removed = false;
                for (key, value_opt) in self.get_envs() {
                    if value_opt.is_none() {
                        if !any_removed {
                            write!(f, "env ")?;
                            any_removed = true;
                        }
                        write!(f, "-u {} ", key.to_string_lossy())?;
                    }
                }
            }
            // Altered env vars can just be added in front of the program.
            for (key, value_opt) in self.get_envs() {
                if let Some(value) = value_opt {
                    write!(f, "{}={value:?} ", key.to_string_lossy())?;
                }
            }
            if self.program != self.args[0] {
                write!(f, "[{:?}] ", self.program)?;
            }
            write!(f, "{:?}", self.args[0])?;

            for arg in &self.args[1..] {
                write!(f, " {:?}", arg)?;
            }
            Ok(())
        }
    }
}

#[derive(PartialEq, Eq, Clone, Copy, Debug, Default)]
pub struct ExitStatus(i32);

impl ExitStatus {
    pub fn exit_ok(&self) -> Result<(), ExitStatusError> {
        if self.0 == 0 { Ok(()) } else { Err(ExitStatusError(*self)) }
    }

    pub fn code(&self) -> Option<i32> {
        Some(self.0)
    }
}

impl fmt::Display for ExitStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "exit status: {}", self.0)
    }
}

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub struct ExitStatusError(ExitStatus);

impl Into<ExitStatus> for ExitStatusError {
    fn into(self) -> ExitStatus {
        self.0
    }
}

impl ExitStatusError {
    pub fn code(self) -> Option<NonZero<i32>> {
        NonZero::new(self.0.0)
    }
}

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub struct ExitCode(u8);

impl ExitCode {
    pub const SUCCESS: ExitCode = ExitCode(0);
    pub const FAILURE: ExitCode = ExitCode(1);

    pub fn as_i32(&self) -> i32 {
        self.0 as i32
    }
}

impl From<u8> for ExitCode {
    fn from(code: u8) -> Self {
        Self(code)
    }
}

pub struct Process {
    pid: i32,
    status: Option<ExitStatus>,
}

impl Process {
    pub fn id(&self) -> u32 {
        self.pid as u32
    }

    pub fn kill(&mut self) -> io::Result<()> {
        let r = syscall::kill(self.pid, syscall::SIGKILL);
        if r < 0 { Err(errno_of(r)) } else { Ok(()) }
    }

    pub fn wait(&mut self) -> io::Result<ExitStatus> {
        if let Some(status) = self.status {
            return Ok(status);
        }
        let (pid, status) = syscall::waitpid(self.pid, 0);
        if pid < 0 {
            return Err(errno_of(pid));
        }
        let status = ExitStatus(status);
        self.status = Some(status);
        Ok(status)
    }

    pub fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        if let Some(status) = self.status {
            return Ok(Some(status));
        }
        let (pid, status) = syscall::waitpid(self.pid, syscall::WNOHANG);
        if pid == -syscall::EAGAIN {
            return Ok(None);
        }
        if pid < 0 {
            return Err(errno_of(pid));
        }
        let status = ExitStatus(status);
        self.status = Some(status);
        Ok(Some(status))
    }
}

pub struct CommandArgs<'a> {
    iter: crate::slice::Iter<'a, OsString>,
}

impl<'a> Iterator for CommandArgs<'a> {
    type Item = &'a OsStr;
    fn next(&mut self) -> Option<&'a OsStr> {
        self.iter.next().map(|os| &**os)
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.iter.size_hint()
    }
}

impl<'a> ExactSizeIterator for CommandArgs<'a> {
    fn len(&self) -> usize {
        self.iter.len()
    }
    fn is_empty(&self) -> bool {
        self.iter.is_empty()
    }
}

impl<'a> fmt::Debug for CommandArgs<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.iter.clone()).finish()
    }
}

pub type ChildPipe = crate::sys::pipe::Pipe;

pub fn read_output(
    out: ChildPipe,
    stdout: &mut Vec<u8>,
    err: ChildPipe,
    stderr: &mut Vec<u8>,
) -> io::Result<()> {
    // Sequential drain: pipes have no poll/select yet, so read stdout to EOF
    // first, then stderr. Fine for bounded outputs.
    out.read_to_end(stdout)?;
    err.read_to_end(stderr)?;
    Ok(())
}

pub fn getpid() -> u32 {
    syscall::getpid() as u32
}

fn errno_of(ret: i32) -> io::Error {
    io::Error::from_raw_os_error(-ret)
}
