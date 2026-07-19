//! Migration/import API for safe data import without adapter internals.
//!
//! This module provides the migration service trait and types for importing
//! data from external sources with dry-run validation, manifest fingerprinting,
//! and gated apply mode.

use engram_runtime::CoreResult;
use serde::{Deserialize, Serialize};

/// Validation report from a migration dry-run.
///
/// This report contains deterministic information about what would be
/// imported, including row counts, validation results, and a manifest fingerprint
/// that must match between dry-run and apply.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationReport {
    /// Row counts by data family.
    pub row_counts: RowCounts,

    /// Unsupported mapping report.
    pub unsupported_mappings: Vec<UnsupportedMapping>,

    /// Scope translation report.
    pub scope_translation: ScopeTranslationReport,

    /// Embedding space validation results.
    pub embedding_space_validation: EmbeddingSpaceValidation,

    /// Manifest fingerprint (SHA-256 of row counts + content hashes).
    pub manifest_fingerprint: String,

    /// Target database path validation result.
    pub target_path_valid: bool,
}

/// Row counts by data family.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RowCounts {
    /// Number of memory records.
    pub memory: usize,

    /// Number of knowledge sources.
    pub knowledge_sources: usize,

    /// Number of knowledge documents.
    pub knowledge_documents: usize,

    /// Number of knowledge chunks.
    pub knowledge_chunks: usize,

    /// Number of knowledge entities.
    pub knowledge_entities: usize,

    /// Number of knowledge relationships.
    pub knowledge_relationships: usize,

    /// Number of concept scheme entries.
    pub concept_schemes: usize,

    /// Number of concept entries.
    pub concepts: usize,

    /// Number of belief records.
    pub beliefs: usize,

    /// Number of contradiction records.
    pub contradictions: usize,

    /// Number of hierarchy nodes.
    pub hierarchy_nodes: usize,

    /// Number of vector embeddings.
    pub vectors: usize,
}

/// Report of an unsupported mapping.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnsupportedMapping {
    /// The type of the unsupported mapping.
    pub mapping_type: String,

    /// The identifier of the unsupported item.
    pub identifier: String,

    /// The reason why this mapping is unsupported.
    pub reason: String,
}

/// Scope translation report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScopeTranslationReport {
    /// Number of scopes that were translated.
    pub translated_count: usize,

    /// Number of scopes that could not be translated.
    pub failed_count: usize,

    /// Details of failed translations.
    pub failures: Vec<ScopeTranslationFailure>,
}

/// A failed scope translation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScopeTranslationFailure {
    /// The source scope that failed translation.
    pub source_scope: String,

    /// The reason why translation failed.
    pub reason: String,
}

/// Embedding space validation results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingSpaceValidation {
    /// Whether the embedding space is valid.
    pub is_valid: bool,

    /// Expected embedding space.
    pub expected_space: Option<String>,

    /// Actual embedding space found.
    pub actual_space: Option<String>,

    /// Validation errors.
    pub errors: Vec<String>,
}

/// Migration manifest for import operations.
///
/// This manifest contains the import data and metadata needed for validation
/// during apply mode.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationManifest {
    /// Validation report from dry-run.
    pub validation_report: ValidationReport,

    /// Import data organized by family.
    pub import_data: ImportData,
}

/// Import data organized by family.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportData {
    /// Memory records to import.
    pub memories: Vec<MemoryImportRecord>,

    /// Knowledge sources to import.
    pub knowledge_sources: Vec<KnowledgeSourceImportRecord>,

    /// Knowledge documents to import.
    pub knowledge_documents: Vec<KnowledgeDocumentImportRecord>,

    /// Knowledge chunks to import.
    pub knowledge_chunks: Vec<KnowledgeChunkImportRecord>,

    /// Knowledge entities to import.
    pub knowledge_entities: Vec<KnowledgeEntityImportRecord>,

    /// Knowledge relationships to import.
    pub knowledge_relationships: Vec<KnowledgeRelationshipImportRecord>,

    /// Concept schemes to import.
    pub concept_schemes: Vec<ConceptSchemeImportRecord>,

    /// Concepts to import.
    pub concepts: Vec<ConceptImportRecord>,

    /// Belief records to import.
    pub beliefs: Vec<BeliefImportRecord>,

    /// Hierarchy nodes to import.
    pub hierarchy_nodes: Vec<HierarchyNodeImportRecord>,

    /// Vector embeddings to import.
    pub vectors: Vec<VectorImportRecord>,
}

