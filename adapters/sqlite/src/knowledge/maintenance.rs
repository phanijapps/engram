//! SQLite implementation of the graph-maintenance port (ADR-0027).
//!
//! Reversibility is archive/restore only. `archived_at` is promoted to a real
//! column (filterable in SQL) AND mirrored into `record_json` (so the domain
//! object read back is consistent — the `put_*` writers keep both in lockstep).
//! The archive/restore primitives take a `&Connection` + the maintenance `Actor`
//! so T5a's `apply_plan` can stage several in one transaction; every mutation
//! re-stamps `Provenance` (actor/method/observed_at) so it is attributable.

use async_trait::async_trait;
use chrono::Utc;
use engram_domain::*;
use engram_knowledge::GraphMaintenanceRepository;
use engram_runtime::{CoreError, CoreResult};
use rusqlite::{Connection, OptionalExtension, params};
use std::collections::{HashMap, HashSet};

use crate::knowledge::SqlKnowledgeStore;
use crate::knowledge::schema::{json_error, sql_error};
use crate::knowledge::scope::scope_allows;

/// `tenant + COALESCE(workspace, '')` — the SQL-side scope pre-filter used by
/// the count/list methods; the precise subject/session/environment check is then
/// applied in-memory via `scope_allows` (matching the existing list precedent).
fn workspace_param(scope: &Scope) -> &str {
    scope.workspace.as_deref().unwrap_or("")
}

fn parse_cursor(after: &Option<Cursor>) -> CoreResult<i64> {
    match after {
        Some(c) => c
            .as_str()
            .parse::<i64>()
            .map_err(|_| CoreError::InvalidRequest {
                reason: format!("invalid maintenance cursor: {}", c.as_str()),
            }),
        None => Ok(0),
    }
}

/// Re-stamp provenance for a maintenance mutation: keep the record's source,
/// evidence, derivations, and confidence; attribute the action itself
/// (actor/method/observed_at) so the mutation is attributable (AC11, ADR-0027).
fn stamp_provenance(orig: &Provenance, actor: &Actor, method: &str, ts: Timestamp) -> Provenance {
    Provenance {
        source: orig.source.clone(),
        actor: actor.clone(),
        observed_at: ts,
        evidence: orig.evidence.clone(),
        derivations: orig.derivations.clone(),
        confidence: orig.confidence,
        method: Some(method.to_string()),
    }
}

#[async_trait]
impl GraphMaintenanceRepository for SqlKnowledgeStore {
    async fn list_entities(
        &self,
        scope: &Scope,
        filter: &EntityFilter,
        after: Option<Cursor>,
        limit: usize,
    ) -> CoreResult<Page<KnowledgeEntity>> {
        let page_limit = limit.clamp(1, 500) as i64;
        let sql_limit = page_limit + 1;
        let after_rowid = parse_cursor(&after)?;
        let workspace = workspace_param(scope);
        let include = if filter.include_archived { 1i64 } else { 0 };
        let graph = filter
            .graph_id
            .as_ref()
            .map(ToString::to_string)
            .unwrap_or_default();

        let conn = self.lock()?;
        let mut stmt = conn
            .prepare(
                "SELECT rowid, record_json FROM knowledge_entities
                 WHERE tenant = ?1 AND COALESCE(workspace, '') = ?2 AND rowid > ?3
                   AND (?4 = 1 OR archived_at IS NULL)
                   AND (?5 = '' OR graph_id = ?5)
                 ORDER BY rowid LIMIT ?6",
            )
            .map_err(sql_error)?;
        let rows = stmt
            .query_map(
                params![
                    scope.tenant,
                    workspace,
                    after_rowid,
                    include,
                    graph,
                    sql_limit
                ],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
            )
            .map_err(sql_error)?;
        let mut fetched: Vec<(i64, String)> = Vec::new();
        for r in rows {
            fetched.push(r.map_err(sql_error)?);
        }
        drop(stmt);

        let has_more = fetched.len() as i64 > page_limit;
        fetched.truncate(page_limit as usize);
        let next_rowid = fetched.last().map(|(r, _)| *r);
        let items: Vec<KnowledgeEntity> = fetched
            .into_iter()
            .filter_map(|(_, json)| {
                let e: KnowledgeEntity = serde_json::from_str(&json).ok()?;
                if !scope_allows(&e.scope, scope) {
                    return None;
                }
                if !filter.kinds.is_empty() && !filter.kinds.contains(&e.kind) {
                    return None;
                }
                if let Some(sid) = &filter.source_id {
                    if !entity_from_source(&e, sid) {
                        return None;
                    }
                }
                if let Some(thr) = filter.min_confidence {
                    if let Some(c) = e.provenance.confidence {
                        if c < thr {
                            return None;
                        }
                    }
                }
                Some(e)
            })
            .collect();
        let next_cursor = if has_more {
            next_rowid.map(|r| Cursor::new(r.to_string()))
        } else {
            None
        };
        Ok(Page::new(items, next_cursor))
    }

    async fn list_relationships(
        &self,
        scope: &Scope,
        filter: &RelationshipFilter,
        after: Option<Cursor>,
        limit: usize,
    ) -> CoreResult<Page<KnowledgeRelationship>> {
        let page_limit = limit.clamp(1, 500) as i64;
        let sql_limit = page_limit + 1;
        let after_rowid = parse_cursor(&after)?;
        let workspace = workspace_param(scope);
        let include = if filter.include_archived { 1i64 } else { 0 };
        let graph = filter
            .graph_id
            .as_ref()
            .map(ToString::to_string)
            .unwrap_or_default();

        let conn = self.lock()?;
        let mut stmt = conn
            .prepare(
                "SELECT rowid, record_json FROM knowledge_relationships
                 WHERE tenant = ?1 AND COALESCE(workspace, '') = ?2 AND rowid > ?3
                   AND (?4 = 1 OR archived_at IS NULL)
                   AND (?5 = '' OR graph_id = ?5)
                 ORDER BY rowid LIMIT ?6",
            )
            .map_err(sql_error)?;
        let rows = stmt
            .query_map(
                params![
                    scope.tenant,
                    workspace,
                    after_rowid,
                    include,
                    graph,
                    sql_limit
                ],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
            )
            .map_err(sql_error)?;
        let mut fetched: Vec<(i64, String)> = Vec::new();
        for r in rows {
            fetched.push(r.map_err(sql_error)?);
        }
        drop(stmt);

        let has_more = fetched.len() as i64 > page_limit;
        fetched.truncate(page_limit as usize);
        let next_rowid = fetched.last().map(|(r, _)| *r);
        let items: Vec<KnowledgeRelationship> = fetched
            .into_iter()
            .filter_map(|(_, json)| {
                let r: KnowledgeRelationship = serde_json::from_str(&json).ok()?;
                if !scope_allows(&r.scope, scope) {
                    return None;
                }
                if let Some(p) = &filter.predicate {
                    if &r.predicate != p {
                        return None;
                    }
                }
                if let Some(sid) = &filter.source_id {
                    if !relationship_from_source(&r, sid) {
                        return None;
                    }
                }
                if let Some(thr) = filter.min_confidence {
                    if let Some(c) = r.confidence.or(r.provenance.confidence) {
                        if c < thr {
                            return None;
                        }
                    }
                }
                Some(r)
            })
            .collect();
        let next_cursor = if has_more {
            next_rowid.map(|r| Cursor::new(r.to_string()))
        } else {
            None
        };
        Ok(Page::new(items, next_cursor))
    }

    async fn build_plan(&self, request: MaintenancePlanRequest) -> CoreResult<MaintenancePlan> {
        let scope = request.scope.clone();
        let ts = Utc::now();
        let conn = self.lock()?;
        let mut previews = Vec::with_capacity(request.mutations.len());
        for m in &request.mutations {
            previews.push(build_mutation_preview(&conn, m, &scope, ts)?);
        }
        let mut plan = MaintenancePlan::new(
            request.graph_id,
            request.scope,
            request.mutations,
            request.policy,
            request.actor,
        );
        plan.previews = previews;
        Ok(plan)
    }

