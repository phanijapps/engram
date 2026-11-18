//! Explicit complete-scope operation reservation, shared only by session-bound handles.
use crate::{
    HelixStore, StoreInner,
    error::{Error, Result},
    transaction::StepState,
};
use engram_runtime::effect::{
    CleanupCertainty, EffectFrame, EffectIdentity, EffectObservation, EffectOutcome,
};
use std::sync::{Arc, Mutex, atomic::Ordering};

pub(crate) struct SessionState {
    owner: Arc<StoreInner>,
    pub(crate) pending: Mutex<Option<StepState>>,
    call: std::sync::atomic::AtomicBool,
    id: String,
    effect: Mutex<Option<(EffectIdentity, EffectFrame)>>,
    #[cfg(test)]
    pub(crate) stage_pause: Mutex<Option<StagePause>>,
}
impl Drop for SessionState {
    fn drop(&mut self) {
        self.owner.reservation.store(false, Ordering::Release);
    }
}
/// Keep this owner alive across canonical orchestration; its store clones are reentrant.
pub struct ScopeSession {
    store: HelixStore,
}
impl ScopeSession {
    pub fn store(&self) -> HelixStore {
        self.store.clone()
    }
}
impl HelixStore {
    pub(crate) fn admit_logical_work(&self) -> Result<()> {
        if self.inner.cleanup_only.load(Ordering::Acquire) {
            if let Some(id) = self
                .inner
                .uncertain_receipts
                .lock()
                .map_err(|_| Error::CleanupUnconfirmed)?
                .keys()
                .next()
            {
                return Err(Error::BlockedByPriorUnknown {
                    operation_id: id.clone(),
                });
            }
            return Err(Error::CleanupUnconfirmed);
        }
        Ok(())
    }
    pub fn scope_session(&self) -> Result<ScopeSession> {
        self.admit_logical_work()?;
        self.inner.lease.as_ref().ok_or(Error::ReadOnly)?;
        if self.session.is_some() {
            return Ok(ScopeSession {
                store: self.clone(),
            });
        }
        self.inner
            .reservation
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| Error::Contention)?;
        Ok(ScopeSession {
            store: HelixStore {
                inner: self.inner.clone(),
                session: Some(Arc::new(SessionState {
                    owner: self.inner.clone(),
                    pending: Mutex::new(None),
                    call: std::sync::atomic::AtomicBool::new(false),
                    id: uuid::Uuid::new_v4().to_string(),
                    effect: Mutex::new(None),
                    #[cfg(test)]
                    stage_pause: Mutex::new(None),
                })),
                reentrant: false,
                step_id: None,
            },
        })
    }
    pub(crate) fn enter_call(&mut self) -> Result<Option<CallGuard>> {
        if self.reentrant {
            self.validate_borrowed_effect()?;
            return Ok(None);
        }
        self.enter_effect_call("storage", "storage")
    }
    pub(crate) fn enter_effect_call(
        &mut self,
        actor: &str,
        item: &str,
    ) -> Result<Option<CallGuard>> {
        if self.reentrant {
            let (identity, _) = self.validate_borrowed_effect()?;
            if identity.actor != actor || identity.item != item {
                return Err(Error::Contention);
            }
            return Ok(None);
        }
        let state = self.session.as_ref().ok_or(Error::Contention)?;
        state
            .call
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| Error::Contention)?;
        let identity = self.effect_identity(actor, item);
        let frame = EffectFrame::new(identity.clone());
        *state.effect.lock().map_err(|_| Error::Storage)? = Some((identity, frame));
        self.reentrant = true;
        Ok(Some(CallGuard(state.clone())))
    }
    fn validate_borrowed_effect(&self) -> Result<(EffectIdentity, EffectFrame)> {
        let (identity, frame) = self.effect()?.ok_or(Error::Contention)?;
        let session = self.session.as_ref().ok_or(Error::Contention)?;
        if identity.session != session.id
            || identity.backend != "helixdb"
            || identity.mode != self.inner.mode
            || identity.store != self.inner.partition.logical_store
            || identity.scope != self.inner.partition.scope
        {
            return Err(Error::Contention);
        }
        frame
            .observation(&identity)
            .map_err(|_| Error::Contention)?;
        Ok((identity, frame))
    }
    pub(crate) fn retain_step_effect(
        &self,
        step: &str,
        frames: &crate::transaction::StepFrames,
    ) -> Result<()> {
        if let Some((identity, frame)) = self.effect()? {
            frame
                .bind_step(&identity, step)
                .map_err(|_| Error::Contention)?;
            let mut frames = frames.lock().map_err(|_| Error::Storage)?;
            if !frames.iter().any(|(prior, _)| prior.call == identity.call) {
                frames.push((identity, frame));
            }
        }
        Ok(())
    }
    fn effect_identity(&self, actor: &str, item: &str) -> EffectIdentity {
        EffectIdentity {
            actor: actor.into(),
            item: item.into(),
            call: uuid::Uuid::new_v4().to_string(),
            session: self
                .session
                .as_ref()
                .map(|s| s.id.clone())
                .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
            backend: "helixdb".into(),
            mode: self.inner.mode.into(),
            store: self.inner.partition.logical_store.clone(),
            scope: self.inner.partition.scope.clone(),
        }
    }
    pub(crate) fn effect(&self) -> Result<Option<(EffectIdentity, EffectFrame)>> {
        self.session
            .as_ref()
            .map(|s| {
                s.effect
                    .lock()
                    .map(|f| f.clone())
                    .map_err(|_| Error::Storage)
            })
            .transpose()
            .map(Option::flatten)
    }
    pub(crate) fn observed_atomic<T>(
        &self,
        actor: &str,
        item: &str,
        work: impl FnOnce(&HelixStore) -> Result<T>,
    ) -> (Result<T>, EffectObservation) {
        let session = match self.scope_session() {
            Ok(session) => session,
            Err(error) => {
                let identity = self.effect_identity(actor, item);
                let frame = EffectFrame::new(identity.clone());
                if let Some(prior) = error.prior_unknown_operation() {
                    let _ = frame.blocked(&identity, prior, CleanupCertainty::Unconfirmed);
                }
                let mut observation = frame.observation(&identity).expect("fresh exact identity");
                if error.prior_unknown_operation().is_none() {
                    observation.outcome = EffectOutcome::DefiniteFailure;
                }
                if matches!(
                    error,
                    Error::CleanupUnconfirmed | Error::CleanupAfterFailure { .. }
                ) {
                    observation.cleanup = CleanupCertainty::Unconfirmed;
                }
                return (Err(error), observation);
            }
        };
        let mut store = session.store();
        let guard = match store.enter_effect_call(actor, item) {
            Ok(guard) => guard,
            Err(error) => {
                let identity = store.effect_identity(actor, item);
                let frame = EffectFrame::new(identity.clone());
                let mut observation = frame.observation(&identity).expect("fresh exact identity");
                observation.outcome = EffectOutcome::DefiniteFailure;
                return (Err(error), observation);
            }
        };
        let evidence = store.effect();
        let result = store.atomic(work);
        let mut observation = evidence
            .ok()
            .flatten()
            .and_then(|(id, frame)| frame.observation(&id).ok())
            .expect("admitted call owns exact effect frame");
        if let Err(error) = &result {
            if matches!(observation.outcome, EffectOutcome::Staged) {
                observation.outcome = EffectOutcome::DefiniteFailure;
                if error.outcome_unknown_operation().is_some()
                    || matches!(
                        error,
                        Error::CleanupUnconfirmed | Error::CleanupAfterFailure { .. }
                    )
                {
                    // An unbound cause cannot be attributed to the current call's commit.
                    observation.cleanup = CleanupCertainty::Unconfirmed;
                }
            }
        }
        drop(guard);
        (result, observation)
    }
    /// Atomically execute storage-only work. An explicit outer step owns its commit.
    pub(crate) fn atomic<T>(&self, work: impl FnOnce(&HelixStore) -> Result<T>) -> Result<T> {
        let session = self.scope_session()?;
        let mut store = session.store();
        let _call = store.enter_call()?;
        let snapshot = store
            .session
            .as_ref()
            .ok_or(Error::Storage)?
            .pending
            .lock()
            .map_err(|_| Error::Storage)?
            .as_ref()
            .map(|step| {
                (
                    step.id.clone(),
                    step.mutations.clone(),
                    step.fences.clone(),
                    step.frames.clone(),
                )
            });
        if let Some((id, snapshot, fences, frames)) = snapshot {
            if store
                .step_id
                .as_ref()
                .is_some_and(|expected| expected != &id)
            {
                return Err(Error::Contention);
            }
            store.step_id = Some(id.clone());
            store.retain_step_effect(&id, &frames)?;
            let result = work(&store);
            if result.is_err() {
                let mut pending = store
                    .session
                    .as_ref()
                    .ok_or(Error::Storage)?
                    .pending
                    .lock()
                    .map_err(|_| Error::Storage)?;
                let current = pending
                    .as_mut()
                    .filter(|step| step.id == id)
                    .ok_or(Error::Contention)?;
                current.mutations = snapshot;
                current.fences = fences;
                if let Some((identity, frame)) = store.effect()? {
                    if frame
                        .is_pending(&identity)
                        .map_err(|_| Error::CleanupUnconfirmed)?
                    {
                        frame
                            .finish(
                                &identity,
                                &id,
                                EffectOutcome::DefiniteFailure,
                                CleanupCertainty::Confirmed,
                            )
                            .map_err(|_| Error::CleanupUnconfirmed)?;
                    }
                }
            }
            return result;
        }
        if store.step_id.is_some() {
            return Err(Error::Contention);
        }
        let step = store.begin_step()?;
        store.step_id = Some(step.id.clone());
        store.retain_step_effect(&step.id, &step.frames)?;
        let result = match work(&store) {
            Ok(result) => result,
            Err(error) => {
                if let Some((identity, frame)) = store.effect()? {
                    if frame
                        .is_pending(&identity)
                        .map_err(|_| Error::CleanupUnconfirmed)?
                    {
                        let cleanup = if error.outcome_unknown_operation().is_some()
                            || matches!(
                                error,
                                Error::CleanupUnconfirmed | Error::CleanupAfterFailure { .. }
                            ) {
                            CleanupCertainty::Unconfirmed
                        } else {
                            CleanupCertainty::Confirmed
                        };
                        frame
                            .finish(&identity, &step.id, EffectOutcome::DefiniteFailure, cleanup)
                            .map_err(|_| Error::CleanupUnconfirmed)?;
                    }
                }
                return Err(error);
            }
        };
        step.commit()?;
        Ok(result)
    }
}
/// Prevent concurrent mutation calls through intentionally shared session clones.
pub(crate) struct CallGuard(Arc<SessionState>);
impl Drop for CallGuard {
    fn drop(&mut self) {
        if let Ok(mut effect) = self.0.effect.lock() {
            *effect = None;
        }
        self.0.call.store(false, Ordering::Release);
    }
}

#[cfg(test)]
pub(crate) struct StagePause {
    pub(crate) entered: Arc<std::sync::Barrier>,
    pub(crate) release: Arc<std::sync::Barrier>,
}
