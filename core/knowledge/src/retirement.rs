//! Typed fenced-retirement target associations (review r5, findings 3/10).
//!
//! A maintenance grant selects canonical scope/repository/generation
//! associations under a pinned authorization view. [`RetirementTarget`] is
//! the engine-neutral form of ONE such association: it travels through the
//! owning-store port INTO the fenced mutation transaction, where the store
//! validates EVERY association (repository membership AND exact generation)
//! before any target changes.
//!
//! Contract (parent-adopted, both adapters implement it identically):
//! - the whole set is validated before the FIRST change;
//! - a target whose durable lineage does not match its expected repository
//!   AND exact generation rejects the whole operation (a wrong- or
//!   old-generation target from an allowed repository is not accepted);
//! - a stale/fenced binding epoch still rejects;
//! - a repeated retirement whose association matches exactly is an
//!   idempotent no-op SUCCESS that does not advance binding/scope epochs;
//! - epochs advance only when at least one target actually needs retirement.

/// One typed expected target association for fenced lineage retirement: the
/// record, its owning repository, and the EXACT generation the grant
/// validated for it.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetirementTarget {
    /// The repository the authorization view bound this target to.
    pub repo_id: String,
    /// The immutable generation the authorization view observed for this
    /// target's lineage. The store rejects a target whose durable lineage
    /// carries a different generation — even from an allowed repository.
    pub generation_id: String,
    /// The durable-lineage record id to retire.
    pub record_id: String,
}
