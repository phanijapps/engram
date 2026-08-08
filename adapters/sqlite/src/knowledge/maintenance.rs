//! SQLite implementation of the graph-maintenance port (ADR-0027).
//!
//! Reversibility is archive/restore only. `archived_at` is promoted to a real
//! column (filterable in SQL) AND mirrored into `record_json` (so the domain
//! object read back is consistent — the `put_*` writers keep both in lockstep).
//! The archive/restore primitives take a `&Connection` + the maintenance `Actor`
//! so T5a's `apply_plan` can stage several in one transaction; every mutation
//! re-stamps `Provenance` (actor/method/observed_at) so it is attributable.

use async_trait::async_trait;
use engram_domain::*;
use engram_knowledge::GraphMaintenanceRepository;
use engram_runtime::{CoreError, CoreResult};
use rusqlite::{Connection, OptionalExtension, params};

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

// ── Archive / restore primitives (run on a borrowed connection) ──────────────
//
// `#[allow(dead_code)]` until T5a's `apply_plan` wires them into the port; today
// they are exercised by the maintenance tests below.

/// Archive an entity (soft-delete): sets `archived_at` on the entity and on its
/// incident relationships (subject or object endpoint), re-stamping `Provenance`.
/// Returns `false` if the entity is missing, out of scope, or already archived.
#[allow(dead_code)]
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
#[allow(dead_code)]
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
#[allow(dead_code)]
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
#[allow(dead_code)]
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
}
