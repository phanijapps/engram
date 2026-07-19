//! Applicability rules — condition-gated bindings between facts and targets.
//!
//! An [`ApplicabilityRule`] binds a target (a graph entity or a taxonomy
//! concept) only when a condition holds: "fact X binds target Y when
//! condition Z." Rules are governed records (scope, policy, provenance) that
//! govern graph shape the way axioms govern static constraints. They are
//! draft-extension types, not part of the frozen v1 schema; the writer surface
//! and validation land in later phases.

use serde::{Deserialize, Serialize};

use crate::{ApplicabilityRuleId, ConceptRef, EntityRef, Policy, Provenance, Scope, Timestamp};

/// The target a rule binds — a graph entity or a taxonomy concept.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleTarget {
    Entity(EntityRef),
    Concept(ConceptRef),
}

/// A condition-gated binding: "fact X binds target Y when condition Z."
///
/// `ApplicabilityRule` records are consumer-declared (like ontology/taxonomy
/// content), not extractor-derived. Validated like ontology axioms
/// (`OntologyValidationFinding`-style) in a later phase.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplicabilityRule {
    pub id: ApplicabilityRuleId,
    pub condition: String,
    pub target: RuleTarget,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub binding: Option<String>,
    pub scope: Scope,
    pub policy: Policy,
    pub provenance: Provenance,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub valid_from: Option<Timestamp>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub valid_until: Option<Timestamp>,
    pub created_at: Timestamp,
}
