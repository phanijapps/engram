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

/// Builds one [`TargetLineage`] from a lineage row, decoding the persisted
/// `CodeLineage` JSON for the durable canonical `source_path`/`span`
/// (final r5 finding 2). A row whose JSON cannot be decoded keeps the
/// fail-closed empty path / absent span — such records can never path- or
/// span-match an exact participant read.
fn target_lineage_of_row(
    record_id: String,
    repo_id: String,
    generation_id: String,
    retired: bool,
    lineage_json: serde_json::Value,
) -> TargetLineage {
    let lineage: Option<CodeLineage> = serde_json::from_value(lineage_json).ok();
    TargetLineage {
        record_id,
        repo_id,
        generation_id,
        retired,
        source_path: lineage
            .as_ref()
            .map(|value| value.source_path.clone())
            .unwrap_or_default(),
        span: lineage.as_ref().and_then(|value| value.span),
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
        // r5 C4 (verification finding 2): same typed mapping as the sqlite
        // port — the shared variant is engine-neutral; the Pg engine
        // observes contention in its own shape (compile-verified here; Pg
        // runtime remains withheld authority).
        CodeBindingError::PublicationContention => CoreError::BackendTransient {
            backend: "engram-code-binding".into(),
            message: "publication contention at the durable publish boundary (the write \
                     fence is held by another durable writer — retryable)"
                .into(),
        },
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

    /// Corrective r4 (canonical 3a) + review r5 (findings 3/10): the owned
    /// maintenance mutation, SEMANTICALLY IDENTICAL to the SQLite cell. One
    /// transaction takes the SAME advisory lock publications take, locks the
    /// involved binding rows FOR UPDATE, validates EVERY binding epoch and
    /// EVERY typed target association (repository membership AND exact
    /// expected generation) BEFORE any change, then CASes the epochs,
    /// retires the durable lineage of every target that still needs
    /// retirement and advances each binding epoch plus the scope epoch. A
    /// repeated retirement whose associations match exactly is an idempotent
    /// no-op SUCCESS that advances no epochs; a wrong- or old-generation
    /// target from an allowed repository rejects the whole set. The first
    /// side effect (the epoch-CAS update) linearizes atomically with the
    /// authority and association checks.
    async fn retire_lineage_targets(
        &self,
        scope: &Scope,
        expected_binding_epochs: &[(RepoId, u64)],
        targets: &[engram_knowledge::RetirementTarget],
    ) -> CoreResult<()> {
        if expected_binding_epochs.is_empty() || targets.is_empty() {
            return Err(CoreError::InvalidRequest {
                reason: "fenced maintenance requires explicit repos and targets".into(),
            });
        }
        let key = serde_json::to_string(scope).map_err(|_| storage_error())?;
        let tenant = scope.tenant.clone();
        let repos: Vec<String> = expected_binding_epochs
            .iter()
            .map(|(repo, _)| repo.0.clone())
            .collect();
        let expected: Vec<i64> = expected_binding_epochs
            .iter()
            .map(|(_, epoch)| i64::try_from(*epoch).map_err(|_| storage_error()))
            .collect::<CoreResult<Vec<i64>>>()?;
        let targets = targets.to_vec();
        self.blocking(move |store| {
            crate::connection::runtime().block_on(async {
                let tx = store
                    .connection
                    .client
                    .transaction()
                    .await
                    .map_err(|_| storage_error())?;
                tx.execute(
                    "SELECT pg_advisory_xact_lock(hashtextextended($1, 0))",
                    &[&format!("{tenant}:{key}")],
                )
                .await
                .map_err(|_| storage_error())?;
                // (1) Whole-set validation BEFORE any change (findings 3/10):
                // every binding epoch under FOR UPDATE row locks, and every
                // typed association — repository membership AND exact
                // expected generation — with its needs-retirement flag.
                for (repo, expected_epoch) in repos.iter().zip(expected.iter()) {
                    let row = tx
                        .query_opt(
                            "SELECT epoch FROM code_repo_bindings WHERE tenant=$1 AND scope_key=$2 AND repo_id=$3 FOR UPDATE",
                            &[&tenant, &key, repo],
                        )
                        .await
                        .map_err(|_| storage_error())?;
                    let Some(row) = row else {
                        return Err(error(CodeBindingError::BindingRequired));
                    };
                    let current: i64 = row.get(0);
                    if current != *expected_epoch {
                        return Err(error(CodeBindingError::StaleGeneration));
                    }
                }
                tx.batch_execute("CREATE TABLE IF NOT EXISTS code_lineage_retirements(tenant TEXT NOT NULL,scope_key TEXT NOT NULL,record_id TEXT NOT NULL,PRIMARY KEY(tenant,scope_key,record_id))").await.map_err(|_| storage_error())?;
                let mut needs = Vec::with_capacity(targets.len());
                for target in &targets {
                    let row = tx
                        .query_opt(
                            "SELECT lineage.repo_id, lineage.generation_id, retirement.record_id IS NOT NULL FROM code_lineage lineage LEFT JOIN code_lineage_retirements retirement ON retirement.tenant = lineage.tenant AND retirement.scope_key = lineage.scope_key AND retirement.record_id = lineage.record_id WHERE lineage.tenant=$1 AND lineage.scope_key=$2 AND lineage.record_id=$3 FOR UPDATE OF lineage",
                            &[&tenant, &key, &target.record_id],
                        )
                        .await
                        .map_err(|_| storage_error())?;
                    let Some(row) = row else {
                        return Err(error(CodeBindingError::StaleGeneration));
                    };
                    let repo_id: String = row.get(0);
                    let generation_id: String = row.get(1);
                    // Review r5 (verification finding 1): the projection is
                    // a PostgreSQL BOOL (`retirement.record_id IS NOT NULL`);
                    // decode it as `bool` exactly like the target_lineage /
                    // record_lineage readers — an i64 decode of a BOOL panics
                    // on every existing-target retirement.
                    let retired: bool = row.get(2);
                    if !repos.iter().any(|repo| repo == &repo_id) {
                        return Err(error(CodeBindingError::StaleGeneration));
                    }
                    // The typed association names the repository it
                    // validated: a declared repository that differs from the
                    // durable lineage repository is a mismatched association
                    // (finding 3).
                    if repo_id != target.repo_id {
                        return Err(error(CodeBindingError::StaleGeneration));
                    }
                    // The typed expected-generation association (finding 3):
                    // a wrong- or old-generation target from an ALLOWED
                    // repository rejects — repository membership alone is
                    // not authority.
                    if generation_id != target.generation_id {
                        return Err(error(CodeBindingError::StaleGeneration));
                    }
                    needs.push(!retired);
                }
                // Idempotent repeated retirement (parent-adopted contract):
                // nothing needs retirement — success WITHOUT advancing
                // binding or scope epochs, and WITHOUT any write.
                if !needs.iter().any(|needed| *needed) {
                    tx.commit().await.map_err(|_| storage_error())?;
                    return Ok(());
                }
                // (2) FIRST SIDE EFFECT: advance every involved binding's
                // epoch inside the same transaction so a racing publication
                // CAS fails against the mutated state.
                for (repo, expected_epoch) in repos.iter().zip(expected.iter()) {
                    let advanced = expected_epoch + 1;
                    let updated = tx
                        .execute(
                            "UPDATE code_repo_bindings SET epoch=$1, record_json=jsonb_set(record_json,'{epoch}',to_jsonb($1::bigint)) WHERE tenant=$2 AND scope_key=$3 AND repo_id=$4 AND epoch=$5",
                            &[&advanced, &tenant, &key, repo, &expected_epoch],
                        )
                        .await
                        .map_err(|_| storage_error())?;
                    if updated != 1 {
                        return Err(error(CodeBindingError::StaleGeneration));
                    }
                }
                // (3) Preserve code ownership after retirement. Deletion
                // would turn a surviving code-owned record into an
                // apparently ordinary record on an unbound/degraded read;
                // the additive table preserves the historical generation.
                for (target, needed) in targets.iter().zip(needs.iter()) {
                    if !needed {
                        continue;
                    }
                    tx.execute(
                        "INSERT INTO code_lineage_retirements(tenant,scope_key,record_id) VALUES($1,$2,$3) ON CONFLICT(tenant,scope_key,record_id) DO NOTHING",
                        &[&tenant, &key, &target.record_id],
                    )
                    .await
                    .map_err(|_| storage_error())?;
                }
                tx.execute(
                    "INSERT INTO code_epochs(tenant,scope_key,epoch) VALUES($1,$2,1) ON CONFLICT(tenant,scope_key) DO UPDATE SET epoch=code_epochs.epoch+1",
                    &[&tenant, &key],
                )
                .await
                .map_err(|_| storage_error())?;
                tx.commit().await.map_err(|_| storage_error())?;
                Ok(())
            })
        })
        .await
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

    async fn put_ref_observation(
        &self,
        scope: &Scope,
        repo_id: &RepoId,
        observation: CodeRefObservation,
    ) -> CoreResult<CodeRefObservation> {
        // The observation must match the BOUND branch: an observation of a
        // different ref is a typed denial, never a silent record for a ref
        // this binding does not authorize (engine-neutral twin of the
        // SQLite cell).
        let binding = self
            .get_binding(scope, repo_id)
            .await?
            .ok_or_else(|| error(CodeBindingError::BindingRequired))?;
        if binding.branch_ref != observation.branch_ref {
            return Err(CoreError::InvalidRequest {
                reason: "BindingConflict".into(),
            });
        }
        let scope = scope.clone();
        let repo = repo_id.0.clone();
        self.blocking(move |store| {
            store
                .put_ref_observation(&scope, &repo, &observation)
                .map_err(error)
        })
        .await
    }

    async fn get_ref_observation(
        &self,
        scope: &Scope,
        repo_id: &RepoId,
    ) -> CoreResult<Option<CodeRefObservation>> {
        let scope = scope.clone();
        let repo = repo_id.0.clone();
        self.blocking(move |store| store.get_ref_observation(&scope, &repo).map_err(error))
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
                store.connection.client.batch_execute("CREATE TABLE IF NOT EXISTS code_lineage_retirements(tenant TEXT NOT NULL,scope_key TEXT NOT NULL,record_id TEXT NOT NULL,PRIMARY KEY(tenant,scope_key,record_id))").await.map_err(|_| storage_error())?;
                let rows = store
                    .connection
                    .client
                    .query(
                        "SELECT lineage.record_id, lineage.repo_id, lineage.generation_id, retirement.record_id IS NOT NULL, lineage.lineage_json FROM code_lineage lineage LEFT JOIN code_lineage_retirements retirement ON retirement.tenant = lineage.tenant AND retirement.scope_key = lineage.scope_key AND retirement.record_id = lineage.record_id WHERE lineage.tenant = $1 AND lineage.scope_key = $2 ORDER BY lineage.record_id",
                        &[&scope.tenant, &key],
                    )
                    .await
                    .map_err(|_| storage_error())?;
                Ok(rows
                    .iter()
                    .map(|row| {
                        target_lineage_of_row(
                            row.get(0),
                            row.get(1),
                            row.get(2),
                            row.get(3),
                            row.get(4),
                        )
                    })
                    .collect())
            })
        })
        .await
    }

    /// Bounded keyed ownership read (work package A3): one (scope, record)
    /// row instead of the full-scope lineage materialization per record.
    async fn record_lineage(
        &self,
        scope: &Scope,
        record_id: &str,
    ) -> CoreResult<Option<TargetLineage>> {
        let scope = scope.clone();
        let record_id = record_id.to_owned();
        self.blocking(move |store| {
            let key = scope_key(&scope).map_err(error)?;
            crate::connection::runtime().block_on(async {
                store.connection.client.batch_execute("CREATE TABLE IF NOT EXISTS code_lineage_retirements(tenant TEXT NOT NULL,scope_key TEXT NOT NULL,record_id TEXT NOT NULL,PRIMARY KEY(tenant,scope_key,record_id))").await.map_err(|_| storage_error())?;
                let row = store
                    .connection
                    .client
                    .query_opt(
                        "SELECT lineage.record_id, lineage.repo_id, lineage.generation_id, retirement.record_id IS NOT NULL, lineage.lineage_json FROM code_lineage lineage LEFT JOIN code_lineage_retirements retirement ON retirement.tenant = lineage.tenant AND retirement.scope_key = lineage.scope_key AND retirement.record_id = lineage.record_id WHERE lineage.tenant = $1 AND lineage.scope_key = $2 AND lineage.record_id = $3",
                        &[&scope.tenant, &key, &record_id],
                    )
                    .await
                    .map_err(|_| storage_error())?;
                Ok(row.map(|row| {
                    target_lineage_of_row(
                        row.get(0),
                        row.get(1),
                        row.get(2),
                        row.get(3),
                        row.get(4),
                    )
                }))
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
                            // Review r5 finding 2: retirement facts are
                            // excluded in SQL BEFORE the limit — identical
                            // semantics to the SQLite gate.
                            "SELECT l.record_id FROM code_lineage l JOIN code_repo_bindings b ON b.tenant = l.tenant AND b.scope_key = l.scope_key AND b.repo_id = l.repo_id AND b.active_generation = l.generation_id WHERE l.tenant = $1 AND l.scope_key = $2 AND NOT EXISTS (SELECT 1 FROM code_lineage_retirements r WHERE r.tenant = l.tenant AND r.scope_key = l.scope_key AND r.record_id = l.record_id) ORDER BY l.record_id LIMIT $3",
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
                                // Review r5 finding 2: explicit ids deny when
                                // the target is retired (identical semantics
                                // to the SQLite gate).
                                "SELECT l.record_id FROM code_lineage l JOIN code_repo_bindings b ON b.tenant = l.tenant AND b.scope_key = l.scope_key AND b.repo_id = l.repo_id AND b.active_generation = l.generation_id WHERE l.record_id = $1 AND l.tenant = $2 AND l.scope_key = $3 AND NOT EXISTS (SELECT 1 FROM code_lineage_retirements r WHERE r.tenant = l.tenant AND r.scope_key = l.scope_key AND r.record_id = l.record_id)",
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
