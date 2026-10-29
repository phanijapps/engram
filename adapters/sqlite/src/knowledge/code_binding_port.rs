//! Neutral repository port over the durable binding cell, with connection serialization.
use super::code_binding::SqlCodeBindingStore;
use async_trait::async_trait;
use engram_domain::*;
use engram_knowledge::*;
use rusqlite::{OptionalExtension, params};
use std::{
    path::Path,
    sync::{Mutex, MutexGuard},
};

pub struct SqlCodeBindingRepository {
    store: Mutex<SqlCodeBindingStore>,
}
fn error(error: CodeBindingError) -> CoreError {
    match error {
        CodeBindingError::BindingConflict => CoreError::Conflict {
            reason: "BindingConflict".into(),
        },
        CodeBindingError::BindingRequired => CoreError::InvalidRequest {
            reason: "BindingRequired".into(),
        },
        CodeBindingError::StaleGeneration => CoreError::Conflict {
            reason: "GenerationChanged".into(),
        },
        CodeBindingError::PersistenceFailure => storage_error(),
        _ => CoreError::InvalidRequest {
            reason: "invalid code binding state".into(),
        },
    }
}
pub(super) fn storage_error() -> CoreError {
    CoreError::Adapter {
        adapter: "engram-code-binding".into(),
        message: "durable code authority is unavailable".into(),
    }
}
fn json<T: serde::Serialize>(value: &T) -> CoreResult<String> {
    serde_json::to_string(value).map_err(|_| storage_error())
}
impl SqlCodeBindingRepository {
    pub fn open(path: impl AsRef<Path>) -> CoreResult<Self> {
        Ok(Self {
            store: Mutex::new(SqlCodeBindingStore::open(path).map_err(|_| storage_error())?),
        })
    }
    pub(super) fn lock(&self) -> CoreResult<MutexGuard<'_, SqlCodeBindingStore>> {
        self.store.lock().map_err(|_| storage_error())
    }
}
#[async_trait]
impl CodeBindingRepository for SqlCodeBindingRepository {
    async fn register(
        &self,
        registration: RepositoryRegistration,
    ) -> CoreResult<RepositoryRegistration> {
        if registration.repo_id.0.is_empty() || registration.locator_id.is_empty() {
            return Err(error(CodeBindingError::InvalidRepoId));
        }
        let store = self.lock()?;
        let conn = &store.connection;
        conn.execute_batch("CREATE TABLE IF NOT EXISTS code_repository_registrations(repo_id TEXT PRIMARY KEY,record_json TEXT NOT NULL)").map_err(|_| storage_error())?;
        let value = json(&registration)?;
        conn.execute("INSERT OR IGNORE INTO code_repository_registrations(repo_id,record_json) VALUES(?1,?2)",params![registration.repo_id.0,value]).map_err(|_| storage_error())?;
        let saved: String = conn
            .query_row(
                "SELECT record_json FROM code_repository_registrations WHERE repo_id=?1",
                [&registration.repo_id.0],
                |r| r.get(0),
            )
            .map_err(|_| storage_error())?;
        if saved != value {
            return Err(error(CodeBindingError::BindingConflict));
        }
        Ok(registration)
    }
    async fn bind(&self, binding: CodeBinding) -> CoreResult<CodeBinding> {
        self.lock()?.bind(&binding).map_err(error)
    }
    async fn get_binding(
        &self,
        scope: &Scope,
        repo_id: &RepoId,
    ) -> CoreResult<Option<CodeBinding>> {
        self.lock()?
            .binding(scope, &repo_id.0)
            .map_err(|_| storage_error())
    }
    async fn begin_attempt(
        &self,
        scope: &Scope,
        repo_id: &RepoId,
        generation: CodeGeneration,
        attempt: CodeAttempt,
    ) -> CoreResult<CodeAttempt> {
        let mut store = self.lock()?;
        store
            .begin_attempt(scope, &repo_id.0, &generation, &attempt)
            .map_err(error)?;
        store
            .attempt(scope, &repo_id.0, &attempt.id)
            .map_err(error)?
            .map(|(_, attempt)| attempt)
            .ok_or_else(storage_error)
    }
    async fn get_attempt(
        &self,
        scope: &Scope,
        repo_id: &RepoId,
        attempt_id: &str,
    ) -> CoreResult<Option<(CodeGeneration, CodeAttempt)>> {
        self.lock()?
            .attempt(scope, &repo_id.0, attempt_id)
            .map_err(error)
    }
    async fn put_generation(&self, generation: CodeGeneration) -> CoreResult<CodeGeneration> {
        self.lock()?.stage_generation(&generation).map_err(error)?;
        Ok(generation)
    }
    async fn put_lineage(
        &self,
        scope: &Scope,
        record_id: &str,
        lineage: CodeLineage,
    ) -> CoreResult<CodeLineage> {
        lineage.validate().map_err(error)?;
        let store = self.lock()?;
        let key = json(scope)?;
        let value = json(&lineage)?;
        store.connection.execute_batch("CREATE TABLE IF NOT EXISTS code_stage_lineage(scope_key TEXT NOT NULL,record_id TEXT NOT NULL,repo_id TEXT NOT NULL,generation_id TEXT NOT NULL,record_json TEXT NOT NULL,PRIMARY KEY(scope_key,record_id))").map_err(|_| storage_error())?;
        let saved: Option<String> = store
            .connection
            .query_row(
                "SELECT record_json FROM code_stage_lineage WHERE scope_key=?1 AND record_id=?2",
                params![key, record_id],
                |r| r.get(0),
            )
            .optional()
            .map_err(|_| storage_error())?;
        if saved.as_ref().is_some_and(|saved| saved != &value) {
            return Err(error(CodeBindingError::InvalidSource));
        }
        store.connection.execute("INSERT OR IGNORE INTO code_stage_lineage(scope_key,record_id,repo_id,generation_id,record_json) VALUES(?1,?2,?3,?4,?5)",params![key,record_id,lineage.repo_id.0,lineage.generation_id,value]).map_err(|_| storage_error())?;
        Ok(lineage)
    }
    /// Corrective r4 (canonical 3a): the owned maintenance mutation. One
    /// IMMEDIATE transaction performs the binding-epoch CAS, retires the
    /// durable staged lineage of every target and advances each involved
    /// binding's epoch plus the scope epoch. The first side effect
    /// (lineage retirement) linearizes atomically with the authority check:
    /// a publication that committed first fails the CAS (no retirement), and
    /// a racing publication that has not committed fails or retries against
    /// the advanced epoch — the mutation cannot apply on stale authority.
    async fn retire_lineage_targets(
        &self,
        scope: &Scope,
        expected_binding_epochs: &[(engram_domain::RepoId, u64)],
        targets: &[String],
    ) -> CoreResult<()> {
        if expected_binding_epochs.is_empty() || targets.is_empty() {
            return Err(CoreError::InvalidRequest {
                reason: "fenced maintenance requires explicit repos and targets".into(),
            });
        }
        let mut store = self.lock()?;
        let key = json(scope)?;
        let tx = store
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(|_| storage_error())?;
        tx.execute_batch("CREATE TABLE IF NOT EXISTS code_stage_lineage(scope_key TEXT NOT NULL,record_id TEXT NOT NULL,repo_id TEXT NOT NULL,generation_id TEXT NOT NULL,record_json TEXT NOT NULL,PRIMARY KEY(scope_key,record_id)); CREATE TABLE IF NOT EXISTS code_stage_lineage_retirements(scope_key TEXT NOT NULL,record_id TEXT NOT NULL,PRIMARY KEY(scope_key,record_id))")
            .map_err(|_| storage_error())?;
        // (1) Authority CAS on every involved binding epoch.
        for (repo_id, expected_epoch) in expected_binding_epochs {
            let record: Option<String> = tx
                .query_row(
                    "SELECT record_json FROM code_repo_bindings WHERE tenant=?1 AND scope_key=?2 AND repo_id=?3",
                    params![scope.tenant, key, repo_id.0],
                    |r| r.get(0),
                )
                .optional()
                .map_err(|_| storage_error())?;
            let Some(record) = record else {
                return Err(CoreError::Conflict {
                    reason: "GenerationChanged: maintenance repo is not bound".into(),
                });
            };
            let mut binding: engram_domain::CodeBinding =
                serde_json::from_str(&record).map_err(|_| storage_error())?;
            if binding.epoch != *expected_epoch {
                return Err(CoreError::Conflict {
                    reason: "GenerationChanged: maintenance authority is stale".into(),
                });
            }
            // (3) Advance the binding epoch inside the same transaction so a
            // racing publication CAS fails against the mutated state.
            binding.epoch += 1;
            let updated = serde_json::to_string(&binding).map_err(|_| storage_error())?;
            tx.execute(
                "UPDATE code_repo_bindings SET record_json=?1, epoch=?2 WHERE tenant=?3 AND scope_key=?4 AND repo_id=?5",
                params![updated, binding.epoch as i64, scope.tenant, key, repo_id.0],
            )
            .map_err(|_| storage_error())?;
        }
        // (2) Preserve durable positive ownership while removing eligibility.
        // Deleting this row would make a surviving code-owned record look
        // ordinary to an unbound/degraded read. Retire only an exact target
        // association from this grant, never another repository's target.
        for target in targets {
            let lineage: Option<(String, String)> = tx
                .query_row(
                    "SELECT repo_id, generation_id FROM code_stage_lineage WHERE scope_key=?1 AND record_id=?2",
                    params![key, target],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()
                .map_err(|_| storage_error())?;
            let Some((repo_id, _generation_id)) = lineage else {
                return Err(CoreError::Conflict {
                    reason: "GenerationChanged: maintenance target has no durable lineage".into(),
                });
            };
            if !expected_binding_epochs
                .iter()
                .any(|(repo, _)| repo.0 == repo_id)
            {
                return Err(CoreError::Conflict {
                    reason: "GenerationChanged: maintenance target belongs to another repository"
                        .into(),
                });
            }
            // Retirement is an additive typed fact. Historical lineage stays
            // immutable, so indexed columns and record_json always agree.
            tx.execute(
                "INSERT OR IGNORE INTO code_stage_lineage_retirements(scope_key,record_id) VALUES(?1,?2)",
                params![key, target],
            )
            .map_err(|_| storage_error())?;
        }
        // (4) Scope-epoch advance: pinned read views revalidate.
        tx.execute(
            "INSERT INTO code_scope_epochs(scope_key,epoch) VALUES(?1,1) ON CONFLICT(scope_key) DO UPDATE SET epoch=epoch+1",
            [&key],
        )
        .map_err(|_| storage_error())?;
        tx.commit().map_err(|_| storage_error())?;
        Ok(())
    }

    async fn put_source_descriptor(
        &self,
        scope: &Scope,
        source: IndexedSource,
    ) -> CoreResult<IndexedSource> {
        source.validate().map_err(error)?;
        let store = self.lock()?;
        let key = json(scope)?;
        let value = json(&source)?;
        store.connection.execute_batch("CREATE TABLE IF NOT EXISTS code_stage_sources(scope_key TEXT NOT NULL,repo_id TEXT NOT NULL,generation_id TEXT NOT NULL,path TEXT NOT NULL,record_json TEXT NOT NULL,PRIMARY KEY(scope_key,repo_id,generation_id,path))").map_err(|_| storage_error())?;
        let saved: Option<String> = store.connection.query_row("SELECT record_json FROM code_stage_sources WHERE scope_key=?1 AND repo_id=?2 AND generation_id=?3 AND path=?4",params![key,source.repo_id.0,source.generation_id,source.path],|r|r.get(0)).optional().map_err(|_| storage_error())?;
        if saved.as_ref().is_some_and(|saved| saved != &value) {
            return Err(error(CodeBindingError::InvalidSource));
        }
        store.connection.execute("INSERT OR IGNORE INTO code_stage_sources(scope_key,repo_id,generation_id,path,record_json) VALUES(?1,?2,?3,?4,?5)",params![key,source.repo_id.0,source.generation_id,source.path,value]).map_err(|_| storage_error())?;
        Ok(source)
    }
    async fn record_receipt(&self, receipt: StageReceipt) -> CoreResult<StageReceipt> {
        let mut store = self.lock()?;
        let generation: Option<String> = store
            .connection
            .query_row(
                "SELECT generation_id FROM code_attempts WHERE attempt_id=?1",
                [&receipt.attempt_id],
                |r| r.get(0),
            )
            .optional()
            .map_err(|_| storage_error())?;
        if generation.as_deref() != Some(&receipt.generation_id) {
            return Err(error(CodeBindingError::InvalidAttempt));
        }
        store
            .record_receipt(&receipt.attempt_id, &receipt.lane, &receipt.digest)
            .map_err(error)?;
        Ok(receipt)
    }
    async fn mark_ready(
        &self,
        scope: &Scope,
        repo_id: &RepoId,
        attempt_id: &str,
        fencing_token: u64,
    ) -> CoreResult<CodeAttempt> {
        let mut store = self.lock()?;
        let (_, attempt) = store
            .attempt(scope, &repo_id.0, attempt_id)
            .map_err(error)?
            .ok_or_else(|| error(CodeBindingError::StaleGeneration))?;
        if attempt.fencing_token != fencing_token {
            return Err(error(CodeBindingError::StaleGeneration));
        }
        let key = json(scope)?;
        let tx = store
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(|_| storage_error())?;
        let latest: u64 = tx.query_row("SELECT MAX(fence) FROM code_attempts WHERE tenant=?1 AND scope_key=?2 AND repo_id=?3",params![scope.tenant,key,repo_id.0],|r|r.get(0)).map_err(|_| storage_error())?;
        if latest != fencing_token {
            return Err(error(CodeBindingError::StaleGeneration));
        }
        let mut expected: Vec<String> =
            serde_json::from_str(&json(&attempt.receipt_kinds)?).map_err(|_| storage_error())?;
        expected.sort();
        let mut actual = tx
            .prepare("SELECT lane FROM code_attempt_receipts WHERE attempt_id=?1")
            .map_err(|_| storage_error())?
            .query_map([attempt_id], |r| r.get::<_, String>(0))
            .map_err(|_| storage_error())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| storage_error())?;
        actual.sort();
        if expected.is_empty() || expected != actual {
            return Err(error(CodeBindingError::StaleGeneration));
        }
        let changed = tx.execute("UPDATE code_attempts SET state='Ready' WHERE attempt_id=?1 AND fence=?2 AND state IN ('Created','Acquiring','Staging','Ready')",params![attempt_id,fencing_token]).map_err(|_| storage_error())?;
        if changed != 1 {
            return Err(error(CodeBindingError::StaleGeneration));
        }
        tx.commit().map_err(|_| storage_error())?;
        let mut ready = attempt;
        ready.state = CodeAttemptState::Ready;
        Ok(ready)
    }
    async fn publish(
        &self,
        scope: &Scope,
        repo_id: &RepoId,
        attempt_id: &str,
        expected_epoch: u64,
    ) -> CoreResult<CodeBinding> {
        let mut store = self.lock()?;
        let (_, attempt) = store
            .attempt(scope, &repo_id.0, attempt_id)
            .map_err(error)?
            .ok_or_else(|| error(CodeBindingError::StaleGeneration))?;
        if expected_epoch != attempt.expected_epoch {
            return Err(error(CodeBindingError::StaleGeneration));
        }
        store.publish(scope, &repo_id.0, attempt_id).map_err(error)
    }
    async fn eligibility(&self, scope: &Scope) -> CoreResult<CodeEligibility> {
        let mut store = self.lock()?;
        let tx = store
            .connection
            .transaction()
            .map_err(|_| storage_error())?;
        let result = read_eligibility(&tx, scope)?;
        tx.commit().map_err(|_| storage_error())?;
        Ok(result)
    }

