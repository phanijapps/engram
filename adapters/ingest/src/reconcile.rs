//! Pre-write graph retraction for the ingest scan path.
//!
//! Before a re-ingested file's new graph is written, any prior graph(s) for
//! the same `(stable_source_key, path)` pair are deleted with their entities
//! and relationships. After deletion, if no document graphs remain for the
//! source key, the per-source `EntityKind::Repository` node is also deleted.
//!
//! This module composes the fine-grained delete ports (`delete_graph`,
//! `delete_entity`) from `KnowledgeGraphRepository` and `KnowledgeRepository`.
//! It must not grow a coarse "reconcile-source" method; that boundary is
//! enforced by RFC-0009.

use engram_domain::*;
use engram_knowledge::{CoreResult, KnowledgeGraphRepository, KnowledgeRepository};

use crate::{extractor::repo_entity_id, source_key::SOURCE_PATH_KEY};

/// Deletes any prior graph(s) for `(stable_source_key, path)` and, if the
/// last document graph for the key was just removed, deletes the per-source
/// `EntityKind::Repository` node.
///
/// Called by the scanner **before** writing a re-ingested file's new graph so
/// a changed file replaces rather than duplicates its predecessor.
///
/// Idempotent: if no prior graph exists, returns `Ok(())` without error.
pub(crate) async fn delete_prior_graphs_for_path<R>(
    repo: &R,
    scope: &Scope,
    stable_source_key: &str,
    path: &str,
) -> CoreResult<()>
where
    R: KnowledgeRepository + KnowledgeGraphRepository + ?Sized,
{
    if stable_source_key.is_empty() {
        return Ok(());
    }

    // Find prior graphs for this (key, path) pair.
    let all_graphs = repo.list_graphs_by_source(scope, stable_source_key).await?;
    let prior_graphs: Vec<KnowledgeGraph> = all_graphs
        .into_iter()
        .filter(|g| {
            g.metadata
                .as_ref()
                .and_then(|m| m.get(SOURCE_PATH_KEY))
                .and_then(|v| v.as_str())
                == Some(path)
        })
        .collect();

    for graph in &prior_graphs {
        // Hard-delete: entities and relationships cascade inside delete_graph.
        repo.delete_graph(&graph.id, scope).await?;
    }

    // NOTE: `maybe_delete_repo_node` is intentionally NOT called here.
    // Callers that are about to write a replacement graph must NOT GC the
    // repo node between the delete and the write — the replacement write
    // re-puts the repo node via upsert, so calling GC here would only widen
    // the window where the repo node is transiently absent.  The scanner's
    // serial removed-path post-pass calls `maybe_delete_repo_node` explicitly
    // after all deletions are complete.

    Ok(())
}

/// If no document graphs remain for `stable_source_key`, deletes the
/// per-source `EntityKind::Repository` node whose id is computed by
/// `repo_entity_id(scope, stable_source_key)`.
///
/// Called after any graph deletion so the Repository node is garbage-collected
/// once its last document graph is gone (T5 lifecycle).
pub(crate) async fn maybe_delete_repo_node<R>(
    repo: &R,
    scope: &Scope,
    stable_source_key: &str,
) -> CoreResult<()>
where
    R: KnowledgeRepository + KnowledgeGraphRepository + ?Sized,
{
    if stable_source_key.is_empty() {
        return Ok(());
    }
    let remaining = repo.list_graphs_by_source(scope, stable_source_key).await?;
    if remaining.is_empty() {
        let repo_id = repo_entity_id(scope, stable_source_key);
        // Returns false if already absent; that's fine — ignore the bool.
        let _ = repo.delete_entity(&repo_id, scope).await?;
    }
    Ok(())
}
