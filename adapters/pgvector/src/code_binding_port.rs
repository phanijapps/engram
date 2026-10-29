//! Neutral repository port over the PostgreSQL code-binding cell.
use crate::{
    code_binding::{scope_key, PgCodeBindingStore},
    connection::PgConnection,
};
use async_trait::async_trait;
use engram_domain::*;
use engram_knowledge::*;
use std::sync::{Arc, Mutex};

pub struct PgCodeBindingRepository {
    store: Arc<Mutex<PgCodeBindingStore>>,
}

pub(super) fn storage_error() -> CoreError {
    CoreError::Adapter {
        adapter: "engram-code-binding".into(),
        message: "durable code authority is unavailable".into(),
    }
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

impl PgCodeBindingRepository {
    pub fn new(connection: PgConnection) -> CoreResult<Self> {
        let store = PgCodeBindingStore::new(connection);
        store.initialize().map_err(error)?;
        crate::connection::runtime().block_on(async {
            store.connection.client.batch_execute("CREATE TABLE IF NOT EXISTS code_read_leases(lease_id TEXT PRIMARY KEY,scope_key TEXT NOT NULL,fence BIGINT NOT NULL,expires_ms BIGINT NOT NULL,record_json JSONB NOT NULL); CREATE TABLE IF NOT EXISTS code_read_lease_tombstones(lease_id TEXT PRIMARY KEY);").await.map_err(|_| storage_error())
        })?;
        Ok(Self {
            store: Arc::new(Mutex::new(store)),
        })
    }

    /// Dispatches owned store work without blocking the caller's async worker.
    pub(super) async fn blocking<T: Send + 'static>(
        &self,
        operation: impl FnOnce(&mut PgCodeBindingStore) -> CoreResult<T> + Send + 'static,
    ) -> CoreResult<T> {
        let store = Arc::clone(&self.store);
        crate::connection::runtime()
            .spawn_blocking(move || {
                let mut store = store.lock().map_err(|_| storage_error())?;
                operation(&mut store)
            })
            .await
            .map_err(|_| storage_error())?
    }
}

#[async_trait]
impl CodeBindingRepository for PgCodeBindingRepository {
    async fn register(
        &self,
        registration: RepositoryRegistration,
    ) -> CoreResult<RepositoryRegistration> {
        if registration.repo_id.0.is_empty() || registration.locator_id.is_empty() {
            return Err(error(CodeBindingError::InvalidRepoId));
        }
        self.blocking(move |store| store.register(&registration).map_err(error))
            .await
    }

    async fn bind(&self, binding: CodeBinding) -> CoreResult<CodeBinding> {
        self.blocking(move |store| store.bind(&binding).map_err(error))
            .await
    }

    async fn get_binding(
        &self,
        scope: &Scope,
        repo_id: &RepoId,
    ) -> CoreResult<Option<CodeBinding>> {
        let scope = scope.clone();
        let repo_id = repo_id.clone();
        self.blocking(move |store| store.get_binding(&scope, &repo_id).map_err(error))
            .await
    }

    async fn begin_attempt(
        &self,
        scope: &Scope,
        repo_id: &RepoId,
        generation: CodeGeneration,
        attempt: CodeAttempt,
    ) -> CoreResult<CodeAttempt> {
        let scope = scope.clone();
        let repo_id = repo_id.clone();
        self.blocking(move |store| {
            store
                .begin_attempt(&scope, &repo_id, &generation, &attempt)
                .map_err(error)
        })
        .await
    }

    async fn get_attempt(
        &self,
        scope: &Scope,
        repo_id: &RepoId,
        attempt_id: &str,
    ) -> CoreResult<Option<(CodeGeneration, CodeAttempt)>> {
        let key = serde_json::to_string(scope).map_err(|_| storage_error())?;
        let tenant = scope.tenant.clone();
        let repo_id = repo_id.clone();
        let attempt_id = attempt_id.to_owned();
        self.blocking(move |store| crate::connection::runtime().block_on(async {
            let row = store
                .connection
                .client
                .query_opt(
                    "SELECT g.record_json,a.record_json,a.state FROM code_attempts a JOIN code_generations g ON g.generation_id=a.generation_id WHERE a.attempt_id=$1 AND a.tenant=$2 AND a.scope_key=$3 AND a.repo_id=$4",
                    &[&attempt_id, &tenant, &key, &repo_id.0],
                )
                .await
                .map_err(|_| storage_error())?;
            row.map(|row| {
                let generation = serde_json::from_value(row.get(0)).map_err(|_| storage_error())?;
                let mut attempt: CodeAttempt =
                    serde_json::from_value(row.get(1)).map_err(|_| storage_error())?;
                attempt.state = serde_json::from_value(serde_json::Value::String(row.get(2)))
                    .map_err(|_| storage_error())?;
                Ok((generation, attempt))
            })
            .transpose()
        })).await
    }

