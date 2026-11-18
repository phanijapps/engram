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
    /// Retained access authority, independent of any recipe configuration assertion.
    pub fn composition_read_only(&self) -> bool {
        self.inner.lease.is_none()
    }
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
                #[cfg(feature = "test-controls")]
                self.store
                    .inner
                    .transport
                    .fixture_observed(observation.clone());
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
#[path = "test_controls.rs"]
pub mod test_controls;

/// Reuse the adapter's fixed safe typed classification at recipe construction.
pub fn native_core_error(error: crate::error::Error) -> engram_runtime::CoreError {
    core(error)
}
