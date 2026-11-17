//! Canonical maintenance semantics over adapter-owned atomic storage.
//! Callers own transaction/session, rollback and commit; this module performs no I/O ownership.
use engram_domain::*;
use engram_runtime::{CoreError, CoreResult};
use std::collections::HashSet;
/// Reads include archived rows and obey the adapter's existing scope matcher.
/// Relationship enumeration preserves the adapter's stable storage order.
pub trait MaintenanceStorage {
    fn visible(&self, record: &Scope, request: &Scope) -> bool;
    fn entity(&self, id: &EntityId, scope: &Scope) -> CoreResult<Option<KnowledgeEntity>>;
    fn relationship(
        &self,
        id: &RelationshipId,
        scope: &Scope,
    ) -> CoreResult<Option<KnowledgeRelationship>>;
    fn entities(&self, scope: &Scope) -> CoreResult<Vec<KnowledgeEntity>>;
    fn relationships(&self, scope: &Scope) -> CoreResult<Vec<KnowledgeRelationship>>;
    fn write_entity(&self, value: &KnowledgeEntity) -> CoreResult<()>;
    fn write_relationship(&self, value: &KnowledgeRelationship) -> CoreResult<()>;
    fn delete_entity(&self, id: &EntityId) -> CoreResult<()>;
    fn delete_relationship(&self, id: &RelationshipId) -> CoreResult<()>;
    fn duplicate_count(&self, scope: &Scope) -> CoreResult<u32>;
}
// A storage collaborator cannot make an unestablished scope visible to previews or effects.
fn visible_entity(
    conn: &dyn MaintenanceStorage,
    id: &EntityId,
    scope: &Scope,
) -> CoreResult<Option<KnowledgeEntity>> {
    Ok(conn
        .entity(id, scope)?
        .filter(|v| conn.visible(&v.scope, scope)))
}
fn visible_relationship(
    conn: &dyn MaintenanceStorage,
    id: &RelationshipId,
    scope: &Scope,
) -> CoreResult<Option<KnowledgeRelationship>> {
    Ok(conn
        .relationship(id, scope)?
        .filter(|v| conn.visible(&v.scope, scope)))
}
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