    async fn put_generation(&self, generation: CodeGeneration) -> CoreResult<CodeGeneration> {
        self.blocking(move |store| store.put_generation(&generation).map_err(error))
            .await
    }

    async fn put_lineage(
        &self,
        scope: &Scope,
        record_id: &str,
        lineage: CodeLineage,
    ) -> CoreResult<CodeLineage> {
        lineage.validate().map_err(error)?;
        let key = serde_json::to_string(scope).map_err(|_| storage_error())?;
        let tenant = scope.tenant.clone();
        let record_id = record_id.to_owned();
        let value = serde_json::to_value(&lineage).map_err(|_| storage_error())?;
        self.blocking(move |store| crate::connection::runtime().block_on(async {
            let inserted = store.connection.client.execute(
                "INSERT INTO code_lineage(tenant,scope_key,repo_id,generation_id,record_id,lineage_json) VALUES($1,$2,$3,$4,$5,$6) ON CONFLICT DO NOTHING",
                &[&tenant, &key, &lineage.repo_id.0, &lineage.generation_id, &record_id, &value],
            ).await.map_err(|_| storage_error())?;
            if inserted == 0 {
                let row = store.connection.client.query_one(
                    "SELECT lineage_json FROM code_lineage WHERE tenant=$1 AND scope_key=$2 AND repo_id=$3 AND generation_id=$4 AND record_id=$5",
                    &[&tenant, &key, &lineage.repo_id.0, &lineage.generation_id, &record_id],
                ).await.map_err(|_| storage_error())?;
                if row.get::<_, serde_json::Value>(0) != value {
                    return Err(error(CodeBindingError::InvalidSource));
                }
            }
            Ok(lineage)
        })).await
    }

    async fn put_source_descriptor(
        &self,
        scope: &Scope,
        source: IndexedSource,
    ) -> CoreResult<IndexedSource> {
        let scope = scope.clone();
        self.blocking(move |store| store.put_source(&scope, &source).map_err(error))
            .await
    }

    async fn record_receipt(&self, receipt: StageReceipt) -> CoreResult<StageReceipt> {
        self.blocking(move |store| store.record_receipt(&receipt).map_err(error))
            .await
    }

    async fn mark_ready(
        &self,
        scope: &Scope,
        repo_id: &RepoId,
        attempt_id: &str,
        fencing_token: u64,
    ) -> CoreResult<CodeAttempt> {
        let key = serde_json::to_string(scope).map_err(|_| storage_error())?;
        let tenant = scope.tenant.clone();
        let repo_id = repo_id.clone();
        let attempt_id = attempt_id.to_owned();
        let fence =
            i64::try_from(fencing_token).map_err(|_| error(CodeBindingError::StaleGeneration))?;
        self.blocking(move |store| crate::connection::runtime().block_on(async {
            let tx = store.connection.client.transaction().await.map_err(|_| storage_error())?;
            tx.execute("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))", &[&format!("{}:{key}", tenant)]).await.map_err(|_| storage_error())?;
            let row = tx.query_opt(
                "SELECT record_json,required_receipts FROM code_attempts WHERE attempt_id=$1 AND tenant=$2 AND scope_key=$3 AND repo_id=$4 AND fence=$5 FOR UPDATE",
                &[&attempt_id, &tenant, &key, &repo_id.0, &fence],
            ).await.map_err(|_| storage_error())?.ok_or_else(|| error(CodeBindingError::StaleGeneration))?;
            let mut attempt: CodeAttempt = serde_json::from_value(row.get(0)).map_err(|_| storage_error())?;
            let latest: Option<i64> = tx.query_one("SELECT MAX(fence) FROM code_attempts WHERE tenant=$1 AND scope_key=$2 AND repo_id=$3", &[&tenant, &key, &repo_id.0]).await.map_err(|_| storage_error())?.get(0);
            if latest != Some(fence) {
                return Err(error(CodeBindingError::StaleGeneration));
            }
            let mut expected: Vec<String> = serde_json::from_value(row.get(1)).map_err(|_| storage_error())?;
            let rows = tx.query("SELECT lane FROM code_attempt_receipts WHERE attempt_id=$1", &[&attempt_id]).await.map_err(|_| storage_error())?;
            let mut actual: Vec<String> = rows.into_iter().map(|row| row.get(0)).collect();
            expected.sort();
            actual.sort();
            if expected.is_empty() || expected != actual {
                return Err(error(CodeBindingError::StaleGeneration));
            }
            let changed = tx.execute(
                "UPDATE code_attempts SET state='ready' WHERE attempt_id=$1 AND fence=$2 AND state IN ('created','acquiring','staging','ready')",
                &[&attempt_id, &fence],
            ).await.map_err(|_| storage_error())?;
            if changed != 1 {
                return Err(error(CodeBindingError::StaleGeneration));
            }
            tx.commit().await.map_err(|_| storage_error())?;
            attempt.state = CodeAttemptState::Ready;
            Ok(attempt)
        })).await
    }

