//! Scan phase 4: serial post-pass sweeps — removed-path retraction, repo-node
//! GC, and the RFC-0020 Phase 2 orphan-ledger healing.
//!
//! Relocated verbatim from `scan_repository` (scanner god-module split);
//! behavior unchanged — the scanner suite is the regression net.

use std::collections::HashSet;

use engram_domain::*;
use engram_knowledge::CoreError;

use engram_knowledge::{KnowledgeGraphRepository, KnowledgeRepository};
use futures::executor::block_on;

use super::CONTRACT_PREFIX;
use super::{ScanOptions, ScanSummary};
use crate::reconcile;

/// FIX 1(c) + FIX 3: delete graphs for prior-manifest paths that were never
/// observed this scan (genuinely-absent files) or were filtered by upgraded
/// minified-asset rules; then GC the per-source Repository node once, after
/// all deletions complete.
pub(super) fn retract_removed_paths<R>(
    repo: &R,
    opts: &ScanOptions,
    source_key: &str,
    observed_paths: &HashSet<String>,
    filtered_paths: &HashSet<String>,
    summary: &mut ScanSummary,
) -> HashSet<String>
where
    R: KnowledgeRepository + KnowledgeGraphRepository + Send + Sync,
{
    // FIX 1(c): Serial post-pass — delete graphs for paths that were in the
    // prior manifest but were never observed during this scan (genuinely-absent
    // files).  Uses `observed_paths` (FIX 3) rather than `readable` so that
    // oversize / denylisted / unclassifiable files that are still present on
    // disk are not treated as removals.
    // Exclude `contract:*` manifest entries — they are metadata keys, not file
    // paths, so they must never be treated as "removed files" and must not
    // trigger a `delete_prior_graphs_for_path` call.
    let removed_paths: Vec<String> = opts
        .manifest
        .keys()
        .filter(|k| {
            !k.starts_with(CONTRACT_PREFIX)
                && (!observed_paths.contains(*k) || filtered_paths.contains(*k))
        })
        .cloned()
        .collect();
    let mut removed_delete_failed: std::collections::HashSet<String> =
        std::collections::HashSet::new();
    for removed_path in &removed_paths {
        match block_on(reconcile::delete_prior_graphs_for_path(
            repo,
            &opts.scope,
            source_key,
            removed_path,
        )) {
            Ok(_prior_validity_of_removed_path) => {}
            Err(_) => {
                // Surface the error and keep the path in the manifest so it
                // is retried next scan rather than silently orphaned (FIX 2).
                summary.errors += 1;
                removed_delete_failed.insert(removed_path.clone());
            }
        }
    }
    // GC: check whether the per-source Repository node can be pruned now that
    // all removed-path deletions are complete.  Run once here — never before a
    // replacement write — because a replacement write re-puts the repo node via
    // upsert so pre-write GC would only widen the transient-absence window
    // (FIX 1 / adversarial Nit 6). A failed GC leaves a harmless orphan repo
    // node that converges on the next scan; surface it for observability
    // instead of swallowing it.
    if block_on(reconcile::maybe_delete_repo_node(
        repo,
        &opts.scope,
        source_key,
    ))
    .is_err()
    {
        summary.errors += 1;
    }

    removed_delete_failed
}

/// RFC-0020 Phase 2 orphan sweep: re-attempt every pending unresolved-ref
/// ledger row against this scan's symbol table. A reference whose target
/// landed in a LATER file resolves here — edge written, row flipped.
/// Relocated verbatim from `scan_repository` (scanner god-module split).
pub(super) fn heal_pending_refs<R>(
    repo: &R,
    opts: &ScanOptions,
    name_index: &engram_code::SymbolIndex,
    summary: &mut ScanSummary,
) where
    R: KnowledgeRepository + KnowledgeGraphRepository + Send + Sync,
{
    {
        let pending =
            block_on(repo.list_unresolved_refs(&opts.scope, UnresolvedReferenceStatus::Pending))
                .unwrap_or_default();
        if !pending.is_empty() {
            for row in pending {
                let reference = row.reference_name.clone();
                // Shared evidence-aware ladder (engram_code::attempt): the
                // sweep previously duplicated the EN-9 receiver rules by
                // hand; it now reuses the exact core ladder (which also adds
                // apex aliasing) and stamps typed evidence from the settling
                // branch — a capitalized-receiver heal stays heuristic.
                let out = engram_code::attempt(name_index, &reference, None, None);
                if let engram_code::Resolution::Resolved(candidate) = out.resolution {
                    let Some(branch) = out.branch else { continue };
                    let scope = row.scope.clone();
                    let subject_id = row.from_entity_id.clone();
                    let object_id = Id::from(candidate.id.clone());
                    let graph_id = row.graph_id.clone();
                    // The healed edge keeps the reference's own shape: an
                    // Endpoint subject (a route) heals as `routes_to`,
                    // everything else as `calls` (the row carries no
                    // predicate; the kind comes from the subject entity).
                    let subject_entity =
                        block_on(repo.get_entity(&row.from_entity_id, &opts.scope))
                            .ok()
                            .flatten();
                    let subject_kind = subject_entity.as_ref().map(|entity| entity.kind.clone());
                    let predicate = match subject_kind {
                        Some(engram_domain::EntityKind::Endpoint) => "routes_to",
                        _ => "calls",
                    };
                    let rel = KnowledgeRelationship {
                        id: RelationshipId::from(format!("sweep-{subject_id}-{object_id}")),
                        graph_id,
                        subject: EntityRef {
                            id: Some(subject_id.clone()),
                            kind: None,
                            // Canonical names (code-graph-quality follow-up):
                            // healed edges previously carried an EMPTY subject
                            // name and the AS-WRITTEN bare object reference —
                            // invisible to name-keyed navigation. Stamp both
                            // endpoints with the resolved entities' names.
                            name: subject_entity.as_ref().map(|e| e.name.clone()),
                            aliases: Vec::new(),
                        },
                        predicate: predicate.to_owned(),
                        // Typed heal evidence from the shared ladder's
                        // settling branch (scoped_name or heuristic).
                        code_resolution: Some(engram_code::sweep_evidence(branch)),
                        object: EntityRef {
                            id: Some(object_id.clone()),
                            kind: None,
                            name: Some(
                                name_index
                                    .lookup(object_id.as_str())
                                    .map(|c| c.name.clone())
                                    .unwrap_or(reference.clone()),
                            ),
                            aliases: Vec::new(),
                        },
                        scope: scope.clone(),
                        evidence: Vec::new(),
                        confidence: Some(0.9),
                        provenance: Provenance {
                            source: "engram-orphans-sweep".to_owned(),
                            actor: opts.actor.clone(),
                            observed_at: chrono::Utc::now(),
                            evidence: Vec::new(),
                            derivations: Vec::new(),
                            confidence: None,
                            method: Some("deterministic_orphan_sweep".to_owned()),
                        },
                        created_at: chrono::Utc::now(),
                        updated_at: None,
                        archived_at: None,
                    };
                    let row_id = row.id.clone();
                    if (block_on(async {
                        repo.put_relationship(rel).await?;
                        repo.update_unresolved_status(
                            &row_id,
                            UnresolvedReferenceStatus::Resolved,
                            &scope,
                        )
                        .await?;
                        Ok::<(), CoreError>(())
                    }))
                    .is_err()
                    {
                        summary.errors += 1;
                    }
                }
            }
        }
    }
}
