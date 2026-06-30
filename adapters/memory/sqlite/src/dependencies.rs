//! Default dependencies for the SQL memory service.
//!
//! SQL service orchestration still depends on core ports for time, IDs, and
//! policy so tests and production integrations can replace local defaults.

use std::sync::atomic::{AtomicU64, Ordering};

use chrono::Utc;
use engram_domain::{Id, Policy, Requester, Scope, Timestamp};
use engram_memory::{Clock, CoreResult, IdGenerator, PolicyAuthorizer};

/// Clock implementation backed by current UTC time.
#[derive(Debug, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Timestamp {
        Utc::now()
    }
}

/// Monotonic local ID generator for SQL service tests and examples.
#[derive(Debug, Default)]
pub struct SequentialIdGenerator {
    value: AtomicU64,
}

impl SequentialIdGenerator {
    /// Creates a fresh deterministic ID sequence for SQL service tests.
    ///
    /// The generated IDs are stable enough for fixture assertions, but they
    /// remain opaque contract identifiers. SQL callers must not derive ordering,
    /// tenancy, or storage placement from the generated string.
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
