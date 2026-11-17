//! Bounded process-owned workers with confirmed-close custody.
use crate::{
    error::{Error, Result},
    ownership::WriterLock,
    transport::Operation,
};
use serde_json::Value;
use std::sync::{Arc, Mutex, OnceLock, mpsc};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

pub(crate) trait Engine: Send {
    fn startup_status(&self) -> Result<()>;
    fn execute(&mut self, operation: &Operation) -> Result<Value>;
    fn cleanup(&mut self) -> Result<()>;
    fn register_cleanup(&mut self, lease: crate::ownership::ScopeLease) -> Result<()>;
}
enum Startup {
    NoEngine(Error),
    Engine(Result<()>),
}
enum Command {
    Execute(Operation, mpsc::SyncSender<Result<Value>>),
    Close(mpsc::SyncSender<Result<()>>),
    Register(crate::ownership::ScopeLease, mpsc::SyncSender<Result<()>>),
}
struct Pending {
    id: String,
    response: mpsc::Receiver<Result<Value>>,
}
struct Lease {
    sender: mpsc::SyncSender<Command>,
    join: Option<JoinHandle<()>>,
    // Parent custody survives worker panic, late startup and lost result channels.
    _lock: Option<WriterLock>,
    poisoned: bool,
    cleanup_only: bool,
    unknown_operation: Option<String>,
    blocked_admission: bool,
    timeout: Duration,
    startup_pending: Option<mpsc::Receiver<Startup>>,
    pending: Option<Pending>,
    registration_pending: Option<mpsc::Receiver<Result<()>>>,
    close_pending: Option<mpsc::Receiver<Result<()>>>,
    engine_closed: bool,
    retained_response: Option<(String, Result<Value>)>,
}
#[derive(Default)]
struct Registry {
    active: usize,
    quarantine: Vec<Lease>,
    retrying: bool,
    unresolved: usize,
}
static REGISTRY: OnceLock<Mutex<Registry>> = OnceLock::new();
fn registry() -> &'static Mutex<Registry> {
    REGISTRY.get_or_init(Mutex::default)
}
const MAX_WORKERS: usize = 64;

