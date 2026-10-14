//! Per-file generation leases and authorized exact source reads.
use super::{
    schema::{json_error, sql_error},
    service::SqlKnowledgeStore,
    source_inventory::{invalid, record_kind, table, validate_inventory},
};
use async_trait::async_trait;
use chrono::Utc;
use engram_domain::*;
use engram_knowledge::{
    CodeSourceRepository, authorize_code_source_read, authorize_code_source_scope,
};
use engram_runtime::{CoreError, CoreResult};
use rusqlite::{Connection, OptionalExtension, params};

pub(super) fn file_key(file: &CodeFileKey) -> CoreResult<String> {
    if !file.is_valid() {
        return Err(CoreError::InvalidRequest {
            reason: "invalid code file identity".into(),
        });
    }
    serde_json::to_string(file).map_err(json_error)
}
fn load(conn: &Connection, key: &str) -> CoreResult<Option<CodeSourceGeneration>> {
    conn.query_row(
        "SELECT record_json FROM code_source_generations WHERE file_key=?1",
        [key],
        |r| r.get::<_, String>(0),
    )
    .optional()
    .map_err(sql_error)?
    .map(|json| serde_json::from_str(&json).map_err(json_error))
    .transpose()
}
fn save(conn: &Connection, m: &CodeSourceGeneration, key: &str) -> CoreResult<()> {
    let state = match m.state {
        CodeSourceState::Updating => "updating",
        CodeSourceState::Incomplete => "incomplete",
        CodeSourceState::Active => "active",
    };
    conn.execute("INSERT INTO code_source_generations(file_key,generation,attempt_id,state,record_json) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(file_key) DO UPDATE SET generation=excluded.generation,attempt_id=excluded.attempt_id,state=excluded.state,record_json=excluded.record_json",params![key,m.generation,m.attempt_id,state,serde_json::to_string(m).map_err(json_error)?]).map_err(sql_error)?;
    conn.execute(
        "UPDATE code_source_revision SET revision=revision+1 WHERE singleton=1",
        [],
    )
    .map_err(sql_error)?;
    Ok(())
}
pub(super) fn current_attempt(
    conn: &Connection,
    key: &str,
    attempt: &str,
) -> CoreResult<CodeSourceGeneration> {
    let m = load(conn, key)?.ok_or_else(|| CoreError::Conflict {
        reason: "code source lease is missing".into(),
    })?;
    if m.attempt_id != attempt || m.state != CodeSourceState::Updating {
        return Err(CoreError::Conflict {
            reason: "code source lease was superseded or is not updating".into(),
        });
    }
    Ok(m)
}

