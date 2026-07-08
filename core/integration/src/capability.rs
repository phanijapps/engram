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
/// before starting workers, routes, tools, or retrieval paths. The report is
/// machine-readable and stable; each capability state includes a reason code
/// explaining why a feature is not supported.
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

    /// Migration/import API with dry-run and apply modes.
    pub migration: CapabilityState,
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
            migration: state,
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

    /// Creates a builder for incrementally constructing capability reports.
    pub fn builder() -> CapabilityReportBuilder {
        CapabilityReportBuilder::new()
    }
}

/// Builder for incrementally constructing capability reports.
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
    /// Creates a new builder with all capabilities set to Unsupported.
    pub fn new() -> Self {
        Self {
            report: CapabilityReport::new(CapabilityState::Unsupported {
                reason: CapabilityReason::ProviderUnavailable,
            }),
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
    fn test_capability_report_includes_all_families() {
        let report = CapabilityReport::new(CapabilityState::Supported);
        // Verify all 10 families are present
        let json = serde_json::to_string(&report).unwrap();
        assert!(json.contains("memory"));
        assert!(json.contains("knowledge"));
        assert!(json.contains("graph"));
        assert!(json.contains("ontology"));
        assert!(json.contains("taxonomy"));
        assert!(json.contains("beliefs"));
        assert!(json.contains("hierarchy"));
        assert!(json.contains("retrieval"));
        assert!(json.contains("vectors"));
        assert!(json.contains("migration"));
    }
}
