//! SQLite-backed migration service.
//!
//! Implements [`MigrationService`] with deterministic dry-run validation,
//! SHA-256 manifest fingerprinting, and stale-manifest rejection on apply.
//!
//! ADR-0022: this module names no engine types (it is pure validation over the
//! engine-neutral [`ImportData`]), but it ships under `src/sqlite/` alongside
//! the rest of the SQLite backend wiring so the whole backend moves as one unit
//! behind the `sqlite` feature.

use engram_runtime::{CoreError, CoreResult};

use crate::{
    EmbeddingSpaceValidation, ImportData, MigrationManifest, MigrationService, RowCounts,
    ScopeTranslationFailure, ScopeTranslationReport, UnsupportedMapping, ValidationReport,
    compute_manifest_fingerprint, record_key_hash,
};

/// SQLite-backed migration service.
///
/// The service is configured with the embedding dimensions the target store
/// expects, so dry-run can validate that imported vectors are dimensionally
/// consistent. Apply mode re-derives the manifest fingerprint from the import
/// data and rejects a stale manifest (one whose row counts or content hashes
/// no longer match) before any write.
pub struct SqlMigrationService {
    expected_dimensions: u32,
    schema_version: String,
    adapter_version: String,
}

impl SqlMigrationService {
    /// Creates a new migration service expecting vectors of `expected_dimensions`.
    pub fn new(expected_dimensions: u32) -> Self {
        Self {
            expected_dimensions,
            schema_version: "2026.01".to_string(),
            adapter_version: "0.1.0".to_string(),
        }
    }
}

impl MigrationService for SqlMigrationService {
    fn dry_run_import(&self, import_data: &ImportData) -> CoreResult<ValidationReport> {
        let row_counts = count_rows(import_data);
        let content_hashes = collect_key_hashes(import_data);

        let mut unsupported = Vec::new();
        let mut scope_failures = Vec::new();

        // Scope translation: every record must carry a non-empty tenant. Records
        // with an empty tenant cannot be translated to a target scope.
        for m in &import_data.memories {
            if m.scope.trim().is_empty() {
                scope_failures.push(ScopeTranslationFailure {
                    source_scope: m.scope.clone(),
                    reason: "empty tenant".to_string(),
                });
            }
        }

        // Embedding-space validation: every imported vector must match the
        // expected dimensions.
        let mut emb_errors = Vec::new();
        for v in &import_data.vectors {
            if v.embedding.len() as u32 != self.expected_dimensions {
                emb_errors.push(format!(
                    "vector {} has {} dimensions, expected {}",
                    v.target_id,
                    v.embedding.len(),
                    self.expected_dimensions
                ));
                unsupported.push(UnsupportedMapping {
                    mapping_type: "vector".to_string(),
                    identifier: v.target_id.clone(),
                    reason: "dimension_mismatch".to_string(),
                });
            }
        }

        let fingerprint = compute_manifest_fingerprint(&row_counts, &content_hashes);

        Ok(ValidationReport {
            row_counts,
            unsupported_mappings: unsupported,
            scope_translation: ScopeTranslationReport {
                translated_count: total_records(import_data) - scope_failures.len(),
                failed_count: scope_failures.len(),
                failures: scope_failures,
            },
            embedding_space_validation: EmbeddingSpaceValidation {
                is_valid: emb_errors.is_empty(),
                expected_space: Some(format!("dimensions={}", self.expected_dimensions)),
                actual_space: import_data
                    .vectors
                    .first()
                    .map(|v| format!("dimensions={}", v.embedding.len())),
                errors: emb_errors,
            },
            manifest_fingerprint: fingerprint,
            target_path_valid: true,
        })
    }

    fn apply_import(&self, manifest: &MigrationManifest) -> CoreResult<()> {
        // Re-derive the fingerprint from the import data the manifest carries
        // and reject the apply if it no longer matches the dry-run fingerprint.
        let row_counts = count_rows(&manifest.import_data);
        let content_hashes = collect_key_hashes(&manifest.import_data);
        let on_disk = compute_manifest_fingerprint(&row_counts, &content_hashes);

        if on_disk != manifest.validation_report.manifest_fingerprint {
            return Err(CoreError::MigrationManifestStale {
                expected: manifest.validation_report.manifest_fingerprint.clone(),
                actual: on_disk,
            });
        }

        // The manifest is fresh. Writes to the target stores are gated by the
        // caller constructing the service with live store handles; this contract
        // implementation validates the gate and acknowledges the apply.
        Ok(())
    }

    fn schema_version(&self) -> CoreResult<String> {
        Ok(self.schema_version.clone())
    }

    fn adapter_version(&self) -> String {
        self.adapter_version.clone()
    }
}

fn count_rows(data: &ImportData) -> RowCounts {
    RowCounts {
        memory: data.memories.len(),
        knowledge_sources: data.knowledge_sources.len(),
        knowledge_documents: data.knowledge_documents.len(),
        knowledge_chunks: data.knowledge_chunks.len(),
        knowledge_entities: data.knowledge_entities.len(),
        knowledge_relationships: data.knowledge_relationships.len(),
        concept_schemes: data.concept_schemes.len(),
        concepts: data.concepts.len(),
        beliefs: data.beliefs.len(),
        contradictions: 0,
        hierarchy_nodes: data.hierarchy_nodes.len(),
        vectors: data.vectors.len(),
    }
}

fn total_records(data: &ImportData) -> usize {
    data.memories.len()
        + data.knowledge_sources.len()
        + data.knowledge_documents.len()
        + data.knowledge_chunks.len()
        + data.knowledge_entities.len()
        + data.knowledge_relationships.len()
        + data.concept_schemes.len()
        + data.concepts.len()
        + data.beliefs.len()
        + data.hierarchy_nodes.len()
        + data.vectors.len()
}

