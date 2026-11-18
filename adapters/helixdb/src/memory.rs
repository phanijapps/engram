//! Memory persistence; lifecycle orchestration remains in the canonical service.
use crate::{HelixStore, records::*};
use async_trait::async_trait;
use engram_domain::*;
use engram_memory::{MemoryEventRepository, MemoryRepository};
use engram_runtime::{CoreError, CoreResult};
#[async_trait]
impl MemoryRepository for HelixStore {
    async fn put_memory(&self, record: MemoryRecord) -> CoreResult<MemoryRecord> {
        self.check_scope(&record.scope).map_err(core)?;
        self.put(MEMORY, &record).map_err(core)
    }
    async fn get_memory(&self, id: &MemoryId, scope: &Scope) -> CoreResult<Option<MemoryRecord>> {
        if !self.visible_scope(scope) {
            return Ok(None);
        }
        self.get(MEMORY, &id.to_string()).map_err(core)
    }
    async fn append_event(&self, event: MemoryEvent) -> CoreResult<MemoryEvent> {
        self.check_scope(&event.scope).map_err(core)?;
        self.atomic(|s| {
            if let Some(existing) = s.get::<MemoryEvent>(EVENT, &event.id.to_string())? {
                if existing != event {
                    return Err(crate::error::Error::InvalidConfig(
                        "event identity is immutable",
                    ));
                }
                return Ok(existing);
            }
            s.put(EVENT, &event)
        })
        .map_err(core)
    }
    async fn update_memory_status(
        &self,
        id: &MemoryId,
        scope: &Scope,
        status: MemoryStatus,
    ) -> CoreResult<MemoryRecord> {
        if !self.visible_scope(scope) {
            return Err(missing("memory", id));
        }
        self.atomic(|s| {
            let Some(mut record) = s.get::<MemoryRecord>(MEMORY, &id.to_string())? else {
                return Ok(None);
            };
            record.status = status;
            record.updated_at = Some(chrono::Utc::now());
            Ok(Some(s.put(MEMORY, &record)?))
        })
        .map_err(core)?
        .ok_or_else(|| missing("memory", id))
    }
    async fn list_memories_paged(
        &self,
        scope: &Scope,
        after: Option<&Cursor>,
        limit: usize,
    ) -> CoreResult<Page<MemoryRecord>> {
        if limit == 0 || limit > 1000 {
            return Err(CoreError::InvalidRequest {
                reason: "memory page limit must be 1..=1000".into(),
            });
        }
        if !self.visible_scope(scope) {
            return Ok(Page::last(vec![]));
        }
        let prefix = format!("{}:", self.inner.partition.discriminator().map_err(core)?);
        let after = after
            .map(|c| {
                c.as_str()
                    .strip_prefix(&prefix)
                    .ok_or(CoreError::InvalidRequest {
                        reason: "cursor belongs to another partition".into(),
                    })
            })
            .transpose()?;
        let mut rows = self
            .scan_window(MEMORY, None, after, Some((0, limit + 1)))
            .map_err(core)?
            .into_iter()
            .map(decode::<MemoryRecord>)
            .collect::<crate::error::Result<Vec<_>>>()
            .map_err(core)?;
        let more = rows.len() > limit;
        rows.truncate(limit);
        let next = if more {
            rows.last()
                .map(|r| Cursor::new(format!("{prefix}{}", r.id)))
        } else {
            None
        };
        Ok(Page::new(rows, next))
    }
}
#[async_trait]
impl MemoryEventRepository for HelixStore {
    async fn get_event(&self, id: &EventId, scope: &Scope) -> CoreResult<Option<MemoryEvent>> {
        if !self.visible_scope(scope) {
            return Ok(None);
        }
        self.get(EVENT, &id.to_string()).map_err(core)
    }
    async fn list_events_for_memory(
        &self,
        id: &MemoryId,
        scope: &Scope,
    ) -> CoreResult<Vec<MemoryEvent>> {
        if !self.visible_scope(scope) {
            return Ok(vec![]);
        }
        let mut rows = self
            .filtered::<MemoryEvent>(EVENT, "memoryId", &id.to_string())
            .map_err(core)?;
        rows.sort_by_key(|e| (e.recorded_at, e.id.to_string()));
        Ok(rows)
    }
    async fn list_events_for_scope(&self, scope: &Scope) -> CoreResult<Vec<MemoryEvent>> {
        if !self.visible_scope(scope) {
            return Ok(vec![]);
        }
        let mut rows = self.all::<MemoryEvent>(EVENT).map_err(core)?;
        rows.sort_by_key(|e| (e.recorded_at, e.id.to_string()));
        Ok(rows)
    }
}

