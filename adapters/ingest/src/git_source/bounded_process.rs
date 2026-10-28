use engram_knowledge::{CoreError, CoreResult};
use std::{
    io::Read,
    process::{Command, Output, Stdio},
    thread,
    time::{Duration, Instant},
};

/// Fixed operation label plus the OS failure captured immediately at the failing call.
/// Diagnostics carry only the fixed stage, numeric raw OS error, error kind and a
/// fixed-form invariant detail; never argv, paths, credentials or free-form strings.
#[derive(Debug)]
pub(crate) struct StageFailure {
    stage: &'static str,
    error: Option<std::io::Error>,
    detail: Option<String>,
}
impl StageFailure {
    pub(crate) fn os(stage: &'static str) -> Self {
        Self {
            stage,
            error: Some(std::io::Error::last_os_error()),
            detail: None,
        }
    }
    pub(crate) fn from_error(stage: &'static str, error: std::io::Error) -> Self {
        Self {
            stage,
            error: Some(error),
            detail: None,
        }
    }
    pub(crate) fn invariant(stage: &'static str, detail: String) -> Self {
        Self {
            stage,
            error: None,
            detail: Some(detail),
        }
    }
    pub(crate) fn report(&self) -> String {
        match &self.error {
            Some(error) => format!(
                "stage={}; os_error={:?}; kind={:?}",
                self.stage,
                error.raw_os_error(),
                error.kind()
            ),
            None => format!(
                "stage={}; os_error=none; kind=Other; detail={}",
                self.stage,
                self.detail.as_deref().unwrap_or("invariant")
            ),
        }
    }
}
fn operation_failed(failure: StageFailure) -> CoreError {
    CoreError::Adapter {
        adapter: "engram-ingest-git".into(),
        message: format!(
            "owned Git subprocess operation failed; {}",
            failure.report()
        ),
    }
}

#[derive(Clone, Debug)]
pub struct GitResourceLimits {
    pub subprocess_timeout: Duration,
    pub max_stdout_bytes: usize,
    pub max_stderr_bytes: usize,
    pub max_source_files: usize,
    pub max_file_bytes: usize,
    pub max_snapshot_bytes: usize,
    pub max_fetched_storage_bytes: u64,
    pub max_remote_transport_bytes: usize,
    pub max_inflated_git_bytes: usize,
    pub max_git_objects: usize,
    pub max_advertised_refs: usize,
    pub max_delta_depth: usize,
    pub max_tree_entries: usize,
    pub max_tree_depth: usize,
}
impl Default for GitResourceLimits {
    fn default() -> Self {
        Self {
            subprocess_timeout: Duration::from_secs(45),
            max_stdout_bytes: 32 * 1024 * 1024,
            max_stderr_bytes: 64 * 1024,
            max_source_files: 100_000,
            max_file_bytes: 2 * 1024 * 1024,
            max_snapshot_bytes: 128 * 1024 * 1024,
            max_fetched_storage_bytes: 256 * 1024 * 1024,
            max_remote_transport_bytes: 256 * 1024 * 1024,
            max_inflated_git_bytes: 256 * 1024 * 1024,
            max_git_objects: 100_000,
            max_advertised_refs: 100_000,
            max_delta_depth: 64,
            max_tree_entries: 100_000,
            max_tree_depth: 128,
        }
    }
}