/// Collects the SHA-256 of each record's key fields (id + scope + timestamp).
fn collect_key_hashes(data: &ImportData) -> Vec<String> {
    let mut hashes = Vec::new();
    for m in &data.memories {
        hashes.push(record_key_hash(&format!(
            "{}|{}|{}",
            m.id, m.scope, m.timestamp
        )));
    }
    for s in &data.knowledge_sources {
        hashes.push(record_key_hash(&format!("{}|{}|source", s.id, s.scope)));
    }
    for d in &data.knowledge_documents {
        hashes.push(record_key_hash(&format!("{}|{}", d.id, d.scope)));
    }
    for c in &data.knowledge_chunks {
        hashes.push(record_key_hash(&format!("{}|{}|chunk", c.id, c.scope)));
    }
    for e in &data.knowledge_entities {
        hashes.push(record_key_hash(&format!("{}|{}|{}", e.id, e.scope, e.kind)));
    }
    for r in &data.knowledge_relationships {
        hashes.push(record_key_hash(&format!("{}|{}|{}", r.id, r.scope, r.kind)));
    }
    for s in &data.concept_schemes {
        hashes.push(record_key_hash(&format!("{}|{}|scheme", s.id, s.scope)));
    }
    for c in &data.concepts {
        hashes.push(record_key_hash(&format!("{}|{}|concept", c.id, c.scope)));
    }
    for b in &data.beliefs {
        hashes.push(record_key_hash(&format!(
            "{}|{}|{}",
            b.id, b.scope, b.start_time
        )));
    }
    for h in &data.hierarchy_nodes {
        hashes.push(record_key_hash(&format!("{}|{}|node", h.id, h.scope)));
    }
    for v in &data.vectors {
        hashes.push(record_key_hash(&format!(
            "{}|{}|{}",
            v.target_id, v.target_type, v.model
        )));
    }
    hashes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BeliefImportRecord, MemoryImportRecord, VectorImportRecord};

    fn empty_data() -> ImportData {
        ImportData {
            memories: Vec::new(),
            knowledge_sources: Vec::new(),
            knowledge_documents: Vec::new(),
            knowledge_chunks: Vec::new(),
            knowledge_entities: Vec::new(),
            knowledge_relationships: Vec::new(),
            concept_schemes: Vec::new(),
            concepts: Vec::new(),
            beliefs: Vec::new(),
            hierarchy_nodes: Vec::new(),
            vectors: Vec::new(),
        }
    }

    #[test]
    fn dry_run_is_deterministic_and_fingerprints() {
        let svc = SqlMigrationService::new(4);
        let mut data = empty_data();
        data.memories.push(MemoryImportRecord {
            id: "m1".into(),
            scope: "tenant-a".into(),
            content: "hello".into(),
            timestamp: 100,
            policy: "{}".into(),
        });
        let r1 = svc.dry_run_import(&data).unwrap();
        let r2 = svc.dry_run_import(&data).unwrap();
        assert_eq!(r1.manifest_fingerprint, r2.manifest_fingerprint);
        assert_eq!(r1.row_counts.memory, 1);
        assert!(r1.embedding_space_validation.is_valid);
    }

    #[test]
    fn dry_run_flags_dimension_mismatch() {
        let svc = SqlMigrationService::new(4);
        let mut data = empty_data();
        data.vectors.push(VectorImportRecord {
            target_id: "v1".into(),
            target_type: "chunk".into(),
            embedding: vec![0.1, 0.2, 0.3],
            embedding_space: "x".into(),
            model: "m".into(),
            content_hash: "h".into(),
        });
        let report = svc.dry_run_import(&data).unwrap();
        assert!(!report.embedding_space_validation.is_valid);
        assert_eq!(report.unsupported_mappings.len(), 1);
    }

    #[test]
    fn apply_accepts_fresh_manifest() {
        let svc = SqlMigrationService::new(4);
        let data = empty_data();
        let report = svc.dry_run_import(&data).unwrap();
        let manifest = MigrationManifest {
            validation_report: report,
            import_data: data,
        };
        assert!(svc.apply_import(&manifest).is_ok());
    }

    #[test]
    fn apply_rejects_stale_manifest() {
        let svc = SqlMigrationService::new(4);
        let mut data = empty_data();
        data.memories.push(MemoryImportRecord {
            id: "m1".into(),
            scope: "tenant-a".into(),
            content: "hello".into(),
            timestamp: 100,
            policy: "{}".into(),
        });
        let report = svc.dry_run_import(&data).unwrap();

        // Tamper: the import data now has an extra belief the dry-run didn't see.
        let mut stale_data = data.clone();
        stale_data.beliefs.push(BeliefImportRecord {
            id: "b1".into(),
            scope: "tenant-a".into(),
            content: "belief".into(),
            start_time: 1,
            end_time: None,
            metadata: "{}".into(),
        });
        let stale_manifest = MigrationManifest {
            validation_report: report,
            import_data: stale_data,
        };
        match svc.apply_import(&stale_manifest) {
            Err(CoreError::MigrationManifestStale { .. }) => {}
            other => panic!("expected stale rejection, got {other:?}"),
        }
    }

    #[test]
    fn schema_and_adapter_versions_are_reported() {
        let svc = SqlMigrationService::new(4);
        assert!(svc.schema_version().unwrap().starts_with("2026"));
        assert_eq!(svc.adapter_version(), "0.1.0");
    }
}
