//! Capability reporting for the Engram integration facade.
//!
//! This module defines the structured capability report that applications
//! receive at bootstrap time, with each feature family reporting its
//! supported state and stable reason codes.

use engram_domain::{CapabilityReason, CapabilityState};
use serde::{Deserialize, Serialize};

/// Capability report for all feature families.
///
/// Applications use this report to discover which features are safe to enable
/// before starting workers, routes, tools, or recall paths. The report is
/// machine-readable and stable; each capability state includes a reason code
/// explaining why a feature is not supported.
///
/// The 19 keys cover every capability area named in the host-SDK brief plus
/// consolidation. The 8 "not-yet-built" areas (hybrid search, episodes/evidence, contradiction,
/// atomic batch, unified recall, export/import, maintenance, observability)
/// default to [`CapabilityState::Unsupported`] with
/// [`CapabilityReason::FeatureDisabled`] until their implementation slices
/// ship — they are present and explicit, never silently absent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityReport {
    /// Memory read/write/retrieve operations.
    pub memory: CapabilityState,

    /// Knowledge sources, documents, chunks, entities, and relationships.
    pub knowledge: CapabilityState,

    /// Knowledge graph traversal and neighbor queries.
    pub graph: CapabilityState,

    /// Ontology axioms, classes, and properties.
    pub ontology: CapabilityState,

    /// Taxonomy concepts and concept schemes.
    pub taxonomy: CapabilityState,

    /// Belief storage, contradiction detection, and resolution.
    pub beliefs: CapabilityState,

    /// Hierarchy node operations (first-class generic concepts).
    pub hierarchy: CapabilityState,

    /// Retrieval composition and fusion across multiple sources.
    pub retrieval: CapabilityState,

    /// Vector index operations with embedding-space validation.
    pub vectors: CapabilityState,

    /// Schema-version migration (dry-run and apply modes).
    pub migration: CapabilityState,

    /// Hybrid lexical + vector search composition.
    pub hybrid_search: CapabilityState,

    /// Durable provenance: source episodes, events, documents, and evidence links.
    pub episodes_evidence: CapabilityState,

    /// Contradiction / tension tracking across beliefs.
    pub contradiction: CapabilityState,

    /// Atomic cross-store semantic ingest (batch transaction).
    pub atomic_batch: CapabilityState,

    /// Unified recall across facts, graph, beliefs, episodes, and taxonomy.
    pub unified_recall: CapabilityState,

    /// Semantic-state export / import + backend-to-backend movement.
    pub export_import: CapabilityState,

    /// Backend-neutral maintenance (compact, reindex, dedup, vacuum).
    pub maintenance: CapabilityState,

    /// Operational introspection (status, counts, index/embedding diagnostics).
    pub observability: CapabilityState,

    /// Consolidation: reflection (derived beliefs) + decay (expiry) + composite
    /// executor dispatch. Wired when both memory + belief stores are available.
    #[serde(default = "default_consolidation")]
    pub consolidation: CapabilityState,

    /// Knowledge-graph identity + consolidation (RFC-0014).
    #[serde(default = "default_identity")]
    pub identity: CapabilityState,
}

/// Serde default for the `identity` field.
fn default_identity() -> CapabilityState {
    CapabilityState::Unsupported {
        reason: CapabilityReason::FeatureDisabled,
    }
}

/// Serde default for the `consolidation` field — `Unsupported { FeatureDisabled }`.
fn default_consolidation() -> CapabilityState {
    CapabilityState::Unsupported {
        reason: CapabilityReason::FeatureDisabled,
    }
}

impl CapabilityReport {
    /// Creates a new capability report with all fields set to the given state.
    pub fn new(state: CapabilityState) -> Self {
        Self {
            memory: state.clone(),
            knowledge: state.clone(),
            graph: state.clone(),
            ontology: state.clone(),
            taxonomy: state.clone(),
            beliefs: state.clone(),
            hierarchy: state.clone(),
            retrieval: state.clone(),
            vectors: state.clone(),
            migration: state.clone(),
            hybrid_search: state.clone(),
            episodes_evidence: state.clone(),
            contradiction: state.clone(),
            atomic_batch: state.clone(),
            unified_recall: state.clone(),
            export_import: state.clone(),
            maintenance: state.clone(),
            observability: state.clone(),
            consolidation: state.clone(),
            identity: state,
        }
    }

