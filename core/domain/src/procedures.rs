//! Procedure records — replayable runbooks with success/failure accounting
//! (RFC-0016 Layer 6). Procedures are operational memory: a named sequence of
//! steps to apply in a recurring situation, with counters the caller bumps as
//! the procedure succeeds or fails. The server records outcomes; it does not
//! decide when a procedure applies.

use serde::{Deserialize, Serialize};

use crate::{Id, Metadata, Policy, Provenance, Scope, Timestamp};

/// Stable identifier for a procedure record.
pub type ProcedureId = Id;

/// A replayable runbook: a named sequence of steps with success/failure
/// accounting.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Procedure {
    pub id: ProcedureId,
    pub scope: Scope,
    pub name: String,
    /// Ordered runbook steps (free text — the action to perform at each step).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub steps: Vec<String>,
    /// When to apply this procedure (free text / trigger description).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trigger: Option<String>,
    #[serde(default)]
    pub success_count: u32,
    #[serde(default)]
    pub failure_count: u32,
    pub provenance: Provenance,
    pub policy: Policy,
    pub created_at: Timestamp,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<Timestamp>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,
}

/// Aggregate statistics over the procedures in a scope.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcedureStats {
    pub total: usize,
    pub total_success: u64,
    pub total_failure: u64,
}