    async fn apply_plan(
        &self,
        plan: &MaintenancePlan,
        mode: ApplyMode,
    ) -> CoreResult<MaintenanceApplyResult> {
        let fingerprint = plan.fingerprint.clone();
        // Preview: report shape without committing (no writes).
        if matches!(mode, ApplyMode::Preview) {
            return Ok(MaintenanceApplyResult {
                applied: 0,
                unchanged: 0,
                failed: 0,
                by_kind: Vec::new(),
                verify_findings: Vec::new(),
                atomicity: Atomicity::BackendDependent,
                plan_fingerprint: fingerprint,
            });
        }
        let scope = &plan.scope;
        let actor = &plan.actor;
        let ts = Utc::now();
        let mut conn = self.lock()?;
        let tx = conn.transaction().map_err(sql_error)?;

        // Entities this plan removes from the active graph (deleted, or archived —
        // archive cascades its incident edges, so only a *leftover* active edge
        // referencing one of these is a plan-induced dangling reference).
        let targeted: HashSet<String> = plan
            .mutations
            .iter()
            .flat_map(|m| match m {
                MaintenanceMutation::Delete {
                    target: MaintenanceTarget::Entity(id),
                }
                | MaintenanceMutation::Archive {
                    target: MaintenanceTarget::Entity(id),
                } => vec![id.to_string()],
                // A correct merge leaves no active edge referencing an absorbed id;
                // the verify backstop catches a redirect miss.
                MaintenanceMutation::Merge { absorbed, .. } => {
                    absorbed.iter().map(|i| i.to_string()).collect()
                }
                _ => vec![],
            })
            .collect();

        let mut applied: u32 = 0;
        let mut unchanged: u32 = 0;
        let mut failed: u32 = 0;
        let mut by_kind: Vec<ApplyKindCount> = Vec::new();
        for m in &plan.mutations {
            let kind = m.kind();
            // Store errors propagate as Err (not swallowed); only Unsupported is soft.
            let slot = match stage_mutation(&tx, m, scope, actor, ts)? {
                StageOutcome::Changed => 0,
                StageOutcome::Unchanged => 1,
            };
            match slot {
                0 => applied += 1,
                1 => unchanged += 1,
                _ => failed += 1,
            }
            let entry = if let Some(e) = by_kind.iter_mut().find(|c| c.kind == kind) {
                e
            } else {
                by_kind.push(ApplyKindCount {
                    kind,
                    applied: 0,
                    unchanged: 0,
                    failed: 0,
                });
                by_kind.last_mut().expect("just pushed")
            };
            match slot {
                0 => entry.applied += 1,
                1 => entry.unchanged += 1,
                _ => entry.failed += 1,
            }
        }

        // Referential-integrity verify before commit, scoped to the plan's targeted
        // entities so pre-existing damage elsewhere stays repairable.
        let verify_findings = if failed == 0 {
            verify_referential_integrity(&tx, scope, &targeted)?
        } else {
            Vec::new()
        };

        if failed > 0 || !verify_findings.is_empty() {
            // Rollback: nothing committed. Staged-but-uncommitted mutations are
            // failures, so by_kind.applied folds into by_kind.failed.
            for entry in by_kind.iter_mut() {
                entry.failed += entry.applied;
                entry.applied = 0;
            }
            return Ok(MaintenanceApplyResult {
                applied: 0,
                unchanged,
                failed: applied + failed + verify_findings.len() as u32,
                by_kind,
                verify_findings,
                atomicity: Atomicity::SingleTransaction,
                plan_fingerprint: fingerprint,
            });
        }
        tx.commit().map_err(sql_error)?;
        Ok(MaintenanceApplyResult {
            applied,
            unchanged,
            failed,
            by_kind,
            verify_findings: Vec::new(),
            atomicity: Atomicity::SingleTransaction,
            plan_fingerprint: fingerprint,
        })
    }
}

/// Best-effort source match for an entity: provenance source or any source_ref.
fn entity_from_source(e: &KnowledgeEntity, source_id: &SourceId) -> bool {
    let s = source_id.as_str();
    e.provenance.source == s
        || e.source_refs
            .iter()
            .any(|r| r.target_id.as_deref() == Some(s))
}

/// Best-effort source match for a relationship: provenance source or any evidence.
fn relationship_from_source(r: &KnowledgeRelationship, source_id: &SourceId) -> bool {
    let s = source_id.as_str();
    r.provenance.source == s || r.evidence.iter().any(|e| e.target_id.as_deref() == Some(s))
}

// ── Plan staging (apply) ─────────────────────────────────────────────────────

/// Outcome of staging one mutation: changed state, or already in target state
/// (idempotent). Store errors propagate as `Err` (not swallowed) so a real
/// SQL/IO failure surfaces instead of a silent `failed`.
enum StageOutcome {
    Changed,
    Unchanged,
}

/// Stage one mutation against an open connection (inside apply_plan's tx).
fn stage_mutation(
    conn: &Connection,
    m: &MaintenanceMutation,
    scope: &Scope,
    actor: &Actor,
    ts: Timestamp,
) -> CoreResult<StageOutcome> {
    match m {
        MaintenanceMutation::Archive { target } => {
            Ok(if archive_target(conn, target, scope, actor, ts, true)? {
                StageOutcome::Changed
            } else {
                StageOutcome::Unchanged
            })
        }
        MaintenanceMutation::Restore { target } => {
            Ok(if archive_target(conn, target, scope, actor, ts, false)? {
                StageOutcome::Changed
            } else {
                StageOutcome::Unchanged
            })
        }
        MaintenanceMutation::Delete { target } => Ok(if delete_target(conn, target, scope)? {
            StageOutcome::Changed
        } else {
            StageOutcome::Unchanged
        }),
        MaintenanceMutation::AddAlias { entity, alias } => {
            stage_alias(conn, entity, alias, true, scope, actor, ts)
        }
        MaintenanceMutation::RemoveAlias { entity, alias } => {
            stage_alias(conn, entity, alias, false, scope, actor, ts)
        }
        MaintenanceMutation::RewriteRelationship {
            relationship,
            new_predicate,
            new_subject,
            new_object,
        } => stage_rewrite(
            conn,
            relationship,
            new_predicate.as_ref(),
            new_subject.as_ref(),
            new_object.as_ref(),
            scope,
            actor,
            ts,
        ),
        MaintenanceMutation::Merge { survivor, absorbed } => {
            stage_merge(conn, survivor, absorbed, scope, actor, ts)
        }
    }
}

fn archive_target(
    conn: &Connection,
    target: &MaintenanceTarget,
    scope: &Scope,
    actor: &Actor,
    ts: Timestamp,
    archive: bool,
) -> CoreResult<bool> {
    match target {
        MaintenanceTarget::Entity(id) => {
            if archive {
                archive_entity(conn, id, scope, actor, ts)
            } else {
                restore_entity(conn, id, scope, actor, ts)
            }
        }
        MaintenanceTarget::Relationship(id) => {
            if archive {
                archive_relationship(conn, id, scope, actor, ts)
            } else {
                restore_relationship(conn, id, scope, actor, ts)
            }
        }
    }
}

fn delete_target(conn: &Connection, target: &MaintenanceTarget, scope: &Scope) -> CoreResult<bool> {
    match target {
        MaintenanceTarget::Entity(id) => hard_delete_entity(conn, id, scope),
        MaintenanceTarget::Relationship(id) => hard_delete_relationship(conn, id, scope),
    }
}

/// Add or remove an alias on an entity (idempotent). Re-stamps Provenance.
fn stage_alias(
    conn: &Connection,
    entity_id: &EntityId,
    alias: &str,
    add: bool,
    scope: &Scope,
    actor: &Actor,
    ts: Timestamp,
) -> CoreResult<StageOutcome> {
    let Some(mut entity) = load_entity(conn, entity_id, scope)? else {
        return Ok(StageOutcome::Unchanged);
    };
    if !scope_allows(&entity.scope, scope) {
        return Ok(StageOutcome::Unchanged);
    }
    let already = entity.aliases.iter().any(|a| a == alias);
    let changed = if add {
        if already {
            false
        } else {
            entity.aliases.push(alias.to_string());
            true
        }
    } else if already {
        entity.aliases.retain(|a| a != alias);
        true
    } else {
        false
    };
    if !changed {
        return Ok(StageOutcome::Unchanged);
    }
    entity.updated_at = Some(ts);
    entity.provenance = stamp_provenance(
        &entity.provenance,
        actor,
        if add { "add_alias" } else { "remove_alias" },
        ts,
    );
    let json = serde_json::to_string(&entity).map_err(json_error)?;
    conn.execute(
        "UPDATE knowledge_entities SET record_json = ?1 WHERE id = ?2",
        params![json, entity_id.to_string()],
    )
    .map_err(sql_error)?;
    Ok(StageOutcome::Changed)
}

/// Rewrite a relationship's predicate/subject/object in place (idempotent).
fn stage_rewrite(
    conn: &Connection,
    rel_id: &RelationshipId,
    new_predicate: Option<&String>,
    new_subject: Option<&EntityId>,
    new_object: Option<&EntityId>,
    scope: &Scope,
    actor: &Actor,
    ts: Timestamp,
) -> CoreResult<StageOutcome> {
    let Some(mut rel) = load_relationship(conn, rel_id, scope)? else {
        return Ok(StageOutcome::Unchanged);
    };
    if !scope_allows(&rel.scope, scope) {
        return Ok(StageOutcome::Unchanged);
    }
    let mut changed = false;
    if let Some(p) = new_predicate {
        if &rel.predicate != p {
            rel.predicate = p.clone();
            changed = true;
        }
    }
    if let Some(s) = new_subject {
        if rel.subject.id.as_ref() != Some(s) {
            rel.subject.id = Some(s.clone());
            changed = true;
        }
    }
    if let Some(o) = new_object {
        if rel.object.id.as_ref() != Some(o) {
            rel.object.id = Some(o.clone());
            changed = true;
        }
    }
    if !changed {
        return Ok(StageOutcome::Unchanged);
    }
    rel.updated_at = Some(ts);
    rel.provenance = stamp_provenance(&rel.provenance, actor, "rewrite_relationship", ts);
    let json = serde_json::to_string(&rel).map_err(json_error)?;
    let subject_id = rel
        .subject
        .id
        .as_ref()
        .map(ToString::to_string)
        .unwrap_or_default();
    conn.execute(
        "UPDATE knowledge_relationships SET subject_id = ?1, record_json = ?2 WHERE id = ?3",
        params![subject_id, json, rel_id.to_string()],
    )
    .map_err(sql_error)?;
    Ok(StageOutcome::Changed)
}