/// Memory import record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryImportRecord {
    /// Record ID.
    pub id: String,

    /// Record scope.
    pub scope: String,

    /// Record content.
    pub content: String,

    /// Record timestamp.
    pub timestamp: i64,

    /// Record policy (JSON string).
    pub policy: String,
}

/// Knowledge source import record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeSourceImportRecord {
    /// Source ID.
    pub id: String,

    /// Source scope.
    pub scope: String,

    /// Source type.
    pub source_type: String,

    /// Source URI.
    pub uri: String,

    /// Source metadata (JSON string).
    pub metadata: String,
}

/// Knowledge document import record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeDocumentImportRecord {
    /// Document ID.
    pub id: String,

    /// Document scope.
    pub scope: String,

    /// Source ID.
    pub source_id: String,

    /// Document title.
    pub title: String,

    /// Document content.
    pub content: String,

    /// Document metadata (JSON string).
    pub metadata: String,
}

/// Knowledge chunk import record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeChunkImportRecord {
    /// Chunk ID.
    pub id: String,

    /// Chunk scope.
    pub scope: String,

    /// Document ID.
    pub document_id: String,

    /// Chunk sequence number.
    pub sequence: u32,

    /// Chunk content.
    pub content: String,

    /// Chunk metadata (JSON string).
    pub metadata: String,
}

/// Knowledge entity import record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeEntityImportRecord {
    /// Entity ID.
    pub id: String,

    /// Entity scope.
    pub scope: String,

    /// Entity kind.
    pub kind: String,

    /// Entity name.
    pub name: String,

    /// Entity metadata (JSON string).
    pub metadata: String,
}

/// Knowledge relationship import record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeRelationshipImportRecord {
    /// Relationship ID.
    pub id: String,

    /// Relationship scope.
    pub scope: String,

    /// Source entity ID.
    pub source_id: String,

    /// Target entity ID.
    pub target_id: String,

    /// Relationship kind.
    pub kind: String,

    /// Relationship metadata (JSON string).
    pub metadata: String,
}

/// Concept scheme import record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConceptSchemeImportRecord {
    /// Scheme ID.
    pub id: String,

    /// Scheme scope.
    pub scope: String,

    /// Scheme title.
    pub title: String,

    /// Scheme description (optional).
    pub description: Option<String>,

    /// Scheme metadata (JSON string).
    pub metadata: String,
}

/// Concept import record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConceptImportRecord {
    /// Concept ID.
    pub id: String,

    /// Concept scope.
    pub scope: String,

    /// Scheme ID.
    pub scheme_id: String,

    /// Concept label.
    pub label: String,

    /// Concept metadata (JSON string).
    pub metadata: String,
}

/// Belief import record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BeliefImportRecord {
    /// Belief ID.
    pub id: String,

    /// Belief scope.
    pub scope: String,

    /// Belief content.
    pub content: String,

    /// Belief start time.
    pub start_time: i64,

    /// Belief end time (optional).
    pub end_time: Option<i64>,

    /// Belief metadata (JSON string).
    pub metadata: String,
}

/// Hierarchy node import record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HierarchyNodeImportRecord {
    /// Node ID.
    pub id: String,

    /// Node scope.
    pub scope: String,

    /// Node label.
    pub label: String,

    /// Node kind (optional).
    pub kind: Option<String>,

    /// Node metadata (JSON string).
    pub metadata: String,
}

/// Vector import record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorImportRecord {
    /// Vector target ID.
    pub target_id: String,

    /// Target type.
    pub target_type: String,

    /// Embedding vector.
    pub embedding: Vec<f32>,

    /// Embedding space identifier.
    pub embedding_space: String,

    /// Model identifier.
    pub model: String,

    /// Content hash.
    pub content_hash: String,
}