pub fn build_plan(
    conn: &dyn MaintenanceStorage,
    request: MaintenancePlanRequest,
) -> CoreResult<MaintenancePlan> {
    let scope = request.scope.clone();
    let ts = chrono::Utc::now();
    let mut previews = Vec::with_capacity(request.mutations.len());
    for m in &request.mutations {
        previews.push(build_mutation_preview(conn, m, &scope, ts)?);
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

pub fn apply_plan(
    conn: &dyn MaintenanceStorage,
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
    let ts = chrono::Utc::now();

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
        let slot = match stage_mutation(conn, m, scope, actor, ts)? {
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
        verify_referential_integrity(conn, scope, &targeted)?
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

/// Outcome of staging one mutation: changed state, or already in target state
/// (idempotent). Store errors propagate as `Err` (not swallowed) so a real
/// SQL/IO failure surfaces instead of a silent `failed`.
enum StageOutcome {
    Changed,
    Unchanged,
}

/// Stage one mutation against an open connection (inside apply_plan's tx).
fn stage_mutation(
    conn: &dyn MaintenanceStorage,
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
            &Rewrite {
                predicate: new_predicate.as_ref(),
                subject: new_subject.as_ref(),
                object: new_object.as_ref(),
            },
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
    conn: &dyn MaintenanceStorage,
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

fn delete_target(
    conn: &dyn MaintenanceStorage,
    target: &MaintenanceTarget,
    scope: &Scope,
) -> CoreResult<bool> {
    match target {
        MaintenanceTarget::Entity(id) => hard_delete_entity(conn, id, scope),
        MaintenanceTarget::Relationship(id) => hard_delete_relationship(conn, id, scope),
    }
}

/// Add or remove an alias on an entity (idempotent). Re-stamps Provenance.
fn stage_alias(
    conn: &dyn MaintenanceStorage,
    entity_id: &EntityId,
    alias: &str,
    add: bool,
    scope: &Scope,
    actor: &Actor,
    ts: Timestamp,
) -> CoreResult<StageOutcome> {
    let Some(mut entity) = visible_entity(conn, entity_id, scope)? else {
        return Ok(StageOutcome::Unchanged);
    };
    if !conn.visible(&entity.scope, scope) {
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
    conn.write_entity(&entity)?;
    Ok(StageOutcome::Changed)
}

/// A rewrite mutation's fields — grouping keeps `stage_rewrite`'s signature
/// within clippy's argument budget.
struct Rewrite<'a> {
    predicate: Option<&'a String>,
    subject: Option<&'a EntityId>,
    object: Option<&'a EntityId>,
}

fn verify_rewrite_endpoints(
    conn: &dyn MaintenanceStorage,
    scope: &Scope,
    rel: &KnowledgeRelationship,
    rewrite: &Rewrite<'_>,
) -> CoreResult<()> {
    for (next, current) in [
        (rewrite.subject, rel.subject.id.as_ref()),
        (rewrite.object, rel.object.id.as_ref()),
    ] {
        if let Some(id) = next {
            if Some(id) != current && visible_entity(conn, id, scope)?.is_none() {
                return Err(CoreError::InvalidRequest {
                    reason: "maintenance rewrite endpoint unavailable in scope".into(),
                });
            }
        }
    }
    Ok(())
}
/// Rewrite a relationship's predicate/subject/object in place (idempotent).
fn stage_rewrite(
    conn: &dyn MaintenanceStorage,
    rel_id: &RelationshipId,
    rewrite: &Rewrite<'_>,
    scope: &Scope,
    actor: &Actor,
    ts: Timestamp,
) -> CoreResult<StageOutcome> {
    let Some(mut rel) = visible_relationship(conn, rel_id, scope)? else {
        return Ok(StageOutcome::Unchanged);
    };
    if !conn.visible(&rel.scope, scope) {
        return Ok(StageOutcome::Unchanged);
    }
    verify_rewrite_endpoints(conn, scope, &rel, rewrite)?;
    let mut changed = false;
    if let Some(p) = rewrite.predicate
        && &rel.predicate != p
    {
        rel.predicate = p.clone();
        changed = true;
    }
    if let Some(s) = rewrite.subject
        && rel.subject.id.as_ref() != Some(s)
    {
        rel.subject.id = Some(s.clone());
        changed = true;
    }
    if let Some(o) = rewrite.object
        && rel.object.id.as_ref() != Some(o)
    {
        rel.object.id = Some(o.clone());
        changed = true;
    }
    if !changed {
        return Ok(StageOutcome::Unchanged);
    }
    rel.updated_at = Some(ts);
    rel.provenance = stamp_provenance(&rel.provenance, actor, "rewrite_relationship", ts);
    conn.write_relationship(&rel)?;
    Ok(StageOutcome::Changed)
}

/// Merge `absorbed` entities into the `survivor`: fold their aliases/source_refs
/// into the survivor, redirect their incident edges to the survivor, then ARCHIVE
/// (not hard-delete) the absorbed entities and coalesced duplicate edges
/// (ADR-0027). Runs against the open tx; Provenance re-stamped.
fn stage_merge(
    conn: &dyn MaintenanceStorage,
    survivor_id: &EntityId,
    absorbed_ids: &[EntityId],
    scope: &Scope,
    actor: &Actor,
    ts: Timestamp,
) -> CoreResult<StageOutcome> {
    let survivor_str = survivor_id.to_string();
    let Some(mut survivor) = visible_entity(conn, survivor_id, scope)? else {
        return Ok(StageOutcome::Unchanged);
    };
    if !conn.visible(&survivor.scope, scope) {
        return Ok(StageOutcome::Unchanged);
    }
    // Fold each absorbed entity's refs into the survivor. Track whether anything
    // actually changed so the survivor row is re-written (and the outcome reported
    // Changed) only on a real fold — a re-apply must not re-stamp the survivor.
    let mut fold_changed = false;
    for absorbed_id in absorbed_ids {
        let Some(absorbed) = visible_entity(conn, absorbed_id, scope)? else {
            continue;
        };
        if !conn.visible(&absorbed.scope, scope) {
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
        conn.write_entity(&survivor)?;
    }

    // Redirect each absorbed entity's edges to the survivor, then archive it.
    let mut changed = fold_changed;
    for absorbed_id in absorbed_ids {
        // A denied/missing member cannot authorize redirects through a known opaque id.
        if visible_entity(conn, absorbed_id, scope)?.is_none() {
            continue;
        }
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
#[derive(Clone, Copy)]
enum PreviewEffect {
    Archive,
    Restore,
    Delete,
}

fn build_mutation_preview(
    conn: &dyn MaintenanceStorage,
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
            let cur = visible_entity(conn, entity, scope)?;
            let after = cur.clone().map(|mut e| {
                if !e.aliases.contains(alias) {
                    e.aliases.push(alias.clone());
                }
                e
            });
            (snap_entity(cur), snap_entity(after))
        }
        MaintenanceMutation::RemoveAlias { entity, alias } => {
            let cur = visible_entity(conn, entity, scope)?;
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
            let rewrite = Rewrite {
                predicate: new_predicate.as_ref(),
                subject: new_subject.as_ref(),
                object: new_object.as_ref(),
            };
            let cur = visible_relationship(conn, relationship, scope)?;
            if let Some(rel) = &cur {
                verify_rewrite_endpoints(conn, scope, rel, &rewrite)?;
            }
            let after = cur.clone().map(|mut r| {
                if let Some(p) = rewrite.predicate {
                    r.predicate = p.clone();
                }
                if let Some(s) = rewrite.subject {
                    r.subject.id = Some(s.clone());
                }
                if let Some(o) = rewrite.object {
                    r.object.id = Some(o.clone());
                }
                r
            });
            (snap_rel(cur), snap_rel(after))
        }
        MaintenanceMutation::Merge { survivor, absorbed } => {
            let mut before = Vec::new();
            let mut surv = visible_entity(conn, survivor, scope)?;
            if let Some(s) = &surv {
                before.push(MutationSnapshot::Entity(s.clone()));
            }
            for a_id in absorbed {
                if let Some(a) = visible_entity(conn, a_id, scope)? {
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
    conn: &dyn MaintenanceStorage,
    target: &MaintenanceTarget,
    scope: &Scope,
    ts: Timestamp,
    effect: PreviewEffect,
) -> CoreResult<(Vec<MutationSnapshot>, Vec<MutationSnapshot>)> {
    Ok(match target {
        MaintenanceTarget::Entity(id) => {
            let cur = visible_entity(conn, id, scope)?;
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
            let cur = visible_relationship(conn, id, scope)?;
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

fn redirect_relationships_to(
    conn: &dyn MaintenanceStorage,
    from: &str,
    to: &str,
    scope: &Scope,
) -> CoreResult<()> {
    for mut rel in conn.relationships(scope)? {
        if rel.archived_at.is_some() || !conn.visible(&rel.scope, scope) {
            continue;
        }
        let mut changed = false;
        if rel.subject.id.as_ref().map(|id| id.as_str()) == Some(from) {
            rel.subject.id = Some(EntityId::from(to));
            changed = true
        }
        if rel.object.id.as_ref().map(|id| id.as_str()) == Some(from) {
            rel.object.id = Some(EntityId::from(to));
            changed = true
        }
        if changed {
            conn.write_relationship(&rel)?;
        }
    }
    Ok(())
}
fn coalesce_survivor_relationships(
    conn: &dyn MaintenanceStorage,
    survivor: &str,
    scope: &Scope,
    actor: &Actor,
    ts: Timestamp,
) -> CoreResult<()> {
    let mut seen = HashSet::new();
    for mut rel in conn.relationships(scope)? {
        if rel.archived_at.is_some() || !conn.visible(&rel.scope, scope) {
            continue;
        }
        let incident = rel
            .subject
            .id
            .as_ref()
            .is_some_and(|id| id.as_str() == survivor)
            || rel
                .object
                .id
                .as_ref()
                .is_some_and(|id| id.as_str() == survivor);
        if incident && !seen.insert(crate::identity::compute_relationship_key(&rel)) {
            rel.archived_at = Some(ts);
            rel.updated_at = Some(ts);
            rel.provenance = stamp_provenance(&rel.provenance, actor, "merge_coalesce", ts);
            conn.write_relationship(&rel)?;
        }
    }
    Ok(())
}
fn hard_delete_entity(
    conn: &dyn MaintenanceStorage,
    id: &EntityId,
    scope: &Scope,
) -> CoreResult<bool> {
    if visible_entity(conn, id, scope)?.is_some_and(|v| conn.visible(&v.scope, scope)) {
        conn.delete_entity(id)?;
        Ok(true)
    } else {
        Ok(false)
    }
}
fn hard_delete_relationship(
    conn: &dyn MaintenanceStorage,
    id: &RelationshipId,
    scope: &Scope,
) -> CoreResult<bool> {
    if visible_relationship(conn, id, scope)?.is_some_and(|v| conn.visible(&v.scope, scope)) {
        conn.delete_relationship(id)?;
        Ok(true)
    } else {
        Ok(false)
    }
}
pub fn archive_entity(
    conn: &dyn MaintenanceStorage,
    id: &EntityId,
    scope: &Scope,
    actor: &Actor,
    ts: Timestamp,
) -> CoreResult<bool> {
    set_entity_archived(conn, id, scope, actor, ts, Some(ts))
}
pub fn restore_entity(
    conn: &dyn MaintenanceStorage,
    id: &EntityId,
    scope: &Scope,
    actor: &Actor,
    ts: Timestamp,
) -> CoreResult<bool> {
    set_entity_archived(conn, id, scope, actor, ts, None)
}
fn set_entity_archived(
    conn: &dyn MaintenanceStorage,
    id: &EntityId,
    scope: &Scope,
    actor: &Actor,
    ts: Timestamp,
    archived: Option<Timestamp>,
) -> CoreResult<bool> {
    let Some(mut v) = visible_entity(conn, id, scope)? else {
        return Ok(false);
    };
    if !conn.visible(&v.scope, scope) || v.archived_at.is_some() == archived.is_some() {
        return Ok(false);
    }
    let method = if archived.is_some() {
        "archive"
    } else {
        "restore"
    };
    v.archived_at = archived;
    v.updated_at = Some(ts);
    v.provenance = stamp_provenance(&v.provenance, actor, method, ts);
    conn.write_entity(&v)?;
    for rel in conn.relationships(scope)? {
        if rel.subject.id.as_ref() == Some(id) || rel.object.id.as_ref() == Some(id) {
            set_relationship_archived(conn, &rel.id, scope, actor, method, archived, ts)?;
        }
    }
    Ok(true)
}
pub fn archive_relationship(
    conn: &dyn MaintenanceStorage,
    id: &RelationshipId,
    scope: &Scope,
    actor: &Actor,
    ts: Timestamp,
) -> CoreResult<bool> {
    set_relationship_archived(conn, id, scope, actor, "archive", Some(ts), ts)
}
pub fn restore_relationship(
    conn: &dyn MaintenanceStorage,
    id: &RelationshipId,
    scope: &Scope,
    actor: &Actor,
    ts: Timestamp,
) -> CoreResult<bool> {
    set_relationship_archived(conn, id, scope, actor, "restore", None, ts)
}
fn set_relationship_archived(
    conn: &dyn MaintenanceStorage,
    id: &RelationshipId,
    scope: &Scope,
    actor: &Actor,
    method: &str,
    archived: Option<Timestamp>,
    ts: Timestamp,
) -> CoreResult<bool> {
    let Some(mut v) = visible_relationship(conn, id, scope)? else {
        return Ok(false);
    };
    if !conn.visible(&v.scope, scope) || v.archived_at.is_some() == archived.is_some() {
        return Ok(false);
    }
    v.archived_at = archived;
    v.updated_at = Some(ts);
    v.provenance = stamp_provenance(&v.provenance, actor, method, ts);
    conn.write_relationship(&v)?;
    Ok(true)
}
fn verify_referential_integrity(
    conn: &dyn MaintenanceStorage,
    scope: &Scope,
    targeted: &HashSet<String>,
) -> CoreResult<Vec<MaintenanceVerifyFinding>> {
    let mut out = vec![];
    if targeted.is_empty() {
        return Ok(out);
    }
    for rel in conn.relationships(scope)? {
        if rel.archived_at.is_none()
            && conn.visible(&rel.scope, scope)
            && (rel
                .subject
                .id
                .as_ref()
                .is_some_and(|id| targeted.contains(id.as_str()))
                || rel
                    .object
                    .id
                    .as_ref()
                    .is_some_and(|id| targeted.contains(id.as_str())))
        {
            out.push(MaintenanceVerifyFinding {
                severity: VerifySeverity::Error,
                message: format!(
                    "relationship {} references an entity removed by the plan (dangling)",
                    rel.id
                ),
                target: Some(MaintenanceTarget::Relationship(rel.id)),
            });
        }
    }
    Ok(out)
}
pub fn graph_health(
    conn: &dyn MaintenanceStorage,
    scope: &Scope,
    graph: Option<&KnowledgeGraphId>,
) -> CoreResult<MaintenanceHealth> {
    let all_entities: Vec<_> = conn
        .entities(scope)?
        .into_iter()
        .filter(|v| {
            conn.visible(&v.scope, scope) && graph.is_none_or(|g| v.graph_id.as_ref() == Some(g))
        })
        .collect();
    let all_relationships: Vec<_> = conn
        .relationships(scope)?
        .into_iter()
        .filter(|v| {
            conn.visible(&v.scope, scope) && graph.is_none_or(|g| v.graph_id.as_ref() == Some(g))
        })
        .collect();
    let archived_entity_count = all_entities
        .iter()
        .filter(|v| v.archived_at.is_some())
        .count() as u32;
    let archived_relationship_count = all_relationships
        .iter()
        .filter(|v| v.archived_at.is_some())
        .count() as u32;
    let entities: Vec<_> = all_entities
        .into_iter()
        .filter(|v| v.archived_at.is_none())
        .collect();
    let relationships: Vec<_> = all_relationships
        .into_iter()
        .filter(|v| v.archived_at.is_none())
        .collect();
    let policy = MaintenancePolicy {
        confidence_threshold: Some(0.5),
        detect_orphans: true,
        detect_low_confidence: true,
        detect_duplicates: false,
        detect_unsupported: false,
    };
    let candidates =
        crate::maintenance::detect_candidates(&entities, &relationships, &[], &[], &policy);
    Ok(MaintenanceHealth {
        scope: scope.clone(),
        graph_id: graph.cloned(),
        orphan_count: candidates
            .iter()
            .filter(|v| v.kind == CandidateKind::Orphan)
            .count() as u32,
        low_confidence_count: candidates
            .iter()
            .filter(|v| v.kind == CandidateKind::LowConfidence)
            .count() as u32,
        unsupported_count: 0,
        duplicate_count: conn.duplicate_count(scope)?,
        archived_entity_count,
        archived_relationship_count,
    })
}
