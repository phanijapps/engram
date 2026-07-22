//! SQLite identity and consolidation — implements `EntityIdentityRepository`
//! over the same connection as `SqlKnowledgeStore`, using `identity_key` and
//! `relationship_key` columns with UNIQUE partial indexes for concurrent
//! convergence.

use std::sync::{Arc, Mutex, MutexGuard};

use async_trait::async_trait;
use engram_domain::*;
use engram_knowledge::identity::{compute_identity_key, compute_relationship_key, merge_entities};
use engram_knowledge::EntityIdentityRepository;
use engram_runtime::{CoreError, CoreResult};
use rusqlite::{params, OptionalExtension};

use crate::knowledge::schema::{json_error, sql_error};

pub struct SqlIdentityStore {
    connection: Arc<Mutex<rusqlite::Connection>>,
}

impl SqlIdentityStore {
    pub fn new(connection: Arc<Mutex<rusqlite::Connection>>) -> Self {
        Self { connection }
    }

    fn lock(&self) -> CoreResult<MutexGuard<'_, rusqlite::Connection>> {
        self.connection.lock().map_err(|e| CoreError::Adapter {
            adapter: "engram-store-sqlite.identity".to_owned(),
            message: format!("lock: {e}"),
        })
    }

    // ── Entity helpers ────────────────────────────────────────────────────

    fn select_entity_by_key(&self, key: &str) -> CoreResult<Option<KnowledgeEntity>> {
        let conn = self.lock()?;
        let json: Option<String> = conn
            .query_row(
                "SELECT record_json FROM knowledge_entities WHERE identity_key = ?1",
                params![key],
                |row| row.get(0),
            )
            .optional()
            .map_err(sql_error)?;
        json.map(|j| serde_json::from_str(&j).map_err(json_error)).transpose()
    }

    fn upsert_entity_with_key(&self, entity: &KnowledgeEntity, key: &str) -> CoreResult<()> {
        let json = serde_json::to_string(entity).map_err(json_error)?;
        let conn = self.lock()?;
        let graph_id = entity.graph_id.as_ref().map(|g| g.to_string()).unwrap_or_default();
        conn.execute(
            r#"INSERT INTO knowledge_entities
               (id, graph_id, tenant, subject, workspace, session, environment, identity_key, record_json)
               VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)
               ON CONFLICT(id) DO UPDATE SET
                   graph_id=excluded.graph_id, tenant=excluded.tenant,
                   subject=excluded.subject, workspace=excluded.workspace,
                   session=excluded.session, environment=excluded.environment,
                   identity_key=excluded.identity_key, record_json=excluded.record_json"#,
            params![entity.id.to_string(), graph_id, entity.scope.tenant.clone(),
                    entity.scope.subject.clone(), entity.scope.workspace.clone(),
                    entity.scope.session.clone(), entity.scope.environment.clone(),
                    key, json],
        ).map_err(sql_error)?;
        Ok(())
    }

    // ── Relationship helpers ──────────────────────────────────────────────

    fn select_relationship_by_key(&self, key: &str) -> CoreResult<Option<KnowledgeRelationship>> {
        let conn = self.lock()?;
        let json: Option<String> = conn
            .query_row(
                "SELECT record_json FROM knowledge_relationships WHERE relationship_key = ?1",
                params![key],
                |row| row.get(0),
            )
            .optional()
            .map_err(sql_error)?;
        json.map(|j| serde_json::from_str(&j).map_err(json_error)).transpose()
    }

    fn upsert_relationship_with_key(&self, rel: &KnowledgeRelationship, key: &str) -> CoreResult<()> {
        let json = serde_json::to_string(rel).map_err(json_error)?;
        let conn = self.lock()?;
        let graph_id = rel.graph_id.as_ref().map(|g| g.to_string()).unwrap_or_default();
        let subject_id = rel.subject.id.as_ref().map(|i| i.to_string()).unwrap_or_default();
        conn.execute(
            r#"INSERT INTO knowledge_relationships
               (id, graph_id, subject_id, tenant, subject, workspace, session, environment, relationship_key, record_json)
               VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)
               ON CONFLICT(id) DO UPDATE SET
                   graph_id=excluded.graph_id, subject_id=excluded.subject_id,
                   tenant=excluded.tenant, subject=excluded.subject,
                   workspace=excluded.workspace, session=excluded.session,
                   environment=excluded.environment,
                   relationship_key=excluded.relationship_key, record_json=excluded.record_json"#,
            params![rel.id.to_string(), graph_id, subject_id,
                    rel.scope.tenant.clone(), rel.scope.subject.clone(),
                    rel.scope.workspace.clone(), rel.scope.session.clone(),
                    rel.scope.environment.clone(), key, json],
        ).map_err(sql_error)?;
        Ok(())
    }
}

