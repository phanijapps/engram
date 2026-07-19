//! Agent decision traces — candidate-only records of an agent run.
//!
//! A [`DecisionTrace`] captures what an agent consulted, the traversal path it
//! followed, the policy it applied, and the output it produced. Traces are
//! evidence, never authoritative facts: promotion to trusted state requires an
//! explicit `Actor` and is enforced in a later phase. The `promote` method does
//! not ship here. Draft-extension, not part of the frozen v1 schema.

use serde::{Deserialize, Serialize};

use crate::{Actor, DecisionTraceId, EvidenceRef, Policy, Provenance, Scope, Timestamp};

/// A candidate-only record of one agent decision run.
///
/// Invariant: traces are evidence, never authoritative facts. Promotion
/// requires an explicit `Actor`; the `promote(actor)` path lands in a later
/// phase and is the only route to feed `ConsolidationRun`. Mirrors (and is no
/// stronger than) the existing `TaxonomyProposal` merge-requires-explicit-actor
/// rule.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DecisionTrace {
    pub id: DecisionTraceId,
    pub scope: Scope,
    pub agent: Actor,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub items_consulted: Vec<EvidenceRef>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub traversal_path: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub policy_applied: Option<Policy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub precedent: Option<EvidenceRef>,
    pub output: String,
    pub provenance: Provenance,
    pub created_at: Timestamp,
}