/// Migration service for safe data import.
///
/// This trait provides methods for dry-run validation and gated apply mode
/// for importing data from external sources.
/// Computes a deterministic SHA-256 manifest fingerprint for import data.
///
/// The fingerprint is taken over row counts and the SHA-256 of each record's
/// key fields (id + scope + timestamp), not full content — content may carry
/// non-deterministic fields (timestamps, generated ids) that would make two
/// identical imports fingerprint differently. A stable fingerprint lets apply
/// mode reject a stale manifest whose row counts or key hashes no longer match
/// the data on disk.
///
/// The returned string is the lowercase hex SHA-256.
pub fn compute_manifest_fingerprint(row_counts: &RowCounts, content_hashes: &[String]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();

    // Row counts first, in a fixed family order.
    hasher.update(b"counts:");
    hasher.update(format!(
        "memory={}|sources={}|documents={}|chunks={}|entities={}|relationships={}|schemes={}|concepts={}|beliefs={}|contradictions={}|hierarchy={}|vectors={}",
        row_counts.memory,
        row_counts.knowledge_sources,
        row_counts.knowledge_documents,
        row_counts.knowledge_chunks,
        row_counts.knowledge_entities,
        row_counts.knowledge_relationships,
        row_counts.concept_schemes,
        row_counts.concepts,
        row_counts.beliefs,
        row_counts.contradictions,
        row_counts.hierarchy_nodes,
        row_counts.vectors,
    ).as_bytes());

    // Then the sorted key-field hashes, so ordering does not matter.
    hasher.update(b"|hashes:");
    let mut sorted = content_hashes.to_vec();
    sorted.sort_unstable();
    for h in &sorted {
        hasher.update(h.as_bytes());
        hasher.update(b";");
    }

    let digest = hasher.finalize();
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// Computes the key-field SHA-256 hash for a single import record.
///
/// `key_fields` should be the record's stable identifiers (id, scope, timestamp)
/// concatenated by the caller. This keeps `compute_manifest_fingerprint`
/// independent of every record type's exact shape.
pub fn record_key_hash(key_fields: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(key_fields.as_bytes());
    let digest = hasher.finalize();
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

pub trait MigrationService: Send + Sync {
    /// Performs a dry-run validation of the import data.
    ///
    /// This method validates the import data without making any changes to the
    /// target database. It returns a validation report with row counts,
    /// validation results, and a manifest fingerprint.
    ///
    /// # Errors
    ///
    /// Returns `CoreError::InvalidRequest` if the import data is invalid.
    /// Returns `CoreError::Adapter` if validation fails.
    fn dry_run_import(&self, import_data: &ImportData) -> CoreResult<ValidationReport>;

    /// Applies the import with manifest fingerprint validation.
    ///
    /// This method validates the manifest fingerprint against the dry-run
    /// report and applies the import if the fingerprint matches. Returns an
    /// error if the manifest is stale (fingerprint mismatch).
    ///
    /// # Errors
    ///
    /// Returns `CoreError::InvalidRequest` if the manifest fingerprint is stale.
    /// Returns `CoreError::MigrationPending` if migrations need to be applied first.
    /// Returns `CoreError::Adapter` if the import fails.
    fn apply_import(&self, manifest: &MigrationManifest) -> CoreResult<()>;

    /// Returns the schema version of the target database.
    ///
    /// This version indicates the current schema version and can be used to
    /// determine if migrations are needed.
    fn schema_version(&self) -> CoreResult<String>;

    /// Returns the adapter version.
    ///
    /// This version indicates the adapter implementation version.
    fn adapter_version(&self) -> String;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validation_report_serialization() {
        let report = ValidationReport {
            row_counts: RowCounts {
                memory: 10,
                knowledge_sources: 5,
                knowledge_documents: 3,
                knowledge_chunks: 15,
                knowledge_entities: 8,
                knowledge_relationships: 12,
                concept_schemes: 2,
                concepts: 20,
                beliefs: 7,
                contradictions: 1,
                hierarchy_nodes: 6,
                vectors: 25,
            },
            unsupported_mappings: vec![],
            scope_translation: ScopeTranslationReport {
                translated_count: 5,
                failed_count: 0,
                failures: vec![],
            },
            embedding_space_validation: EmbeddingSpaceValidation {
                is_valid: true,
                expected_space: Some("fastembed/BAAI/bge-small-en-v1.5".to_string()),
                actual_space: Some("fastembed/BAAI/bge-small-en-v1.5".to_string()),
                errors: vec![],
            },
            manifest_fingerprint: "test-fingerprint".to_string(),
            target_path_valid: true,
        };

        let json = serde_json::to_string(&report).unwrap();
        let parsed: ValidationReport = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.row_counts.memory, 10);
        assert_eq!(parsed.row_counts.vectors, 25);
        assert!(parsed.embedding_space_validation.is_valid);
    }

    #[test]
    fn test_import_data_empty() {
        let data = ImportData {
            memories: vec![],
            knowledge_sources: vec![],
            knowledge_documents: vec![],
            knowledge_chunks: vec![],
            knowledge_entities: vec![],
            knowledge_relationships: vec![],
            concept_schemes: vec![],
            concepts: vec![],
            beliefs: vec![],
            hierarchy_nodes: vec![],
            vectors: vec![],
        };

        assert!(data.memories.is_empty());
        assert!(data.vectors.is_empty());
    }
}
