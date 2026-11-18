//! Typed storage contributions; deterministic memory behavior belongs to core/memory.
use crate::{HelixStore, error::Error, records::*};
use async_trait::async_trait;
use engram_domain::*;
use engram_memory::MemoryStorage;
use engram_runtime::{CoreResult, effect::ObservedResult};
use serde_json::{Value, json};

fn response_id(store: &HelixStore, key: &str) -> crate::error::Result<String> {
    // Actual operation receipts use canonical UUIDs; this reserved namespace cannot alias them.
    Ok(format!(
        "memory-write-response:{}",
        store.inner.partition.record_key(RECEIPT, key)?
    ))
}
fn response(store: &HelixStore, key: &str) -> crate::error::Result<Option<WriteMemoryResponse>> {
    let id = response_id(store, key)?;
    let Some(envelope) = store.get::<Value>(RECEIPT, &id)? else {
        return Ok(None);
    };
    if envelope.get("kind").and_then(Value::as_str) != Some("memory-write-response-v1")
        || envelope.get("key").and_then(Value::as_str) != Some(key)
    {
        return Err(Error::IncompatibleSchema);
    }
    let response: WriteMemoryResponse = decode(
        envelope
            .get("response")
            .cloned()
            .ok_or(Error::IncompatibleSchema)?,
    )?;
    store.check_scope(&response.record.scope)?;
    store.check_scope(&response.event.scope)?;
    Ok(Some(response))
}
#[async_trait]
impl MemoryStorage for HelixStore {
    async fn enumerate(&self, scope: &Scope) -> CoreResult<Vec<MemoryRecord>> {
        if !self.visible_scope(scope) {
            return Ok(vec![]);
        }
        self.all(MEMORY).map_err(core)
    }
    async fn idempotent_response(
        &self,
        scope: &Scope,
        key: &str,
    ) -> CoreResult<Option<WriteMemoryResponse>> {
        self.check_scope(scope).map_err(core)?;
        response(self, key).map_err(core)
    }
    async fn commit_write(
        &self,
        key: Option<&str>,
        record: MemoryRecord,
        event: MemoryEvent,
    ) -> ObservedResult<WriteMemoryResponse> {
        let actor = event.actor.id.to_string();
        let item = record.id.to_string();
        let work = |store: &HelixStore| {
            store.check_scope(&record.scope)?;
            store.check_scope(&event.scope)?;
            if event.memory_id.as_ref() != Some(&record.id)
                || event.kind != MemoryEventKind::Written
            {
                return Err(Error::InvalidConfig(
                    "memory write event association mismatch",
                ));
            }
            if let Some(key) = key {
                if let Some(mut prior) = response(store, key)? {
                    prior.deduplicated = Some(true);
                    return Ok(prior);
                }
            }
            if store
                .get::<Value>(MEMORY, &record.id.to_string())?
                .is_some()
                || store.get::<Value>(EVENT, &event.id.to_string())?.is_some()
            {
                return Err(Error::InvalidConfig("memory write identity already exists"));
            }
            store.put(MEMORY, &record)?;
            store.put(EVENT, &event)?;
            let written = WriteMemoryResponse {
                record,
                event,
                deduplicated: Some(false),
            };
            if let Some(key) = key {
                let id = response_id(store, key)?;
                store.put(RECEIPT,&json!({"id":id,"kind":"memory-write-response-v1","key":key,"response":written}))?;
            }
            Ok(written)
        };
        let (result, observation) = if self.reentrant {
            match self.effect() {
                Ok(Some((identity, frame))) => {
                    let result = self.atomic(|store| {
                        // Borrow the admitted semantic item before core constructs
                        // its record UUID; explicit unrelated calls remain strict.
                        if identity.actor != actor || identity.item != key.unwrap_or("memory-write")
                        {
                            return Err(Error::Contention);
                        }
                        work(store)
                    });
                    let observation = frame.observation(&identity).unwrap_or(EffectObservation {
                        identity,
                        step: None,
                        commit_id: None,
                        outcome: EffectOutcome::DefiniteFailure,
                        cleanup: CleanupCertainty::Unconfirmed,
                    });
                    (result, observation)
                }
                _ => {
                    let (result, mut observation) =
                        self.observed_atomic(&actor, &item, |_| Err(Error::Contention));
                    observation.cleanup = CleanupCertainty::Unconfirmed;
                    (result, observation)
                }
            }
        } else {
            self.observed_atomic(&actor, &item, work)
        };
        ObservedResult {
            result: result.map_err(core),
            observation,
        }
    }
    async fn remove(&self, id: &MemoryId, scope: &Scope) -> CoreResult<bool> {
        self.delete_memory_record(id, scope)
    }
}

