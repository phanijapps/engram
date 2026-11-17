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
                if let Some(sid) = &filter.source_id
                    && !entity_from_source(&e, sid)
                {
                    return None;
                }
                if let Some(thr) = filter.min_confidence
                    && let Some(c) = e.provenance.confidence
                    && c < thr
                {
                    return None;
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
                if let Some(p) = &filter.predicate
                    && &r.predicate != p
                {
                    return None;
                }
                if let Some(sid) = &filter.source_id
                    && !relationship_from_source(&r, sid)
                {
                    return None;
                }
                if let Some(thr) = filter.min_confidence
                    && let Some(c) = r.confidence.or(r.provenance.confidence)
                    && c < thr
                {
                    return None;
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
        let conn = self.lock()?;
        composition::build_plan(&SqlMaintenance(&conn), request)
    }
    async fn apply_plan(
        &self,
        plan: &MaintenancePlan,
        mode: ApplyMode,
    ) -> CoreResult<MaintenanceApplyResult> {
        let mut conn = self.lock()?;
        if mode == ApplyMode::Preview {
            return composition::apply_plan(&SqlMaintenance(&conn), plan, mode);
        }
        let tx = conn.transaction().map_err(sql_error)?;
        let result = composition::apply_plan(&SqlMaintenance(&tx), plan, mode)?;
        if result.failed == 0 && result.verify_findings.is_empty() {
            tx.commit().map_err(sql_error)?;
        }
        Ok(result)
    }
    async fn graph_health(
        &self,
        scope: &Scope,
        graph: Option<&KnowledgeGraphId>,
    ) -> CoreResult<MaintenanceHealth> {
        let conn = self.lock()?;
        composition::graph_health(&SqlMaintenance(&conn), scope, graph)
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

use engram_knowledge::maintenance_composition::{self as composition, MaintenanceStorage};
struct SqlMaintenance<'a>(&'a Connection);
impl MaintenanceStorage for SqlMaintenance<'_> {
    fn visible(&self, record: &Scope, request: &Scope) -> bool {
        scope_allows(record, request)
    }
    fn entity(&self, id: &EntityId, scope: &Scope) -> CoreResult<Option<KnowledgeEntity>> {
        load_entity(self.0, id, scope)
            .map(|v| v.filter(|record| scope_allows(&record.scope, scope)))
    }
    fn relationship(
        &self,
        id: &RelationshipId,
        scope: &Scope,
    ) -> CoreResult<Option<KnowledgeRelationship>> {
        load_relationship(self.0, id, scope)
            .map(|v| v.filter(|record| scope_allows(&record.scope, scope)))
    }
    fn entities(&self, scope: &Scope) -> CoreResult<Vec<KnowledgeEntity>> {
        let mut st=self.0.prepare("SELECT record_json FROM knowledge_entities WHERE tenant = ?1 AND COALESCE(workspace,'') = ?2 ORDER BY rowid").map_err(sql_error)?;
        let rows = st
            .query_map(params![scope.tenant, workspace_param(scope)], |r| {
                r.get::<_, String>(0)
            })
            .map_err(sql_error)?;
        let mut out = vec![];
        for row in rows {
            let v: KnowledgeEntity =
                serde_json::from_str(&row.map_err(sql_error)?).map_err(json_error)?;
            if scope_allows(&v.scope, scope) {
                out.push(v)
            }
        }
        Ok(out)
    }
    fn relationships(&self, scope: &Scope) -> CoreResult<Vec<KnowledgeRelationship>> {
        let mut st=self.0.prepare("SELECT record_json FROM knowledge_relationships WHERE tenant = ?1 AND COALESCE(workspace,'') = ?2 ORDER BY rowid").map_err(sql_error)?;
        let rows = st
            .query_map(params![scope.tenant, workspace_param(scope)], |r| {
                r.get::<_, String>(0)
            })
            .map_err(sql_error)?;
        let mut out = vec![];
        for row in rows {
            let v: KnowledgeRelationship =
                serde_json::from_str(&row.map_err(sql_error)?).map_err(json_error)?;
            if scope_allows(&v.scope, scope) {
                out.push(v)
            }
        }
        Ok(out)
    }
    fn write_entity(&self, v: &KnowledgeEntity) -> CoreResult<()> {
        let json = serde_json::to_string(v).map_err(json_error)?;
        self.0
            .execute(
                "UPDATE knowledge_entities SET archived_at=?1, record_json=?2 WHERE id=?3",
                params![
                    v.archived_at.map(|t| t.to_rfc3339()),
                    json,
                    v.id.to_string()
                ],
            )
            .map_err(sql_error)?;
        Ok(())
    }
    fn write_relationship(&self, v: &KnowledgeRelationship) -> CoreResult<()> {
        let json = serde_json::to_string(v).map_err(json_error)?;
        self.0.execute("UPDATE knowledge_relationships SET archived_at=?1,subject_id=?2,record_json=?3,relationship_key=?4 WHERE id=?5",params![v.archived_at.map(|t|t.to_rfc3339()),v.subject.id.as_ref().map(ToString::to_string).unwrap_or_default(),json,engram_knowledge::identity::compute_relationship_key(v),v.id.to_string()]).map_err(sql_error)?;
        Ok(())
    }
    fn delete_entity(&self, id: &EntityId) -> CoreResult<()> {
        self.0
            .execute(
                "DELETE FROM knowledge_entities WHERE id=?1",
                [id.to_string()],
            )
            .map_err(sql_error)?;
        Ok(())
    }
    fn delete_relationship(&self, id: &RelationshipId) -> CoreResult<()> {
        self.0
            .execute(
                "DELETE FROM knowledge_relationships WHERE id=?1",
                [id.to_string()],
            )
            .map_err(sql_error)?;
        Ok(())
    }
    fn duplicate_count(&self, scope: &Scope) -> CoreResult<u32> {
        self.0.query_row("SELECT COALESCE(SUM(c-1),0) FROM (SELECT COUNT(*) AS c FROM knowledge_entities WHERE identity_key IS NOT NULL AND tenant=?1 GROUP BY identity_key HAVING COUNT(*)>1)",[&scope.tenant],|r|r.get(0)).map_err(sql_error)
    }
}
pub fn archive_entity(
    conn: &Connection,
    id: &EntityId,
    scope: &Scope,
    actor: &Actor,
    ts: Timestamp,
) -> CoreResult<bool> {
    composition::archive_entity(&SqlMaintenance(conn), id, scope, actor, ts)
}
pub fn restore_entity(
    conn: &Connection,
    id: &EntityId,
    scope: &Scope,
    actor: &Actor,
    ts: Timestamp,
) -> CoreResult<bool> {
    composition::restore_entity(&SqlMaintenance(conn), id, scope, actor, ts)
}
pub fn archive_relationship(
    conn: &Connection,
    id: &RelationshipId,
    scope: &Scope,
    actor: &Actor,
    ts: Timestamp,
) -> CoreResult<bool> {
    composition::archive_relationship(&SqlMaintenance(conn), id, scope, actor, ts)
}
pub fn restore_relationship(
    conn: &Connection,
    id: &RelationshipId,
    scope: &Scope,
    actor: &Actor,
    ts: Timestamp,
) -> CoreResult<bool> {
    composition::restore_relationship(&SqlMaintenance(conn), id, scope, actor, ts)
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
            code_resolution: None,
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

    fn evref() -> EvidenceRef {
        EvidenceRef {
            target_type: EvidenceTargetType::Document,
            target_id: Some("d".to_string()),
            uri: None,
            quote: None,
            location: None,
        }
    }

    #[test]
    fn graph_health_counts_orphans_low_confidence_and_archived() {
        let store = SqlKnowledgeStore::open_in_memory().unwrap();
        let scope = scope_t();
        // e_orphan: no edges, no source_refs -> orphan.
        let e_orphan = entity("orphan");
        // e_low: has an incident edge + source_refs, confidence 0.1 -> low-confidence only.
        let mut e_low = entity("low");
        e_low.provenance.confidence = Some(0.1);
        e_low.source_refs.push(evref());
        // e_ok: has an edge + source, confidence 0.9 -> neither.
        let mut e_ok = entity("ok");
        e_ok.provenance.confidence = Some(0.9);
        e_ok.source_refs.push(evref());
        block_on(store.put_entity(e_orphan)).unwrap();
        block_on(store.put_entity(e_low.clone())).unwrap();
        block_on(store.put_entity(e_ok.clone())).unwrap();
        block_on(store.put_relationship(rel("r1", "low", "ok"))).unwrap();
        // archive one entity.
        let mut e_arch = entity("arch");
        e_arch.source_refs.push(evref());
        block_on(store.put_entity(e_arch)).unwrap();
        {
            let conn = store.lock().unwrap();
            assert!(
                archive_entity(&conn, &EntityId::from("arch"), &scope, &actor(), ts()).unwrap()
            );
        }
        let health = block_on(
            <SqlKnowledgeStore as GraphMaintenanceRepository>::graph_health(&store, &scope, None),
        )
        .unwrap();
        assert!(health.orphan_count >= 1, "orphan detected");
        assert!(health.low_confidence_count >= 1, "low-confidence detected");
        assert_eq!(health.archived_entity_count, 1, "one archived entity");
        assert_eq!(health.archived_relationship_count, 0);
    }

    #[test]
    fn full_scope_preview_and_apply_never_expose_hidden_targets() {
        let store = SqlKnowledgeStore::open_in_memory().unwrap();
        let mut private = entity("private-entity");
        private.scope.subject = Some("owner".into());
        private.scope.session = Some("owned-session".into());
        private.scope.environment = Some("owned-environment".into());
        let owner = private.scope.clone();
        let mut absorbed = entity("private-absorbed");
        absorbed.scope = owner.clone();
        block_on(store.put_entity(private.clone())).unwrap();
        block_on(store.put_entity(absorbed.clone())).unwrap();
        let mut relationship = rel("private-rel", "private-entity", "private-absorbed");
        relationship.scope = owner.clone();
        block_on(store.put_relationship(relationship.clone())).unwrap();
        let mutations = vec![
            MaintenanceMutation::Archive {
                target: MaintenanceTarget::Entity(private.id.clone()),
            },
            MaintenanceMutation::Restore {
                target: MaintenanceTarget::Entity(private.id.clone()),
            },
            MaintenanceMutation::Delete {
                target: MaintenanceTarget::Entity(private.id.clone()),
            },
            MaintenanceMutation::AddAlias {
                entity: private.id.clone(),
                alias: "hidden-alias".into(),
            },
            MaintenanceMutation::RemoveAlias {
                entity: private.id.clone(),
                alias: "hidden-alias".into(),
            },
            MaintenanceMutation::RewriteRelationship {
                relationship: relationship.id.clone(),
                new_predicate: Some("hidden-rewrite".into()),
                new_subject: None,
                new_object: None,
            },
            MaintenanceMutation::Merge {
                survivor: private.id.clone(),
                absorbed: vec![absorbed.id.clone()],
            },
            MaintenanceMutation::Archive {
                target: MaintenanceTarget::Relationship(relationship.id.clone()),
            },
            MaintenanceMutation::Restore {
                target: MaintenanceTarget::Relationship(relationship.id.clone()),
            },
            MaintenanceMutation::Delete {
                target: MaintenanceTarget::Relationship(relationship.id.clone()),
            },
        ];
        for field in ["subject", "session", "environment"] {
            let mut requested = owner.clone();
            match field {
                "subject" => requested.subject = Some("other".into()),
                "session" => requested.session = Some("other".into()),
                _ => requested.environment = Some("other".into()),
            }
            for mutation in &mutations {
                let plan = block_on(store.build_plan(MaintenancePlanRequest {
                    graph_id: None,
                    scope: requested.clone(),
                    mutations: vec![mutation.clone()],
                    policy: MaintenancePolicy::default(),
                    actor: actor(),
                }))
                .unwrap();
                assert!(
                    plan.previews
                        .iter()
                        .all(|p| p.before.is_empty() && p.after.is_empty()),
                    "hidden {field} target cannot enter any preview variant"
                );
                let applied = block_on(store.apply_plan(&plan, ApplyMode::Apply)).unwrap();
                assert_eq!(applied.applied, 0, "hidden {field} targets cannot mutate");
                let positive = block_on(store.build_plan(MaintenancePlanRequest {
                    graph_id: None,
                    scope: owner.clone(),
                    mutations: vec![mutation.clone()],
                    policy: MaintenancePolicy::default(),
                    actor: actor(),
                }))
                .unwrap();
                assert!(
                    !positive.previews[0].before.is_empty(),
                    "visible control for each variant"
                );
            }
            let mut visible = entity(&format!("visible-{field}"));
            visible.scope = requested.clone();
            block_on(store.put_entity(visible.clone())).unwrap();
            let mut edge = rel(
                &format!("visible-rel-{field}"),
                visible.id.as_str(),
                visible.id.as_str(),
            );
            edge.scope = requested.clone();
            block_on(store.put_relationship(edge.clone())).unwrap();
            let rewrite = MaintenanceMutation::RewriteRelationship {
                relationship: edge.id.clone(),
                new_predicate: None,
                new_subject: None,
                new_object: Some(private.id.clone()),
            };
            assert!(
                block_on(store.build_plan(MaintenancePlanRequest {
                    graph_id: None,
                    scope: requested.clone(),
                    mutations: vec![rewrite.clone()],
                    policy: MaintenancePolicy::default(),
                    actor: actor()
                }))
                .is_err(),
                "hidden new rewrite endpoint is not an eligible target"
            );
            let direct = MaintenancePlan::new(
                None,
                requested.clone(),
                vec![rewrite],
                MaintenancePolicy::default(),
                actor(),
            );
            assert!(block_on(store.apply_plan(&direct, ApplyMode::Apply)).is_err());
            assert_eq!(
                block_on(store.get_relationship(&edge.id, &requested)).unwrap(),
                Some(edge)
            );
            let mut cross = rel(
                &format!("mixed-rel-{field}"),
                visible.id.as_str(),
                private.id.as_str(),
            );
            cross.scope = requested.clone();
            block_on(store.put_relationship(cross.clone())).unwrap();
            let mixed = block_on(store.build_plan(MaintenancePlanRequest {
                graph_id: None,
                scope: requested.clone(),
                mutations: vec![MaintenanceMutation::Merge {
                    survivor: visible.id.clone(),
                    absorbed: vec![private.id.clone()],
                }],
                policy: MaintenancePolicy::default(),
                actor: actor(),
            }))
            .unwrap();
            assert_eq!(
                mixed.previews[0].before,
                vec![MutationSnapshot::Entity(visible.clone())],
                "hidden absorbed member supplies no snapshot"
            );
            assert_eq!(
                mixed.previews[0].after,
                vec![MutationSnapshot::Entity(visible.clone())],
                "hidden member cannot supply aliases or references"
            );
            let result = block_on(store.apply_plan(&mixed, ApplyMode::Apply)).unwrap();
            assert_eq!(result.applied, 0);
            assert!(
                result.failed > 0,
                "pre-existing cross-scope edge is reported without hidden-member redirects"
            );
            assert_eq!(
                block_on(store.get_entity(&visible.id, &requested)).unwrap(),
                Some(visible)
            );
            assert_eq!(
                block_on(store.get_relationship(&cross.id, &requested)).unwrap(),
                Some(cross),
                "denied merge member cannot redirect a visible relationship"
            );
        }
        assert_eq!(
            block_on(store.get_entity(&private.id, &owner)).unwrap(),
            Some(private.clone())
        );
        assert_eq!(
            block_on(store.get_relationship(&relationship.id, &owner)).unwrap(),
            Some(relationship)
        );
        let mut wildcard = owner;
        wildcard.subject = None;
        wildcard.session = None;
        wildcard.environment = None;
        let broad = block_on(store.build_plan(MaintenancePlanRequest {
            graph_id: None,
            scope: wildcard,
            mutations: vec![MaintenanceMutation::Archive {
                target: MaintenanceTarget::Entity(private.id),
            }],
            policy: MaintenancePolicy::default(),
            actor: actor(),
        }))
        .unwrap();
        assert_eq!(
            broad.previews[0].before.len(),
            1,
            "None retains the canonical optional-dimension wildcard semantics"
        );
    }
}