/// Reserve before touching storage or credentials. Already admitted workers bound quarantine growth.
pub(crate) struct Admission {
    transferred: bool,
}
impl Admission {
    pub(crate) fn acquire() -> Result<Self> {
        let mut state = registry().lock().map_err(|_| Error::CleanupUnconfirmed)?;
        if state.unresolved != 0
            || state.retrying
            || !state.quarantine.is_empty()
            || state.active >= MAX_WORKERS
        {
            return Err(Error::CleanupUnconfirmed);
        }
        state.active += 1;
        Ok(Self { transferred: false })
    }
    pub(crate) fn start(
        mut self,
        capacity: usize,
        lock: Option<WriterLock>,
        timeout: Duration,
        open: impl FnOnce() -> Result<Box<dyn Engine>> + Send + 'static,
    ) -> Result<Arc<RuntimeOwner>> {
        if capacity == 0
            || capacity > 1024
            || timeout.is_zero()
            || timeout > Duration::from_secs(301)
        {
            return Err(Error::InvalidConfig("invalid runtime queue/deadline bound"));
        }
        let (sender, receiver) = mpsc::sync_channel(capacity);
        let (startup, ready) = mpsc::sync_channel(1);
        let join = thread::Builder::new()
            .name("engram-helix-owned".into())
            .stack_size(16 * 1024 * 1024)
            .spawn(move || {
                let mut engine = match open() {
                    Ok(engine) => engine,
                    Err(error) => {
                        let _ = startup.send(Startup::NoEngine(error));
                        return;
                    }
                };
                let _ = startup.send(Startup::Engine(engine.startup_status()));
                while let Ok(command) = receiver.recv() {
                    match command {
                        Command::Execute(operation, response) => {
                            let _ = response.send(engine.execute(&operation));
                        }
                        Command::Register(lease, response) => {
                            let _ = response.send(engine.register_cleanup(lease));
                        }
                        Command::Close(response) => {
                            let result = engine.cleanup();
                            let closed = result.is_ok();
                            let _ = response.send(result);
                            if closed {
                                return;
                            }
                        }
                    }
                }
                // Defensive orphan path; all ordinary failures retain the live sender and JoinHandle.
                while engine.cleanup().is_err() {
                    eprintln!("engram: Helix orphan cleanup unconfirmed; runtime remains owned");
                    thread::park_timeout(Duration::from_millis(100));
                }
            })
            .map_err(|_| Error::Storage)?;
        let mut lease = Lease {
            sender,
            join: Some(join),
            _lock: lock,
            poisoned: false,
            cleanup_only: false,
            unknown_operation: None,
            blocked_admission: false,
            timeout,
            startup_pending: None,
            pending: None,
            registration_pending: None,
            close_pending: None,
            engine_closed: false,
            retained_response: None,
        };
        match ready.recv_timeout(timeout) {
            Ok(Startup::Engine(Ok(()))) => {
                self.transferred = true;
                Ok(Arc::new(RuntimeOwner {
                    lease: Mutex::new(Some(lease)),
                }))
            }
            Ok(Startup::NoEngine(error))
                if !matches!(
                    &error,
                    Error::CleanupUnconfirmed | Error::OutcomeUnknown { .. }
                ) =>
            {
                lease.engine_closed = true;
                self.transferred = true;
                if lease.finish_join().is_ok() {
                    return Err(error);
                }
                lease.quarantine();
                Err(Error::CleanupAfterFailure {
                    cause: Box::new(error),
                })
            }
            Ok(Startup::NoEngine(_)) => {
                lease.poisoned = true;
                self.transferred = true;
                lease.quarantine();
                Err(Error::CleanupUnconfirmed)
            }
            Ok(Startup::Engine(Err(error))) => {
                self.transferred = true;
                if let Some(id) = error.outcome_unknown_operation() {
                    lease.unknown_operation = Some(id.into());
                }
                if matches!(
                    &error,
                    Error::CleanupUnconfirmed
                        | Error::CleanupAfterFailure { .. }
                        | Error::OutcomeUnknown { .. }
                ) {
                    lease.quarantine();
                    return Err(error);
                }
                // Known marker rejection: reader/runtime cleanup and join precede lock release.
                if lease.stop().is_ok() {
                    return Err(error);
                }
                lease.quarantine();
                Err(Error::CleanupAfterFailure {
                    cause: Box::new(error),
                })
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                lease.startup_pending = Some(ready);
                self.transferred = true;
                lease.quarantine();
                Err(Error::CleanupUnconfirmed)
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                lease.poisoned = true;
                self.transferred = true;
                lease.quarantine();
                Err(Error::CleanupUnconfirmed)
            }
        }
    }
}
impl Drop for Admission {
    fn drop(&mut self) {
        if !self.transferred {
            registry().lock().unwrap_or_else(|e| e.into_inner()).active -= 1;
        }
    }
}
impl Lease {
    fn block_admission(&mut self) {
        if !self.blocked_admission {
            registry()
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .unresolved += 1;
            self.blocked_admission = true;
        }
    }
    fn quarantine(mut self) {
        self.cleanup_only = true;
        self.block_admission();
        eprintln!(
            "engram: Helix cleanup unconfirmed; worker, pending replies and lock remain owned"
        );
        registry()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .quarantine
            .push(self);
    }
    fn poll_pending(&mut self) -> Result<bool> {
        if let Some(response) = self.startup_pending.take() {
            match response.try_recv() {
                Ok(Startup::Engine(_)) => {}
                Ok(Startup::NoEngine(error)) => {
                    if matches!(
                        error,
                        Error::CleanupUnconfirmed | Error::OutcomeUnknown { .. }
                    ) {
                        self.poisoned = true;
                    } else {
                        self.engine_closed = true;
                    }
                }
                Err(mpsc::TryRecvError::Empty) => {
                    self.startup_pending = Some(response);
                    return Ok(false);
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.poisoned = true;
                    return Err(Error::CleanupUnconfirmed);
                }
            }
        }
        if let Some(pending) = self.pending.take() {
            match pending.response.try_recv() {
                Ok(result) => self.retained_response = Some((pending.id, result)),
                Err(mpsc::TryRecvError::Empty) => {
                    self.pending = Some(pending);
                    return Ok(false);
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.poisoned = true;
                    return Err(Error::CleanupUnconfirmed);
                }
            }
        }
        if let Some(response) = self.registration_pending.take() {
            match response.try_recv() {
                Ok(_) => {}
                Err(mpsc::TryRecvError::Empty) => {
                    self.registration_pending = Some(response);
                    return Ok(false);
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.poisoned = true;
                    return Err(Error::CleanupUnconfirmed);
                }
            }
        }
        Ok(true)
    }
    fn finish_join(&mut self) -> Result<()> {
        let deadline = Instant::now() + self.timeout;
        let Some(join) = self.join.as_ref() else {
            self.block_admission();
            return Err(Error::CleanupUnconfirmed);
        };
        while !join.is_finished() {
            if Instant::now() >= deadline {
                self.block_admission();
                return Err(Error::CleanupUnconfirmed);
            }
            thread::sleep(Duration::from_millis(1));
        }
        if self.join.take().expect("owned join").join().is_err() {
            self.poisoned = true;
            self.block_admission();
            return Err(Error::CleanupUnconfirmed);
        }
        let mut state = registry().lock().unwrap_or_else(|e| e.into_inner());
        state.active -= 1;
        if self.blocked_admission {
            state.unresolved -= 1;
            self.blocked_admission = false;
        }
        Ok(())
    }
    fn stop(&mut self) -> Result<()> {
        self.cleanup_only = true;
        if !self.poll_pending().unwrap_or(false)
            || self.poisoned
            || self.unknown_operation.is_some()
            || self
                .join
                .as_ref()
                .is_some_and(|j| j.thread().id() == thread::current().id())
        {
            self.block_admission();
            return Err(Error::CleanupUnconfirmed);
        }
        if self.engine_closed {
            return self.finish_join();
        }
        if self.close_pending.is_none() {
            let (response, result) = mpsc::sync_channel(1);
            if self.sender.try_send(Command::Close(response)).is_err() {
                self.block_admission();
                return Err(Error::CleanupUnconfirmed);
            }
            self.close_pending = Some(result);
        }
        let result = self
            .close_pending
            .as_ref()
            .expect("owned close reply")
            .recv_timeout(self.timeout);
        match result {
            Ok(Ok(())) => {
                self.close_pending.take();
                self.engine_closed = true;
                self.finish_join()
            }
            Ok(Err(_)) => {
                self.close_pending.take();
                self.block_admission();
                Err(Error::CleanupUnconfirmed)
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                self.block_admission();
                Err(Error::CleanupUnconfirmed)
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                self.poisoned = true;
                self.block_admission();
                Err(Error::CleanupUnconfirmed)
            }
        }
    }
    fn receive(
        &mut self,
        operation: &Operation,
        response: mpsc::Receiver<Result<Value>>,
    ) -> Result<Value> {
        match response.recv_timeout(self.timeout) {
            Ok(result) => {
                if matches!(
                    &result,
                    Err(Error::OutcomeUnknown { .. } | Error::CleanupUnconfirmed)
                ) {
                    self.cleanup_only = true;
                    self.block_admission();
                    if operation.mutates {
                        self.unknown_operation = Some(operation.id.clone());
                    }
                }
                result
            }
            Err(error) => {
                self.cleanup_only = true;
                self.block_admission();
                if operation.mutates {
                    self.unknown_operation = Some(operation.id.clone());
                }
                if error == mpsc::RecvTimeoutError::Disconnected {
                    self.poisoned = true;
                }
                self.pending = Some(Pending {
                    id: operation.id.clone(),
                    response,
                });
                Err(operation.unknown())
            }
        }
    }
}