/// Merge `absorbed` entities into the `survivor`: fold their aliases/source_refs
/// into the survivor, redirect their incident edges to the survivor, then ARCHIVE
/// (not hard-delete) the absorbed entities and coalesced duplicate edges
/// (ADR-0027). Runs against the open tx; Provenance re-stamped.
fn stage_merge(
    conn: &Connection,
    survivor_id: &EntityId,
    absorbed_ids: &[EntityId],
    scope: &Scope,
    actor: &Actor,
    ts: Timestamp,
) -> CoreResult<StageOutcome> {
    let survivor_str = survivor_id.to_string();
    let Some(mut survivor) = load_entity(conn, survivor_id, scope)? else {
        return Ok(StageOutcome::Unchanged);
    };
    if !scope_allows(&survivor.scope, scope) {
        return Ok(StageOutcome::Unchanged);
    }
    // Fold each absorbed entity's refs into the survivor. Track whether anything
    // actually changed so the survivor row is re-written (and the outcome reported
    // Changed) only on a real fold — a re-apply must not re-stamp the survivor.
    let mut fold_changed = false;
    for absorbed_id in absorbed_ids {
        let Some(absorbed) = load_entity(conn, absorbed_id, scope)? else {
            continue;
        };
        if !scope_allows(&absorbed.scope, scope) {
            continue;
        }
        for a in &absorbed.aliases {
            if !survivor.aliases.contains(a) {
                survivor.aliases.push(a.clone());
                fold_changed = true;
            }
        }
        for r in &absorbed.source_refs {
            if !survivor.source_refs.contains(r) {
                survivor.source_refs.push(r.clone());
                fold_changed = true;
            }
        }
        for c in &absorbed.concept_refs {
            if !survivor.concept_refs.contains(c) {
                survivor.concept_refs.push(c.clone());
                fold_changed = true;
            }
        }
        for o in &absorbed.ontology_class_refs {
            if !survivor.ontology_class_refs.contains(o) {
                survivor.ontology_class_refs.push(o.clone());
                fold_changed = true;
            }
        }
    }
    if fold_changed {
        survivor.updated_at = Some(ts);
        survivor.provenance = stamp_provenance(&survivor.provenance, actor, "merge", ts);
        let sjson = serde_json::to_string(&survivor).map_err(json_error)?;
        conn.execute(
            "UPDATE knowledge_entities SET record_json = ?1 WHERE id = ?2",
            params![sjson, survivor_str],
        )
        .map_err(sql_error)?;
    }

    // Redirect each absorbed entity's edges to the survivor, then archive it.
    let mut changed = fold_changed;
    for absorbed_id in absorbed_ids {
        let absorbed_str = absorbed_id.to_string();
        redirect_relationships_to(conn, &absorbed_str, &survivor_str, scope)?;
        if archive_entity(conn, absorbed_id, scope, actor, ts)? {
            changed = true;
        }
    }
    // Coalesce duplicate edges incident to the survivor (archive the losers).
    coalesce_survivor_relationships(conn, &survivor_str, scope, actor, ts)?;
    Ok(if changed {
        StageOutcome::Changed
    } else {
        StageOutcome::Unchanged
    })
}

/// Redirect every active edge whose subject or object endpoint is `from` to `to`,
/// recomputing the relationship_key. Subject endpoint uses the indexed
/// `subject_id` column; object endpoint lives only in record_json.
fn redirect_relationships_to(
    conn: &Connection,
    from: &str,
    to: &str,
    scope: &Scope,
) -> CoreResult<()> {
    let ws = workspace_param(scope);
    let update = |rel: &mut KnowledgeRelationship| {
        let mut changed = false;
        if rel.subject.id.as_ref().map(|i| i.as_str()) == Some(from) {
            rel.subject.id = Some(EntityId::from(to));
            changed = true;
        }
        if rel.object.id.as_ref().map(|i| i.as_str()) == Some(from) {
            rel.object.id = Some(EntityId::from(to));
            changed = true;
        }
        changed
    };
    let write = |conn: &Connection, rel: &KnowledgeRelationship, id: &str| -> CoreResult<()> {
        let json = serde_json::to_string(rel).map_err(json_error)?;
        let subject_id = rel
            .subject
            .id
            .as_ref()
            .map(ToString::to_string)
            .unwrap_or_default();
        let rkey = engram_knowledge::identity::compute_relationship_key(rel);
        conn.execute(
            "UPDATE knowledge_relationships SET subject_id = ?1, record_json = ?2, relationship_key = ?3 WHERE id = ?4",
            params![subject_id, json, rkey, id],
        )
        .map_err(sql_error)?;
        Ok(())
    };

    // Subject endpoint (indexed column).
    let mut stmt = conn
        .prepare(
            "SELECT id, record_json FROM knowledge_relationships
             WHERE subject_id = ?1 AND tenant = ?2 AND COALESCE(workspace, '') = ?3 AND archived_at IS NULL",
        )
        .map_err(sql_error)?;
    let rows: Vec<(String, String)> = stmt
        .query_map(params![from, scope.tenant, ws], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })
        .map_err(sql_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql_error)?;
    drop(stmt);
    for (id, json) in rows {
        let mut rel: KnowledgeRelationship = serde_json::from_str(&json).map_err(json_error)?;
        if !scope_allows(&rel.scope, scope) {
            continue;
        }
        if update(&mut rel) {
            write(conn, &rel, &id)?;
        }
    }

    // Object endpoint (record_json only) — catches edges whose object is `from`.
    let pat = format!("%{from}%");
    let mut stmt = conn
        .prepare(
            "SELECT id, record_json FROM knowledge_relationships
             WHERE record_json LIKE ?1 AND tenant = ?2 AND COALESCE(workspace, '') = ?3 AND archived_at IS NULL",
        )
        .map_err(sql_error)?;
    let rows: Vec<(String, String)> = stmt
        .query_map(params![pat, scope.tenant, ws], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })
        .map_err(sql_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql_error)?;
    drop(stmt);
    for (id, json) in rows {
        let mut rel: KnowledgeRelationship = serde_json::from_str(&json).map_err(json_error)?;
        if !scope_allows(&rel.scope, scope) {
            continue;
        }
        if update(&mut rel) {
            write(conn, &rel, &id)?;
        }
    }
    Ok(())
}

/// Among the survivor's active edges, archive duplicates by relationship_key
/// (keep the lowest-rowid one per key). Scoped to subject_id = survivor so it only
/// collapses edges the merge brought together.
fn coalesce_survivor_relationships(
    conn: &Connection,
    survivor: &str,
    scope: &Scope,
    actor: &Actor,
    ts: Timestamp,
) -> CoreResult<()> {
    let ws = workspace_param(scope);
    // Load active edges incident to the survivor on EITHER endpoint, group by
    // computed relationship_key in Rust (robust to a NULL relationship_key column),
    // and archive the losers — so both outgoing and incoming duplicates collapse.
    let mut stmt = conn
        .prepare(
            "SELECT rowid, record_json FROM knowledge_relationships
             WHERE tenant = ?1 AND COALESCE(workspace, '') = ?2 AND archived_at IS NULL",
        )
        .map_err(sql_error)?;
    let rows: Vec<(i64, String)> = stmt
        .query_map(params![scope.tenant, ws], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
        })
        .map_err(sql_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql_error)?;
    drop(stmt);

    let mut entries: Vec<(i64, KnowledgeRelationship)> = Vec::new();
    for (rowid, json) in rows {
        let rel: KnowledgeRelationship = serde_json::from_str(&json).map_err(json_error)?;
        if !scope_allows(&rel.scope, scope) {
            continue;
        }
        let incident = rel
            .subject
            .id
            .as_ref()
            .map(|i| i.as_str() == survivor)
            .unwrap_or(false)
            || rel
                .object
                .id
                .as_ref()
                .map(|i| i.as_str() == survivor)
                .unwrap_or(false);
        if incident {
            entries.push((rowid, rel));
        }
    }
    entries.sort_by_key(|(rowid, _)| *rowid);
    let mut seen: HashMap<String, i64> = HashMap::new();
    let mut to_archive: Vec<(i64, KnowledgeRelationship)> = Vec::new();
    for (rowid, rel) in entries {
        let key = engram_knowledge::identity::compute_relationship_key(&rel);
        if seen.contains_key(&key) {
            to_archive.push((rowid, rel));
        } else {
            seen.insert(key, rowid);
        }
    }
    for (rowid, mut rel) in to_archive {
        rel.archived_at = Some(ts);
        rel.updated_at = Some(ts);
        rel.provenance = stamp_provenance(&rel.provenance, actor, "merge_coalesce", ts);
        let new_json = serde_json::to_string(&rel).map_err(json_error)?;
        conn.execute(
            "UPDATE knowledge_relationships SET archived_at = ?1, record_json = ?2 WHERE rowid = ?3",
            params![ts.to_rfc3339(), new_json, rowid],
        )
        .map_err(sql_error)?;
    }
    Ok(())
}