impl HelixStore {
    /// Hard-delete only the governed memory record; retained lifecycle events remain immutable.
    pub fn delete_memory_record(&self, id: &MemoryId, scope: &Scope) -> CoreResult<bool> {
        if !self.visible_scope(scope) {
            return Ok(false);
        }
        self.remove(MEMORY, &id.to_string()).map_err(core)
    }
}

impl HelixStore {
    /// Compose the one governed implementation with collision-safe adapter IDs.
    pub fn memory_service(
        &self,
        authorizer: std::sync::Arc<dyn engram_runtime::PolicyAuthorizer>,
    ) -> engram_memory::GovernedMemoryService {
        engram_memory::GovernedMemoryService::new(
            std::sync::Arc::new(self.clone()),
            authorizer,
            std::sync::Arc::new(MemoryClock),
            std::sync::Arc::new(MemoryIds),
        )
    }
}
struct MemoryClock;
impl engram_runtime::Clock for MemoryClock {
    fn now(&self) -> Timestamp {
        chrono::Utc::now()
    }
}
struct MemoryIds;
impl engram_runtime::IdGenerator for MemoryIds {
    fn new_id(&self, _kind: &'static str) -> Id {
        uuid::Uuid::new_v4().to_string().into()
    }
}

#[cfg(all(test, feature = "embedded", feature = "service"))]
mod observation_contracts {
    use super::*;
    use crate::{
        StoreInner,
        error::{Error, Result},
        test_data_fixture::*,
        transport::{Operation, Transport},
    };
    use engram_memory::MemoryStorage;
    use engram_runtime::effect::{CleanupCertainty, EffectOutcome};
    use futures::executor::block_on;
    use serde_json::Value;
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    };
    struct Control {
        actual: Arc<dyn Transport>,
        armed: AtomicBool,
        sends: AtomicUsize,
        definite: bool,
        cleanup: bool,
    }
    impl Transport for Control {
        fn execute(&self, operation: &Operation) -> Result<Value> {
            let armed = operation.mutates && self.armed.swap(false, Ordering::SeqCst);
            if operation.mutates {
                self.sends.fetch_add(1, Ordering::SeqCst);
            }
            if armed && self.definite {
                return Err(Error::Storage);
            }
            let response = self.actual.execute(operation)?;
            if armed {
                let error = Error::OutcomeUnknown {
                    operation_id: operation.id.clone(),
                };
                return Err(if self.cleanup {
                    Error::CleanupAfterFailure {
                        cause: Box::new(Error::CleanupAfterFailure {
                            cause: Box::new(error),
                        }),
                    }
                } else {
                    error
                });
            }
            Ok(response)
        }
        fn close(&self) -> Result<()> {
            self.actual.close()
        }
        // This control owns the lost acknowledgement above the actual transport.
        // The real transport received success and remains available for exact receipt reads.
        fn reconcile_read(&self, o: &Operation) -> Result<Value> {
            self.actual.execute(o)
        }
        fn confirm_operation(&self, _id: &str) -> Result<()> {
            Ok(())
        }
        fn register_cleanup(&self, lease: crate::ownership::ScopeLease) -> Result<()> {
            self.actual.register_cleanup(lease)
        }
    }
    fn wrap(actual: &HelixStore, control: Arc<Control>) -> HelixStore {
        HelixStore {
            inner: Arc::new(StoreInner {
                transport: control,
                mode: actual.inner.mode,
                partition: actual.inner.partition.clone(),
                lease: actual.inner.lease.clone(),
                reservation: AtomicBool::new(false),
                cleanup_only: AtomicBool::new(false),
                uncertain_receipts: Mutex::new(std::collections::BTreeMap::new()),
            }),
            session: None,
            reentrant: false,
            step_id: None,
        }
    }
    fn exercise(mode: Mode, cleanup: bool, definite: bool) {
        let f = Fixture::new(mode);
        let control = Arc::new(Control {
            actual: f.store.inner.transport.clone(),
            armed: AtomicBool::new(true),
            sends: AtomicUsize::new(0),
            definite,
            cleanup,
        });
        let store = wrap(&f.store, control.clone());
        let record = memory("observed-memory");
        let event: MemoryEvent = make(
            "observed-event",
            serde_json::json!({"kind":"written","actor":{"id":"actor","kind":"agent"},"memoryId":record.id,"payload":{},"occurredAt":"2026-11-17T00:00:00Z","recordedAt":"2026-11-17T00:00:00Z"}),
        );
        let observed =
            block_on(store.commit_write(Some("observed-key"), record.clone(), event.clone()));
        let error = observed.result.unwrap_err();
        assert_eq!(control.sends.load(Ordering::SeqCst), 1);
        if definite {
            assert_eq!(observed.observation.outcome, EffectOutcome::DefiniteFailure);
            assert!(store.inner.uncertain_receipts.lock().unwrap().is_empty());
            assert!(!error.to_redacted().to_string().contains("outcome unknown"));
            assert!(
                block_on(f.store.get_memory(&record.id, &scope()))
                    .unwrap()
                    .is_none()
            );
            let retry = block_on(store.commit_write(Some("observed-key"), record, event));
            assert!(retry.result.is_ok());
            assert_eq!(control.sends.load(Ordering::SeqCst), 2);
        } else {
            let id = observed.observation.commit_id.clone().unwrap();
            assert_eq!(
                observed.observation.outcome,
                EffectOutcome::Unknown {
                    operation_id: id.clone()
                }
            );
            assert_eq!(
                observed.observation.cleanup,
                if cleanup {
                    CleanupCertainty::Unconfirmed
                } else {
                    CleanupCertainty::Confirmed
                }
            );
            assert!(error.to_redacted().to_string().contains(&id));
            assert!(
                store
                    .inner
                    .uncertain_receipts
                    .lock()
                    .unwrap()
                    .contains_key(&id)
            );
            let blocked = block_on(store.commit_write(Some("later-key"), record.clone(), event));
            assert_eq!(
                blocked.observation.outcome,
                EffectOutcome::BlockedByPriorUnknown {
                    operation_id: id.clone()
                }
            );
            assert_eq!(blocked.observation.commit_id, None);
            assert_eq!(blocked.observation.step, None);
            assert!(
                blocked
                    .result
                    .unwrap_err()
                    .to_redacted()
                    .to_string()
                    .contains(&id)
            );
            assert_eq!(
                control.sends.load(Ordering::SeqCst),
                1,
                "blocked admission sends nothing"
            );
            assert_eq!(
                store.inner.uncertain_receipts.lock().unwrap().len(),
                1,
                "no replacement UUID"
            );
            let receipt = store.reconcile_operation(&id).unwrap();
            assert_eq!(receipt["id"], id);
            assert_eq!(receipt["mutation_count"], 3);
            assert_eq!(
                control.sends.load(Ordering::SeqCst),
                1,
                "reconciliation never replays"
            );
            assert_eq!(
                block_on(f.store.get_memory(&record.id, &scope())).unwrap(),
                Some(record)
            );
            assert_eq!(
                block_on(f.store.list_events_for_scope(&scope()))
                    .unwrap()
                    .len(),
                1
            );
        }
        store.close().unwrap();
        drop(store);
        drop(control);
        drop(f);
    }
    fn staged_exact(mode: Mode) {
        let f = Fixture::new(mode);
        let control = Arc::new(Control {
            actual: f.store.inner.transport.clone(),
            armed: AtomicBool::new(false),
            sends: AtomicUsize::new(0),
            definite: false,
            cleanup: true,
        });
        let store = wrap(&f.store, control.clone());
        let session = store.scope_session().unwrap();
        let bound = session.store();
        let contribution = |name: &str| {
            let record = memory(name);
            let event: MemoryEvent = make(
                &format!("{name}-event"),
                serde_json::json!({"kind":"written","actor":{"id":"actor","kind":"agent"},"memoryId":record.id,"payload":{},"occurredAt":"2026-11-17T00:00:00Z","recordedAt":"2026-11-17T00:00:00Z"}),
            );
            (record, event)
        };
        let step = bound.begin_step().unwrap();
        let frames = step.frames.clone();
        let step_id = step.id.clone();
        for name in ["outer-one", "outer-two"] {
            let (record, event) = contribution(name);
            let observed = block_on(bound.commit_write(Some(name), record.clone(), event));
            assert!(observed.result.is_ok());
            assert_eq!(observed.observation.outcome, EffectOutcome::Staged);
            assert_eq!(observed.observation.step.as_deref(), Some(step_id.as_str()));
            assert_eq!(observed.observation.commit_id, None);
            assert!(
                block_on(f.store.get_memory(&record.id, &scope()))
                    .unwrap()
                    .is_none()
            );
        }
        let mut wrong_executed = false;
        let (nested, nested_observation) =
            bound.observed_atomic("actor", "nested-item", |nested| {
                nested.atomic(|_| Ok(()))?; // Generic storage calls borrow the current exact frame.
                let current = nested.effect()?.unwrap().0;
                let (positive, borrowed) =
                    nested.observed_atomic("actor", "nested-item", |_| Ok(()));
                assert!(positive.is_ok());
                assert_eq!(borrowed.identity.call, current.call);
                for (actor, item) in [("wrong-actor", "nested-item"), ("actor", "wrong-item")] {
                    let (result, _) = nested.observed_atomic(actor, item, |_| {
                        wrong_executed = true;
                        Ok(())
                    });
                    assert!(result.is_err());
                }
                Ok(())
            });
        assert!(nested.is_ok());
        assert!(!wrong_executed);
        assert_eq!(nested_observation.outcome, EffectOutcome::Staged);
        assert_eq!(control.sends.load(Ordering::SeqCst), 0);
        let commit = step.commit().unwrap();
        for (identity, frame) in frames.lock().unwrap().iter() {
            let final_observation = frame.observation(identity).unwrap();
            assert_eq!(final_observation.outcome, EffectOutcome::Acknowledged);
            assert_eq!(final_observation.step.as_deref(), Some(step_id.as_str()));
            assert_eq!(
                final_observation.commit_id.as_deref(),
                Some(commit.as_str())
            );
        }
        assert_eq!(
            store.operation_receipt(&commit).unwrap().unwrap()["mutation_count"],
            6
        );
        for name in ["outer-one", "outer-two"] {
            assert!(
                block_on(f.store.get_memory(&name.into(), &scope()))
                    .unwrap()
                    .is_some()
            );
            assert_eq!(
                block_on(f.store.list_events_for_memory(&name.into(), &scope()))
                    .unwrap()
                    .len(),
                1
            );
            assert!(
                block_on(f.store.idempotent_response(&scope(), name))
                    .unwrap()
                    .is_some()
            );
        }
        let abandoned = bound.begin_step().unwrap();
        let cancelled_frames = abandoned.frames.clone();
        let (record, event) = contribution("outer-abandoned");
        assert_eq!(
            block_on(bound.commit_write(Some("outer-abandoned"), record.clone(), event))
                .observation
                .outcome,
            EffectOutcome::Staged
        );
        drop(abandoned);
        for (identity, frame) in cancelled_frames.lock().unwrap().iter() {
            let cancelled = frame.observation(identity).unwrap();
            assert_eq!(cancelled.outcome, EffectOutcome::Cancelled);
            assert_eq!(cancelled.commit_id, None);
        }
        assert!(
            block_on(f.store.get_memory(&record.id, &scope()))
                .unwrap()
                .is_none()
        );
        assert!(
            block_on(f.store.idempotent_response(&scope(), "outer-abandoned"))
                .unwrap()
                .is_none()
        );
        let unknown_step = bound.begin_step().unwrap();
        let unknown_frames = unknown_step.frames.clone();
        let unknown_step_id = unknown_step.id.clone();
        let (record, event) = contribution("outer-unknown");
        assert_eq!(
            block_on(bound.commit_write(Some("outer-unknown"), record.clone(), event.clone()))
                .observation
                .outcome,
            EffectOutcome::Staged
        );
        control.armed.store(true, Ordering::SeqCst);
        let error = unknown_step.commit().unwrap_err();
        let original = error.outcome_unknown_operation().unwrap().to_owned();
        for (identity, frame) in unknown_frames.lock().unwrap().iter() {
            let observed = frame.observation(identity).unwrap();
            assert_eq!(
                observed.outcome,
                EffectOutcome::Unknown {
                    operation_id: original.clone()
                }
            );
            assert_eq!(observed.step.as_deref(), Some(unknown_step_id.as_str()));
            assert_eq!(observed.commit_id.as_deref(), Some(original.as_str()));
        }
        let sends = control.sends.load(Ordering::SeqCst);
        let blocked = block_on(bound.commit_write(Some("must-not-send"), record.clone(), event));
        assert_eq!(
            blocked.observation.outcome,
            EffectOutcome::BlockedByPriorUnknown {
                operation_id: original.clone()
            }
        );
        assert_eq!(blocked.observation.commit_id, None);
        assert_eq!(control.sends.load(Ordering::SeqCst), sends);
        assert_eq!(store.inner.uncertain_receipts.lock().unwrap().len(), 1);
        assert_eq!(
            store.reconcile_operation(&original).unwrap()["id"],
            original
        );
        assert!(
            block_on(f.store.get_memory(&record.id, &scope()))
                .unwrap()
                .is_some()
        );
        println!(
            "STAGED_CAUSALITY mode={mode:?} acknowledged_commit={commit} unknown_original={original} native_sends={sends} explicit_mismatch_effects=0"
        );
        drop(bound);
        drop(session);
        store.close().unwrap();
        drop(store);
        drop(control);
        drop(f);
    }
    #[test]
    fn actual_embedded_staged_frames_commit_cancel_unknown_and_explicit_identity() {
        staged_exact(Mode::Embedded);
    }
    #[test]
    fn actual_service_staged_frames_commit_cancel_unknown_and_explicit_identity() {
        staged_exact(Mode::Service);
    }
    fn direct_knowledge(mode: Mode) {
        use engram_knowledge::KnowledgeRepository;
        let f = Fixture::new(mode);
        let control = Arc::new(Control {
            actual: f.store.inner.transport.clone(),
            armed: AtomicBool::new(true),
            sends: AtomicUsize::new(0),
            definite: false,
            cleanup: true,
        });
        let store = wrap(&f.store, control.clone());
        let error = block_on(store.put_entity(entity("actual-knowledge", &[]))).unwrap_err();
        let id = store
            .inner
            .uncertain_receipts
            .lock()
            .unwrap()
            .keys()
            .next()
            .unwrap()
            .clone();
        let public = error.to_redacted().to_string();
        assert!(public.contains(&id));
        assert!(public.contains("outcome unknown"));
        assert!(!public.contains("fingerprint"));
        assert!(!public.contains("owner"));
        assert_eq!(control.sends.load(Ordering::SeqCst), 1);
        let receipt = store.reconcile_operation(&id).unwrap();
        assert_eq!(receipt["id"], id);
        store.close().unwrap();
        drop(store);
        drop(control);
        drop(f);
    }
    #[test]
    fn actual_embedded_exact_unknown_recursive_cleanup_blocked_and_definite() {
        for (cleanup, definite) in [(false, false), (true, false), (false, true)] {
            exercise(Mode::Embedded, cleanup, definite);
        }
        direct_knowledge(Mode::Embedded);
    }
    #[test]
    fn actual_service_exact_unknown_recursive_cleanup_blocked_and_definite() {
        for (cleanup, definite) in [(false, false), (true, false), (false, true)] {
            exercise(Mode::Service, cleanup, definite);
        }
        direct_knowledge(Mode::Service);
    }
}