#[async_trait]
impl EntityIdentityRepository for SqlIdentityStore {
    async fn resolve_or_put_entity(
        &self,
        request: EntityWriteRequest,
    ) -> CoreResult<EntityWriteOutcome> {
        let key = compute_identity_key(&request.entity, &request.identity);
        match key {
            None => {
                // IdOnly — plain insert (identity_key = NULL).
                self.upsert_entity_with_key(&request.entity, "")?;
                // Blank key means no identity resolution; set it to NULL in DB.
                if !request.entity.id.to_string().is_empty() {
                    let conn = self.lock()?;
                    let _ = conn.execute(
                        "UPDATE knowledge_entities SET identity_key = NULL WHERE id = ?1",
                        params![request.entity.id.to_string()],
                    );
                }
                Ok(EntityWriteOutcome::Created { entity: request.entity })
            }
            Some(key) => {
                let existing = self.select_entity_by_key(&key)?;
                match existing {
                    None => {
                        self.upsert_entity_with_key(&request.entity, &key)?;
                        Ok(EntityWriteOutcome::Created { entity: request.entity })
                    }
                    Some(existing_entity) => {
                        let (merged, changed, conflicts) =
                            merge_entities(&existing_entity, &request.entity, &request.merge_policy);
                        if changed.is_empty() {
                            Ok(EntityWriteOutcome::Matched { entity: existing_entity })
                        } else {
                            self.upsert_entity_with_key(&merged, &key)?;
                            Ok(EntityWriteOutcome::Merged {
                                entity: merged,
                                changed_fields: changed,
                                conflicts,
                            })
                        }
                    }
                }
            }
        }
    }

    async fn resolve_or_put_relationship(
        &self,
        relationship: KnowledgeRelationship,
    ) -> CoreResult<KnowledgeRelationship> {
        let key = compute_relationship_key(&relationship);
        let existing = self.select_relationship_by_key(&key)?;
        match existing {
            None => {
                self.upsert_relationship_with_key(&relationship, &key)?;
                Ok(relationship)
            }
            Some(mut existing_rel) => {
                // Merge evidence, provenance, confidence from the new into existing.
                for ev in &relationship.evidence {
                    if !existing_rel.evidence.iter().any(|e| e.target_id == ev.target_id) {
                        existing_rel.evidence.push(ev.clone());
                    }
                }
                if relationship.confidence.is_some() && existing_rel.confidence.is_none() {
                    existing_rel.confidence = relationship.confidence;
                }
                existing_rel.updated_at = relationship.created_at.into();
                self.upsert_relationship_with_key(&existing_rel, &key)?;
                Ok(existing_rel)
            }
        }
    }