use engram_runtime::effect::{
    CleanupCertainty, EffectFrame, EffectIdentity, EffectObservation, EffectOutcome,
};
use std::sync::Mutex;

/// Actual immutable ownership of the retained native session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeBinding {
    pub backend: String,
    pub mode: String,
    pub store: String,
    pub session: String,
    pub scope: Scope,
}
/// Reserves the complete selected partition across canonical orchestration.
pub struct NativeSession {
    store: HelixStore,
    binding: NativeBinding,
}
struct NativeOwned {
    // Uncommitted contributions are abandoned before the call reservation clears.
    step: Option<crate::transaction::StorageStep>,
    guard: Option<crate::session::CallGuard>,
    completed: Option<(EffectObservation, bool)>,
}
/// Opaque call owner. Its bound handles cannot outlive the exact admitted frame.
pub struct NativeCall {
    store: HelixStore,
    identity: EffectIdentity,
    frame: EffectFrame,
    owned: Mutex<NativeOwned>,
}
impl HelixStore {
    pub fn composition_scope(&self) -> &Scope {
        &self.inner.partition.scope
    }
    pub fn composition_embedding_space(&self) -> Option<&EmbeddingSpace> {
        self.inner.partition.embedding_space.as_ref()
    }
    pub fn native_session(&self, expected: &Scope) -> CoreResult<NativeSession> {
        self.check_scope(expected).map_err(core)?;
        let session = self.scope_session().map_err(core)?;
        let mut probe = session.store();
        // Read back the actual retained session identity; no storage step or send.
        let guard = probe
            .enter_effect_call("session-binding", "session-binding")
            .map_err(core)?;
        let (identity, _) = probe.effect().map_err(core)?.ok_or_else(native_mismatch)?;
        drop(guard);
        Ok(NativeSession {
            store: session.store(),
            binding: NativeBinding {
                backend: identity.backend,
                mode: identity.mode,
                store: identity.store,
                session: identity.session,
                scope: identity.scope,
            },
        })
    }
}
fn native_mismatch() -> engram_runtime::CoreError {
    engram_runtime::CoreError::Conflict {
        reason: "native composition ownership mismatch".into(),
    }
}
impl NativeSession {
    pub fn binding(&self) -> &NativeBinding {
        &self.binding
    }
    pub fn begin_call(&self, actor: &str, item: &str) -> ObservedResult<NativeCall> {
        let mut store = self.store.clone();
        let admitted: crate::error::Result<_> = (|| {
            store.admit_logical_work()?;
            let guard = store.enter_effect_call(actor, item)?;
            let (identity, frame) = store.effect()?.ok_or(Error::Contention)?;
            let step = store.begin_step()?;
            store.step_id = Some(step.id.clone());
            Ok((guard, identity, frame, step))
        })();
        match admitted {
            Ok((guard, identity, frame, step)) => {
                let observation = frame
                    .observation(&identity)
                    .expect("exact admitted native frame");
                ObservedResult {
                    result: Ok(NativeCall {
                        store,
                        identity,
                        frame,
                        owned: Mutex::new(NativeOwned {
                            step: Some(step),
                            guard,
                            completed: None,
                        }),
                    }),
                    observation,
                }
            }
            Err(error) => {
                let identity = EffectIdentity {
                    actor: actor.into(),
                    item: item.into(),
                    call: uuid::Uuid::new_v4().to_string(),
                    backend: self.binding.backend.clone(),
                    mode: self.binding.mode.clone(),
                    store: self.binding.store.clone(),
                    session: self.binding.session.clone(),
                    scope: self.binding.scope.clone(),
                };
                let observation = EffectObservation {
                    identity,
                    step: None,
                    commit_id: None,
                    outcome: error.prior_unknown_operation().map_or(
                        EffectOutcome::DefiniteFailure,
                        |id| EffectOutcome::BlockedByPriorUnknown {
                            operation_id: id.into(),
                        },
                    ),
                    cleanup: if error.prior_unknown_operation().is_some()
                        || matches!(
                            error,
                            Error::CleanupUnconfirmed | Error::CleanupAfterFailure { .. }
                        ) {
                        CleanupCertainty::Unconfirmed
                    } else {
                        CleanupCertainty::Confirmed
                    },
                };
                ObservedResult {
                    result: Err(core(error)),
                    observation,
                }
            }
        }
    }
}
impl NativeCall {
    pub fn store(&self) -> HelixStore {
        self.store.clone()
    }
    pub fn identity(&self) -> &EffectIdentity {
        &self.identity
    }
    /// Complete the native contribution before the typed port returns its result.
    /// This preserves definite commit errors as ordinary per-item failures.
    /// Reject stale/unrelated call handles before any delegated read or write.
    pub fn validate_live(&self) -> CoreResult<()> {
        let owned = self.owned.lock().map_err(|_| native_mismatch())?;
        if owned.completed.is_some() {
            return Err(native_mismatch());
        }
        let step = owned.step.as_ref().ok_or_else(native_mismatch)?;
        let current = self
            .store
            .effect()
            .map_err(core)?
            .ok_or_else(native_mismatch)?;
        if current.0 != self.identity || self.store.step_id.as_ref() != Some(&step.id) {
            return Err(native_mismatch());
        }
        current.1.observation(&self.identity)?;
        let pending = self
            .store
            .session
            .as_ref()
            .ok_or_else(native_mismatch)?
            .pending
            .lock()
            .map_err(|_| native_mismatch())?;
        if pending.as_ref().is_none_or(|p| p.id != step.id) {
            return Err(native_mismatch());
        }
        Ok(())
    }
    pub fn complete<T>(&self, result: CoreResult<T>) -> CoreResult<T> {
        self.validate_live()?;
        let mut owned = self.owned.lock().map_err(|_| native_mismatch())?;
        if owned.completed.is_some() {
            return Err(native_mismatch());
        }
        let step = owned.step.take().ok_or_else(native_mismatch)?;
        let has_mutations = self
            .store
            .session
            .as_ref()
            .ok_or_else(native_mismatch)?
            .pending
            .lock()
            .map_err(|_| native_mismatch())?
            .as_ref()
            .filter(|pending| pending.id == step.id)
            .ok_or_else(native_mismatch)?
            .mutations
            .len()
            > 0;
        let result = if result.is_ok() && has_mutations {
            match step.commit() {
                Ok(_) => result,
                Err(error) => Err(core(error)),
            }
        } else {
            let observation = self.frame.observation(&self.identity)?;
            if self.frame.is_pending(&self.identity)? && observation.step.is_some() {
                self.frame.finish(
                    &self.identity,
                    &step.id,
                    if result.is_err() {
                        EffectOutcome::DefiniteFailure
                    } else {
                        EffectOutcome::Acknowledged
                    },
                    CleanupCertainty::Confirmed,
                )?;
            }
            drop(step);
            result
        };
        let mut observation = self.frame.observation(&self.identity)?;
        if matches!(observation.outcome, EffectOutcome::Staged) && observation.step.is_none() {
            observation.outcome = if result.is_err() {
                EffectOutcome::DefiniteFailure
            } else {
                EffectOutcome::Acknowledged
            };
        }
        owned.completed = Some((observation, result.is_err()));
        drop(owned.guard.take());
        result
    }
    pub fn observation(&self, failed: bool) -> CoreResult<EffectObservation> {
        let owned = self.owned.lock().map_err(|_| native_mismatch())?;
        let (observation, actual_failed) = owned.completed.as_ref().ok_or_else(native_mismatch)?;
        if failed != *actual_failed {
            return Err(native_mismatch());
        }
        Ok(observation.clone())
    }
}