    /// Returns true if all capability families are fully supported.
    ///
    /// This is a convenient helper for applications that require all features
    /// to be available before proceeding.
    pub fn all_supported(&self) -> bool {
        self.memory.is_supported()
            && self.knowledge.is_supported()
            && self.graph.is_supported()
            && self.ontology.is_supported()
            && self.taxonomy.is_supported()
            && self.beliefs.is_supported()
            && self.hierarchy.is_supported()
            && self.retrieval.is_supported()
            && self.vectors.is_supported()
            && self.migration.is_supported()
            && self.hybrid_search.is_supported()
            && self.episodes_evidence.is_supported()
            && self.contradiction.is_supported()
            && self.atomic_batch.is_supported()
            && self.unified_recall.is_supported()
            && self.export_import.is_supported()
            && self.maintenance.is_supported()
            && self.observability.is_supported()
            && self.consolidation.is_supported()
            && self.identity.is_supported()
    }

    /// Returns true if memory operations are supported.
    pub fn memory_supported(&self) -> bool {
        self.memory.is_supported()
    }

    /// Returns true if knowledge operations are supported.
    pub fn knowledge_supported(&self) -> bool {
        self.knowledge.is_supported()
    }

    /// Returns true if graph operations are supported.
    pub fn graph_supported(&self) -> bool {
        self.graph.is_supported()
    }

    /// Returns true if ontology operations are supported.
    pub fn ontology_supported(&self) -> bool {
        self.ontology.is_supported()
    }

    /// Returns true if taxonomy operations are supported.
    pub fn taxonomy_supported(&self) -> bool {
        self.taxonomy.is_supported()
    }

    /// Returns true if belief operations are supported.
    pub fn beliefs_supported(&self) -> bool {
        self.beliefs.is_supported()
    }

    /// Returns true if hierarchy operations are supported.
    pub fn hierarchy_supported(&self) -> bool {
        self.hierarchy.is_supported()
    }

    /// Returns true if retrieval operations are supported.
    pub fn retrieval_supported(&self) -> bool {
        self.retrieval.is_supported()
    }

    /// Returns true if vector operations are supported.
    pub fn vectors_supported(&self) -> bool {
        self.vectors.is_supported()
    }

    /// Returns true if migration operations are supported.
    pub fn migration_supported(&self) -> bool {
        self.migration.is_supported()
    }

    /// Returns true if hybrid lexical+vector search is supported.
    pub fn hybrid_search_supported(&self) -> bool {
        self.hybrid_search.is_supported()
    }

    /// Returns true if episode/evidence provenance is supported.
    pub fn episodes_evidence_supported(&self) -> bool {
        self.episodes_evidence.is_supported()
    }

    /// Returns true if contradiction tracking is supported.
    pub fn contradiction_supported(&self) -> bool {
        self.contradiction.is_supported()
    }

    /// Returns true if atomic batch ingest is supported.
    pub fn atomic_batch_supported(&self) -> bool {
        self.atomic_batch.is_supported()
    }

    /// Returns true if unified recall is supported.
    pub fn unified_recall_supported(&self) -> bool {
        self.unified_recall.is_supported()
    }

    /// Returns true if export/import is supported.
    pub fn export_import_supported(&self) -> bool {
        self.export_import.is_supported()
    }

    /// Returns true if maintenance operations are supported.
    pub fn maintenance_supported(&self) -> bool {
        self.maintenance.is_supported()
    }

    /// Returns true if observability is supported.
    pub fn observability_supported(&self) -> bool {
        self.observability.is_supported()
    }

    /// Returns true if consolidation (reflection + decay) is supported.
    pub fn consolidation_supported(&self) -> bool {
        self.consolidation.is_supported()
    }

    /// Creates a builder for incrementally constructing capability reports.
    pub fn builder() -> CapabilityReportBuilder {
        CapabilityReportBuilder::new()
    }
}

/// Builder for incrementally constructing capability reports.
///
/// The 10 implemented families default to [`CapabilityReason::ProviderUnavailable`]
/// (no backend wired); the 8 not-yet-built areas default to
/// [`CapabilityReason::FeatureDisabled`] (the capability exists in the report
/// but its implementation slice has not shipped). Both are
/// [`CapabilityState::Unsupported`]; callers attach a handle and mark a family
/// [`CapabilityState::Supported`] only when its conformance fixture passes.
#[derive(Debug, Clone)]
pub struct CapabilityReportBuilder {
    report: CapabilityReport,
}

