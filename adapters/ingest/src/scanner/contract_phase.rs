//! Contract extraction phase — OpenAPI entity/edge emission for files
//! detected as OpenAPI documents during the parallel ingest phase.
//! (T4 entity emission, T5 source-ref union, T6 skip-and-warn.)

use engram_domain::{Provenance, Scope};
use engram_knowledge::{KnowledgeGraphRepository, KnowledgeRepository};
use futures::executor::block_on;

/// Attempts OpenAPI contract extraction for a single file during the parallel
/// ingest phase (T4 entity emission, T5 source-ref union, T6 skip-and-warn).
///
/// Returns `(contract_keys, parse_failed, had_write_error)`:
/// - `contract_keys`: normalized keys for operations whose entity AND edge were
///   successfully persisted; empty for non-OpenAPI files or total-parse-failure.
///   Keys for individual write failures are excluded so the manifest does not
///   record unpersisted ops.
/// - `parse_failed`: `true` when the file had an OpenAPI marker but could not
///   be parsed; the caller increments `ScanSummary.skipped`.
/// - `had_write_error`: `true` when at least one entity or edge persist failed;
///   the caller increments `ScanSummary.skipped` and logs a warning.
pub(crate) fn extract_contract_entities<R>(
    repo: &R,
    scope: &Scope,
    stable_source_key: &str,
    text: &str,
    ext: &str,
    provenance: &Provenance,
) -> (Vec<String>, bool, bool)
where
    R: KnowledgeRepository + KnowledgeGraphRepository + Send + Sync,
{
    use chrono::Utc;

    match crate::contract::detect_and_parse_openapi(text, ext) {
        Ok(None) => (Vec::new(), false, false),
        Err(_) => (Vec::new(), true, false), // malformed OpenAPI: caller increments skipped
        Ok(Some(ops)) => {
            let now = Utc::now();
            let mut keys = Vec::with_capacity(ops.len());
            let mut had_write_error = false;
            for op in &ops {
                let entity = crate::contract::build_api_entity(
                    scope,
                    stable_source_key,
                    op,
                    provenance,
                    now,
                );
                // Read-modify-write union for cross-repo merge (T5).
                match block_on(crate::contract::upsert_api_entity_with_source_ref(
                    repo, scope, entity,
                )) {
                    Ok(()) => {
                        let rel = crate::contract::build_exposes_rel(
                            scope,
                            stable_source_key,
                            op,
                            provenance,
                            now,
                        );
                        match block_on(repo.put_relationship(rel)) {
                            Ok(_) => {
                                // Both entity and edge persisted — record the key.
                                keys.push(op.normalized_key.clone());
                            }
                            Err(e) => {
                                eprintln!(
                                    "[engram-ingest] warning: failed to persist exposes edge \
                                     for '{}': {e}",
                                    op.normalized_key
                                );
                                had_write_error = true;
                                // Do NOT push key — manifest must not record an
                                // unpersisted op.
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!(
                            "[engram-ingest] warning: failed to persist Api entity for '{}': {e}",
                            op.normalized_key
                        );
                        had_write_error = true;
                    }
                }
            }
            (keys, false, had_write_error)
        }
    }
}