/// Explicit opt-in native proof controls. This module is absent without the
/// nondefault feature; it never replaces the retained physical engine.
#[cfg(feature = "test-controls")]
pub mod test_controls {
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
    }
    #[derive(Default, Clone, Debug)]
    pub struct Snapshot {
        pub all_sends: usize,
        pub reconciliation_reads: usize,
        pub mutation_attempts: Vec<String>,
        pub landed_commits: Vec<String>,
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
    }
    pub struct TestControl {
        actual: Arc<dyn Transport>,
        state: Mutex<State>,
    }
    impl TestControl {
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
        fn execute(&self, operation: &Operation) -> crate::error::Result<Value> {
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
            let response = self.actual.execute(operation)?;
            if operation.mutates {
                self.state
                    .lock()
                    .map_err(|_| Error::Storage)?
                    .snapshot
                    .landed_commits
                    .push(operation.id.clone());
            }
            if matches!(fault, Some(Fault::LostAck)) {
                self.state
                    .lock()
                    .map_err(|_| Error::Storage)?
                    .injected_pending
                    .insert(operation.id.clone());
                return Err(Error::OutcomeUnknown {
                    operation_id: operation.id.clone(),
                });
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
}

/// Reuse the adapter's fixed safe typed classification at recipe construction.
pub fn native_core_error(error: crate::error::Error) -> engram_runtime::CoreError {
    core(error)
}
