//! Explicit complete-scope operation reservation, shared only by session-bound handles.
use crate::{
    HelixStore, StoreInner,
    error::{Error, Result},
    transaction::StepState,
};
use std::sync::{Arc, Mutex, atomic::Ordering};

pub(crate) struct SessionState {
    owner: Arc<StoreInner>,
    pub(crate) pending: Mutex<Option<StepState>>,
    call: std::sync::atomic::AtomicBool,
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
                return Err(Error::OutcomeUnknown {
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
            return Ok(None);
        }
        let state = self.session.as_ref().ok_or(Error::Contention)?;
        state
            .call
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| Error::Contention)?;
        self.reentrant = true;
        Ok(Some(CallGuard(state.clone())))
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
            .map(|step| (step.id.clone(), step.mutations.clone(), step.fences.clone()));
        if let Some((id, snapshot, fences)) = snapshot {
            if store
                .step_id
                .as_ref()
                .is_some_and(|expected| expected != &id)
            {
                return Err(Error::Contention);
            }
            store.step_id = Some(id.clone());
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
            }
            return result;
        }
        if store.step_id.is_some() {
            return Err(Error::Contention);
        }
        let step = store.begin_step()?;
        store.step_id = Some(step.id.clone());
        let result = work(&store)?;
        step.commit()?;
        Ok(result)
    }
}
/// Prevent concurrent mutation calls through intentionally shared session clones.
pub(crate) struct CallGuard(Arc<SessionState>);
impl Drop for CallGuard {
    fn drop(&mut self) {
        self.0.call.store(false, Ordering::Release);
    }
}

#[cfg(test)]
pub(crate) struct StagePause {
    pub(crate) entered: Arc<std::sync::Barrier>,
    pub(crate) release: Arc<std::sync::Barrier>,
}
