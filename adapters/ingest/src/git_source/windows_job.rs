//! Assign a suspended child to an owned kill-on-close job before running any helper code.
use super::bounded_process::StageFailure;
use std::{
    io, mem,
    os::windows::{
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
        process::CommandExt,
    },
    process::{Child, Command, ExitStatus},
    ptr,
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicU64, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::INVALID_HANDLE_VALUE,
    System::{
        Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First, Thread32Next,
        },
        JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            JOBOBJECT_BASIC_ACCOUNTING_INFORMATION, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
            JobObjectBasicAccountingInformation, JobObjectExtendedLimitInformation,
            QueryInformationJobObject, SetInformationJobObject, TerminateJobObject,
        },
        Threading::{CREATE_SUSPENDED, OpenThread, ResumeThread, THREAD_SUSPEND_RESUME},
    },
};

const MAX_OWNED_JOBS: usize = 16;
static RESERVED: AtomicUsize = AtomicUsize::new(0);
struct Permit;
impl Permit {
    fn acquire() -> Result<Self, StageFailure> {
        RESERVED
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                (n < MAX_OWNED_JOBS).then_some(n + 1)
            })
            .map(|_| Self)
            .map_err(|_| StageFailure::invariant("job.permit", "capacity=16".into()))
    }
}
impl Drop for Permit {
    fn drop(&mut self) {
        RESERVED.fetch_sub(1, Ordering::AcqRel);
    }
}
pub(super) struct OwnedJob {
    handle: OwnedHandle,
    _permit: Permit,
    id: u64,
}
struct Pending {
    child: Child,
    job: OwnedJob,
}
static PENDING: OnceLock<Mutex<Vec<Pending>>> = OnceLock::new();
fn registry() -> &'static Mutex<Vec<Pending>> {
    PENDING.get_or_init(|| Mutex::new(Vec::with_capacity(MAX_OWNED_JOBS)))
}
#[cfg(test)]
pub(super) fn pending_count() -> usize {
    registry().lock().unwrap_or_else(|e| e.into_inner()).len()
}
#[derive(Debug)]
struct CleanupPending {
    id: u64,
    pid: u32,
    operation: &'static str,
    cause: Option<String>,
}
impl std::fmt::Display for CleanupPending {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.cause {
            Some(cause) => write!(
                f,
                "cleanup_pending; retry_owner=windows-owned-job-registry; registry_id={}; pid={}; operation={}; cause={}; retry uses retained handles, never PID alone",
                self.id, self.pid, self.operation, cause
            ),
            None => write!(
                f,
                "cleanup_pending; retry_owner=windows-owned-job-registry; registry_id={}; pid={}; operation={}; retry uses retained handles, never PID alone",
                self.id, self.pid, self.operation
            ),
        }
    }
}
impl std::error::Error for CleanupPending {}
pub(super) fn diagnostic(error: &io::Error) -> String {
    if error.get_ref().is_some_and(|e| e.is::<CleanupPending>()) {
        error.to_string()
    } else {
        "owned Windows helper operation failed".into()
    }
}