    async fn publish(
        &self,
        scope: &Scope,
        repo_id: &RepoId,
        attempt_id: &str,
        expected_epoch: u64,
    ) -> CoreResult<CodeBinding> {
        let scope = scope.clone();
        let repo_id = repo_id.clone();
        let attempt_id = attempt_id.to_owned();
        self.blocking(move |store| {
            store
                .publish(&scope, &repo_id, &attempt_id, expected_epoch)
                .map_err(error)
        })
        .await
    }

    async fn eligibility(&self, scope: &Scope) -> CoreResult<CodeEligibility> {
        let scope = scope.clone();
        self.blocking(move |store| store.eligibility(&scope).map_err(error))
            .await
    }

    async fn get_generation(
        &self,
        scope: &Scope,
        repo_id: &RepoId,
        generation_id: &str,
    ) -> CoreResult<Option<CodeGeneration>> {
        // Corrective r2 (finding 2): a generation id alone is not authority.
        // The record resolves only when THIS scope + repository durably owns
        // an attempt for that generation (code_attempts carries the
        // tenant/scope/repo/generation ownership association).
        let generation_id = generation_id.to_owned();
        let scope = scope.clone();
        let repo_id = repo_id.0.clone();
        self.blocking(move |store| {
            let key = scope_key(&scope).map_err(error)?;
            crate::connection::runtime().block_on(async {
                let row = store
                    .connection
                    .client
                    .query_opt(
                        "SELECT g.record_json FROM code_generations g WHERE g.generation_id = $1 AND EXISTS (SELECT 1 FROM code_attempts a WHERE a.generation_id = $1 AND a.tenant = $2 AND a.scope_key = $3 AND a.repo_id = $4)",
                        &[&generation_id, &scope.tenant, &key, &repo_id],
                    )
                    .await
                    .map_err(|_| storage_error())?;
                row.map(|row| {
                    serde_json::from_value::<CodeGeneration>(row.get(0)).map_err(|_| storage_error())
                })
                .transpose()
            })
        })
        .await
    }

    async fn target_lineage(&self, scope: &Scope) -> CoreResult<Vec<TargetLineage>> {
        let scope = scope.clone();
        self.blocking(move |store| {
            let key = scope_key(&scope).map_err(error)?;
            crate::connection::runtime().block_on(async {
                let rows = store
                    .connection
                    .client
                    .query(
                        "SELECT record_id, repo_id, generation_id FROM code_lineage WHERE tenant = $1 AND scope_key = $2 ORDER BY record_id",
                        &[&scope.tenant, &key],
                    )
                    .await
                    .map_err(|_| storage_error())?;
                Ok(rows
                    .iter()
                    .map(|row| TargetLineage {
                        record_id: row.get(0),
                        repo_id: row.get(1),
                        generation_id: row.get(2),
                    })
                    .collect())
            })
        })
        .await
    }