    async fn get_generation(
        &self,
        scope: &Scope,
        repo_id: &RepoId,
        generation_id: &str,
    ) -> CoreResult<Option<CodeGeneration>> {
        // Corrective r2 (finding 2): a generation id alone is not authority.
        // The record is returned only when THIS scope + repository durably
        // owns an attempt for that generation (code_attempts carries the
        // tenant/scope/repo/generation ownership association); a foreign
        // scope's or repository's generation id resolves to None here.
        let store = self.lock()?;
        let record: Option<String> = store
            .connection
            .query_row(
                "SELECT g.record_json FROM code_generations g \
                 WHERE g.generation_id = ?1 AND EXISTS ( \
                   SELECT 1 FROM code_attempts a \
                   WHERE a.generation_id = ?1 AND a.tenant = ?2 \
                     AND a.scope_key = ?3 AND a.repo_id = ?4 )",
                rusqlite::params![generation_id, scope.tenant, json(scope)?, repo_id.0],
                |row| row.get(0),
            )
            .optional()
            .map_err(|_| storage_error())?;
        record
            .map(|json| serde_json::from_str(&json).map_err(|_| storage_error()))
            .transpose()
    }

    async fn target_lineage(
        &self,
        scope: &Scope,
    ) -> CoreResult<Vec<engram_knowledge::TargetLineage>> {
        let store = self.lock()?;
        store
            .connection
            .execute_batch("CREATE TABLE IF NOT EXISTS code_stage_lineage(scope_key TEXT NOT NULL,record_id TEXT NOT NULL,repo_id TEXT NOT NULL,generation_id TEXT NOT NULL,record_json TEXT NOT NULL,PRIMARY KEY(scope_key,record_id)); CREATE TABLE IF NOT EXISTS code_stage_lineage_retirements(scope_key TEXT NOT NULL,record_id TEXT NOT NULL,PRIMARY KEY(scope_key,record_id))")
            .map_err(|_| storage_error())?;
        let mut statement = store
            .connection
            .prepare(
                "SELECT lineage.record_id, lineage.repo_id, lineage.generation_id, retirement.record_id IS NOT NULL FROM code_stage_lineage lineage \
                 LEFT JOIN code_stage_lineage_retirements retirement ON retirement.scope_key = lineage.scope_key AND retirement.record_id = lineage.record_id \
                 WHERE lineage.scope_key = ?1 ORDER BY lineage.record_id",
            )
            .map_err(|_| storage_error())?;
        let rows = statement
            .query_map(rusqlite::params![json(scope)?], |row| {
                Ok(engram_knowledge::TargetLineage {
                    record_id: row.get(0)?,
                    repo_id: row.get(1)?,
                    generation_id: row.get(2)?,
                    retired: row.get(3)?,
                })
            })
            .map_err(|_| storage_error())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| storage_error())?;
        Ok(rows)
    }

    async fn get_source_descriptor(
        &self,
        scope: &Scope,
        repo_id: &RepoId,
        generation_id: &str,
        path: &str,
    ) -> CoreResult<Option<IndexedSource>> {
        let store = self.lock()?;
        store
            .connection
            .execute_batch("CREATE TABLE IF NOT EXISTS code_stage_sources(scope_key TEXT NOT NULL,repo_id TEXT NOT NULL,generation_id TEXT NOT NULL,path TEXT NOT NULL,record_json TEXT NOT NULL,PRIMARY KEY(scope_key,repo_id,generation_id,path))")
            .map_err(|_| storage_error())?;
        let record: Option<String> = store
            .connection
            .query_row(
                "SELECT record_json FROM code_stage_sources \
                 WHERE scope_key = ?1 AND repo_id = ?2 AND generation_id = ?3 AND path = ?4",
                rusqlite::params![json(scope)?, repo_id.0, generation_id, path],
                |row| row.get(0),
            )
            .optional()
            .map_err(|_| storage_error())?;
        record
            .map(|json| serde_json::from_str(&json).map_err(|_| storage_error()))
            .transpose()
    }
    async fn preview_reset(&self, _: &Scope, _: &RepoId) -> CoreResult<CodeResetPreview> {
        Err(CoreError::CapabilityUnsupported {
            capability: "code-reset".into(),
            reason: "permission-bound exact reset preview is not yet wired".into(),
        })
    }
    async fn journal_reset(&self, _: CodeResetJournal) -> CoreResult<CodeResetJournal> {
        Err(CoreError::CapabilityUnsupported {
            capability: "code-reset".into(),
            reason: "permission-bound reset journal is not yet wired".into(),
        })
    }
}

