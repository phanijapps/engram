use engram_knowledge::{CoreError, CoreResult};
use std::{
    io::Read,
    process::{Command, Output, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

#[derive(Clone, Debug)]
pub struct GitResourceLimits {
    pub subprocess_timeout: Duration,
    pub max_stdout_bytes: usize,
    pub max_stderr_bytes: usize,
    pub max_source_files: usize,
    pub max_file_bytes: usize,
    pub max_snapshot_bytes: usize,
    pub max_fetched_storage_bytes: u64,
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
        }
    }
}

#[derive(Clone, Copy)]
enum Stream {
    Stdout,
    Stderr,
}
enum Event {
    Data(Stream, Vec<u8>),
    LimitExceeded,
}
fn drain<R: Read + Send + 'static>(
    mut reader: R,
    limit: usize,
    stream: Stream,
    tx: mpsc::Sender<Event>,
) {
    let mut bytes = Vec::with_capacity(limit.min(64 * 1024));
    let mut buffer = [0u8; 8192];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) => {
                let _ = tx.send(Event::Data(stream, bytes));
                return;
            }
            Ok(n) => {
                if n > limit.saturating_sub(bytes.len()) {
                    let _ = tx.send(Event::LimitExceeded);
                    return;
                }
                bytes.extend_from_slice(&buffer[..n]);
            }
            Err(_) => {
                let _ = tx.send(Event::LimitExceeded);
                return;
            }
        }
    }
}

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
    mut check_storage: impl FnMut() -> CoreResult<()>,
) -> CoreResult<Output> {
    #[cfg(not(unix))]
    return Err(CoreError::InvalidRequest {
        reason: "bounded Git acquisition requires Unix process-group support".into(),
    });
    command
        .stdin(Stdio::null())
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
    let mut child = command.spawn().map_err(|_| CoreError::Adapter {
        adapter: "engram-ingest-git".into(),
        message: "Git subprocess could not start".into(),
    })?;
    let stdout = child.stdout.take().expect("piped stdout");
    let stderr = child.stderr.take().expect("piped stderr");
    let (tx, rx) = mpsc::channel();
    let out_tx = tx.clone();
    let out_thread = thread::spawn(move || drain(stdout, stdout_limit, Stream::Stdout, out_tx));
    let stderr_limit = limits.max_stderr_bytes;
    let err_thread = thread::spawn(move || drain(stderr, stderr_limit, Stream::Stderr, tx));
    let deadline = Instant::now() + limits.subprocess_timeout;
    let mut out = None;
    let mut err = None;
    let mut status = None;
    let mut failure = None;
    loop {
        while let Ok(event) = rx.try_recv() {
            match event {
                Event::Data(Stream::Stdout, bytes) => out = Some(bytes),
                Event::Data(Stream::Stderr, bytes) => err = Some(bytes),
                Event::LimitExceeded => {
                    failure = Some("Git subprocess output exceeded its configured limit")
                }
            }
        }
        if check_storage().is_err() {
            failure = Some("Git subprocess exceeded its configured storage limit");
        }
        if failure.is_some() {
            break;
        }
        match child.try_wait() {
            Ok(Some(done)) => status = Some(done),
            Ok(None) => {}
            Err(_) => {
                failure = Some("Git subprocess status could not be collected");
                break;
            }
        }
        if status.is_some() && out.is_some() && err.is_some() {
            break;
        }
        if Instant::now() >= deadline {
            failure = Some("Git subprocess exceeded its configured time limit");
            break;
        }
        thread::sleep(Duration::from_millis(2));
    }
    // Even a successfully exited leader may leave a helper holding a pipe.
    // Kill the whole owned group before joining pipe readers on every path.
    #[cfg(unix)]
    unsafe {
        libc::kill(-(child.id() as libc::pid_t), libc::SIGKILL);
    }
    if failure.is_some() {
        let _ = child.kill();
    }
    let waited = child.wait();
    let _ = out_thread.join();
    let _ = err_thread.join();
    let final_status = waited.map_err(|_| CoreError::Adapter {
        adapter: "engram-ingest-git".into(),
        message: "Git subprocess could not be reaped".into(),
    })?;
    if let Some(reason) = failure {
        return Err(CoreError::InvalidRequest {
            reason: reason.into(),
        });
    }
    Ok(Output {
        status: final_status,
        stdout: out.unwrap_or_default(),
        stderr: err.unwrap_or_default(),
    })
}

#[cfg(test)]
mod task_008 {
    use super::*;
    #[test]
    fn child_timeout_is_killed_and_reaped() {
        let mut limits = GitResourceLimits::default();
        limits.subprocess_timeout = Duration::from_millis(50);
        let mut command = Command::new("python3");
        command.args(["-c", "import time; time.sleep(10)"]);
        let start = Instant::now();
        let result = run_limited(&mut command, &limits, 64);
        assert!(
            matches!(result, Err(CoreError::InvalidRequest { reason }) if reason.contains("time limit"))
        );
        assert!(start.elapsed() < Duration::from_secs(3));
    }
    #[test]
    fn concurrent_pipe_drain_caps_output_before_allocation() {
        let mut limits = GitResourceLimits::default();
        limits.subprocess_timeout = Duration::from_secs(2);
        limits.max_stderr_bytes = 64;
        let mut stdout = Command::new("python3");
        stdout.args(["-c", "import sys; sys.stdout.write('x'*10000000)"]);
        assert!(
            matches!(run_limited(&mut stdout, &limits, 128), Err(CoreError::InvalidRequest { reason }) if reason.contains("output exceeded"))
        );
        let mut stderr = Command::new("python3");
        stderr.args(["-c", "import sys; sys.stderr.write('x'*10000000)"]);
        assert!(
            matches!(run_limited(&mut stderr, &limits, 128), Err(CoreError::InvalidRequest { reason }) if reason.contains("output exceeded"))
        );
    }

    #[test]
    fn inherited_pipe_descendant_cannot_block_cleanup() {
        let mut limits = GitResourceLimits::default();
        limits.subprocess_timeout = Duration::from_millis(200);
        let mut command = Command::new("python3");
        command.args([
            "-c",
            "import os,time; p=os.fork(); time.sleep(30) if p==0 else None",
        ]);
        let start = Instant::now();
        assert!(
            matches!(run_limited(&mut command, &limits, 128), Err(CoreError::InvalidRequest { reason }) if reason.contains("time limit"))
        );
        assert!(start.elapsed() < Duration::from_secs(3));
    }

    #[test]
    fn storage_budget_checked_before_process_completion() {
        let mut command = Command::new("python3");
        command.args(["-c", "import time; time.sleep(30)"]);
        let start = Instant::now();
        let result = run_limited_checked(&mut command, &GitResourceLimits::default(), 128, || {
            Err(CoreError::InvalidRequest {
                reason: "fixture quota".into(),
            })
        });
        assert!(
            matches!(result, Err(CoreError::InvalidRequest { reason }) if reason.contains("storage limit"))
        );
        assert!(start.elapsed() < Duration::from_secs(3));
    }
}