impl OwnedJob {
    fn new() -> Result<Self, StageFailure> {
        // SAFETY: null security/name pointers request an unnamed, non-inheritable owned handle.
        let permit = Permit::acquire()?;
        let raw = unsafe { CreateJobObjectW(ptr::null(), ptr::null()) };
        if raw.is_null() {
            return Err(StageFailure::os("job.create"));
        }
        // SAFETY: successful CreateJobObjectW transfers this handle to this guard exactly once.
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let job = Self {
            handle: unsafe { OwnedHandle::from_raw_handle(raw) },
            _permit: permit,
            id: NEXT.fetch_add(1, Ordering::Relaxed),
        };
        let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { mem::zeroed() };
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        // SAFETY: the input structure lives for the call and its size/type match the information class.
        let ok = unsafe {
            SetInformationJobObject(
                job.handle.as_raw_handle(),
                JobObjectExtendedLimitInformation,
                (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                mem::size_of_val(&limits) as u32,
            )
        };
        if ok == 0 {
            return Err(StageFailure::os("job.set_limits"));
        }
        Ok(job)
    }
    fn terminate(&self) -> Result<(), StageFailure> {
        if unsafe { TerminateJobObject(self.handle.as_raw_handle(), 1) } == 0 {
            return Err(StageFailure::os("job.terminate"));
        }
        Ok(())
    }
    fn active(&self) -> Result<u32, StageFailure> {
        let mut info: JOBOBJECT_BASIC_ACCOUNTING_INFORMATION = unsafe { mem::zeroed() };
        if unsafe {
            QueryInformationJobObject(
                self.handle.as_raw_handle(),
                JobObjectBasicAccountingInformation,
                (&mut info as *mut JOBOBJECT_BASIC_ACCOUNTING_INFORMATION).cast(),
                mem::size_of_val(&info) as u32,
                ptr::null_mut(),
            )
        } == 0
        {
            return Err(StageFailure::os("job.query_active"));
        }
        Ok(info.ActiveProcesses)
    }
}
fn resume_initial_thread(pid: u32) -> Result<(), StageFailure> {
    // SAFETY: a system snapshot handle is owned below; this call mutates no foreign process.
    let raw = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) };
    if raw == INVALID_HANDLE_VALUE {
        return Err(StageFailure::os("thread.snapshot"));
    }
    let snapshot = unsafe { OwnedHandle::from_raw_handle(raw) };
    let mut entry: THREADENTRY32 = unsafe { mem::zeroed() };
    entry.dwSize = mem::size_of_val(&entry) as u32;
    // SAFETY: the snapshot and initialized structure live for enumeration.
    let mut found = unsafe { Thread32First(snapshot.as_raw_handle(), &mut entry) };
    while found != 0 {
        if entry.th32OwnerProcessID == pid {
            // Only the suspended child's initial thread is eligible; it cannot have spawned descendants yet.
            let raw = unsafe { OpenThread(THREAD_SUSPEND_RESUME, 0, entry.th32ThreadID) };
            if raw.is_null() {
                return Err(StageFailure::os("thread.open"));
            }
            let thread = unsafe { OwnedHandle::from_raw_handle(raw) };
            let resumed = unsafe { ResumeThread(thread.as_raw_handle()) };
            if resumed == u32::MAX {
                // Distinguish the failure sentinel from a successful-but-unexpected count.
                return Err(StageFailure::os("thread.resume"));
            }
            if resumed != 1 {
                return Err(StageFailure::invariant(
                    "thread.resume.count",
                    format!("suspend-count={resumed}"),
                ));
            }
            return Ok(());
        }
        found = unsafe { Thread32Next(snapshot.as_raw_handle(), &mut entry) };
    }
    Err(StageFailure::invariant(
        "thread.missing",
        "initial-thread".into(),
    ))
}