/// Hard-delete an entity (escalated, permanent — ADR-0027). Scope-checked.
fn hard_delete_entity(conn: &Connection, id: &EntityId, scope: &Scope) -> CoreResult<bool> {
    let Some(entity) = load_entity(conn, id, scope)? else {
        return Ok(false);
    };
    if !scope_allows(&entity.scope, scope) {
        return Ok(false);
    }
    conn.execute(
        "DELETE FROM knowledge_entities WHERE id = ?1",
        params![id.to_string()],
    )
    .map_err(sql_error)?;
    Ok(true)
}

/// Hard-delete a relationship. Scope-checked.
fn hard_delete_relationship(
    conn: &Connection,
    id: &RelationshipId,
    scope: &Scope,
) -> CoreResult<bool> {
    let Some(rel) = load_relationship(conn, id, scope)? else {
        return Ok(false);
    };
    if !scope_allows(&rel.scope, scope) {
        return Ok(false);
    }
    conn.execute(
        "DELETE FROM knowledge_relationships WHERE id = ?1",
        params![id.to_string()],
    )
    .map_err(sql_error)?;
    Ok(true)
}

/// Load a scope-visible entity by id (SQL tenant/workspace pre-filter).
fn load_entity(
    conn: &Connection,
    id: &EntityId,
    scope: &Scope,
) -> CoreResult<Option<KnowledgeEntity>> {
    let ws = workspace_param(scope);
    let json: Option<String> = conn
        .query_row(
            "SELECT record_json FROM knowledge_entities WHERE id = ?1 AND tenant = ?2 AND COALESCE(workspace, '') = ?3",
            params![id.to_string(), scope.tenant, ws],
            |row| row.get(0),
        )
        .optional()
        .map_err(sql_error)?;
    json.map(|j| serde_json::from_str(&j).map_err(json_error))
        .transpose()
}

/// Load a scope-visible relationship by id.
fn load_relationship(
    conn: &Connection,
    id: &RelationshipId,
    scope: &Scope,
) -> CoreResult<Option<KnowledgeRelationship>> {
    let ws = workspace_param(scope);
    let json: Option<String> = conn
        .query_row(
            "SELECT record_json FROM knowledge_relationships WHERE id = ?1 AND tenant = ?2 AND COALESCE(workspace, '') = ?3",
            params![id.to_string(), scope.tenant, ws],
            |row| row.get(0),
        )
        .optional()
        .map_err(sql_error)?;
    json.map(|j| serde_json::from_str(&j).map_err(json_error))
        .transpose()
}

// ── Dry-run preview ──────────────────────────────────────────────────────────

#[derive(Clone, Copy)]
enum PreviewEffect {
    Archive,
    Restore,
    Delete,
}

fn build_mutation_preview(
    conn: &Connection,
    m: &MaintenanceMutation,
    scope: &Scope,
    ts: Timestamp,
) -> CoreResult<MaintenanceMutationPreview> {
    let mutation = m.clone();
    let (before, after) = match m {
        MaintenanceMutation::Archive { target } => {
            preview_target(conn, target, scope, ts, PreviewEffect::Archive)?
        }
        MaintenanceMutation::Restore { target } => {
            preview_target(conn, target, scope, ts, PreviewEffect::Restore)?
        }
        MaintenanceMutation::Delete { target } => {
            preview_target(conn, target, scope, ts, PreviewEffect::Delete)?
        }
        MaintenanceMutation::AddAlias { entity, alias } => {
            let cur = load_entity(conn, entity, scope)?;
            let after = cur.clone().map(|mut e| {
                if !e.aliases.contains(alias) {
                    e.aliases.push(alias.clone());
                }
                e
            });
            (snap_entity(cur), snap_entity(after))
        }
        MaintenanceMutation::RemoveAlias { entity, alias } => {
            let cur = load_entity(conn, entity, scope)?;
            let after = cur.clone().map(|mut e| {
                e.aliases.retain(|a| a != alias);
                e
            });
            (snap_entity(cur), snap_entity(after))
        }
        MaintenanceMutation::RewriteRelationship {
            relationship,
            new_predicate,
            new_subject,
            new_object,
        } => {
            let cur = load_relationship(conn, relationship, scope)?;
            let after = cur.clone().map(|mut r| {
                if let Some(p) = new_predicate {
                    r.predicate = p.clone();
                }
                if let Some(s) = new_subject {
                    r.subject.id = Some(s.clone());
                }
                if let Some(o) = new_object {
                    r.object.id = Some(o.clone());
                }
                r
            });
            (snap_rel(cur), snap_rel(after))
        }
        MaintenanceMutation::Merge { survivor, absorbed } => {
            let mut before = Vec::new();
            let mut surv = load_entity(conn, survivor, scope)?;
            if let Some(s) = &surv {
                before.push(MutationSnapshot::Entity(s.clone()));
            }
            for a_id in absorbed {
                if let Some(a) = load_entity(conn, a_id, scope)? {
                    before.push(MutationSnapshot::Entity(a.clone()));
                    if let Some(s) = &mut surv {
                        for al in &a.aliases {
                            if !s.aliases.contains(al) {
                                s.aliases.push(al.clone());
                            }
                        }
                    }
                }
            }
            (before, snap_entity(surv))
        }
    };
    Ok(MaintenanceMutationPreview {
        mutation,
        before,
        after,
    })
}

/// Build before/after snapshots for a target under a preview effect.
fn preview_target(
    conn: &Connection,
    target: &MaintenanceTarget,
    scope: &Scope,
    ts: Timestamp,
    effect: PreviewEffect,
) -> CoreResult<(Vec<MutationSnapshot>, Vec<MutationSnapshot>)> {
    Ok(match target {
        MaintenanceTarget::Entity(id) => {
            let cur = load_entity(conn, id, scope)?;
            let after = match effect {
                PreviewEffect::Archive => cur.clone().map(|mut e| {
                    e.archived_at = Some(ts);
                    e
                }),
                PreviewEffect::Restore => cur.clone().map(|mut e| {
                    e.archived_at = None;
                    e
                }),
                PreviewEffect::Delete => None,
            };
            (snap_entity(cur), snap_entity(after))
        }
        MaintenanceTarget::Relationship(id) => {
            let cur = load_relationship(conn, id, scope)?;
            let after = match effect {
                PreviewEffect::Archive => cur.clone().map(|mut r| {
                    r.archived_at = Some(ts);
                    r
                }),
                PreviewEffect::Restore => cur.clone().map(|mut r| {
                    r.archived_at = None;
                    r
                }),
                PreviewEffect::Delete => None,
            };
            (snap_rel(cur), snap_rel(after))
        }
    })
}

fn snap_entity(e: Option<KnowledgeEntity>) -> Vec<MutationSnapshot> {
    e.map(MutationSnapshot::Entity).into_iter().collect()
}

fn snap_rel(r: Option<KnowledgeRelationship>) -> Vec<MutationSnapshot> {
    r.map(MutationSnapshot::Relationship).into_iter().collect()
}

// ── Referential-integrity verify ─────────────────────────────────────────────