#[cfg(unix)]
fn prepare_read_pipe<T: std::os::fd::AsRawFd>(
    pipe: &T,
    get_stage: &'static str,
    set_stage: &'static str,
) -> Result<(), StageFailure> {
    let fd = pipe.as_raw_fd();
    // SAFETY: the live owned pipe descriptor is not shared with another reader/writer.
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags < 0 {
        return Err(StageFailure::os(get_stage));
    }
    if unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
        return Err(StageFailure::os(set_stage));
    }
    Ok(())
}
#[cfg(unix)]
fn prepare_write_pipe<T: std::os::fd::AsRawFd>(
    pipe: &T,
    get_stage: &'static str,
    set_stage: &'static str,
) -> Result<(), StageFailure> {
    prepare_read_pipe(pipe, get_stage, set_stage)
}
#[cfg(windows)]
fn prepare_write_pipe<T: std::os::windows::io::AsRawHandle>(
    pipe: &T,
    set_stage: &'static str,
) -> Result<(), StageFailure> {
    use windows_sys::Win32::System::Pipes::{
        PIPE_NOWAIT, PIPE_READMODE_BYTE, SetNamedPipeHandleState,
    };
    // Native observation (run 40756669401): read-endpoint state changes fail with
    // os_error=5 because the parent endpoints lack FILE_WRITE_ATTRIBUTES, and the
    // write endpoint lacks the read attributes its state query would need. The write
    // endpoint has documented write access, so set byte+nowait mode directly without
    // querying current state. Reads never change mode: the sole reader peeks first and
    // reads no more bytes than the peek reports available.
    let mode = PIPE_READMODE_BYTE | PIPE_NOWAIT;
    if unsafe {
        SetNamedPipeHandleState(
            pipe.as_raw_handle(),
            &mode,
            std::ptr::null(),
            std::ptr::null(),
        )
    } == 0
    {
        return Err(StageFailure::os(set_stage));
    }
    Ok(())
}
#[cfg(unix)]
fn read_ready<T: Read + std::os::fd::AsRawFd>(
    pipe: &mut T,
    buffer: &mut [u8],
    _peek_stage: &'static str,
    read_stage: &'static str,
) -> Result<Option<usize>, StageFailure> {
    match pipe.read(buffer) {
        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Ok(None),
        other => other
            .map(Some)
            .map_err(|e| StageFailure::from_error(read_stage, e)),
    }
}
#[cfg(windows)]
fn read_ready<T: Read + std::os::windows::io::AsRawHandle>(
    pipe: &mut T,
    buffer: &mut [u8],
    peek_stage: &'static str,
    read_stage: &'static str,
) -> Result<Option<usize>, StageFailure> {
    use windows_sys::Win32::{
        Foundation::{ERROR_BROKEN_PIPE, ERROR_PIPE_NOT_CONNECTED},
        System::Pipes::PeekNamedPipe,
    };
    let mut available = 0;
    let ok = unsafe {
        PeekNamedPipe(
            pipe.as_raw_handle(),
            std::ptr::null_mut(),
            0,
            std::ptr::null_mut(),
            &mut available,
            std::ptr::null_mut(),
        )
    };
    if ok == 0 {
        let error = std::io::Error::last_os_error();
        return if matches!(error.raw_os_error(),Some(code) if code==ERROR_BROKEN_PIPE as i32 || code==ERROR_PIPE_NOT_CONNECTED as i32)
        {
            Ok(Some(0))
        } else {
            Err(StageFailure::from_error(peek_stage, error))
        };
    }
    if available == 0 {
        return Ok(None);
    }
    let len = (available as usize).min(buffer.len());
    pipe.read(&mut buffer[..len])
        .map(Some)
        .map_err(|e| StageFailure::from_error(read_stage, e))
}
#[cfg(windows)]
fn write_initial(pipe: &mut std::process::ChildStdin, bytes: &[u8]) -> std::io::Result<usize> {
    use std::io::Write;
    match pipe.write(bytes) {
        Err(e)
            if e.raw_os_error() == Some(windows_sys::Win32::Foundation::ERROR_NO_DATA as i32) =>
        {
            Err(std::io::Error::from(std::io::ErrorKind::WouldBlock))
        }
        result => result,
    }
}
#[cfg(unix)]
fn write_initial(pipe: &mut std::process::ChildStdin, bytes: &[u8]) -> std::io::Result<usize> {
    use std::io::Write;
    pipe.write(bytes)
}
#[cfg(unix)]
fn leader_exited(child: &mut std::process::Child) -> std::io::Result<bool> {
    // Do not reap until the final owned-group kill; WNOWAIT reserves the group leader's PID against reuse.
    let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
    let status = unsafe {
        libc::waitid(
            libc::P_PID,
            child.id() as libc::id_t,
            &mut info,
            libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
        )
    };
    if status != 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(unsafe { info.si_pid() } != 0)
}
#[cfg(windows)]
fn leader_exited(child: &mut std::process::Child) -> std::io::Result<bool> {
    child.try_wait().map(|s| s.is_some())
}
#[cfg(unix)]
const LEADER_STAGE: &str = "child.waitid";
#[cfg(windows)]
const LEADER_STAGE: &str = "child.try_wait";