    async fn get_source_descriptor(
        &self,
        scope: &Scope,
        repo_id: &RepoId,
        generation_id: &str,
        path: &str,
    ) -> CoreResult<Option<IndexedSource>> {
        let scope = scope.clone();
        let repo_id = repo_id.0.clone();
        let generation_id = generation_id.to_owned();
        let path = path.to_owned();
        self.blocking(move |store| {
            let key = scope_key(&scope).map_err(error)?;
            crate::connection::runtime().block_on(async {
                let row = store
                    .connection
                    .client
                    .query_opt(
                        "SELECT source_json FROM code_sources WHERE tenant = $1 AND scope_key = $2 AND repo_id = $3 AND generation_id = $4 AND path = $5",
                        &[&scope.tenant, &key, &repo_id, &generation_id, &path],
                    )
                    .await
                    .map_err(|_| storage_error())?;
                row.map(|row| {
                    serde_json::from_value::<IndexedSource>(row.get(0)).map_err(|_| storage_error())
                })
                .transpose()
            })
        })
        .await
    }

    async fn preview_reset(&self, scope: &Scope, repo_id: &RepoId) -> CoreResult<CodeResetPreview> {
        let scope = scope.clone();
        let repo_id = repo_id.clone();
        self.blocking(move |store| store.reset_preview(&scope, &repo_id).map_err(error))
            .await
    }

    async fn journal_reset(&self, journal: CodeResetJournal) -> CoreResult<CodeResetJournal> {
        self.blocking(move |store| store.journal_reset(&journal, "pending").map_err(error))
            .await
    }
}

/// Shared scoped authorization for explicit code target ids, wildcard listing
/// and maintenance mutations (task_016) — the PostgreSQL cell's twin of the
/// SQLite gate. A target id is authorized exactly when its lineage row
/// belongs to a PUBLISHED generation of a binding in this scope; foreign
/// scopes, staged generations and unknown ids deny, and a wildcard widens to
/// the published set only.
#[async_trait]
impl CodeEligibilityGate for PgCodeBindingRepository {
    async fn authorize_query(
        &self,
        scope: &Scope,
        requested_target_ids: &[String],
        limit: usize,
    ) -> CoreResult<ConstrainedCodeQuery> {
        if limit == 0 {
            return Err(CoreError::InvalidRequest {
                reason: "authorized code query requires a nonzero budget".into(),
            });
        }
        let scope = scope.clone();
        let requested = requested_target_ids.to_vec();
        self.blocking(move |store| {
            let mut eligibility = store.eligibility(&scope).map_err(error)?;
            if eligibility.generations.is_empty() {
                if requested.is_empty() {
                    eligibility.generations.clear();
                    eligibility.repository_ids.clear();
                    return Ok(ConstrainedCodeQuery {
                        eligibility,
                        target_ids: Vec::new(),
                        limit,
                    });
                }
                return Err(foreign_target(&requested));
            }
            let key = scope_key(&scope).map_err(error)?;
            let target_ids = if requested.is_empty() {
                let rows = crate::connection::runtime().block_on(async {
                    store
                        .connection
                        .client
                        .query(
 "SELECT l.record_id FROM code_lineage l JOIN code_repo_bindings b ON b.tenant = l.tenant AND b.scope_key = l.scope_key AND b.repo_id = l.repo_id AND b.active_generation = l.generation_id WHERE l.tenant = $1 AND l.scope_key = $2 ORDER BY l.record_id LIMIT $3",
                            &[&scope.tenant, &key, &(limit as i64)],
                        )
                        .await
                        .map_err(|_| storage_error())
                })?;
                rows.iter().map(|row| row.get::<_, String>(0)).collect()
            } else {
                let mut authorized = Vec::with_capacity(requested.len());
                for id in &requested {
                    let found = crate::connection::runtime().block_on(async {
                        store
                            .connection
                            .client
                            .query_opt(
 "SELECT l.record_id FROM code_lineage l JOIN code_repo_bindings b ON b.tenant = l.tenant AND b.scope_key = l.scope_key AND b.repo_id = l.repo_id AND b.active_generation = l.generation_id WHERE l.record_id = $1 AND l.tenant = $2 AND l.scope_key = $3",
                                &[id, &scope.tenant, &key],
                            )
                            .await
                            .map_err(|_| storage_error())
                    })?;
                    match found {
                        Some(row) => authorized.push(row.get::<_, String>(0)),
                        None => return Err(foreign_target(&[id.clone()])),
                    }
                }
                authorized
            };
            Ok(ConstrainedCodeQuery {
                eligibility,
                target_ids,
                limit,
            })
        })
        .await
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