/// Enforceable integrity gate (not advisory `validate_graph`): every ACTIVE
/// relationship's subject/object id must resolve to an ACTIVE entity. Run against
/// the open transaction so it sees staged mutations; findings trip rollback.
fn verify_referential_integrity(
    tx: &Connection,
    scope: &Scope,
    targeted: &HashSet<String>,
) -> CoreResult<Vec<MaintenanceVerifyFinding>> {
    if targeted.is_empty() {
        return Ok(Vec::new());
    }
    let ws = workspace_param(scope);
    let mut findings = Vec::new();
    let mut stmt = tx
        .prepare(
            "SELECT id, record_json FROM knowledge_relationships
             WHERE tenant = ?1 AND COALESCE(workspace, '') = ?2 AND archived_at IS NULL",
        )
        .map_err(sql_error)?;
    let rows = stmt
        .query_map(params![scope.tenant, ws], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(sql_error)?;
    for r in rows {
        let (id, json) = r.map_err(sql_error)?;
        let Ok(rel) = serde_json::from_str::<KnowledgeRelationship>(&json) else {
            continue;
        };
        if !scope_allows(&rel.scope, scope) {
            continue;
        }
        // A plan-induced dangling reference: an active edge whose subject or object
        // is an entity the plan removed (deleted, or archived without its edge
        // archived too). Archive cascades edges, so only Delete (or a missed edge)
        // trips this — pre-existing damage elsewhere is left repairable.
        let subj_dangling = rel
            .subject
            .id
            .as_ref()
            .map(|i| targeted.contains(i.as_str()))
            .unwrap_or(false);
        let obj_dangling = rel
            .object
            .id
            .as_ref()
            .map(|i| targeted.contains(i.as_str()))
            .unwrap_or(false);
        if subj_dangling || obj_dangling {
            findings.push(MaintenanceVerifyFinding {
                severity: VerifySeverity::Error,
                message: format!(
                    "relationship {id} references an entity removed by the plan (dangling)"
                ),
                target: Some(MaintenanceTarget::Relationship(RelationshipId::from(
                    id.as_str(),
                ))),
            });
        }
    }
    drop(stmt);
    Ok(findings)
}

// ── Archive / restore primitives (run on a borrowed connection) ──────────────

/// Archive an entity (soft-delete): sets `archived_at` on the entity and on its
/// incident relationships (subject or object endpoint), re-stamping `Provenance`.
/// Returns `false` if the entity is missing, out of scope, or already archived.
pub fn archive_entity(
    conn: &Connection,
    id: &EntityId,
    scope: &Scope,
    actor: &Actor,
    ts: Timestamp,
) -> CoreResult<bool> {
    let id_str = id.to_string();
    let workspace = workspace_param(scope);
    let json: Option<String> = conn
        .query_row(
            "SELECT record_json FROM knowledge_entities
             WHERE id = ?1 AND tenant = ?2 AND COALESCE(workspace, '') = ?3 AND archived_at IS NULL",
            params![id_str, scope.tenant, workspace],
            |row| row.get(0),
        )
        .optional()
        .map_err(sql_error)?;
    let Some(json) = json else {
        return Ok(false);
    };
    let mut e: KnowledgeEntity = serde_json::from_str(&json).map_err(json_error)?;
    if !scope_allows(&e.scope, scope) {
        return Ok(false);
    }
    e.archived_at = Some(ts);
    e.updated_at = Some(ts);
    e.provenance = stamp_provenance(&e.provenance, actor, "archive", ts);
    let new_json = serde_json::to_string(&e).map_err(json_error)?;
    conn.execute(
        "UPDATE knowledge_entities SET archived_at = ?1, record_json = ?2 WHERE id = ?3",
        params![ts.to_rfc3339(), new_json, id_str],
    )
    .map_err(sql_error)?;
    cascade_set_incident_archived(conn, &id_str, scope, actor, ts, Some(ts))?;
    Ok(true)
}

/// Restore an entity: clears `archived_at` on the entity and on its incident
/// relationships (node and edges restore together).
pub fn restore_entity(
    conn: &Connection,
    id: &EntityId,
    scope: &Scope,
    actor: &Actor,
    ts: Timestamp,
) -> CoreResult<bool> {
    let id_str = id.to_string();
    let workspace = workspace_param(scope);
    let json: Option<String> = conn
        .query_row(
            "SELECT record_json FROM knowledge_entities
             WHERE id = ?1 AND tenant = ?2 AND COALESCE(workspace, '') = ?3 AND archived_at IS NOT NULL",
            params![id_str, scope.tenant, workspace],
            |row| row.get(0),
        )
        .optional()
        .map_err(sql_error)?;
    let Some(json) = json else {
        return Ok(false);
    };
    let mut e: KnowledgeEntity = serde_json::from_str(&json).map_err(json_error)?;
    if !scope_allows(&e.scope, scope) {
        return Ok(false);
    }
    e.archived_at = None;
    e.updated_at = Some(ts);
    e.provenance = stamp_provenance(&e.provenance, actor, "restore", ts);
    let new_json = serde_json::to_string(&e).map_err(json_error)?;
    conn.execute(
        "UPDATE knowledge_entities SET archived_at = NULL, record_json = ?1 WHERE id = ?2",
        params![new_json, id_str],
    )
    .map_err(sql_error)?;
    cascade_set_incident_archived(conn, &id_str, scope, actor, ts, None)?;
    Ok(true)
}

/// Archive a single relationship (no children to cascade).
pub fn archive_relationship(
    conn: &Connection,
    id: &RelationshipId,
    scope: &Scope,
    actor: &Actor,
    ts: Timestamp,
) -> CoreResult<bool> {
    set_relationship_archived(conn, id, scope, actor, "archive", Some(ts), ts)
}

/// Restore a single relationship.
pub fn restore_relationship(
    conn: &Connection,
    id: &RelationshipId,
    scope: &Scope,
    actor: &Actor,
    ts: Timestamp,
) -> CoreResult<bool> {
    set_relationship_archived(conn, id, scope, actor, "restore", None, ts)
}

fn set_relationship_archived(
    conn: &Connection,
    id: &RelationshipId,
    scope: &Scope,
    actor: &Actor,
    method: &str,
    archived_at: Option<Timestamp>,
    ts: Timestamp,
) -> CoreResult<bool> {
    let id_str = id.to_string();
    let workspace = workspace_param(scope);
    let guard = if archived_at.is_some() {
        "AND archived_at IS NULL"
    } else {
        "AND archived_at IS NOT NULL"
    };
    let sql = format!(
        "SELECT record_json FROM knowledge_relationships
         WHERE id = ?1 AND tenant = ?2 AND COALESCE(workspace, '') = ?3 {guard}"
    );
    let json: Option<String> = conn
        .query_row(&sql, params![id_str, scope.tenant, workspace], |row| {
            row.get(0)
        })
        .optional()
        .map_err(sql_error)?;
    let Some(json) = json else {
        return Ok(false);
    };
    let mut r: KnowledgeRelationship = serde_json::from_str(&json).map_err(json_error)?;
    if !scope_allows(&r.scope, scope) {
        return Ok(false);
    }
    r.archived_at = archived_at;
    r.updated_at = Some(ts);
    r.provenance = stamp_provenance(&r.provenance, actor, method, ts);
    let new_json = serde_json::to_string(&r).map_err(json_error)?;
    if let Some(archived_ts) = archived_at {
        conn.execute(
            "UPDATE knowledge_relationships SET archived_at = ?1, record_json = ?2 WHERE id = ?3",
            params![archived_ts.to_rfc3339(), new_json, id_str],
        )
        .map_err(sql_error)?;
    } else {
        conn.execute(
            "UPDATE knowledge_relationships SET archived_at = NULL, record_json = ?1 WHERE id = ?2",
            params![new_json, id_str],
        )
        .map_err(sql_error)?;
    }
    Ok(true)
}

/// Set `archived_at` (Some to archive, None to restore) on relationships incident
/// to `entity_id` (subject or object endpoint), re-stamping Provenance.
fn cascade_set_incident_archived(
    conn: &Connection,
    entity_id: &str,
    scope: &Scope,
    actor: &Actor,
    ts: Timestamp,
    archived_at: Option<Timestamp>,
) -> CoreResult<()> {
    let method = if archived_at.is_some() {
        "archive"
    } else {
        "restore"
    };
    for (id, mut rel) in incident_relationships(conn, entity_id, scope)? {
        if !scope_allows(&rel.scope, scope) {
            continue;
        }
        // Only flip rows whose state matches the requested transition.
        if archived_at.is_some() && rel.archived_at.is_some() {
            continue;
        }
        if archived_at.is_none() && rel.archived_at.is_none() {
            continue;
        }
        rel.archived_at = archived_at;
        rel.updated_at = Some(ts);
        rel.provenance = stamp_provenance(&rel.provenance, actor, method, ts);
        let new_json = serde_json::to_string(&rel).map_err(json_error)?;
        if let Some(a) = archived_at {
            conn.execute(
                "UPDATE knowledge_relationships SET archived_at = ?1, record_json = ?2 WHERE id = ?3",
                params![a.to_rfc3339(), new_json, id],
            )
            .map_err(sql_error)?;
        } else {
            conn.execute(
                "UPDATE knowledge_relationships SET archived_at = NULL, record_json = ?1 WHERE id = ?2",
                params![new_json, id],
            )
            .map_err(sql_error)?;
        }
    }
    Ok(())
}

/// Load active+archived relationships incident to `entity_id` (subject or object
/// endpoint), deduped by id.
fn incident_relationships(
    conn: &Connection,
    entity_id: &str,
    scope: &Scope,
) -> CoreResult<Vec<(String, KnowledgeRelationship)>> {
    let workspace = workspace_param(scope);
    let mut out: Vec<(String, KnowledgeRelationship)> = Vec::new();

    let mut stmt = conn
        .prepare(
            "SELECT id, record_json FROM knowledge_relationships
             WHERE subject_id = ?1 AND tenant = ?2 AND COALESCE(workspace, '') = ?3",
        )
        .map_err(sql_error)?;
    let rows = stmt
        .query_map(params![entity_id, scope.tenant, workspace], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(sql_error)?;
    for r in rows {
        let (id, json) = r.map_err(sql_error)?;
        if let Ok(rel) = serde_json::from_str::<KnowledgeRelationship>(&json) {
            out.push((id, rel));
        }
    }
    drop(stmt);

    let pat = format!("%{entity_id}%");
    let mut stmt = conn
        .prepare(
            "SELECT id, record_json FROM knowledge_relationships
             WHERE record_json LIKE ?1 AND tenant = ?2 AND COALESCE(workspace, '') = ?3",
        )
        .map_err(sql_error)?;
    let rows = stmt
        .query_map(params![pat, scope.tenant, workspace], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(sql_error)?;
    for r in rows {
        let (id, json) = r.map_err(sql_error)?;
        if let Ok(rel) = serde_json::from_str::<KnowledgeRelationship>(&json) {
            let matches = rel
                .object
                .id
                .as_ref()
                .map(|i| i.as_str() == entity_id)
                .unwrap_or(false);
            if matches {
                out.push((id, rel));
            }
        }
    }
    drop(stmt);

    out.sort_by(|a, b| a.0.cmp(&b.0));
    out.dedup_by(|a, b| a.0 == b.0);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::knowledge::SqlKnowledgeStore;
    use engram_knowledge::KnowledgeRepository;
    use futures::executor::block_on;

    fn ts() -> Timestamp {
        "2026-01-01T00:00:00Z".parse().unwrap()
    }
    fn scope_t() -> Scope {
        Scope {
            tenant: "t".into(),
            subject: None,
            workspace: None,
            session: None,
            environment: None,
        }
    }
    fn actor() -> Actor {
        Actor {
            id: ActorId::from("maintainer"),
            kind: ActorKind::Agent,
            display_name: Some("engram-maintain".into()),
            metadata: None,
        }
    }
    fn prov() -> Provenance {
        Provenance {
            source: "test".into(),
            actor: Actor {
                id: ActorId::from("extractor"),
                kind: ActorKind::System,
                display_name: None,
                metadata: None,
            },
            observed_at: ts(),
            evidence: Vec::new(),
            derivations: Vec::new(),
            confidence: None,
            method: Some("extraction".into()),
        }
    }
    fn entity(id: &str) -> KnowledgeEntity {
        KnowledgeEntity {
            id: EntityId::from(id),
            graph_id: None,
            kind: EntityKind::Concept,
            name: id.into(),
            aliases: Vec::new(),
            scope: scope_t(),
            source_refs: Vec::new(),
            concept_refs: Vec::new(),
            ontology_class_refs: Vec::new(),
            provenance: prov(),
            created_at: ts(),
            updated_at: None,
            valid_from: None,
            valid_until: None,
            archived_at: None,
            metadata: None,
        }
    }
    fn rel(id: &str, subj: &str, obj: &str) -> KnowledgeRelationship {
        KnowledgeRelationship {
            id: RelationshipId::from(id),
            graph_id: None,
            subject: EntityRef {
                id: Some(EntityId::from(subj)),
                kind: None,
                name: None,
                aliases: Vec::new(),
            },
            predicate: "calls".into(),
            object: EntityRef {
                id: Some(EntityId::from(obj)),
                kind: None,
                name: None,
                aliases: Vec::new(),
            },
            scope: scope_t(),
            evidence: Vec::new(),
            confidence: None,
            provenance: prov(),
            created_at: ts(),
            updated_at: None,
            archived_at: None,
        }
    }

    #[test]
    fn archive_excludes_from_reads_stamps_provenance_and_restore_returns_it() {
        let store = SqlKnowledgeStore::open_in_memory().unwrap();
        block_on(store.put_entity(entity("e1"))).unwrap();
        let scope = scope_t();
        let actor = actor();

        let page = block_on(
            <SqlKnowledgeStore as GraphMaintenanceRepository>::list_entities(
                &store,
                &scope,
                &EntityFilter::default(),
                None,
                10,
            ),
        )
        .unwrap();
        assert_eq!(page.items.len(), 1);

        {
            let conn = store.lock().unwrap();
            assert!(archive_entity(&conn, &EntityId::from("e1"), &scope, &actor, ts()).unwrap());
        }
        // Active read excludes it; include_archived sees it + Provenance was re-stamped.
        let page = block_on(
            <SqlKnowledgeStore as GraphMaintenanceRepository>::list_entities(
                &store,
                &scope,
                &EntityFilter::default(),
                None,
                10,
            ),
        )
        .unwrap();
        assert!(page.items.is_empty());
        let page = block_on(
            <SqlKnowledgeStore as GraphMaintenanceRepository>::list_entities(
                &store,
                &scope,
                &EntityFilter {
                    include_archived: true,
                    ..EntityFilter::default()
                },
                None,
                10,
            ),
        )
        .unwrap();
        assert_eq!(page.items.len(), 1);
        assert!(page.items[0].archived_at.is_some());
        assert_eq!(page.items[0].provenance.actor.id.as_str(), "maintainer");
        assert_eq!(page.items[0].provenance.method.as_deref(), Some("archive"));

        {
            let conn = store.lock().unwrap();
            assert!(restore_entity(&conn, &EntityId::from("e1"), &scope, &actor, ts()).unwrap());
        }
        let page = block_on(
            <SqlKnowledgeStore as GraphMaintenanceRepository>::list_entities(
                &store,
                &scope,
                &EntityFilter::default(),
                None,
                10,
            ),
        )
        .unwrap();
        assert_eq!(page.items.len(), 1);
        assert!(page.items[0].archived_at.is_none());
    }

    #[test]
    fn archive_cascades_to_incident_edges() {
        let store = SqlKnowledgeStore::open_in_memory().unwrap();
        block_on(store.put_entity(entity("a"))).unwrap();
        block_on(store.put_entity(entity("b"))).unwrap();
        block_on(store.put_entity(entity("c"))).unwrap();
        block_on(store.put_relationship(rel("r1", "a", "b"))).unwrap();
        block_on(store.put_relationship(rel("r2", "c", "a"))).unwrap();
        block_on(store.put_relationship(rel("r3", "b", "c"))).unwrap();
        let scope = scope_t();
        let actor = actor();

        {
            let conn = store.lock().unwrap();
            assert!(archive_entity(&conn, &EntityId::from("a"), &scope, &actor, ts()).unwrap());
        }
        let rels = block_on(
            <SqlKnowledgeStore as GraphMaintenanceRepository>::list_relationships(
                &store,
                &scope,
                &RelationshipFilter::default(),
                None,
                10,
            ),
        )
        .unwrap();
        let active_ids: Vec<&str> = rels.items.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(active_ids, vec!["r3"]);

        {
            let conn = store.lock().unwrap();
            assert!(restore_entity(&conn, &EntityId::from("a"), &scope, &actor, ts()).unwrap());
        }
        let rels = block_on(
            <SqlKnowledgeStore as GraphMaintenanceRepository>::list_relationships(
                &store,
                &scope,
                &RelationshipFilter::default(),
                None,
                10,
            ),
        )
        .unwrap();
        assert_eq!(rels.items.len(), 3);
    }

    #[test]
    fn archive_and_restore_single_relationship() {
        let store = SqlKnowledgeStore::open_in_memory().unwrap();
        block_on(store.put_entity(entity("a"))).unwrap();
        block_on(store.put_entity(entity("b"))).unwrap();
        block_on(store.put_relationship(rel("r1", "a", "b"))).unwrap();
        let scope = scope_t();
        let actor = actor();

        {
            let conn = store.lock().unwrap();
            assert!(
                archive_relationship(&conn, &RelationshipId::from("r1"), &scope, &actor, ts())
                    .unwrap()
            );
            assert!(
                !archive_relationship(&conn, &RelationshipId::from("r1"), &scope, &actor, ts())
                    .unwrap()
            );
        }
        let rels = block_on(
            <SqlKnowledgeStore as GraphMaintenanceRepository>::list_relationships(
                &store,
                &scope,
                &RelationshipFilter::default(),
                None,
                10,
            ),
        )
        .unwrap();
        assert!(rels.items.is_empty());

        {
            let conn = store.lock().unwrap();
            assert!(
                restore_relationship(&conn, &RelationshipId::from("r1"), &scope, &actor, ts())
                    .unwrap()
            );
        }
        let rels = block_on(
            <SqlKnowledgeStore as GraphMaintenanceRepository>::list_relationships(
                &store,
                &scope,
                &RelationshipFilter::default(),
                None,
                10,
            ),
        )
        .unwrap();
        assert_eq!(rels.items.len(), 1);
        assert!(rels.items[0].archived_at.is_none());
    }

    #[test]
    fn list_filters_kind_source_confidence_graph_and_paginates() {
        let store = SqlKnowledgeStore::open_in_memory().unwrap();
        let scope = scope_t();
        let mut e_low = entity("e1");
        e_low.provenance.confidence = Some(0.1);
        let mut e_mid = entity("e2");
        e_mid.provenance.confidence = Some(0.6);
        let mut e_fn = entity("e3");
        e_fn.kind = EntityKind::Function;
        let mut e_g = entity("e4");
        e_g.graph_id = Some(KnowledgeGraphId::from("g2"));
        block_on(store.put_entity(e_low)).unwrap();
        block_on(store.put_entity(e_mid)).unwrap();
        block_on(store.put_entity(e_fn)).unwrap();
        block_on(store.put_entity(e_g)).unwrap();

        // kind filter
        let p = block_on(
            <SqlKnowledgeStore as GraphMaintenanceRepository>::list_entities(
                &store,
                &scope,
                &EntityFilter {
                    kinds: vec![EntityKind::Function],
                    ..EntityFilter::default()
                },
                None,
                10,
            ),
        )
        .unwrap();
        assert_eq!(p.items.len(), 1);

        // graph_id filter (only g2)
        let p = block_on(
            <SqlKnowledgeStore as GraphMaintenanceRepository>::list_entities(
                &store,
                &scope,
                &EntityFilter {
                    graph_id: Some(KnowledgeGraphId::from("g2")),
                    ..EntityFilter::default()
                },
                None,
                10,
            ),
        )
        .unwrap();
        assert_eq!(p.items.len(), 1);
        assert_eq!(p.items[0].id.as_str(), "e4");

        // min_confidence: e2 (0.6) + e3 (None, included) + e4 (None, included); e1 (0.1) excluded
        let p = block_on(
            <SqlKnowledgeStore as GraphMaintenanceRepository>::list_entities(
                &store,
                &scope,
                &EntityFilter {
                    min_confidence: Some(0.5),
                    ..EntityFilter::default()
                },
                None,
                10,
            ),
        )
        .unwrap();
        let ids: Vec<&str> = p.items.iter().map(|e| e.id.as_str()).collect();
        assert!(!ids.contains(&"e1"));
        assert!(ids.contains(&"e2"));

        // source filter (provenance.source == "test" matches all here)
        let p = block_on(
            <SqlKnowledgeStore as GraphMaintenanceRepository>::list_entities(
                &store,
                &scope,
                &EntityFilter {
                    source_id: Some(SourceId::from("test")),
                    ..EntityFilter::default()
                },
                None,
                10,
            ),
        )
        .unwrap();
        assert_eq!(p.items.len(), 4);
        // a non-matching source excludes all
        let p = block_on(
            <SqlKnowledgeStore as GraphMaintenanceRepository>::list_entities(
                &store,
                &scope,
                &EntityFilter {
                    source_id: Some(SourceId::from("nope")),
                    ..EntityFilter::default()
                },
                None,
                10,
            ),
        )
        .unwrap();
        assert!(p.items.is_empty());

        // cursor pagination walks all 4 active rows
        let mut collected = Vec::new();
        let mut cursor = None;
        loop {
            let page = block_on(
                <SqlKnowledgeStore as GraphMaintenanceRepository>::list_entities(
                    &store,
                    &scope,
                    &EntityFilter::default(),
                    cursor,
                    2,
                ),
            )
            .unwrap();
            collected.extend(page.items);
            cursor = page.next_cursor;
            if cursor.is_none() {
                break;
            }
        }
        assert_eq!(collected.len(), 4);
    }

    #[test]
    fn re_put_preserves_a_prior_archive() {
        let store = SqlKnowledgeStore::open_in_memory().unwrap();
        let scope = scope_t();
        let actor = actor();
        block_on(store.put_entity(entity("e1"))).unwrap();
        {
            let conn = store.lock().unwrap();
            assert!(archive_entity(&conn, &EntityId::from("e1"), &scope, &actor, ts()).unwrap());
        }
        // Re-put (as ingestion does) with archived_at = None — the archive persists.
        let mut re_put = entity("e1");
        re_put.archived_at = None;
        block_on(store.put_entity(re_put)).unwrap();
        let page = block_on(
            <SqlKnowledgeStore as GraphMaintenanceRepository>::list_entities(
                &store,
                &scope,
                &EntityFilter::default(),
                None,
                10,
            ),
        )
        .unwrap();
        assert!(page.items.is_empty(), "re-put must not un-archive");
        let page = block_on(
            <SqlKnowledgeStore as GraphMaintenanceRepository>::list_entities(
                &store,
                &scope,
                &EntityFilter {
                    include_archived: true,
                    ..EntityFilter::default()
                },
                None,
                10,
            ),
        )
        .unwrap();
        assert_eq!(page.items.len(), 1);
        assert!(
            page.items[0].archived_at.is_some(),
            "archive preserved across re-put"
        );
    }

    #[test]
    fn archive_is_scope_isolated() {
        let store = SqlKnowledgeStore::open_in_memory().unwrap();
        let mut e = entity("e1");
        e.scope.subject = Some("alice".into());
        block_on(store.put_entity(e)).unwrap();
        // A different subject cannot archive alice's entity.
        let mut other = scope_t();
        other.subject = Some("bob".into());
        let actor = actor();
        {
            let conn = store.lock().unwrap();
            assert!(!archive_entity(&conn, &EntityId::from("e1"), &other, &actor, ts()).unwrap());
        }
        let alice = Scope {
            tenant: "t".into(),
            subject: Some("alice".into()),
            workspace: None,
            session: None,
            environment: None,
        };
        let page = block_on(
            <SqlKnowledgeStore as GraphMaintenanceRepository>::list_entities(
                &store,
                &alice,
                &EntityFilter::default(),
                None,
                10,
            ),
        )
        .unwrap();
        assert_eq!(
            page.items.len(),
            1,
            "cross-scope archive is denied; entity stays active"
        );
    }

    fn plan_request(scope: Scope, mutations: Vec<MaintenanceMutation>) -> MaintenancePlanRequest {
        MaintenancePlanRequest {
            graph_id: None,
            scope,
            mutations,
            policy: MaintenancePolicy::default(),
            actor: actor(),
        }
    }

    #[test]
    fn build_plan_preview_carries_before_and_after() {
        let store = SqlKnowledgeStore::open_in_memory().unwrap();
        block_on(store.put_entity(entity("a"))).unwrap();
        let req = plan_request(
            scope_t(),
            vec![MaintenanceMutation::Archive {
                target: MaintenanceTarget::Entity(EntityId::from("a")),
            }],
        );
        let plan = block_on(store.build_plan(req)).unwrap();
        assert_eq!(plan.previews.len(), 1);
        let preview = &plan.previews[0];
        assert_eq!(
            preview.before.len(),
            1,
            "before snapshot of the current entity"
        );
        assert_eq!(preview.after.len(), 1, "after snapshot");
        let before_archived = match &preview.before[0] {
            MutationSnapshot::Entity(e) => e.archived_at,
            _ => unreachable!(),
        };
        let after_archived = match &preview.after[0] {
            MutationSnapshot::Entity(e) => e.archived_at,
            _ => unreachable!(),
        };
        assert!(before_archived.is_none(), "before is the active state");
        assert!(after_archived.is_some(), "after reflects the archive");
        // build_plan never mutates: a is still active afterwards.
        let page = block_on(
            <SqlKnowledgeStore as GraphMaintenanceRepository>::list_entities(
                &store,
                &scope_t(),
                &EntityFilter::default(),
                None,
                10,
            ),
        )
        .unwrap();
        assert_eq!(page.items.len(), 1);
    }

    #[test]
    fn apply_archives_in_one_transaction() {
        let store = SqlKnowledgeStore::open_in_memory().unwrap();
        block_on(store.put_entity(entity("a"))).unwrap();
        block_on(store.put_entity(entity("b"))).unwrap();
        block_on(store.put_relationship(rel("r1", "a", "b"))).unwrap();
        let plan = MaintenancePlan::new(
            None,
            scope_t(),
            vec![MaintenanceMutation::Archive {
                target: MaintenanceTarget::Entity(EntityId::from("a")),
            }],
            MaintenancePolicy::default(),
            actor(),
        );
        let result = block_on(store.apply_plan(&plan, ApplyMode::Apply)).unwrap();
        assert_eq!(result.applied, 1);
        assert_eq!(result.failed, 0);
        assert_eq!(result.atomicity, Atomicity::SingleTransaction);
        // a + its incident edge r1 archived (cascade); b stays active.
        let rels = block_on(
            <SqlKnowledgeStore as GraphMaintenanceRepository>::list_relationships(
                &store,
                &scope_t(),
                &RelationshipFilter::default(),
                None,
                10,
            ),
        )
        .unwrap();
        assert!(rels.items.is_empty(), "incident edge archived with a");
        let ents = block_on(
            <SqlKnowledgeStore as GraphMaintenanceRepository>::list_entities(
                &store,
                &scope_t(),
                &EntityFilter::default(),
                None,
                10,
            ),
        )
        .unwrap();
        assert_eq!(ents.items.len(), 1, "b active; a archived");
    }

    #[test]
    fn apply_rolls_back_on_referential_failure() {
        let store = SqlKnowledgeStore::open_in_memory().unwrap();
        block_on(store.put_entity(entity("a"))).unwrap();
        block_on(store.put_entity(entity("b"))).unwrap();
        block_on(store.put_relationship(rel("r1", "a", "b"))).unwrap();
        // Delete a — still referenced by r1 -> verify fails -> rollback.
        let plan = MaintenancePlan::new(
            None,
            scope_t(),
            vec![MaintenanceMutation::Delete {
                target: MaintenanceTarget::Entity(EntityId::from("a")),
            }],
            MaintenancePolicy::default(),
            actor(),
        );
        let result = block_on(store.apply_plan(&plan, ApplyMode::Apply)).unwrap();
        assert_eq!(result.applied, 0, "nothing committed");
        assert!(
            !result.verify_findings.is_empty(),
            "referential finding surfaced"
        );
        assert!(
            result.by_kind.iter().all(|c| c.applied == 0),
            "by_kind.applied is consistent with the rolled-back top-level applied=0"
        );
        // a survived the rollback.
        let ents = block_on(
            <SqlKnowledgeStore as GraphMaintenanceRepository>::list_entities(
                &store,
                &scope_t(),
                &EntityFilter::default(),
                None,
                10,
            ),
        )
        .unwrap();
        assert_eq!(ents.items.len(), 2, "graph unchanged after rollback");
    }

    #[test]
    fn apply_is_idempotent() {
        let store = SqlKnowledgeStore::open_in_memory().unwrap();
        block_on(store.put_entity(entity("a"))).unwrap();
        let plan = MaintenancePlan::new(
            None,
            scope_t(),
            vec![MaintenanceMutation::Archive {
                target: MaintenanceTarget::Entity(EntityId::from("a")),
            }],
            MaintenancePolicy::default(),
            actor(),
        );
        let first = block_on(store.apply_plan(&plan, ApplyMode::Apply)).unwrap();
        assert_eq!(first.applied, 1);
        // Re-apply: a is already archived -> unchanged.
        let second = block_on(store.apply_plan(&plan, ApplyMode::Apply)).unwrap();
        assert_eq!(second.applied, 0);
        assert_eq!(second.unchanged, 1);
    }

    #[test]
    fn apply_restore_and_delete_are_idempotent() {
        let store = SqlKnowledgeStore::open_in_memory().unwrap();
        block_on(store.put_entity(entity("a"))).unwrap();
        block_on(store.put_entity(entity("b"))).unwrap();
        let scope = scope_t();
        // Archive a first so Restore has something to act on.
        {
            let conn = store.lock().unwrap();
            assert!(archive_entity(&conn, &EntityId::from("a"), &scope, &actor(), ts()).unwrap());
        }
        // Restore a -> applied; re-restore (already active) -> unchanged.
        let restore_plan = MaintenancePlan::new(
            None,
            scope_t(),
            vec![MaintenanceMutation::Restore {
                target: MaintenanceTarget::Entity(EntityId::from("a")),
            }],
            MaintenancePolicy::default(),
            actor(),
        );
        assert_eq!(
            block_on(store.apply_plan(&restore_plan, ApplyMode::Apply))
                .unwrap()
                .applied,
            1
        );
        let r2 = block_on(store.apply_plan(&restore_plan, ApplyMode::Apply)).unwrap();
        assert_eq!(r2.applied, 0);
        assert_eq!(r2.unchanged, 1);
        // Delete b (no incident edges) -> applied; re-delete (gone) -> unchanged.
        let delete_plan = MaintenancePlan::new(
            None,
            scope_t(),
            vec![MaintenanceMutation::Delete {
                target: MaintenanceTarget::Entity(EntityId::from("b")),
            }],
            MaintenancePolicy::default(),
            actor(),
        );
        assert_eq!(
            block_on(store.apply_plan(&delete_plan, ApplyMode::Apply))
                .unwrap()
                .applied,
            1
        );
        let d2 = block_on(store.apply_plan(&delete_plan, ApplyMode::Apply)).unwrap();
        assert_eq!(d2.applied, 0);
        assert_eq!(d2.unchanged, 1);
    }

    #[test]
    fn add_and_remove_alias_are_idempotent() {
        let store = SqlKnowledgeStore::open_in_memory().unwrap();
        block_on(store.put_entity(entity("a"))).unwrap();
        let scope = scope_t();
        let add = MaintenancePlan::new(
            None,
            scope.clone(),
            vec![MaintenanceMutation::AddAlias {
                entity: EntityId::from("a"),
                alias: "x".to_string(),
            }],
            MaintenancePolicy::default(),
            actor(),
        );
        assert_eq!(
            block_on(store.apply_plan(&add, ApplyMode::Apply))
                .unwrap()
                .applied,
            1
        );
        let again = block_on(store.apply_plan(&add, ApplyMode::Apply)).unwrap();
        assert_eq!(again.applied, 0);
        assert_eq!(again.unchanged, 1);
        let e = block_on(store.get_entity(&EntityId::from("a"), &scope))
            .unwrap()
            .unwrap();
        assert!(e.aliases.contains(&"x".to_string()));
        let rm = MaintenancePlan::new(
            None,
            scope,
            vec![MaintenanceMutation::RemoveAlias {
                entity: EntityId::from("a"),
                alias: "x".to_string(),
            }],
            MaintenancePolicy::default(),
            actor(),
        );
        assert_eq!(
            block_on(store.apply_plan(&rm, ApplyMode::Apply))
                .unwrap()
                .applied,
            1
        );
        assert_eq!(
            block_on(store.apply_plan(&rm, ApplyMode::Apply))
                .unwrap()
                .applied,
            0
        );
    }

    #[test]
    fn rewrite_relationship_edits_in_place() {
        let store = SqlKnowledgeStore::open_in_memory().unwrap();
        block_on(store.put_entity(entity("a"))).unwrap();
        block_on(store.put_entity(entity("b"))).unwrap();
        block_on(store.put_entity(entity("c"))).unwrap();
        block_on(store.put_relationship(rel("r1", "a", "b"))).unwrap();
        let plan = MaintenancePlan::new(
            None,
            scope_t(),
            vec![MaintenanceMutation::RewriteRelationship {
                relationship: RelationshipId::from("r1"),
                new_predicate: Some("implements".to_string()),
                new_subject: None,
                new_object: Some(EntityId::from("c")),
            }],
            MaintenancePolicy::default(),
            actor(),
        );
        assert_eq!(
            block_on(store.apply_plan(&plan, ApplyMode::Apply))
                .unwrap()
                .applied,
            1
        );
        let rels = block_on(
            <SqlKnowledgeStore as GraphMaintenanceRepository>::list_relationships(
                &store,
                &scope_t(),
                &RelationshipFilter::default(),
                None,
                10,
            ),
        )
        .unwrap();
        assert_eq!(rels.items.len(), 1);
        assert_eq!(rels.items[0].predicate, "implements");
        assert_eq!(rels.items[0].object.id.as_ref().unwrap().as_str(), "c");
        // re-rewrite to the same values -> unchanged
        assert_eq!(
            block_on(store.apply_plan(&plan, ApplyMode::Apply))
                .unwrap()
                .applied,
            0
        );
    }

    #[test]
    fn merge_archives_absorbed_redirects_and_coalesces() {
        let store = SqlKnowledgeStore::open_in_memory().unwrap();
        let mut s = entity("s");
        s.aliases = vec!["s-alias".to_string()];
        let mut a = entity("a");
        a.aliases = vec!["a-alias".to_string()];
        block_on(store.put_entity(s)).unwrap();
        block_on(store.put_entity(a)).unwrap();
        block_on(store.put_entity(entity("x"))).unwrap();
        block_on(store.put_relationship(rel("r1", "s", "x"))).unwrap();
        block_on(store.put_relationship(rel("r2", "a", "x"))).unwrap();
        let plan = MaintenancePlan::new(
            None,
            scope_t(),
            vec![MaintenanceMutation::Merge {
                survivor: EntityId::from("s"),
                absorbed: vec![EntityId::from("a")],
            }],
            MaintenancePolicy::default(),
            actor(),
        );
        let r = block_on(store.apply_plan(&plan, ApplyMode::Apply)).unwrap();
        assert_eq!(r.applied, 1);
        // a archived; s + x active.
        let ents = block_on(
            <SqlKnowledgeStore as GraphMaintenanceRepository>::list_entities(
                &store,
                &scope_t(),
                &EntityFilter::default(),
                None,
                10,
            ),
        )
        .unwrap();
        let ids: Vec<&str> = ents.items.iter().map(|e| e.id.as_str()).collect();
        assert!(ids.contains(&"s") && ids.contains(&"x") && !ids.contains(&"a"));
        // one active edge s->x (r2 redirected to s->x and coalesced with r1).
        let rels = block_on(
            <SqlKnowledgeStore as GraphMaintenanceRepository>::list_relationships(
                &store,
                &scope_t(),
                &RelationshipFilter::default(),
                None,
                10,
            ),
        )
        .unwrap();
        assert_eq!(rels.items.len(), 1, "duplicate edge coalesced");
        assert_eq!(rels.items[0].subject.id.as_ref().unwrap().as_str(), "s");
        // survivor absorbed a's alias.
        let sv = block_on(store.get_entity(&EntityId::from("s"), &scope_t()))
            .unwrap()
            .unwrap();
        assert!(sv.aliases.contains(&"a-alias".to_string()));
        // idempotent: re-apply (a already archived) -> unchanged.
        // idempotent: re-apply (a already archived) -> unchanged, survivor NOT re-stamped.
        let sv1 = block_on(store.get_entity(&EntityId::from("s"), &scope_t()))
            .unwrap()
            .unwrap();
        let r2 = block_on(store.apply_plan(&plan, ApplyMode::Apply)).unwrap();
        assert_eq!(r2.applied, 0);
        let sv2 = block_on(store.get_entity(&EntityId::from("s"), &scope_t()))
            .unwrap()
            .unwrap();
        assert_eq!(sv1.updated_at, sv2.updated_at);
        assert_eq!(sv1.provenance.observed_at, sv2.provenance.observed_at);
    }
}