/// Runs one child with bounded, concurrently drained pipes, a wall-clock deadline, and a Unix file-size limit.
pub(crate) fn run_limited(
    command: &mut Command,
    limits: &GitResourceLimits,
    stdout_limit: usize,
) -> CoreResult<Output> {
    run_limited_checked(command, limits, stdout_limit, || Ok(()))
}

/// Check a caller-owned storage budget while the child is running, not only after exit.
pub(crate) fn run_limited_checked(
    command: &mut Command,
    limits: &GitResourceLimits,
    stdout_limit: usize,
    check_storage: impl FnMut() -> CoreResult<()>,
) -> CoreResult<Output> {
    run_limited_input(command, limits, stdout_limit, None, check_storage)
}

pub(crate) fn run_limited_input(
    command: &mut Command,
    limits: &GitResourceLimits,
    stdout_limit: usize,
    input: Option<Vec<u8>>,
    mut check_storage: impl FnMut() -> CoreResult<()>,
) -> CoreResult<Output> {
    #[cfg(not(any(unix, windows)))]
    return Err(CoreError::InvalidRequest {
        reason: "bounded Git acquisition requires a supported child-process platform".into(),
    });
    if input.as_ref().is_some_and(|bytes| bytes.len() > 512) {
        return Err(CoreError::InvalidRequest {
            reason: "Git subprocess input exceeds its protocol limit".into(),
        });
    }
    command
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let file_limit = limits.max_fetched_storage_bytes;
        // SAFETY: this pre-exec hook only invokes async-signal-safe setpgid/getrlimit/setrlimit calls.
        unsafe {
            command.pre_exec(move || {
                if libc::setpgid(0, 0) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                let mut current: libc::rlimit = std::mem::zeroed();
                if libc::getrlimit(libc::RLIMIT_FSIZE, &mut current) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                current.rlim_cur = (file_limit as libc::rlim_t).min(current.rlim_max);
                if libc::setrlimit(libc::RLIMIT_FSIZE, &current) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
    }
    #[cfg(unix)]
    let mut child = command.spawn().map_err(|_| CoreError::Adapter {
        adapter: "engram-ingest-git".into(),
        message: "Git subprocess could not start".into(),
    })?;
    #[cfg(windows)]
    let (mut child, owned_job) = super::windows_job::spawn(command).map_err(operation_failed)?;
    let outcome = (|| {
        let mut stdout = child.stdout.take().expect("piped stdout");
        let mut stderr = child.stderr.take().expect("piped stderr");
        #[cfg(unix)]
        {
            prepare_read_pipe(&stdout, "stdout.get_state", "stdout.set_state")
                .map_err(operation_failed)?;
            prepare_read_pipe(&stderr, "stderr.get_state", "stderr.set_state")
                .map_err(operation_failed)?;
        }
        let mut stdin = child.stdin.take();
        if let Some(stdin) = &stdin {
            #[cfg(unix)]
            prepare_write_pipe(stdin, "stdin.get_state", "stdin.set_state")
                .map_err(operation_failed)?;
            #[cfg(windows)]
            prepare_write_pipe(stdin, "stdin.set_state").map_err(operation_failed)?;
        }
        let input = input.unwrap_or_default();
        let mut written = 0;
        let deadline = Instant::now()
            .checked_add(limits.subprocess_timeout)
            .ok_or_else(|| {
                operation_failed(StageFailure::invariant(
                    "deadline.overflow",
                    "arithmetic".into(),
                ))
            })?;
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let (mut out_done, mut err_done) = (false, false);
        let mut buffer = [0u8; 8192];
        loop {
            if Instant::now() >= deadline {
                return Err(CoreError::InvalidRequest {
                    reason: "Git subprocess exceeded its configured time limit".into(),
                });
            }
            if check_storage().is_err() {
                return Err(CoreError::InvalidRequest {
                    reason: "Git subprocess exceeded its configured storage limit".into(),
                });
            }
            if written < input.len() {
                let stdin_handle = stdin.as_mut().ok_or_else(|| {
                    operation_failed(StageFailure::invariant(
                        "stdin.closed",
                        "handle-dropped".into(),
                    ))
                })?;
                match write_initial(stdin_handle, &input[written..]) {
                    Ok(n) => written += n,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                    Err(e) => {
                        return Err(operation_failed(StageFailure::from_error("stdin.write", e)));
                    }
                }
            }
            if written == input.len() {
                stdin.take();
            }
            for _ in 0..4 {
                if out_done {
                    break;
                }
                match read_ready(&mut stdout, &mut buffer, "stdout.peek", "stdout.read")
                    .map_err(operation_failed)?
                {
                    Some(0) => out_done = true,
                    Some(n) => {
                        if n > stdout_limit.saturating_sub(out.len()) {
                            return Err(output_error());
                        }
                        out.extend_from_slice(&buffer[..n]);
                    }
                    None => break,
                }
            }
            for _ in 0..4 {
                if err_done {
                    break;
                }
                match read_ready(&mut stderr, &mut buffer, "stderr.peek", "stderr.read")
                    .map_err(operation_failed)?
                {
                    Some(0) => err_done = true,
                    Some(n) => {
                        if n > limits.max_stderr_bytes.saturating_sub(err.len()) {
                            return Err(output_error());
                        }
                        err.extend_from_slice(&buffer[..n]);
                    }
                    None => break,
                }
            }
            if leader_exited(&mut child)
                .map_err(|e| operation_failed(StageFailure::from_error(LEADER_STAGE, e)))?
                && out_done
                && err_done
            {
                return Ok((out, err));
            }
            thread::sleep(Duration::from_millis(2));
        }
    })();
    #[cfg(unix)]
    let status = {
        unsafe {
            libc::kill(-(child.id() as libc::pid_t), libc::SIGKILL);
        }
        if outcome.is_err() {
            let _ = child.kill();
        }
        child
            .wait()
            .map_err(|e| operation_failed(StageFailure::from_error("child.wait", e)))?
    };
    #[cfg(windows)]
    let status = super::windows_job::finish(child, owned_job);
    // Return the primary operation failure first; unresolved cleanup custody is retained
    // in the owned-job registry and its evidence is reachable through diagnostic().
    let (stdout, stderr) = outcome?;
    #[cfg(windows)]
    let status = status.map_err(|error| CoreError::Adapter {
        adapter: "engram-ingest-git".into(),
        message: super::windows_job::diagnostic(&error),
    })?;
    Ok(Output {
        status,
        stdout,
        stderr,
    })
}
fn output_error() -> CoreError {
    CoreError::InvalidRequest {
        reason: "Git subprocess output exceeded its configured limit".into(),
    }
}

#[cfg(test)]
mod task_008 {
    use super::*;
    fn fixture_python() -> std::ffi::OsString {
        if let Some(python) = std::env::var_os("ENGRAM_TEST_PYTHON") {
            return python;
        }
        if cfg!(windows) {
            "python".into()
        } else {
            "python3".into()
        }
    }
    #[test]
    fn child_timeout_is_killed_and_reaped() {
        let mut limits = GitResourceLimits::default();
        limits.subprocess_timeout = Duration::from_millis(50);
        let mut command = Command::new(fixture_python());
        command.args(["-c", "import time; time.sleep(10)"]);
        let start = Instant::now();
        let result = run_limited(&mut command, &limits, 64);
        assert!(
            matches!(&result, Err(CoreError::InvalidRequest { reason }) if reason.contains("time limit")),
            "sanitized result: {result:?}"
        );
        assert!(start.elapsed() < Duration::from_secs(3));
    }
    #[test]
    fn concurrent_pipe_drain_caps_output_before_allocation() {
        let mut limits = GitResourceLimits::default();
        limits.subprocess_timeout = Duration::from_secs(2);
        limits.max_stderr_bytes = 64;
        let mut stdout = Command::new(fixture_python());
        stdout.args(["-c", "import sys; sys.stdout.write('x'*10000000)"]);
        let stdout_result = run_limited(&mut stdout, &limits, 128);
        assert!(
            matches!(&stdout_result, Err(CoreError::InvalidRequest { reason }) if reason.contains("output exceeded")),
            "sanitized result: {stdout_result:?}"
        );
        let mut stderr = Command::new(fixture_python());
        stderr.args(["-c", "import sys; sys.stderr.write('x'*10000000)"]);
        let stderr_result = run_limited(&mut stderr, &limits, 128);
        assert!(
            matches!(&stderr_result, Err(CoreError::InvalidRequest { reason }) if reason.contains("output exceeded")),
            "sanitized result: {stderr_result:?}"
        );
    }

    #[test]
    fn inherited_pipe_descendant_cannot_block_cleanup() {
        let temp = tempfile::tempdir().unwrap();
        let marker = temp.path().join("descendant.pid");
        let mut limits = GitResourceLimits::default();
        limits.subprocess_timeout = Duration::from_millis(700);
        let mut command = Command::new(fixture_python());
        command.args(["-c","import subprocess,sys; p=subprocess.Popen([sys.executable,'-c','import time; time.sleep(30)']); open(sys.argv[1],'w').write(str(p.pid))"]).arg(&marker);
        let mut observed = false;
        #[cfg(windows)]
        let mut keeper = None;
        let start = Instant::now();
        let result = run_limited_checked(&mut command, &limits, 128, || {
            if !observed {
                if let Ok(pid) = std::fs::read_to_string(&marker)
                    .unwrap_or_default()
                    .parse::<u32>()
                {
                    #[cfg(windows)]
                    {
                        use std::os::windows::io::FromRawHandle;
                        use windows_sys::Win32::System::Threading::*;
                        let handle = unsafe {
                            OpenProcess(
                                PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
                                0,
                                pid,
                            )
                        };
                        if handle.is_null() {
                            return Err(operation_failed(StageFailure::os(
                                "descendant.open_process",
                            )));
                        }
                        keeper = Some(unsafe {
                            std::os::windows::io::OwnedHandle::from_raw_handle(handle)
                        });
                    }
                    #[cfg(unix)]
                    let _ = pid;
                    observed = true;
                }
            }
            Ok(())
        });
        assert!(
            observed,
            "owned fixture must start the actual pipe-holding descendant; sanitized result: {result:?}"
        );
        assert!(
            matches!(&result,Err(CoreError::InvalidRequest {reason}) if reason.contains("time limit")),
            "sanitized result: {result:?}"
        );
        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawHandle;
            assert_eq!(
                unsafe {
                    windows_sys::Win32::System::Threading::WaitForSingleObject(
                        keeper.unwrap().as_raw_handle(),
                        0,
                    )
                },
                windows_sys::Win32::Foundation::WAIT_OBJECT_0
            );
        }
        assert!(start.elapsed() < Duration::from_secs(3));
    }
    #[test]
    fn task_008_early_child_exit_preserves_status_and_reaps_without_io_threads() {
        let mut command = Command::new(fixture_python());
        command.args(["-c", "import sys;sys.exit(7)"]);
        let start = Instant::now();
        let output = run_limited(&mut command, &GitResourceLimits::default(), 128);
        assert!(
            matches!(&output, Ok(out) if out.status.code() == Some(7) && out.stdout.is_empty()),
            "sanitized result: {output:?}"
        );
        assert!(start.elapsed() < Duration::from_secs(3));
    }

    #[cfg(unix)]
    #[test]
    fn task_008_session_escape_cannot_hold_reader_threads() {
        let dir = tempfile::tempdir().unwrap();
        let pid_path = dir.path().join("escaped.pid");
        let mut command = Command::new(fixture_python());
        command.args(["-c", "import os,sys,time; p=os.fork();\nif p==0:\n os.setsid(); open(sys.argv[1],'w').write(str(os.getpid())); time.sleep(30)\nelse:\n time.sleep(30)"]).arg(&pid_path);
        let mut limits = GitResourceLimits::default();
        limits.subprocess_timeout = Duration::from_millis(300);
        let started = Instant::now();
        let result = run_limited(&mut command, &limits, 128);
        let pid = std::fs::read_to_string(pid_path)
            .expect("owned escaping fixture must establish its custody marker")
            .parse::<libc::pid_t>()
            .unwrap();
        // The escaped test process is deliberately outside the owned group; the fixture retains exact cleanup custody.
        assert_eq!(unsafe { libc::kill(pid, 0) }, 0);
        assert_eq!(
            unsafe { libc::kill(pid, libc::SIGKILL) },
            0,
            "exact separately owned escaped fixture must be stopped"
        );
        assert!(
            matches!(result,Err(CoreError::InvalidRequest {reason}) if reason.contains("time limit"))
        );
        assert!(started.elapsed() < Duration::from_secs(3));
    }
    #[test]
    fn task_008_blocked_stdin_is_pre_admitted_and_never_uses_an_io_thread() {
        let mut command = Command::new(fixture_python());
        command.args(["-c", "import time; time.sleep(30)"]);
        let mut limits = GitResourceLimits::default();
        limits.subprocess_timeout = Duration::from_millis(50);
        let admitted = run_limited_input(&mut command, &limits, 128, Some(vec![0; 512]), || Ok(()));
        assert!(
            matches!(&admitted,Err(CoreError::InvalidRequest {reason}) if reason.contains("time limit")),
            "sanitized result: {admitted:?}"
        );
        let rejected = run_limited_input(&mut command, &limits, 128, Some(vec![0; 513]), || Ok(()));
        assert!(
            matches!(&rejected,Err(CoreError::InvalidRequest {reason}) if reason.contains("input exceeds")),
            "sanitized result: {rejected:?}"
        );
    }
    #[test]
    fn task_008_native_stage_observation_smallest_child_without_stdin() {
        let mut command = Command::new(fixture_python());
        command.args([
            "-c",
            "import sys; sys.stdout.write('out'); sys.stderr.write('err'); sys.exit(7)",
        ]);
        let result = run_limited(&mut command, &GitResourceLimits::default(), 128);
        assert!(
            matches!(&result, Ok(output) if output.status.code() == Some(7)
                && output.stdout.as_slice() == b"out"
                && output.stderr.as_slice() == b"err"),
            "smallest-child no-stdin observation: {result:?}"
        );
    }
    #[test]
    fn task_008_native_stage_observation_protocol_stdin_roundtrip() {
        let mut command = Command::new(fixture_python());
        command.args([
            "-c",
            "import sys; data = sys.stdin.buffer.read(); sys.stdout.write(str(len(data))); sys.exit(0)",
        ]);
        let result = run_limited_input(
            &mut command,
            &GitResourceLimits::default(),
            128,
            Some(vec![b'x'; 512]),
            || Ok(()),
        );
        assert!(
            matches!(&result, Ok(output) if output.status.code() == Some(0)
                && output.stdout.as_slice() == b"512"),
            "protocol-stdin observation: {result:?}"
        );
    }
    #[test]
    fn storage_budget_checked_before_process_completion() {
        let mut command = Command::new(fixture_python());
        command.args(["-c", "import time; time.sleep(30)"]);
        let start = Instant::now();
        let result = run_limited_checked(&mut command, &GitResourceLimits::default(), 128, || {
            Err(CoreError::InvalidRequest {
                reason: "fixture quota".into(),
            })
        });
        assert!(
            matches!(&result, Err(CoreError::InvalidRequest { reason }) if reason.contains("storage limit")),
            "sanitized result: {result:?}"
        );
        assert!(start.elapsed() < Duration::from_secs(3));
    }
}
