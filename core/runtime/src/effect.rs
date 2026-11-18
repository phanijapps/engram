//! Internal, exact-call effect evidence. This is independent of public error shapes.
use crate::{CoreError, CoreResult};
use engram_domain::Scope;
use std::sync::{Arc, Mutex};

/// Immutable authority and causal identity supplied by the operation owner.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EffectIdentity {
    pub actor: String,
    pub call: String,
    pub session: String,
    pub backend: String,
    pub mode: String,
    pub store: String,
    pub scope: Scope,
    pub item: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EffectOutcome {
    Staged,
    Cancelled,
    Acknowledged,
    DefiniteFailure,
    Unknown { operation_id: String },
    BlockedByPriorUnknown { operation_id: String },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CleanupCertainty {
    Confirmed,
    Unconfirmed,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EffectObservation {
    pub identity: EffectIdentity,
    pub step: Option<String>,
    pub commit_id: Option<String>,
    pub outcome: EffectOutcome,
    pub cleanup: CleanupCertainty,
}
/// Result and evidence travel together; no subsequent global error lookup is required.
#[derive(Debug)]
pub struct ObservedResult<T> {
    pub result: CoreResult<T>,
    pub observation: EffectObservation,
}
#[derive(Clone, Debug)]
pub struct EffectFrame(Arc<Mutex<FrameState>>);
#[derive(Debug)]
struct FrameState {
    observation: EffectObservation,
    completed: bool,
}
fn mismatch() -> CoreError {
    CoreError::Conflict {
        reason: "effect observation identity mismatch".into(),
    }
}
impl EffectFrame {
    pub fn new(identity: EffectIdentity) -> Self {
        Self(Arc::new(Mutex::new(FrameState {
            observation: EffectObservation {
                identity,
                step: None,
                commit_id: None,
                outcome: EffectOutcome::Staged,
                cleanup: CleanupCertainty::Confirmed,
            },
            completed: false,
        })))
    }
    pub fn observation(&self, identity: &EffectIdentity) -> CoreResult<EffectObservation> {
        let state = self.0.lock().map_err(|_| mismatch())?;
        let current = &state.observation;
        if &current.identity != identity {
            return Err(mismatch());
        }
        Ok(current.clone())
    }
    /// Attach contribution identity before its first staged mutation.
    pub fn bind_step(&self, identity: &EffectIdentity, step: &str) -> CoreResult<()> {
        let mut state = self.0.lock().map_err(|_| mismatch())?;
        if state.completed
            || &state.observation.identity != identity
            || step.is_empty()
            || state
                .observation
                .step
                .as_deref()
                .is_some_and(|actual| actual != step)
        {
            return Err(mismatch());
        }
        state.observation.step = Some(step.into());
        Ok(())
    }
    /// Abandon uncommitted contributions. Completed native outcomes never change.
    pub fn cancel(&self, identity: &EffectIdentity, step: &str) -> CoreResult<()> {
        let mut state = self.0.lock().map_err(|_| mismatch())?;
        if &state.observation.identity != identity
            || state.observation.step.as_deref() != Some(step)
        {
            return Err(mismatch());
        }
        if !state.completed {
            state.observation.outcome = EffectOutcome::Cancelled;
            state.observation.commit_id = None;
            state.completed = true;
        }
        Ok(())
    }
    pub fn is_pending(&self, identity: &EffectIdentity) -> CoreResult<bool> {
        let state = self.0.lock().map_err(|_| mismatch())?;
        if &state.observation.identity != identity {
            return Err(mismatch());
        }
        Ok(!state.completed)
    }
    /// Associate the actual transaction ID before its first possible send.
    pub fn bind_commit(
        &self,
        identity: &EffectIdentity,
        step: &str,
        commit_id: &str,
    ) -> CoreResult<()> {
        let mut state = self.0.lock().map_err(|_| mismatch())?;
        if state.completed {
            return Err(mismatch());
        }
        let current = &mut state.observation;
        if &current.identity != identity
            || step.is_empty()
            || commit_id.is_empty()
            || current.step.as_deref().is_some_and(|s| s != step)
            || current
                .commit_id
                .as_deref()
                .is_some_and(|id| id != commit_id)
            || matches!(
                current.outcome,
                EffectOutcome::Unknown { .. } | EffectOutcome::BlockedByPriorUnknown { .. }
            )
        {
            return Err(mismatch());
        }
        current.step = Some(step.into());
        current.commit_id = Some(commit_id.into());
        Ok(())
    }
    pub fn finish(
        &self,
        identity: &EffectIdentity,
        step: &str,
        outcome: EffectOutcome,
        cleanup: CleanupCertainty,
    ) -> CoreResult<()> {
        let mut state = self.0.lock().map_err(|_| mismatch())?;
        if state.completed {
            return Err(mismatch());
        }
        let current = &mut state.observation;
        if &current.identity != identity
            || current.step.as_deref() != Some(step)
            || matches!(
                current.outcome,
                EffectOutcome::Unknown { .. } | EffectOutcome::BlockedByPriorUnknown { .. }
            )
        {
            return Err(mismatch());
        }
        if let EffectOutcome::Unknown { operation_id } = &outcome {
            if current.commit_id.as_ref() != Some(operation_id) {
                return Err(mismatch());
            }
        }
        if matches!(
            outcome,
            EffectOutcome::BlockedByPriorUnknown { .. }
                | EffectOutcome::Staged
                | EffectOutcome::Cancelled
        ) {
            return Err(mismatch());
        }
        current.outcome = outcome;
        current.cleanup = cleanup;
        state.completed = true;
        Ok(())
    }
    /// Admission rejection has no new step, commit ID, or transport effect.
    pub fn blocked(
        &self,
        identity: &EffectIdentity,
        prior: &str,
        cleanup: CleanupCertainty,
    ) -> CoreResult<()> {
        let mut state = self.0.lock().map_err(|_| mismatch())?;
        if state.completed {
            return Err(mismatch());
        }
        let current = &mut state.observation;
        if &current.identity != identity || current.step.is_some() || prior.is_empty() {
            return Err(mismatch());
        }
        current.outcome = EffectOutcome::BlockedByPriorUnknown {
            operation_id: prior.into(),
        };
        current.cleanup = cleanup;
        state.completed = true;
        Ok(())
    }
}