    async fn discover_collisions(
        &self,
        scope: &Scope,
        mode: &EntityIdentityMode,
    ) -> CoreResult<Vec<CollisionGroup>> {
        let conn = self.lock()?;
        let mut stmt = conn
            .prepare(
                "SELECT identity_key, GROUP_CONCAT(id, ',') FROM knowledge_entities
                 WHERE identity_key IS NOT NULL AND tenant = ?1
                 GROUP BY identity_key HAVING COUNT(*) > 1",
            )
            .map_err(sql_error)?;
        let rows: Vec<(String, String)> = stmt
            .query_map(params![&scope.tenant], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .map_err(sql_error)?
            .filter_map(|r| r.ok())
            .collect();
        let _ = mode; // the mode is implicit in which identity_key values exist
        Ok(rows
            .into_iter()
            .map(|(key, ids)| CollisionGroup {
                identity_key: key,
                entity_ids: ids.split(',').map(EntityId::from).collect(),
            })
            .collect())
    }

    async fn consolidate_entities(
        &self,
        request: EntityMergeRequest,
    ) -> CoreResult<EntityMergeResult> {
        let mut conn = self.lock()?;
        let tx = conn.transaction().map_err(sql_error)?;
        let canonical_id = request.canonical_id.to_string();
        let mut redirected = 0usize;
        let mut coalesced = 0usize;
        let mut all_conflicts = Vec::new();

        for dup_id in &request.duplicate_ids {
            let dup_id_str = dup_id.to_string();

            // Load the duplicate entity.
            let dup_json: Option<String> = tx
                .query_row(
                    "SELECT record_json FROM knowledge_entities WHERE id = ?1 AND tenant = ?2",
                    params![&dup_id_str, &request.scope.tenant],
                    |row| row.get(0),
                )
                .optional()
                .map_err(sql_error)?;

            let Some(dup_json) = dup_json else { continue };
            let dup_entity: KnowledgeEntity = serde_json::from_str(&dup_json).map_err(json_error)?;

            // Load the canonical entity.
            let canon_json: String = tx
                .query_row(
                    "SELECT record_json FROM knowledge_entities WHERE id = ?1 AND tenant = ?2",
                    params![&canonical_id, &request.scope.tenant],
                    |row| row.get(0),
                )
                .map_err(sql_error)?;
            let canon_entity: KnowledgeEntity = serde_json::from_str(&canon_json).map_err(json_error)?;

            // Merge.
            let (merged, _, conflicts) = merge_entities(&canon_entity, &dup_entity, &request.policy);
            all_conflicts.extend(conflicts);
            let merged_json = serde_json::to_string(&merged).map_err(json_error)?;

            // Update canonical entity record.
            let _merged_key = tx
                .query_row::<Option<String>, _, _>(
                    "SELECT identity_key FROM knowledge_entities WHERE id = ?1",
                    params![&canonical_id],
                    |row| row.get(0),
                )
                .optional()
                .map_err(sql_error)?;
            tx.execute(
                "UPDATE knowledge_entities SET record_json = ?1 WHERE id = ?2",
                params![merged_json, &canonical_id],
            )
            .map_err(sql_error)?;

            // Redirect relationships: subject_id.
            redirected += tx.execute(
                "UPDATE knowledge_relationships SET subject_id = ?1 WHERE subject_id = ?2",
                params![&canonical_id, &dup_id_str],
            )
            .map_err(sql_error)?;

            // Redirect relationships: object_id (stored in record_json).
            // Load relationships where object references the duplicate, fix the JSON.
            {
                let mut stmt = tx
                    .prepare("SELECT id, record_json FROM knowledge_relationships WHERE record_json LIKE ?1")
                    .map_err(sql_error)?;
                let to_fix: Vec<(String, String)> = stmt
                    .query_map(params![format!("%{}%", dup_id_str)], |row| {
                        Ok((row.get(0)?, row.get(1)?))
                    })
                    .map_err(sql_error)?
                    .filter_map(|r| r.ok())
                    .collect();
                drop(stmt);
                for (rel_id, rel_json) in to_fix {
                    if let Ok(mut rel) = serde_json::from_str::<KnowledgeRelationship>(&rel_json) {
                        let changed = replace_entity_ref_id(&mut rel.object, dup_id, &request.canonical_id)
                            || replace_entity_ref_id(&mut rel.subject, dup_id, &request.canonical_id);
                        if changed {
                            let new_json = serde_json::to_string(&rel).unwrap_or(rel_json);
                            let new_key = compute_relationship_key(&rel);
                            tx.execute(
                                "UPDATE knowledge_relationships SET record_json = ?1, relationship_key = ?2 WHERE id = ?3",
                                params![new_json, new_key, rel_id],
                            ).map_err(sql_error)?;
                        }
                    }
                }
            }

            // Coalesce: delete duplicate relationships by relationship_key.
            coalesced += tx.execute(
                "DELETE FROM knowledge_relationships WHERE rowid NOT IN (
                    SELECT MIN(rowid) FROM knowledge_relationships
                    WHERE relationship_key IS NOT NULL
                    GROUP BY relationship_key
                ) AND relationship_key IS NOT NULL",
                [],
            )
            .map_err(sql_error)?;

            // Delete the duplicate entity.
            tx.execute(
                "DELETE FROM knowledge_entities WHERE id = ?1",
                params![&dup_id_str],
            )
            .map_err(sql_error)?;
        }

        // Load final canonical entity for the result.
        let final_json: String = tx
            .query_row(
                "SELECT record_json FROM knowledge_entities WHERE id = ?1",
                params![&canonical_id],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        let final_entity: KnowledgeEntity = serde_json::from_str(&final_json).map_err(json_error)?;

        tx.commit().map_err(sql_error)?;

        Ok(EntityMergeResult {
            canonical_entity: final_entity,
            redirected_relationships: redirected,
            coalesced_relationships: coalesced,
            deleted_entities: request.duplicate_ids.len(),
            conflicts: all_conflicts,
            audit_id: format!("consolidate-{}", canonical_id),
        })
    }
}

/// Replace an EntityRef's id if it matches old_id. Returns true if changed.
fn replace_entity_ref_id(ref_: &mut EntityRef, old_id: &EntityId, new_id: &EntityId) -> bool {
    if ref_.id.as_ref() == Some(old_id) {
        ref_.id = Some(new_id.clone());
        true
    } else {
        false
    }
}