/// Selectable cleanup-stage fault for tests; production passes `None`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CleanupFault {
    Termination,
    StatusQuery,
    ActiveQuery,
    Deadline,
}
fn stop(child: &mut Child, job: &OwnedJob, until: Instant) -> Result<ExitStatus, StageFailure> {
    stop_injected(child, job, until, None)
}
fn stop_injected(
    child: &mut Child,
    job: &OwnedJob,
    until: Instant,
    fault: Option<CleanupFault>,
) -> Result<ExitStatus, StageFailure> {
    // A failed termination or an exhausted allowance means custody genuinely cannot
    // be resolved here: return the stage failure immediately so the pending registry
    // retains the handles (the killed leader alone would otherwise drain accounting
    // to zero and hide the unresolved-termination scenario the fault models).
    if matches!(fault, Some(CleanupFault::Termination)) {
        let _ = child.kill();
        return Err(StageFailure::os("job.terminate"));
    }
    if matches!(fault, Some(CleanupFault::Deadline)) {
        let _ = child.kill();
        return Err(StageFailure::invariant(
            "cleanup.deadline",
            "job_terminate=1;child_kill=1".into(),
        ));
    }
    let termination = job.terminate();
    let direct = child.kill();
    let mut status = None;
    loop {
        if status.is_none() {
            if matches!(fault, Some(CleanupFault::StatusQuery)) {
                return Err(StageFailure::os("cleanup.try_wait"));
            }
            status = child
                .try_wait()
                .map_err(|e| StageFailure::from_error("cleanup.try_wait", e))?;
        }
        if matches!(fault, Some(CleanupFault::ActiveQuery)) {
            return Err(StageFailure::os("job.query_active"));
        }
        if status.is_some() && job.active()? == 0 {
            return Ok(status.unwrap());
        }
        if Instant::now() >= until {
            return Err(StageFailure::invariant(
                "cleanup.deadline",
                format!(
                    "job_terminate={};child_kill={}",
                    termination.is_ok() as u8,
                    direct.is_ok() as u8
                ),
            ));
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}
fn retry_pending() {
    let mut pending = registry().lock().unwrap_or_else(|e| e.into_inner());
    let mut index = 0;
    while index < pending.len() {
        let entry = &mut pending[index];
        if stop(&mut entry.child, &entry.job, Instant::now()).is_ok() {
            pending.swap_remove(index);
        } else {
            index += 1;
        }
    }
}
fn retain_pending(
    child: Child,
    job: OwnedJob,
    operation: &'static str,
    cause: Option<String>,
) -> io::Error {
    let error = CleanupPending {
        id: job.id,
        pid: child.id(),
        operation,
        cause,
    };
    // Every active or pending job retains one of the same 16 permits, so this registry cannot grow without bound.
    registry()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .push(Pending { child, job });
    io::Error::other(error)
}
pub(super) fn finish(mut child: Child, job: OwnedJob) -> io::Result<ExitStatus> {
    finish_injected(child, job, None)
}
pub(super) fn finish_injected(
    mut child: Child,
    job: OwnedJob,
    fault: Option<CleanupFault>,
) -> io::Result<ExitStatus> {
    match stop_injected(
        &mut child,
        &job,
        Instant::now() + Duration::from_millis(250),
        fault,
    ) {
        Ok(status) => Ok(status),
        Err(failure) => Err(retain_pending(
            child,
            job,
            "termination-and-reap",
            Some(format!(
                "owned Git subprocess operation failed; {}",
                failure.report()
            )),
        )),
    }
}
fn assign(job: &OwnedJob, child: &Child) -> Result<(), StageFailure> {
    if unsafe { AssignProcessToJobObject(job.handle.as_raw_handle(), child.as_raw_handle()) } == 0 {
        return Err(StageFailure::os("job.assign"));
    }
    Ok(())
}
fn spawn_with(
    command: &mut Command,
    assign_child: impl FnOnce(&OwnedJob, &Child) -> Result<(), StageFailure>,
    resume: impl FnOnce(u32) -> Result<(), StageFailure>,
) -> Result<(Child, OwnedJob), StageFailure> {
    retry_pending();
    let job = OwnedJob::new()?;
    command.creation_flags(CREATE_SUSPENDED);
    let mut child = command
        .spawn()
        .map_err(|e| StageFailure::from_error("child.spawn", e))?;
    let started = assign_child(&job, &child).and_then(|_| resume(child.id()));
    if let Err(error) = started {
        let cause = format!("owned Git subprocess operation failed; {}", error.report());
        if stop(
            &mut child,
            &job,
            Instant::now() + Duration::from_millis(250),
        )
        .is_err()
        {
            return Err(stage_from_pending(retain_pending(
                child,
                job,
                "assignment-or-resume",
                Some(cause),
            )));
        }
        return Err(error);
    }
    Ok((child, job))
}
fn stage_from_pending(error: io::Error) -> StageFailure {
    StageFailure::invariant("cleanup.pending", error.to_string())
}
pub(super) fn spawn(command: &mut Command) -> Result<(Child, OwnedJob), StageFailure> {
    spawn_with(command, assign, resume_initial_thread)
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows_sys::Win32::{
        Foundation::WAIT_OBJECT_0,
        System::Threading::{
            OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
            WaitForSingleObject,
        },
    };
    #[test]
    fn task_008_windows_assignment_and_resume_failures_reap_the_exact_suspended_child() {
        for after_assignment in [false, true] {
            let temp = tempfile::tempdir().unwrap();
            let marker = temp.path().join("must-not-run");
            let mut command = Command::new("cmd.exe");
            command
                .args(["/D", "/C", "echo helper-ran > must-not-run"])
                .current_dir(temp.path());
            let keeper = std::cell::RefCell::new(None);
            let result = spawn_with(
                &mut command,
                |job, child| {
                    let raw = unsafe {
                        OpenProcess(
                            PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
                            0,
                            child.id(),
                        )
                    };
                    assert!(!raw.is_null());
                    *keeper.borrow_mut() = Some(unsafe { OwnedHandle::from_raw_handle(raw) });
                    if after_assignment {
                        assign(job, child)
                    } else {
                        Err(StageFailure::invariant("job.assign", "injected".into()))
                    }
                },
                |_| Err(StageFailure::invariant("thread.resume", "injected".into())),
            );
            assert!(result.is_err());
            let keeper = keeper.into_inner().unwrap();
            assert_eq!(
                unsafe { WaitForSingleObject(keeper.as_raw_handle(), 0) },
                WAIT_OBJECT_0
            );
            assert!(!marker.exists());
            assert!(registry().lock().unwrap().is_empty());
        }
    }
}