impl Default for CapabilityReportBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl CapabilityReportBuilder {
    /// Creates a new builder: implemented families `Unsupported` (provider
    /// unavailable), not-yet-built areas `Unsupported` (feature disabled).
    pub fn new() -> Self {
        let provider_unavailable = CapabilityState::Unsupported {
            reason: CapabilityReason::ProviderUnavailable,
        };
        let feature_disabled = CapabilityState::Unsupported {
            reason: CapabilityReason::FeatureDisabled,
        };
        Self {
            report: CapabilityReport {
                memory: provider_unavailable.clone(),
                knowledge: provider_unavailable.clone(),
                graph: provider_unavailable.clone(),
                ontology: provider_unavailable.clone(),
                taxonomy: provider_unavailable.clone(),
                beliefs: provider_unavailable.clone(),
                hierarchy: provider_unavailable.clone(),
                retrieval: provider_unavailable.clone(),
                vectors: provider_unavailable.clone(),
                migration: provider_unavailable,
                hybrid_search: feature_disabled.clone(),
                episodes_evidence: feature_disabled.clone(),
                contradiction: feature_disabled.clone(),
                atomic_batch: feature_disabled.clone(),
                unified_recall: feature_disabled.clone(),
                export_import: feature_disabled.clone(),
                maintenance: feature_disabled.clone(),
                observability: feature_disabled.clone(),
                consolidation: feature_disabled.clone(),
                identity: feature_disabled,
            },
        }
    }

    /// Sets the memory capability state.
    pub fn memory(mut self, state: CapabilityState) -> Self {
        self.report.memory = state;
        self
    }

    /// Sets the knowledge capability state.
    pub fn knowledge(mut self, state: CapabilityState) -> Self {
        self.report.knowledge = state;
        self
    }

    /// Sets the graph capability state.
    pub fn graph(mut self, state: CapabilityState) -> Self {
        self.report.graph = state;
        self
    }

    /// Sets the ontology capability state.
    pub fn ontology(mut self, state: CapabilityState) -> Self {
        self.report.ontology = state;
        self
    }

    /// Sets the taxonomy capability state.
    pub fn taxonomy(mut self, state: CapabilityState) -> Self {
        self.report.taxonomy = state;
        self
    }

    /// Sets the beliefs capability state.
    pub fn beliefs(mut self, state: CapabilityState) -> Self {
        self.report.beliefs = state;
        self
    }

    /// Sets the hierarchy capability state.
    pub fn hierarchy(mut self, state: CapabilityState) -> Self {
        self.report.hierarchy = state;
        self
    }

    /// Sets the retrieval capability state.
    pub fn retrieval(mut self, state: CapabilityState) -> Self {
        self.report.retrieval = state;
        self
    }

    /// Sets the vectors capability state.
    pub fn vectors(mut self, state: CapabilityState) -> Self {
        self.report.vectors = state;
        self
    }

    /// Sets the migration capability state.
    pub fn migration(mut self, state: CapabilityState) -> Self {
        self.report.migration = state;
        self
    }

    /// Sets the hybrid-search capability state.
    pub fn hybrid_search(mut self, state: CapabilityState) -> Self {
        self.report.hybrid_search = state;
        self
    }

    /// Sets the episodes/evidence capability state.
    pub fn episodes_evidence(mut self, state: CapabilityState) -> Self {
        self.report.episodes_evidence = state;
        self
    }

    /// Sets the contradiction capability state.
    pub fn contradiction(mut self, state: CapabilityState) -> Self {
        self.report.contradiction = state;
        self
    }

    /// Sets the atomic-batch capability state.
    pub fn atomic_batch(mut self, state: CapabilityState) -> Self {
        self.report.atomic_batch = state;
        self
    }

    /// Sets the unified-recall capability state.
    pub fn unified_recall(mut self, state: CapabilityState) -> Self {
        self.report.unified_recall = state;
        self
    }

    /// Sets the export/import capability state.
    pub fn export_import(mut self, state: CapabilityState) -> Self {
        self.report.export_import = state;
        self
    }

    /// Sets the maintenance capability state.
    pub fn maintenance(mut self, state: CapabilityState) -> Self {
        self.report.maintenance = state;
        self
    }

    /// Sets the observability capability state.
    pub fn observability(mut self, state: CapabilityState) -> Self {
        self.report.observability = state;
        self
    }

    /// Sets the consolidation capability state.
    pub fn consolidation(mut self, state: CapabilityState) -> Self {
        self.report.consolidation = state;
        self
    }

    /// Sets the knowledge-graph identity capability state.
    pub fn identity(mut self, state: CapabilityState) -> Self {
        self.report.identity = state;
        self
    }

