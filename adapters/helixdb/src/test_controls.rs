//! Nonproduction fixture transport evidence; omitted without test-controls.
use super::*;
use crate::{
    ownership::ScopeLease,
    transport::{Operation, Transport},
};
use std::sync::{Arc, Mutex};
#[derive(Clone, Copy)]
enum Fault {
    Definite,
    LostAck,
    LostAckCleanup,
}
#[derive(Clone, Debug)]
pub struct TraceEvent {
    pub phase: &'static str,
    pub observation: EffectObservation,
}
#[derive(Default, Clone, Debug)]
pub struct Snapshot {
    pub all_sends: usize,
    pub reconciliation_reads: usize,
    pub mutation_attempts: Vec<String>,
    pub landed_commits: Vec<String>,
    pub events: Vec<TraceEvent>,
    pub observations: Vec<EffectObservation>,
    pub native_receipts: Vec<Value>,
    pub cleanup_guard_refusals: Vec<String>,
}
impl Snapshot {
    pub fn observations_json(&self) -> Value {
        Value::Array(
            self.observations
                .iter()
                .map(|observation| {
                    let event = TraceEvent {
                        phase: "Observed",
                        observation: observation.clone(),
                    };
                    let mut value = Snapshot {
                        events: vec![event],
                        ..Default::default()
                    }
                    .events_json()[0]
                        .clone();
                    let (kind, original) = match &observation.outcome {
                        EffectOutcome::Staged => ("Staged", None),
                        EffectOutcome::Cancelled => ("Cancelled", None),
                        EffectOutcome::Acknowledged => ("Acknowledged", None),
                        EffectOutcome::DefiniteFailure => ("DefiniteFailure", None),
                        EffectOutcome::Unknown { operation_id } => ("Unknown", Some(operation_id)),
                        EffectOutcome::BlockedByPriorUnknown { operation_id } => {
                            ("BlockedByPriorUnknown", Some(operation_id))
                        }
                    };
                    value["outcome"] = json!(kind);
                    value["operationId"] = json!(original);
                    value["cleanup"] = json!(match observation.cleanup {
                        CleanupCertainty::Confirmed => "Confirmed",
                        CleanupCertainty::Unconfirmed => "Unconfirmed",
                    });
                    value
                })
                .collect(),
        )
    }
    /// Private fixture projection; no record payload or diagnostic cause.
    pub fn events_json(&self) -> Value {
        Value::Array(
            self.events
                .iter()
                .map(|event| {
                    let observation = &event.observation;
                    let identity = &observation.identity;
                    json!({"phase":event.phase,"actor":identity.actor,"call":identity.call,
                "session":identity.session,"backend":identity.backend,"mode":identity.mode,
                "store":identity.store,"scope":identity.scope,"item":identity.item,
                "step":observation.step,"commitId":observation.commit_id})
                })
                .collect(),
        )
    }
}
#[derive(Default)]
struct State {
    injected_pending: std::collections::BTreeSet<String>,
    receipt_reads: std::collections::BTreeSet<String>,
    fault: Option<(usize, Fault)>,
    pause: Option<(
        usize,
        std::sync::mpsc::Sender<()>,
        std::sync::mpsc::Receiver<()>,
    )>,
    snapshot: Snapshot,
    effects: std::collections::BTreeMap<String, Vec<EffectObservation>>,
    receipts: std::collections::BTreeMap<
        String,
        (Option<String>, Value, crate::transport::ReceiptValidator),
    >,
    recorder: Option<std::fs::File>,
    recorder_binding: Option<Value>,
    recorder_failed: bool,
    instructions_root: Option<std::path::PathBuf>,
}
impl State {
    fn record(&mut self, value: Value) {
        use std::io::Write;
        if let Some(file) = &mut self.recorder {
            let row = json!({"binding":self.recorder_binding,"event":value});
            let result = serde_json::to_writer(&mut *file, &row)
                .map_err(std::io::Error::other)
                .and_then(|_| file.write_all(b"\n"))
                .and_then(|_| file.flush());
            if result.is_err() {
                self.recorder_failed = true;
            }
        }
    }
}
pub struct TestControl {
    actual: Arc<dyn Transport>,
    state: Mutex<State>,
}
impl TestControl {
    /// Bind a private, invocation-owned append-only recorder before fixture calls.
    pub fn configure_trace_recorder(
        &self,
        root: &std::path::Path,
        binding: Value,
    ) -> crate::error::Result<()> {
        let canonical = root
            .canonicalize()
            .map_err(|_| Error::InvalidConfig("fixture root unavailable"))?;
        if canonical != root
            || root
                .symlink_metadata()
                .map_err(|_| Error::Storage)?
                .file_type()
                .is_symlink()
        {
            return Err(Error::InvalidConfig("fixture root must be canonical"));
        }
        let mut state = self.state.lock().map_err(|_| Error::Storage)?;
        if state.recorder.is_some() || !state.snapshot.events.is_empty() {
            return Err(Error::Contention);
        }
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(root.join("effect-events.jsonl"))
            .map_err(|_| Error::Storage)?;
        state.recorder = Some(file);
        state.recorder_binding = Some(binding);
        state.instructions_root = Some(root.to_path_buf());
        Ok(())
    }
    fn consume_instruction(&self, operation_id: &str) -> crate::error::Result<()> {
        let mut state = self.state.lock().map_err(|_| Error::Storage)?;
        let Some(root) = state.instructions_root.clone() else {
            return Ok(());
        };
        let path = root.join("fixture-control.json");
        if !path.exists() {
            return Ok(());
        }
        if path
            .symlink_metadata()
            .map_err(|_| Error::Storage)?
            .file_type()
            .is_symlink()
            || path.metadata().map_err(|_| Error::Storage)?.len() > 16384
        {
            return Err(Error::InvalidConfig("invalid private fixture instruction"));
        }
        let value: Value =
            serde_json::from_slice(&std::fs::read(&path).map_err(|_| Error::Storage)?)
                .map_err(|_| Error::InvalidConfig("invalid private fixture instruction"))?;
        if value.get("binding") != state.recorder_binding.as_ref() {
            return Err(Error::InvalidConfig(
                "private fixture instruction binding differs",
            ));
        }
        let fault = match value["operation"].as_str() {
            Some("loseAck") => Fault::LostAck,
            Some("loseAckCleanup") => Fault::LostAckCleanup,
            Some("definiteFailure") => Fault::Definite,
            _ => return Err(Error::InvalidConfig("invalid private fixture instruction")),
        };
        let remaining = value["acknowledgedBefore"]
            .as_u64()
            .unwrap_or(0)
            .checked_add(1)
            .and_then(|value| usize::try_from(value).ok())
            .ok_or(Error::InvalidConfig(
                "private fixture ordinal exceeds bound",
            ))?;
        let consumed = root.join(format!("fixture-control-consumed-{operation_id}.json"));
        if consumed.exists() {
            return Err(Error::Contention);
        }
        std::fs::rename(path, consumed).map_err(|_| Error::Storage)?;
        state.fault = Some((remaining, fault));
        Ok(())
    }
    pub fn recorder_failed(&self) -> bool {
        self.state
            .lock()
            .map(|state| state.recorder_failed)
            .unwrap_or(true)
    }
    /// Exercise the fixture-owned close guard after a real landed lost acknowledgement.
    pub fn lose_mutation_ack_with_cleanup_failure_after(&self, acknowledged_before: usize) {
        self.state.lock().expect("owned control").fault =
            Some((acknowledged_before + 1, Fault::LostAckCleanup));
    }
    pub fn lose_next_mutation_ack(&self) {
        self.state.lock().expect("owned control").fault = Some((1, Fault::LostAck));
    }
    pub fn lose_mutation_ack_after(&self, acknowledged_before: usize) {
        self.state.lock().expect("owned control").fault =
            Some((acknowledged_before + 1, Fault::LostAck));
    }
    pub fn reject_next_mutation_definitely(&self) {
        self.state.lock().expect("owned control").fault = Some((1, Fault::Definite));
    }
    pub fn snapshot(&self) -> Snapshot {
        self.state.lock().expect("owned control").snapshot.clone()
    }
    pub fn pause_mutation_after(
        &self,
        before: usize,
        entered: std::sync::mpsc::Sender<()>,
        release: std::sync::mpsc::Receiver<()>,
    ) {
        self.state.lock().expect("owned control").pause = Some((before + 1, entered, release));
    }
}
impl Transport for TestControl {
    fn fixture_observed(&self, observation: EffectObservation) {
        if let Ok(mut state) = self.state.lock() {
            let value = Snapshot {
                observations: vec![observation.clone()],
                ..Default::default()
            }
            .observations_json()[0]
                .clone();
            state.record(value);
            state.snapshot.observations.push(observation);
        }
    }
    fn fixture_bind_effects(
        &self,
        operation_id: &str,
        effects: Vec<EffectObservation>,
        receipt_hash: Option<String>,
        receipt_context: Value,
        validate_receipt: crate::transport::ReceiptValidator,
    ) {
        // Keep the exact owner-provided frame. Missing/invalid association
        // yields missing proof, never an invented causal identity.
        if uuid::Uuid::parse_str(operation_id)
            .ok()
            .is_none_or(|id| id.to_string() != operation_id)
            || effects.iter().any(|effect| {
                effect.commit_id.as_deref() != Some(operation_id)
                    || effect.step.is_none()
                    || effect.identity.call.is_empty()
                    || effect.identity.session.is_empty()
            })
        {
            return;
        }
        if let Ok(mut state) = self.state.lock() {
            state.effects.insert(operation_id.into(), effects);
            state.receipts.insert(
                operation_id.into(),
                (receipt_hash, receipt_context, validate_receipt),
            );
        }
    }
    fn fixture_after_ack(&self, operation_id: &str, effects: Vec<EffectObservation>) {
        if let Ok(mut state) = self.state.lock() {
            let Some(bound) = state.effects.get(operation_id) else {
                return;
            };
            if bound.len() != effects.len()
                || !bound.iter().zip(&effects).all(|(before, after)| {
                    before.identity == after.identity
                        && before.step == after.step
                        && before.commit_id == after.commit_id
                        && after.commit_id.as_deref() == Some(operation_id)
                        && after.outcome == EffectOutcome::Acknowledged
                })
            {
                return;
            }
            for observation in effects {
                let event = TraceEvent {
                    phase: "AfterAck",
                    observation,
                };
                state.record(
                    Snapshot {
                        events: vec![event.clone()],
                        ..Default::default()
                    }
                    .events_json()[0]
                        .clone(),
                );
                state.snapshot.events.push(event);
            }
        }
    }
    fn execute(&self, operation: &Operation) -> crate::error::Result<Value> {
        if operation.mutates {
            self.consume_instruction(&operation.id)?;
        }
        let (fault, pause) = {
            let mut state = self.state.lock().map_err(|_| Error::Storage)?;
            state.snapshot.all_sends += 1;
            if operation.mutates {
                state.snapshot.mutation_attempts.push(operation.id.clone());
                let fault = match state.fault {
                    Some((1, fault)) => {
                        state.fault = None;
                        Some(fault)
                    }
                    Some((remaining, fault)) => {
                        state.fault = Some((remaining - 1, fault));
                        None
                    }
                    None => None,
                };
                let pause = match state.pause.take() {
                    Some((1, entered, release)) => Some((entered, release)),
                    Some((remaining, entered, release)) => {
                        state.pause = Some((remaining - 1, entered, release));
                        None
                    }
                    None => None,
                };
                (fault, pause)
            } else {
                (None, None)
            }
        };
        if let Some((entered, release)) = pause {
            entered.send(()).map_err(|_| Error::Storage)?;
            release
                .recv_timeout(std::time::Duration::from_secs(120))
                .map_err(|_| Error::Storage)?;
        }
        if matches!(fault, Some(Fault::Definite)) {
            return Err(Error::Storage);
        }
        if operation.mutates {
            let mut state = self.state.lock().map_err(|_| Error::Storage)?;
            let effects = state
                .effects
                .get(&operation.id)
                .cloned()
                .unwrap_or_default();
            for observation in effects {
                let event = TraceEvent {
                    phase: "BeforeSend",
                    observation,
                };
                let mut value = Snapshot {
                    events: vec![event.clone()],
                    ..Default::default()
                }
                .events_json()[0]
                    .clone();
                value["expectedReceiptSha256"] = json!(
                    state
                        .receipts
                        .get(&operation.id)
                        .and_then(|receipt| receipt.0.clone())
                );
                value["receiptContext"] = state
                    .receipts
                    .get(&operation.id)
                    .map(|receipt| receipt.1.clone())
                    .unwrap_or(Value::Null);
                state.record(value);
                state.snapshot.events.push(event);
            }
        }
        let response = self.actual.execute(operation)?;
        if operation.mutates {
            let mut state = self.state.lock().map_err(|_| Error::Storage)?;
            state.snapshot.landed_commits.push(operation.id.clone());
            if let Some((expected_hash, _, validator)) = state.receipts.get(&operation.id) {
                let value = json!({"phase":"NativeReply","commitId":operation.id,
                    "expectedReceiptSha256":expected_hash,"canonicalReceiptMatches":validator(&response)});
                state.record(value.clone());
                state.snapshot.native_receipts.push(value);
            }
        }
        if matches!(fault, Some(Fault::LostAck | Fault::LostAckCleanup)) {
            self.state
                .lock()
                .map_err(|_| Error::Storage)?
                .injected_pending
                .insert(operation.id.clone());
            let cause = Error::OutcomeUnknown {
                operation_id: operation.id.clone(),
            };
            if matches!(fault, Some(Fault::LostAckCleanup))
                && matches!(self.close(), Err(Error::CleanupUnconfirmed))
            {
                self.state
                    .lock()
                    .map_err(|_| Error::Storage)?
                    .snapshot
                    .cleanup_guard_refusals
                    .push(operation.id.clone());
                return Err(Error::CleanupAfterFailure {
                    cause: Box::new(cause),
                });
            }
            return Err(cause);
        }
        Ok(response)
    }
    fn close(&self) -> crate::error::Result<()> {
        if !self
            .state
            .lock()
            .map_err(|_| Error::CleanupUnconfirmed)?
            .injected_pending
            .is_empty()
        {
            return Err(Error::CleanupUnconfirmed);
        }
        self.actual.close()
    }
    fn reconcile_read(&self, operation: &Operation) -> crate::error::Result<Value> {
        let owned = {
            let mut state = self.state.lock().map_err(|_| Error::CleanupUnconfirmed)?;
            if operation.mutates {
                return Err(Error::InvalidConfig("reconciliation must not mutate"));
            }
            state.snapshot.all_sends += 1;
            state.snapshot.reconciliation_reads += 1;
            state.injected_pending.contains(&operation.id)
        };
        if !owned {
            return self.actual.reconcile_read(operation);
        }
        // This exact loss was injected above a successful native response.
        // Canonical receipts.rs still verifies its partition/key/owner/epoch/
        // fingerprint before calling confirm_operation below.
        let response = self.actual.execute(operation)?;
        self.state
            .lock()
            .map_err(|_| Error::CleanupUnconfirmed)?
            .receipt_reads
            .insert(operation.id.clone());
        Ok(response)
    }
    fn confirm_operation(&self, id: &str) -> crate::error::Result<()> {
        let mut state = self.state.lock().map_err(|_| Error::CleanupUnconfirmed)?;
        if state.injected_pending.contains(id) {
            if !state.receipt_reads.remove(id) {
                return Err(Error::OutcomeUnknown {
                    operation_id: id.into(),
                });
            }
            state.injected_pending.remove(id);
            return Ok(());
        }
        drop(state);
        self.actual.confirm_operation(id)
    }
    fn register_cleanup(&self, lease: ScopeLease) -> crate::error::Result<()> {
        self.actual.register_cleanup(lease)
    }
}
impl Drop for TestControl {
    fn drop(&mut self) {
        let state = self
            .state
            .get_mut()
            .unwrap_or_else(|poison| poison.into_inner());
        if !state.injected_pending.is_empty() {
            // Exceptional proof failure: retain the actual runtime and its
            // existing registered lease until this owned test process exits.
            // Never silently release unresolved injected work on Drop.
            eprintln!(
                "HELIX_TEST_UNRESOLVED_NATIVE_OWNER pending={:?} physical_close=UNCONFIRMED owner_retained_until_process_exit=true",
                state.injected_pending
            );
            std::mem::forget(self.actual.clone());
        }
    }
}
#[cfg(test)]
mod contracts {
    use super::*;
    #[derive(Default)]
    struct Actual(Mutex<(usize, usize, usize, usize)>);
    impl Transport for Actual {
        fn execute(&self, _: &Operation) -> crate::error::Result<Value> {
            self.0.lock().unwrap().0 += 1;
            Ok(serde_json::json!({}))
        }
        fn close(&self) -> crate::error::Result<()> {
            self.0.lock().unwrap().1 += 1;
            Ok(())
        }
        fn reconcile_read(&self, _: &Operation) -> crate::error::Result<Value> {
            self.0.lock().unwrap().2 += 1;
            Err(Error::Storage)
        }
        fn confirm_operation(&self, _: &str) -> crate::error::Result<()> {
            self.0.lock().unwrap().3 += 1;
            Ok(())
        }
        fn register_cleanup(&self, _: ScopeLease) -> crate::error::Result<()> {
            Ok(())
        }
    }
    #[test]
    fn cleanup_wrapper_requires_actual_landed_write_and_owned_guard_refusal() {
        let actual = Arc::new(Actual::default());
        let control = TestControl {
            actual: actual.clone(),
            state: Mutex::new(State::default()),
        };
        let id = uuid::Uuid::new_v4().to_string();
        let operation = Operation {
            id: id.clone(),
            bytes: vec![],
            write: true,
            mutates: true,
        };
        control.lose_mutation_ack_with_cleanup_failure_after(0);
        let error = control.execute(&operation).unwrap_err();
        assert!(matches!(error, Error::CleanupAfterFailure { .. }));
        assert_eq!(error.outcome_unknown_operation(), Some(id.as_str()));
        assert_eq!(control.snapshot().cleanup_guard_refusals, vec![id.clone()]);
        assert_eq!(*actual.0.lock().unwrap(), (1, 0, 0, 0)); // Actual close was not attempted.
        control
            .reconcile_read(&Operation {
                mutates: false,
                ..operation
            })
            .unwrap();
        control.confirm_operation(&id).unwrap();
        control.close().unwrap();
        assert_eq!(*actual.0.lock().unwrap(), (2, 1, 0, 0));
    }
    #[test]
    fn injected_uncertainty_requires_own_read_before_confirmation_and_close() {
        let actual = Arc::new(Actual::default());
        let control = TestControl {
            actual: actual.clone(),
            state: Mutex::new(State::default()),
        };
        let id = uuid::Uuid::new_v4().to_string();
        let write = Operation {
            id: id.clone(),
            bytes: vec![],
            write: true,
            mutates: true,
        };
        control.lose_next_mutation_ack();
        assert!(control.execute(&write).is_err());
        assert!(control.close().is_err());
        assert!(control.confirm_operation(&id).is_err());
        assert!(control.reconcile_read(&write).is_err());
        assert_eq!(actual.0.lock().unwrap().0, 1);
        let wrong = Operation {
            id: uuid::Uuid::new_v4().to_string(),
            mutates: false,
            ..write.clone()
        };
        assert!(control.reconcile_read(&wrong).is_err());
        assert_eq!(actual.0.lock().unwrap().2, 1);
        assert!(control.confirm_operation(&id).is_err());
        let read = Operation {
            mutates: false,
            ..write
        };
        control.reconcile_read(&read).unwrap();
        control.confirm_operation(&id).unwrap();
        control.close().unwrap();
        control.confirm_operation(&wrong.id).unwrap();
        assert_eq!(*actual.0.lock().unwrap(), (2, 1, 1, 1));
        assert_eq!(control.snapshot().reconciliation_reads, 2);
    }
}
impl HelixStore {
    /// Instrument only an unshared, invocation-owned fresh handle.
    pub fn instrument_native_test_control(&mut self) -> crate::error::Result<Arc<TestControl>> {
        if self.session.is_some() {
            return Err(Error::Contention);
        }
        let inner = Arc::get_mut(&mut self.inner).ok_or(Error::Contention)?;
        let control = Arc::new(TestControl {
            actual: inner.transport.clone(),
            state: Mutex::new(State::default()),
        });
        inner.transport = control.clone();
        Ok(control)
    }
}