#[async_trait]
impl CodeSourceRepository for SqlKnowledgeStore {
    async fn get_code_source(
        &self,
        file: &CodeFileKey,
    ) -> CoreResult<Option<CodeSourceGeneration>> {
        let key = file_key(file)?;
        let conn = self.lock()?;
        load(&conn, &key)
    }
    async fn begin_code_source(
        &self,
        manifest: CodeSourceGeneration,
        expected_attempt: Option<&str>,
    ) -> CoreResult<()> {
        let key = file_key(&manifest.file)?;
        if manifest.state != CodeSourceState::Updating
            || manifest.attempt_id.is_empty()
            || manifest.generation.is_empty()
            || manifest.source_hash.is_empty()
            || manifest.fingerprint.is_empty()
        {
            return Err(invalid("invalid updating source manifest"));
        }
        let mut conn = self.lock()?;
        let tx = conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let prior = load(&tx, &key)?;
        if prior.as_ref().map(|p| p.attempt_id.as_str()) != expected_attempt
            || prior
                .as_ref()
                .is_some_and(|p| p.attempt_id == manifest.attempt_id)
        {
            return Err(CoreError::Conflict {
                reason: "source attempt compare-and-set failed".into(),
            });
        }
        let reused:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM code_source_attempts WHERE file_key=?1 AND attempt_id=?2)",params![key,manifest.attempt_id],|r|r.get(0)).map_err(sql_error)?;
        if reused || manifest.attempt_id.starts_with("legacy:") {
            return Err(CoreError::Conflict {
                reason: "source attempt identity was already used or is reserved".into(),
            });
        }
        tx.execute(
            "INSERT INTO code_source_attempts(file_key,attempt_id) VALUES(?1,?2)",
            params![key, manifest.attempt_id],
        )
        .map_err(sql_error)?;
        super::source_legacy::attribute_legacy_records(&tx, &manifest.file, &key)?;
        save(&tx, &manifest, &key)?;
        tx.commit().map_err(sql_error)
    }
    async fn stage_code_records(
        &self,
        file: &CodeFileKey,
        attempt: &str,
        records: &[CodeSourceRecord],
    ) -> CoreResult<()> {
        let key = file_key(file)?;
        let mut conn = self.lock()?;
        let tx = conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let mut manifest = current_attempt(&tx, &key, attempt)?;
        for record in records {
            if record.id.is_empty() {
                return Err(invalid("empty source contribution identity"));
            }
            tx.execute("INSERT OR IGNORE INTO code_source_contributions(file_key,attempt_id,kind,record_id) VALUES(?1,?2,?3,?4)",params![key,attempt,record_kind(record.kind),record.id]).map_err(sql_error)?;
            if !manifest.records.contains(record) {
                manifest.records.push(record.clone());
            }
        }
        save(&tx, &manifest, &key)?;
        tx.commit().map_err(sql_error)
    }
    async fn write_code_record(
        &self,
        file: &CodeFileKey,
        attempt: &str,
        payload: engram_knowledge::CodeSourceWrite,
    ) -> CoreResult<()> {
        let key = file_key(file)?;
        let mut conn = self.lock()?;
        let tx = conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let manifest = current_attempt(&tx, &key, attempt)?;
        super::guarded_source_write::write(&tx, &key, &manifest, payload)?;
        tx.commit().map_err(sql_error)
    }
    async fn activate_code_source(&self, mut manifest: CodeSourceGeneration) -> CoreResult<()> {
        let mut seen = std::collections::HashSet::new();
        manifest
            .records
            .retain(|r| seen.insert(format!("{:?}:{}", r.kind, r.id)));
        let key = file_key(&manifest.file)?;
        let mut conn = self.lock()?;
        let tx = conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let prior = current_attempt(&tx, &key, &manifest.attempt_id)?;
        if manifest.state != CodeSourceState::Active
            || manifest.generation != prior.generation
            || manifest.source_hash != prior.source_hash
            || manifest.fingerprint != prior.fingerprint
            || manifest.byte_length != prior.byte_length
            || manifest.policy != prior.policy
            || manifest.provenance != prior.provenance
        {
            return Err(invalid(
                "activation changed immutable source identity or policy",
            ));
        }
        validate_inventory(&tx, &manifest, &key)?;
        save(&tx, &manifest, &key)?;
        tx.commit().map_err(sql_error)
    }
    async fn fail_code_source(&self, file: &CodeFileKey, attempt: &str) -> CoreResult<()> {
        let key = file_key(file)?;
        let mut conn = self.lock()?;
        let tx = conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let mut m = load(&tx, &key)?.ok_or_else(|| CoreError::Conflict {
            reason: "source lease is missing".into(),
        })?;
        if m.attempt_id != attempt {
            return Err(CoreError::Conflict {
                reason: "source lease was superseded".into(),
            });
        }
        m.state = CodeSourceState::Incomplete;
        save(&tx, &m, &key)?;
        tx.commit().map_err(sql_error)
    }
    async fn prior_code_records(
        &self,
        file: &CodeFileKey,
        attempt: &str,
    ) -> CoreResult<Vec<(String, CodeSourceRecord)>> {
        let key = file_key(file)?;
        let conn = self.lock()?;
        current_attempt(&conn, &key, attempt)?;
        let mut stmt=conn.prepare("SELECT attempt_id,kind,record_id FROM code_source_contributions WHERE file_key=?1 AND attempt_id<>?2 ORDER BY kind,record_id").map_err(sql_error)?;
        let rows = stmt
            .query_map(params![key, attempt], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })
            .map_err(sql_error)?;
        let mut output = vec![];
        for row in rows {
            let (owner, kind, id) = row.map_err(sql_error)?;
            let kind: CodeSourceRecordKind =
                serde_json::from_value(serde_json::Value::String(kind)).map_err(json_error)?;
            output.push((owner, CodeSourceRecord { kind, id }));
        }
        Ok(output)
    }
    async fn remove_code_record(
        &self,
        file: &CodeFileKey,
        attempt: &str,
        superseded_attempt: &str,
        record: &CodeSourceRecord,
    ) -> CoreResult<bool> {
        let key = file_key(file)?;
        let mut conn = self.lock()?;
        let tx = conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let manifest = current_attempt(&tx, &key, attempt)?;
        if superseded_attempt == attempt {
            return Err(CoreError::Conflict {
                reason: "cleanup cannot remove the current attempt".into(),
            });
        }
        let owned:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM code_source_contributions WHERE file_key=?1 AND attempt_id=?2 AND kind=?3 AND record_id=?4)",params![key,superseded_attempt,record_kind(record.kind),record.id],|r|r.get(0)).map_err(sql_error)?;
        if !owned {
            return Err(CoreError::PolicyDenied {
                reason: "source does not own superseded cleanup record".into(),
            });
        }
        let retained:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM code_source_contributions c JOIN code_source_generations g ON g.file_key=c.file_key AND g.attempt_id=c.attempt_id WHERE c.kind=?1 AND c.record_id=?2 AND (g.state='active' OR (c.file_key=?3 AND c.attempt_id=?4)))",params![record_kind(record.kind),record.id,key,attempt],|r|r.get(0)).map_err(sql_error)?;
        if !retained {
            tx.execute(
                &format!("DELETE FROM {} WHERE id=?1", table(record.kind)),
                [&record.id],
            )
            .map_err(sql_error)?;
        }
        tx.execute("DELETE FROM code_source_contributions WHERE file_key=?1 AND attempt_id=?2 AND kind=?3 AND record_id=?4",params![key,superseded_attempt,record_kind(record.kind),record.id]).map_err(sql_error)?;
        save(&tx, &manifest, &key)?;
        tx.commit().map_err(sql_error)?;
        Ok(!retained)
    }
    async fn list_code_sources(&self, scope: &Scope) -> CoreResult<Vec<CodeSourceGeneration>> {
        let conn = self.lock()?;
        let mut stmt = conn
            .prepare("SELECT record_json FROM code_source_generations")
            .map_err(sql_error)?;
        let rows = stmt
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(sql_error)?;
        let mut output = vec![];
        for row in rows {
            let m: CodeSourceGeneration =
                serde_json::from_str(&row.map_err(sql_error)?).map_err(json_error)?;
            if m.file.scope == *scope {
                output.push(m);
            }
        }
        Ok(output)
    }
    async fn prune_repository_context(
        &self,
        scope: &Scope,
        repository: &str,
        id: &EntityId,
    ) -> CoreResult<bool> {
        let mut conn = self.lock()?;
        let tx = conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let mut retained = false;
        {
            let mut stmt = tx
                .prepare(
                    "SELECT record_json FROM knowledge_graphs_records WHERE stable_source_key=?1",
                )
                .map_err(sql_error)?;
            let rows = stmt
                .query_map([repository], |r| r.get::<_, String>(0))
                .map_err(sql_error)?;
            for row in rows {
                let graph: KnowledgeGraph =
                    serde_json::from_str(&row.map_err(sql_error)?).map_err(json_error)?;
                if graph.scope == *scope {
                    retained = true;
                    break;
                }
            }
        }
        if retained {
            return Ok(false);
        }
        let json: Option<String> = tx
            .query_row(
                "SELECT record_json FROM knowledge_entities_records WHERE id=?1",
                [id.as_str()],
                |r| r.get(0),
            )
            .optional()
            .map_err(sql_error)?;
        if let Some(json) = json {
            let root: KnowledgeEntity = serde_json::from_str(&json).map_err(json_error)?;
            if root.scope != *scope
                || root.kind != EntityKind::Repository
                || root.graph_id.is_some()
                || root
                    .metadata
                    .as_ref()
                    .and_then(|v| v.get("stableSourceKey"))
                    .and_then(|v| v.as_str())
                    != Some(repository)
            {
                return Err(CoreError::PolicyDenied {
                    reason: "repository GC identity mismatch".into(),
                });
            }
            super::guarded_source_write::reject_unfenced_write(
                &tx,
                CodeSourceRecordKind::Entity,
                id.as_str(),
            )?;
            tx.execute(
                "DELETE FROM knowledge_entities_records WHERE id=?1",
                [id.as_str()],
            )
            .map_err(sql_error)?;
            tx.commit().map_err(sql_error)?;
            return Ok(true);
        }
        Ok(false)
    }
    async fn ensure_repository_context(
        &self,
        file: &CodeFileKey,
        entity: KnowledgeEntity,
    ) -> CoreResult<()> {
        file_key(file)?;
        if entity.kind != EntityKind::Repository
            || entity.scope != file.scope
            || entity.graph_id.is_some()
            || entity
                .metadata
                .as_ref()
                .and_then(|v| v.get("stableSourceKey"))
                .and_then(|v| v.as_str())
                != Some(file.repository.as_str())
        {
            return Err(invalid("invalid repository context identity"));
        }
        let mut conn = self.lock()?;
        let tx = conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let existing: Option<String> = tx
            .query_row(
                "SELECT record_json FROM knowledge_entities_records WHERE id=?1",
                [entity.id.as_str()],
                |r| r.get(0),
            )
            .optional()
            .map_err(sql_error)?;
        if let Some(json) = existing {
            let prior: KnowledgeEntity = serde_json::from_str(&json).map_err(json_error)?;
            if prior.scope != entity.scope
                || prior.kind != entity.kind
                || prior.name != entity.name
                || prior.graph_id != entity.graph_id
                || prior
                    .metadata
                    .as_ref()
                    .and_then(|v| v.get("stableSourceKey"))
                    != entity
                        .metadata
                        .as_ref()
                        .and_then(|v| v.get("stableSourceKey"))
            {
                return Err(CoreError::Conflict {
                    reason: "repository context collides with existing identity".into(),
                });
            }
        } else {
            super::guarded_source_write::reject_unfenced_write(
                &tx,
                CodeSourceRecordKind::Entity,
                entity.id.as_str(),
            )?;
            super::record_writes::put_entity(&tx, entity)?;
        }
        tx.commit().map_err(sql_error)
    }
    async fn export_code_source(
        &self,
        request: CodeSourceReadRequest,
        authorized_scope: &Scope,
    ) -> CoreResult<CodeSourceBundle> {
        authorize_code_source_scope(&request, authorized_scope)?;
        if !request
            .requester
            .permissions
            .iter()
            .any(|p| p == "code:source:export")
        {
            return Err(CoreError::PolicyDenied {
                reason: "source-export authority required".into(),
            });
        }
        let key = file_key(&request.file)?;
        let mut conn = self.lock()?;
        let tx = conn.transaction().map_err(sql_error)?;
        let m = load(&tx, &key)?.ok_or_else(|| CoreError::ProviderUnavailable {
            provider: "code source unavailable".into(),
        })?;
        authorize_code_source_read(&request, authorized_scope, &m, Utc::now())?;
        validate_inventory(&tx, &m, &key)?;
        let mut bundle = CodeSourceBundle {
            version: 1,
            manifest: m.clone(),
            sources: vec![],
            documents: vec![],
            chunks: vec![],
            graphs: vec![],
            entities: vec![],
            relationships: vec![],
            unresolved_references: vec![],
            context_entities: vec![],
        };
        for record in &m.records {
            let json: String = tx
                .query_row(
                    &format!("SELECT record_json FROM {} WHERE id=?1", table(record.kind)),
                    [&record.id],
                    |r| r.get(0),
                )
                .map_err(sql_error)?;
            match record.kind {
                CodeSourceRecordKind::Source => bundle
                    .sources
                    .push(serde_json::from_str(&json).map_err(json_error)?),
                CodeSourceRecordKind::Document => bundle
                    .documents
                    .push(serde_json::from_str(&json).map_err(json_error)?),
                CodeSourceRecordKind::Chunk => bundle
                    .chunks
                    .push(serde_json::from_str(&json).map_err(json_error)?),
                CodeSourceRecordKind::Graph => bundle
                    .graphs
                    .push(serde_json::from_str(&json).map_err(json_error)?),
                CodeSourceRecordKind::Entity => bundle
                    .entities
                    .push(serde_json::from_str(&json).map_err(json_error)?),
                CodeSourceRecordKind::Relationship => bundle
                    .relationships
                    .push(serde_json::from_str(&json).map_err(json_error)?),
                CodeSourceRecordKind::UnresolvedReference => bundle
                    .unresolved_references
                    .push(serde_json::from_str(&json).map_err(json_error)?),
            }
        }
        let mut seen = std::collections::HashSet::new();
        for relation in bundle
            .relationships
            .iter()
            .filter(|r| r.predicate == "belongs_to")
        {
            let id = relation
                .object
                .id
                .as_ref()
                .ok_or_else(|| invalid("repository context reference has no ID"))?;
            if !seen.insert(id.as_str()) {
                continue;
            }
            let json: String = tx
                .query_row(
                    "SELECT record_json FROM knowledge_entities_records WHERE id=?1",
                    [id.as_str()],
                    |r| r.get(0),
                )
                .map_err(sql_error)?;
            let entity: KnowledgeEntity = serde_json::from_str(&json).map_err(json_error)?;
            if entity.kind != EntityKind::Repository || entity.scope != m.file.scope {
                return Err(invalid(
                    "repository context reference is not in source scope",
                ));
            }
            bundle.context_entities.push(entity);
        }
        tx.commit().map_err(sql_error)?;
        Ok(bundle)
    }
    async fn read_code_source(
        &self,
        request: CodeSourceReadRequest,
        authorized_scope: &Scope,
    ) -> CoreResult<CodeSourceReadResult> {
        authorize_code_source_scope(&request, authorized_scope)?;
        let key = file_key(&request.file)?;
        let mut conn = self.lock()?;
        // A database read transaction also fences other store instances and
        // processes; the Rust mutex alone only protects this handle.
        let tx = conn.transaction().map_err(sql_error)?;
        let m = load(&tx, &key)?.ok_or_else(|| CoreError::ProviderUnavailable {
            provider: "requested code source generation is unavailable".into(),
        })?;
        authorize_code_source_read(&request, authorized_scope, &m, Utc::now())?;
        let text = validate_inventory(&tx, &m, &key)?;
        let mut coverage = Vec::new();
        for r in m
            .records
            .iter()
            .filter(|r| r.kind == CodeSourceRecordKind::Chunk)
        {
            let json: String = tx
                .query_row(
                    "SELECT record_json FROM knowledge_chunks_records WHERE id=?1",
                    [&r.id],
                    |r| r.get(0),
                )
                .map_err(sql_error)?;
            let c: KnowledgeChunk = serde_json::from_str(&json).map_err(json_error)?;
            if let Some(code) = c.code {
                if code.role != CodeChunkRole::SourceSnapshot {
                    coverage.push(code);
                }
            }
        }
        tx.commit().map_err(sql_error)?;
        Ok(CodeSourceReadResult {
            manifest: m,
            text,
            coverage,
        })
    }
}

impl SqlKnowledgeStore {
    /// Monotonic eligibility version for graph snapshots; scoped reads still
    /// perform their normal authorization independently of this cache token.
    pub fn code_source_revision(&self) -> CoreResult<u64> {
        self.lock()?
            .query_row(
                "SELECT revision FROM code_source_revision WHERE singleton=1",
                [],
                |r| r.get(0),
            )
            .map_err(sql_error)
    }
}