pub(crate) struct RuntimeOwner {
    lease: Mutex<Option<Lease>>,
}
impl RuntimeOwner {
    pub(crate) fn execute(&self, operation: &Operation) -> Result<Value> {
        let mut guard = self.lease.lock().map_err(|_| Error::CleanupUnconfirmed)?;
        let lease = guard.as_mut().ok_or(Error::Storage)?;
        if lease.poisoned || lease.cleanup_only {
            return Err(Error::CleanupUnconfirmed);
        }
        let (response, result) = mpsc::sync_channel(1);
        if let Err(error) = lease
            .sender
            .try_send(Command::Execute(operation.clone(), response))
        {
            if matches!(error, mpsc::TrySendError::Full(_)) {
                return Err(Error::Contention);
            }
            lease.poisoned = true;
            lease.cleanup_only = true;
            lease.block_admission();
            if operation.mutates {
                lease.unknown_operation = Some(operation.id.clone());
            }
            return Err(operation.unknown());
        }
        lease.receive(operation, result)
    }
    pub(crate) fn reconcile_read(&self, operation: &Operation) -> Result<Value> {
        if operation.mutates {
            return Err(Error::InvalidConfig(
                "reconciliation must contain no logical mutation",
            ));
        }
        let mut guard = self.lease.lock().map_err(|_| Error::CleanupUnconfirmed)?;
        let lease = guard.as_mut().ok_or(Error::CleanupUnconfirmed)?;
        let original = lease
            .unknown_operation
            .clone()
            .ok_or(Error::CleanupUnconfirmed)?;
        let unresolved = || Error::OutcomeUnknown {
            operation_id: original.clone(),
        };
        if !lease.poll_pending().unwrap_or(false) || lease.poisoned || operation.id != original {
            return Err(unresolved());
        }
        let (response, result) = mpsc::sync_channel(1);
        lease
            .sender
            .try_send(Command::Execute(operation.clone(), response))
            .map_err(|_| unresolved())?;
        match lease.receive(operation, result) {
            Ok(value) => Ok(value),
            Err(_) => Err(unresolved()),
        }
    }
    pub(crate) fn confirm_operation(&self, id: &str) -> Result<()> {
        let mut guard = self.lease.lock().map_err(|_| Error::CleanupUnconfirmed)?;
        let lease = guard.as_mut().ok_or(Error::CleanupUnconfirmed)?;
        if lease.unknown_operation.as_deref() != Some(id) {
            return Err(lease
                .unknown_operation
                .as_ref()
                .map(|id| Error::OutcomeUnknown {
                    operation_id: id.clone(),
                })
                .unwrap_or(Error::CleanupUnconfirmed));
        }
        lease.unknown_operation = None;
        Ok(()) // Remains cleanup-only; reconciliation never resumes logical queries implicitly.
    }
    pub(crate) fn register_cleanup(&self, scope: crate::ownership::ScopeLease) -> Result<()> {
        let mut guard = self.lease.lock().map_err(|_| Error::CleanupUnconfirmed)?;
        let lease = guard.as_mut().ok_or(Error::CleanupUnconfirmed)?;
        if lease.cleanup_only {
            return Err(Error::CleanupUnconfirmed);
        }
        let (response, result) = mpsc::sync_channel(1);
        lease
            .sender
            .try_send(Command::Register(scope, response))
            .map_err(|_| Error::CleanupUnconfirmed)?;
        match result.recv_timeout(lease.timeout) {
            Ok(result) => result,
            Err(_) => {
                lease.registration_pending = Some(result);
                lease.cleanup_only = true;
                lease.block_admission();
                Err(Error::CleanupUnconfirmed)
            }
        }
    }
    pub(crate) fn close(&self) -> Result<()> {
        let mut guard = self.lease.lock().map_err(|_| Error::CleanupUnconfirmed)?;
        if let Some(lease) = guard.as_mut() {
            lease.stop()?;
        }
        guard.take();
        Ok(())
    }
}
impl Drop for RuntimeOwner {
    fn drop(&mut self) {
        if let Some(mut lease) = self
            .lease
            .get_mut()
            .unwrap_or_else(|e| e.into_inner())
            .take()
        {
            if lease.stop().is_err() {
                lease.quarantine();
            }
        }
    }
}
/// Retry idempotent cleanup only; opaque failures/panics require external quiescence reconciliation.
pub fn retry_retained_cleanup() -> Result<()> {
    let leases = {
        let mut state = registry().lock().map_err(|_| Error::CleanupUnconfirmed)?;
        if state.retrying {
            return Err(Error::CleanupUnconfirmed);
        }
        if state.quarantine.is_empty() {
            return if state.unresolved == 0 {
                Ok(())
            } else {
                Err(Error::CleanupUnconfirmed)
            };
        }
        state.retrying = true;
        std::mem::take(&mut state.quarantine)
    };
    let mut retained = Vec::new();
    for mut lease in leases {
        if lease.stop().is_err() {
            retained.push(lease);
        }
    }
    let mut state = registry().lock().unwrap_or_else(|e| e.into_inner());
    state.quarantine.extend(retained);
    state.retrying = false;
    if state.unresolved != 0 || !state.quarantine.is_empty() {
        Err(Error::CleanupUnconfirmed)
    } else {
        Ok(())
    }
}

#[cfg(test)]
#[path = "../tests/support/runtime_controls.rs"]
mod controls;