    /// Builds the final capability report.
    pub fn build(self) -> CapabilityReport {
        self.report
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capability_report_construction() {
        let report = CapabilityReport::new(CapabilityState::Supported);
        assert!(report.all_supported());
        assert!(report.memory_supported());
        assert!(report.knowledge_supported());
    }

    #[test]
    fn test_capability_report_builder() {
        let report = CapabilityReport::builder()
            .memory(CapabilityState::Supported)
            .knowledge(CapabilityState::Degraded {
                reason: CapabilityReason::DimensionMismatch,
            })
            .vectors(CapabilityState::Unsupported {
                reason: CapabilityReason::ProviderUnavailable,
            })
            .build();

        assert!(report.memory_supported());
        assert!(!report.knowledge_supported());
        assert!(!report.vectors_supported());
        assert!(!report.all_supported());
    }

    #[test]
    fn test_capability_report_serialization() {
        let report = CapabilityReport::builder()
            .memory(CapabilityState::Supported)
            .vectors(CapabilityState::RequiresReindex {
                reason: CapabilityReason::EmbeddingSpaceMismatch,
            })
            .build();

        let json = serde_json::to_string(&report).unwrap();
        let deserialized: CapabilityReport = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, report);
    }

    #[test]
    fn test_all_supported_returns_false_when_any_unsupported() {
        let report = CapabilityReport::builder()
            .memory(CapabilityState::Supported)
            .knowledge(CapabilityState::Supported)
            .vectors(CapabilityState::Unsupported {
                reason: CapabilityReason::ProviderUnavailable,
            })
            .build();

        assert!(!report.all_supported());
    }

    #[test]
    fn test_capability_report_includes_all_nineteen_keys() {
        let report = CapabilityReport::new(CapabilityState::Supported);
        // Verify all 19 capability keys are present in the serialized report.
        let json = serde_json::to_string(&report).unwrap();
        for key in [
            "memory",
            "knowledge",
            "graph",
            "ontology",
            "taxonomy",
            "beliefs",
            "hierarchy",
            "retrieval",
            "vectors",
            "migration",
            "hybrid_search",
            "episodes_evidence",
            "contradiction",
            "atomic_batch",
            "unified_recall",
            "export_import",
            "maintenance",
            "observability",
            "consolidation",
        ] {
            assert!(
                json.contains(key),
                "report must include key `{key}`: {json}"
            );
        }
    }

    /// AC1 / AC2: the not-yet-built areas are present and explicitly
    /// `Unsupported { FeatureDisabled }` on a default builder — never silently
    /// absent, and distinguished from the implemented families' `ProviderUnavailable`.
    #[test]
    fn builder_defaults_not_yet_built_areas_to_feature_disabled() {
        let report = CapabilityReport::builder().build();
        let feature_disabled = CapabilityState::Unsupported {
            reason: CapabilityReason::FeatureDisabled,
        };
        let provider_unavailable = CapabilityState::Unsupported {
            reason: CapabilityReason::ProviderUnavailable,
        };
        // Implemented families default to ProviderUnavailable (no backend wired).
        assert_eq!(report.memory, provider_unavailable);
        // Not-yet-built areas default to FeatureDisabled (slice not shipped).
        assert_eq!(report.hybrid_search, feature_disabled);
        assert_eq!(report.episodes_evidence, feature_disabled);
        assert_eq!(report.contradiction, feature_disabled);
        assert_eq!(report.atomic_batch, feature_disabled);
        assert_eq!(report.unified_recall, feature_disabled);
        assert_eq!(report.export_import, feature_disabled);
        assert_eq!(report.maintenance, feature_disabled);
        assert_eq!(report.observability, feature_disabled);
    }

    /// AC1 regression: `all_supported()` must reflect all 18 families. A report
    /// whose only Supported families are the original 10 must return false,
    /// because the 8 not-yet-built areas remain Unsupported — guarding against
    /// `all_supported()` silently staying at 10 fields.
    #[test]
    fn all_supported_is_false_when_new_families_unsupported() {
        let supported = CapabilityState::Supported;
        let report = CapabilityReport::builder()
            .memory(supported.clone())
            .knowledge(supported.clone())
            .graph(supported.clone())
            .ontology(supported.clone())
            .taxonomy(supported.clone())
            .beliefs(supported.clone())
            .hierarchy(supported.clone())
            .retrieval(supported.clone())
            .vectors(supported.clone())
            .migration(supported)
            .build();
        assert!(
            !report.all_supported(),
            "all_supported() must account for the 8 not-yet-built areas"
        );
    }
}
