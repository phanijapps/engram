//! Default dependencies for the in-memory adapter.
//!
//! These implementations keep local development convenient while still routing
//! behavior through `engram-core` ports. Tests can replace them with fixed
//! clocks, scripted ID generators, or stricter policy authorizers.

use std::sync::atomic::{AtomicU64, Ordering};

use chrono::Utc;
use engram_core::{Clock, CoreResult, IdGenerator, PolicyAuthorizer};
use engram_domain::{Id, Policy, Requester, Scope, Timestamp};

/// Clock implementation backed by the current system UTC time.
///
/// This is the default for local development. Tests that need exact timestamps
/// should inject a deterministic `Clock` through
/// `InMemoryMemoryService::with_dependencies`.
#[derive(Debug, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Timestamp {
        Utc::now()
    }
}

/// Monotonic process-local ID generator for in-memory adapters.
///
/// IDs are opaque contract values. The counter exists only to make local tests
/// deterministic and must not be treated as a storage ordering guarantee by
/// callers.
#[derive(Debug, Default)]
pub struct SequentialIdGenerator {
    value: AtomicU64,
}

impl SequentialIdGenerator {
    /// Creates a fresh process-local sequence for deterministic adapter IDs.
    ///
    /// The first emitted identifier ends in `000001`, but callers must still
    /// treat the full value as opaque. Tests may rely on the sequence for stable
    /// assertions; portable contracts must not infer ordering or scope from it.
    pub fn new() -> Self {
        Self {
            value: AtomicU64::new(1),
        }
    }
}

impl IdGenerator for SequentialIdGenerator {
    fn new_id(&self, entity_type: &'static str) -> Id {
        let value = self.value.fetch_add(1, Ordering::Relaxed);
        Id::from(format!("{entity_type}-{value:06}"))
    }
}

/// Policy authorizer that permits every operation.
///
/// This is a first-slice stub for tests and local development. Real deployments
/// should provide a stricter authorizer that understands requester roles,
/// permissions, visibility, retention, sensitivity, and allowed uses.
#[derive(Debug, Default)]
pub struct AllowAllPolicyAuthorizer;

impl PolicyAuthorizer for AllowAllPolicyAuthorizer {
    fn can_write(
        &self,
        _requester: &Requester,
        _scope: &Scope,
        _policy: &Policy,
    ) -> CoreResult<()> {
        Ok(())
    }

    fn can_retrieve(
        &self,
        _requester: &Requester,
        _scope: &Scope,
        _policy: &Policy,
    ) -> CoreResult<()> {
        Ok(())
    }

    fn can_forget(
        &self,
        _requester: &Requester,
        _scope: &Scope,
        _policy: &Policy,
    ) -> CoreResult<()> {
        Ok(())
    }
}