pub(super) fn read_eligibility(
    connection: &rusqlite::Connection,
    scope: &Scope,
) -> CoreResult<CodeEligibility> {
    let key = json(scope)?;
    let epoch = connection
        .query_row(
            "SELECT epoch FROM code_scope_epochs WHERE scope_key=?1",
            [&key],
            |r| r.get::<_, u64>(0),
        )
        .optional()
        .map_err(|_| storage_error())?
        .unwrap_or(0);
    let mut statement = connection.prepare("SELECT repo_id,active_generation FROM code_repo_bindings WHERE tenant=?1 AND scope_key=?2 AND active_generation IS NOT NULL ORDER BY repo_id").map_err(|_|storage_error())?;
    let rows = statement
        .query_map(params![scope.tenant, key], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })
        .map_err(|_| storage_error())?;
    let mut result = CodeEligibility {
        scope: scope.clone(),
        epoch,
        generations: Vec::new(),
        repository_ids: Vec::new(),
    };
    for row in rows {
        let (repo, generation) = row.map_err(|_| storage_error())?;
        result.repository_ids.push(repo);
        result.generations.push(generation);
    }
    Ok(result)
}

/// Shared scoped authorization for explicit code target ids, wildcard listing
/// and maintenance mutations (task_016). The cell owns the durable truth:
/// a target id is authorized exactly when its lineage record belongs to a
/// PUBLISHED generation of a binding in this scope. Foreign scopes,
/// staged-only generations and unknown ids all deny — never a silent drop,
/// never a widened wildcard.
#[async_trait]
impl engram_knowledge::CodeEligibilityGate for SqlCodeBindingRepository {
    async fn authorize_query(
        &self,
        scope: &Scope,
        requested_target_ids: &[String],
        limit: usize,
    ) -> CoreResult<engram_knowledge::ConstrainedCodeQuery> {
        if limit == 0 {
            return Err(CoreError::InvalidRequest {
                reason: "authorized code query requires a nonzero budget".into(),
            });
        }
        let mut store = self.lock()?;
        let eligibility = {
            let tx = store
                .connection
                .transaction()
                .map_err(|_| storage_error())?;
            let eligibility = read_eligibility(&tx, scope)?;
            tx.commit().map_err(|_| storage_error())?;
            eligibility
        };
        if eligibility.generations.is_empty() {
            // No published binding in scope: explicit ids are foreign, and a
            // wildcard has nothing eligible to widen to.
            if requested_target_ids.is_empty() {
                return Ok(engram_knowledge::ConstrainedCodeQuery {
                    eligibility,
                    target_ids: Vec::new(),
                    limit,
                });
            }
            return Err(foreign_target(requested_target_ids));
        }
        let target_ids = if requested_target_ids.is_empty() {
            // Wildcard: every lineage target of a published generation in
            // this scope, bounded by the requested budget — never staged or
            // foreign repositories.
            let tx = store
                .connection
                .transaction()
                .map_err(|_| storage_error())?;
            let rows = {
                let mut statement = tx
                    .prepare(
                        "SELECT l.record_id FROM code_stage_lineage l JOIN code_repo_bindings b ON b.repo_id = l.repo_id AND b.active_generation = l.generation_id WHERE l.scope_key = ?1 AND b.tenant = ?2 AND b.scope_key = ?1 ORDER BY l.record_id LIMIT ?3",
                    )
                    .map_err(|_| storage_error())?;
                statement
                    .query_map(
                        rusqlite::params![json(scope)?, scope.tenant, limit as i64],
                        |row| row.get::<_, String>(0),
                    )
                    .map_err(|_| storage_error())?
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|_| storage_error())?
            };
            tx.commit().map_err(|_| storage_error())?;
            rows
        } else {
            // Explicit ids: EVERY requested id must resolve to lineage of a
            // published generation in this scope; one foreign id denies the
            // whole request (fail closed — no partial authorization).
            let tx = store
                .connection
                .transaction()
                .map_err(|_| storage_error())?;
            let mut authorized = Vec::with_capacity(requested_target_ids.len());
            {
                let mut statement = tx
                    .prepare(
                        "SELECT l.record_id FROM code_stage_lineage l JOIN code_repo_bindings b ON b.repo_id = l.repo_id AND b.active_generation = l.generation_id WHERE l.record_id = ?1 AND b.tenant = ?2 AND b.scope_key = ?3 AND l.scope_key = ?3",
                    )
                    .map_err(|_| storage_error())?;
                for id in requested_target_ids {
                    let found = statement
                        .query_row(rusqlite::params![id, scope.tenant, json(scope)?], |row| {
                            row.get::<_, String>(0)
                        })
                        .optional()
                        .map_err(|_| storage_error())?;
                    match found {
                        Some(record_id) => authorized.push(record_id),
                        None => {
                            return Err(foreign_target(&[id.clone()]));
                        }
                    }
                }
            }
            tx.commit().map_err(|_| storage_error())?;
            authorized
        };
        Ok(engram_knowledge::ConstrainedCodeQuery {
            eligibility,
            target_ids,
            limit,
        })
    }

    async fn authorize_maintenance(
        &self,
        scope: &Scope,
        requested_target_ids: &[String],
    ) -> CoreResult<Vec<String>> {
        if requested_target_ids.is_empty() {
            return Err(CoreError::InvalidRequest {
                reason: "maintenance authorization requires explicit target ids".into(),
            });
        }
        let query = self
            .authorize_query(scope, requested_target_ids, requested_target_ids.len())
            .await?;
        Ok(query.target_ids)
    }
}

/// Typed denial for a foreign/unpublished/unknown code target id.
fn foreign_target(ids: &[String]) -> CoreError {
    CoreError::InvalidRequest {
        reason: format!(
            "BindingRequired: code target ids are foreign or unpublished in this scope: {}",
            ids.join(", ")
        ),
    }
}
