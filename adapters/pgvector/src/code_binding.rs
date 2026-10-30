//! PostgreSQL persistence for project-bound code identity and fenced publication.
use crate::connection::{runtime, PgConnection};
use engram_domain::{
    CodeAttempt, CodeBinding, CodeBindingError, CodeGeneration, CodeLineage, CodeResetJournal,
    CodeResetPreview, IndexedSource, PreservationInventory, RepoId, Scope,
};
use engram_knowledge::{AcquisitionMode, CodeEligibility, RepositoryRegistration, StageReceipt};
use tokio_postgres::IsolationLevel;

pub struct PgCodeBindingStore {
    pub(crate) connection: PgConnection,
}
fn db<T>(r: Result<T, tokio_postgres::Error>) -> Result<T, CodeBindingError> {
    r.map_err(|_| CodeBindingError::PersistenceFailure)
}
pub(super) fn scope_key(scope: &Scope) -> Result<String, CodeBindingError> {
    serde_json::to_string(scope).map_err(|_| CodeBindingError::PersistenceFailure)
}
fn json<T: serde::Serialize>(value: &T) -> Result<serde_json::Value, CodeBindingError> {
    serde_json::to_value(value).map_err(|_| CodeBindingError::PersistenceFailure)
}
impl PgCodeBindingStore {
    pub fn new(connection: PgConnection) -> Self {
        Self { connection }
    }
    pub fn initialize(&self) -> Result<(), CodeBindingError> {
        runtime().block_on(async{db(self.connection.client.batch_execute("CREATE TABLE IF NOT EXISTS code_repository_registrations(repo_id TEXT PRIMARY KEY, locator_id TEXT NOT NULL, acquisition TEXT NOT NULL, record_json JSONB NOT NULL); CREATE TABLE IF NOT EXISTS code_repo_bindings(tenant TEXT NOT NULL,scope_key TEXT NOT NULL,repo_id TEXT NOT NULL,branch_ref TEXT NOT NULL,version BIGINT NOT NULL,active_generation TEXT,epoch BIGINT NOT NULL,record_json JSONB NOT NULL,PRIMARY KEY(tenant,scope_key,repo_id)); CREATE TABLE IF NOT EXISTS code_generations(generation_id TEXT PRIMARY KEY,record_json JSONB NOT NULL); CREATE TABLE IF NOT EXISTS code_attempts(attempt_id TEXT PRIMARY KEY,tenant TEXT NOT NULL,scope_key TEXT NOT NULL,repo_id TEXT NOT NULL,generation_id TEXT NOT NULL,binding_version BIGINT NOT NULL,expected_epoch BIGINT NOT NULL,expected_active_generation TEXT,fence BIGINT NOT NULL,idempotency_key TEXT NOT NULL,state TEXT NOT NULL,required_receipts JSONB NOT NULL,record_json JSONB NOT NULL,UNIQUE(tenant,scope_key,repo_id,idempotency_key)); CREATE TABLE IF NOT EXISTS code_attempt_receipts(attempt_id TEXT NOT NULL,lane TEXT NOT NULL,digest TEXT NOT NULL,PRIMARY KEY(attempt_id,lane)); CREATE TABLE IF NOT EXISTS code_lineage(tenant TEXT NOT NULL,scope_key TEXT NOT NULL,repo_id TEXT NOT NULL,generation_id TEXT NOT NULL,record_id TEXT NOT NULL,lineage_json JSONB NOT NULL,PRIMARY KEY(tenant,scope_key,repo_id,generation_id,record_id)); CREATE TABLE IF NOT EXISTS code_sources(tenant TEXT NOT NULL,scope_key TEXT NOT NULL,repo_id TEXT NOT NULL,generation_id TEXT NOT NULL,path TEXT NOT NULL,source_json JSONB NOT NULL,PRIMARY KEY(tenant,scope_key,repo_id,generation_id,path)); CREATE TABLE IF NOT EXISTS code_reset_journals(preview_hash TEXT PRIMARY KEY,tenant TEXT NOT NULL,scope_key TEXT NOT NULL,repo_id TEXT NOT NULL,journal_json JSONB NOT NULL,checkpoint TEXT NOT NULL); CREATE TABLE IF NOT EXISTS code_reset_journal_progress(reset_id TEXT NOT NULL,record_id TEXT NOT NULL,checkpoint TEXT NOT NULL,PRIMARY KEY(reset_id,record_id)); CREATE TABLE IF NOT EXISTS code_epochs(tenant TEXT NOT NULL,scope_key TEXT NOT NULL,epoch BIGINT NOT NULL,PRIMARY KEY(tenant,scope_key)); CREATE TABLE IF NOT EXISTS code_ref_observations(tenant TEXT NOT NULL,scope_key TEXT NOT NULL,repo_id TEXT NOT NULL,branch_ref TEXT NOT NULL,observed_oid TEXT NOT NULL,observed_at TEXT NOT NULL,policy TEXT NOT NULL,record_json JSONB NOT NULL,PRIMARY KEY(tenant,scope_key,repo_id));").await)})
    }
    pub fn register(
        &self,
        r: &RepositoryRegistration,
    ) -> Result<RepositoryRegistration, CodeBindingError> {
        let value = json(r)?;
        let mode = match r.acquisition {
            AcquisitionMode::LocalOnly => "local_only",
            AcquisitionMode::ConfiguredRemote => "configured_remote",
        };
        runtime().block_on(async{db(self.connection.client.execute("INSERT INTO code_repository_registrations(repo_id,locator_id,acquisition,record_json) VALUES($1,$2,$3,$4) ON CONFLICT(repo_id) DO NOTHING",&[&r.repo_id.0,&r.locator_id,&mode,&value]).await)?;let row=db(self.connection.client.query_one("SELECT record_json FROM code_repository_registrations WHERE repo_id=$1",&[&r.repo_id.0]).await)?;let stored:serde_json::Value=row.get(0);if stored!=value{return Err(CodeBindingError::BindingConflict)}Ok(r.clone())})
    }
    pub fn bind(&mut self, b: &CodeBinding) -> Result<CodeBinding, CodeBindingError> {
        b.validate()?;
        let key = scope_key(&b.scope)?;
        let value = json(b)?;
        runtime().block_on(async{let tx=self.connection.client.transaction().await.map_err(|_|CodeBindingError::PersistenceFailure)?;db(tx.execute("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))",&[&format!("{}:{key}",b.scope.tenant)]).await)?;if let Some(row)=db(tx.query_opt("SELECT branch_ref,record_json FROM code_repo_bindings WHERE tenant=$1 AND scope_key=$2 AND repo_id=$3 FOR UPDATE",&[&b.scope.tenant,&key,&b.repo_id.0]).await)?{let branch:String=row.get(0);if branch!=b.branch_ref{return Err(CodeBindingError::BindingConflict)}let stored:serde_json::Value=row.get(1);let existing=serde_json::from_value(stored).map_err(|_|CodeBindingError::PersistenceFailure)?;tx.commit().await.map_err(|_|CodeBindingError::PersistenceFailure)?;return Ok(existing)}let inserted=db(tx.execute("INSERT INTO code_repo_bindings(tenant,scope_key,repo_id,branch_ref,version,active_generation,epoch,record_json) VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT DO NOTHING",&[&b.scope.tenant,&key,&b.repo_id.0,&b.branch_ref,&i64::try_from(b.version).map_err(|_|CodeBindingError::InvalidBindingVersion)?,&b.active_generation,&i64::try_from(b.epoch).map_err(|_|CodeBindingError::InvalidBindingVersion)?,&value]).await)?;if inserted==0{let row=db(tx.query_one("SELECT branch_ref,record_json FROM code_repo_bindings WHERE tenant=$1 AND scope_key=$2 AND repo_id=$3",&[&b.scope.tenant,&key,&b.repo_id.0]).await)?;let branch:String=row.get(0);if branch!=b.branch_ref{return Err(CodeBindingError::BindingConflict)}let v:serde_json::Value=row.get(1);let existing=serde_json::from_value(v).map_err(|_|CodeBindingError::PersistenceFailure)?;tx.commit().await.map_err(|_|CodeBindingError::PersistenceFailure)?;return Ok(existing)}db(tx.execute("INSERT INTO code_epochs(tenant,scope_key,epoch) VALUES($1,$2,1) ON CONFLICT(tenant,scope_key) DO UPDATE SET epoch=code_epochs.epoch+1",&[&b.scope.tenant,&key]).await)?;tx.commit().await.map_err(|_|CodeBindingError::PersistenceFailure)?;Ok(b.clone())})
    }
    pub fn get_binding(
        &self,
        scope: &Scope,
        repo: &RepoId,
    ) -> Result<Option<CodeBinding>, CodeBindingError> {
        let key = scope_key(scope)?;
        runtime().block_on(async{let row=db(self.connection.client.query_opt("SELECT record_json FROM code_repo_bindings WHERE tenant=$1 AND scope_key=$2 AND repo_id=$3",&[&scope.tenant,&key,&repo.0]).await)?;row.map(|r|serde_json::from_value(r.get(0)).map_err(|_|CodeBindingError::PersistenceFailure)).transpose()})
    }
    pub fn put_generation(&self, g: &CodeGeneration) -> Result<CodeGeneration, CodeBindingError> {
        g.validate()?;
        let value = json(g)?;
        runtime().block_on(async{db(self.connection.client.execute("INSERT INTO code_generations(generation_id,record_json) VALUES($1,$2) ON CONFLICT(generation_id) DO NOTHING",&[&g.id,&value]).await)?;let row=db(self.connection.client.query_one("SELECT record_json FROM code_generations WHERE generation_id=$1",&[&g.id]).await)?;let stored:serde_json::Value=row.get(0);if stored!=value{return Err(CodeBindingError::InvalidSource)}Ok(g.clone())})
    }
    pub fn begin_attempt(
        &mut self,
        scope: &Scope,
        repo: &RepoId,
        generation: &CodeGeneration,
        request: &CodeAttempt,
    ) -> Result<CodeAttempt, CodeBindingError> {
        generation.validate()?;
        let key = scope_key(scope)?;
        let generation_json = json(generation)?;
        runtime().block_on(async {
            let tx = self.connection.client.transaction().await.map_err(|_| CodeBindingError::PersistenceFailure)?;
            db(tx.execute("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))", &[&format!("{}:{key}", scope.tenant)]).await)?;
            let binding = db(tx.query_opt("SELECT version,epoch,active_generation FROM code_repo_bindings WHERE tenant=$1 AND scope_key=$2 AND repo_id=$3 FOR UPDATE", &[&scope.tenant, &key, &repo.0]).await)?.ok_or(CodeBindingError::BindingRequired)?;
            let version: i64 = binding.get(0);
            let epoch: i64 = binding.get(1);
            let active: Option<String> = binding.get(2);
            if u64::try_from(version).map_err(|_| CodeBindingError::StaleGeneration)? != request.binding_version
                || u64::try_from(epoch).map_err(|_| CodeBindingError::StaleGeneration)? != request.expected_epoch
                || active != request.expected_active_generation
                || generation.binding_version != request.binding_version {
                return Err(CodeBindingError::StaleGeneration);
            }
            if let Some(row) = db(tx.query_opt("SELECT g.record_json,a.record_json,a.fence FROM code_attempts a JOIN code_generations g ON g.generation_id=a.generation_id WHERE a.tenant=$1 AND a.scope_key=$2 AND a.repo_id=$3 AND a.idempotency_key=$4", &[&scope.tenant, &key, &repo.0, &request.idempotency_key]).await)? {
                let stored_generation: CodeGeneration = serde_json::from_value(row.get(0)).map_err(|_| CodeBindingError::PersistenceFailure)?;
                let stored: CodeAttempt = serde_json::from_value(row.get(1)).map_err(|_| CodeBindingError::PersistenceFailure)?;
                let mut canonical_request = request.clone();
                if canonical_request.fencing_token == 0 {
                    canonical_request.fencing_token = stored.fencing_token;
                }
                if stored_generation == *generation && stored == canonical_request {
                    tx.commit().await.map_err(|_| CodeBindingError::PersistenceFailure)?;
                    return Ok(stored);
                }
                return Err(CodeBindingError::InvalidAttempt);
            }
            let max: Option<i64> = db(tx.query_one("SELECT MAX(fence) FROM code_attempts WHERE tenant=$1 AND scope_key=$2 AND repo_id=$3", &[&scope.tenant, &key, &repo.0]).await)?.get(0);
            let next = max.unwrap_or(0).checked_add(1).ok_or(CodeBindingError::InvalidAttempt)?;
            let mut attempt = request.clone();
            if attempt.fencing_token == 0 {
                attempt.fencing_token = u64::try_from(next).map_err(|_| CodeBindingError::InvalidAttempt)?;
            }
            let fence = i64::try_from(attempt.fencing_token).map_err(|_| CodeBindingError::InvalidAttempt)?;
            if fence != next {
                return Err(CodeBindingError::InvalidAttempt);
            }
            attempt.validate()?;
            let attempt_json = json(&attempt)?;
            let required = json(&attempt.receipt_kinds)?;
            db(tx.execute("INSERT INTO code_generations(generation_id,record_json) VALUES($1,$2) ON CONFLICT(generation_id) DO NOTHING", &[&generation.id, &generation_json]).await)?;
            let stored_generation: serde_json::Value = db(tx.query_one("SELECT record_json FROM code_generations WHERE generation_id=$1", &[&generation.id]).await)?.get(0);
            if stored_generation != generation_json { return Err(CodeBindingError::InvalidSource); }
            db(tx.execute("INSERT INTO code_attempts(attempt_id,tenant,scope_key,repo_id,generation_id,binding_version,expected_epoch,expected_active_generation,fence,idempotency_key,state,required_receipts,record_json) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)", &[&attempt.id,&scope.tenant,&key,&repo.0,&generation.id,&version,&epoch,&attempt.expected_active_generation,&fence,&attempt.idempotency_key,&serde_json::to_value(&attempt.state).unwrap().as_str().unwrap_or(""),&required,&attempt_json]).await)?;
            tx.commit().await.map_err(|_| CodeBindingError::PersistenceFailure)?;
            Ok(attempt)
        })
    }
    pub fn put_lineage(
        &self,
        scope: &Scope,
        lineage: &CodeLineage,
    ) -> Result<CodeLineage, CodeBindingError> {
        lineage.validate()?;
        let key = scope_key(scope)?;
        let v = json(lineage)?;
        let id = format!("{}:{}", lineage.source_path, lineage.generation_id);
        runtime().block_on(async{let n=db(self.connection.client.execute("INSERT INTO code_lineage(tenant,scope_key,repo_id,generation_id,record_id,lineage_json) VALUES($1,$2,$3,$4,$5,$6) ON CONFLICT DO NOTHING",&[&scope.tenant,&key,&lineage.repo_id.0,&lineage.generation_id,&id,&v]).await)?;if n==0{let row=db(self.connection.client.query_one("SELECT lineage_json FROM code_lineage WHERE tenant=$1 AND scope_key=$2 AND repo_id=$3 AND generation_id=$4 AND record_id=$5",&[&scope.tenant,&key,&lineage.repo_id.0,&lineage.generation_id,&id]).await)?;if row.get::<_,serde_json::Value>(0)!=v{return Err(CodeBindingError::InvalidSource)}}Ok(lineage.clone())})
    }
    pub fn put_source(
        &self,
        scope: &Scope,
        s: &IndexedSource,
    ) -> Result<IndexedSource, CodeBindingError> {
        s.validate()?;
        let key = scope_key(scope)?;
        let v = json(s)?;
        runtime().block_on(async{let n=db(self.connection.client.execute("INSERT INTO code_sources(tenant,scope_key,repo_id,generation_id,path,source_json) VALUES($1,$2,$3,$4,$5,$6) ON CONFLICT DO NOTHING",&[&scope.tenant,&key,&s.repo_id.0,&s.generation_id,&s.path,&v]).await)?;if n==0{let row=db(self.connection.client.query_one("SELECT source_json FROM code_sources WHERE tenant=$1 AND scope_key=$2 AND repo_id=$3 AND generation_id=$4 AND path=$5",&[&scope.tenant,&key,&s.repo_id.0,&s.generation_id,&s.path]).await)?;if row.get::<_,serde_json::Value>(0)!=v{return Err(CodeBindingError::InvalidSource)}}Ok(s.clone())})
    }
    pub fn record_receipt(&self, r: &StageReceipt) -> Result<StageReceipt, CodeBindingError> {
        if r.lane.is_empty() || r.digest.is_empty() {
            return Err(CodeBindingError::InvalidAttempt);
        }
        runtime().block_on(async{let attempt=db(self.connection.client.query_opt("SELECT generation_id,required_receipts FROM code_attempts WHERE attempt_id=$1",&[&r.attempt_id]).await)?.ok_or(CodeBindingError::InvalidAttempt)?;let generation:String=attempt.get(0);let required:Vec<String>=serde_json::from_value(attempt.get(1)).map_err(|_|CodeBindingError::PersistenceFailure)?;if generation!=r.generation_id||!required.iter().any(|v|v==&r.lane){return Err(CodeBindingError::InvalidAttempt)}let n=db(self.connection.client.execute("INSERT INTO code_attempt_receipts(attempt_id,lane,digest) VALUES($1,$2,$3) ON CONFLICT DO NOTHING",&[&r.attempt_id,&r.lane,&r.digest]).await)?;if n==0{let row=db(self.connection.client.query_one("SELECT digest FROM code_attempt_receipts WHERE attempt_id=$1 AND lane=$2",&[&r.attempt_id,&r.lane]).await)?;if row.get::<_,String>(0)!=r.digest{return Err(CodeBindingError::InvalidAttempt)}}Ok(r.clone())})
    }
    // clippy roll 2026-10: SQL row tuple matches fixed query column order.
    #[allow(clippy::type_complexity)]
    pub fn publish(
        &mut self,
        scope: &Scope,
        repo: &RepoId,
        attempt_id: &str,
        expected_epoch: u64,
    ) -> Result<CodeBinding, CodeBindingError> {
        let key = scope_key(scope)?;
        runtime().block_on(async{if let Some(done)=db(self.connection.client.query_opt("SELECT a.state,a.generation_id,b.epoch,b.record_json FROM code_attempts a JOIN code_repo_bindings b ON b.tenant=a.tenant AND b.scope_key=a.scope_key AND b.repo_id=a.repo_id WHERE a.attempt_id=$1 AND a.tenant=$2 AND a.scope_key=$3 AND a.repo_id=$4",&[&attempt_id,&scope.tenant,&key,&repo.0]).await)?{let state:String=done.get(0);let generation:String=done.get(1);let epoch:i64=done.get(2);if state=="published"&&u64::try_from(epoch).ok()==expected_epoch.checked_add(1){let binding=jsonb_to_binding(done.get(3))?;if binding.active_generation.as_deref()==Some(generation.as_str()){return Ok(binding)}}}let tx=self.connection.client.transaction().await.map_err(|_|CodeBindingError::PersistenceFailure)?;db(tx.execute("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))",&[&format!("{}:{key}",scope.tenant)]).await)?;let row=db(tx.query_opt("SELECT a.expected_epoch,a.binding_version,a.expected_active_generation,a.generation_id,a.fence,a.state,a.required_receipts,b.epoch,b.version,b.active_generation,b.record_json FROM code_attempts a JOIN code_repo_bindings b ON b.tenant=a.tenant AND b.scope_key=a.scope_key AND b.repo_id=a.repo_id WHERE a.attempt_id=$1 AND a.tenant=$2 AND a.scope_key=$3 AND a.repo_id=$4 FOR UPDATE OF b",&[&attempt_id,&scope.tenant,&key,&repo.0]).await)?.ok_or(CodeBindingError::StaleGeneration)?;let (epoch,ver,prev,generation_id,fence,state,required,current_epoch,binding_ver,current_active,_bjson):(i64,i64,Option<String>,String,i64,String,serde_json::Value,i64,i64,Option<String>,serde_json::Value)=(row.get(0),row.get(1),row.get(2),row.get(3),row.get(4),row.get(5),row.get(6),row.get(7),row.get(8),row.get(9),row.get(10));if u64::try_from(epoch).map_err(|_|CodeBindingError::StaleGeneration)?!=expected_epoch||epoch!=current_epoch||ver!=binding_ver||prev!=current_active||fence<=0||state!="ready"{return Err(CodeBindingError::StaleGeneration)}let latest:Option<i64>=db(tx.query_one("SELECT MAX(fence) FROM code_attempts WHERE tenant=$1 AND scope_key=$2 AND repo_id=$3",&[&scope.tenant,&key,&repo.0]).await)?.get(0);if latest!=Some(fence){return Err(CodeBindingError::StaleGeneration)}let mut expected:Vec<String>=serde_json::from_value(required).map_err(|_|CodeBindingError::PersistenceFailure)?;expected.sort();let rows=db(tx.query("SELECT lane FROM code_attempt_receipts WHERE attempt_id=$1",&[&attempt_id]).await)?;let mut actual:Vec<String>=rows.iter().map(|r|r.get(0)).collect();actual.sort();if expected.is_empty()||expected!=actual{return Err(CodeBindingError::StaleGeneration)}let n=db(tx.execute("UPDATE code_repo_bindings SET active_generation=$1,epoch=epoch+1,record_json=jsonb_set(jsonb_set(record_json,'{activeGeneration}',to_jsonb($1::text)),'{epoch}',to_jsonb(epoch+1)) WHERE tenant=$2 AND scope_key=$3 AND repo_id=$4 AND epoch=$5 AND version=$6 AND active_generation IS NOT DISTINCT FROM $7",&[&generation_id,&scope.tenant,&key,&repo.0,&i64::try_from(expected_epoch).map_err(|_|CodeBindingError::StaleGeneration)?,&ver,&prev]).await)?;if n!=1{return Err(CodeBindingError::StaleGeneration)}db(tx.execute("INSERT INTO code_epochs(tenant,scope_key,epoch) VALUES($1,$2,1) ON CONFLICT(tenant,scope_key) DO UPDATE SET epoch=code_epochs.epoch+1",&[&scope.tenant,&key]).await)?;db(tx.execute("UPDATE code_attempts SET state='published' WHERE attempt_id=$1 AND fence=$2",&[&attempt_id,&fence]).await)?;let updated=jsonb_to_binding(tx.query_one("SELECT record_json FROM code_repo_bindings WHERE tenant=$1 AND scope_key=$2 AND repo_id=$3",&[&scope.tenant,&key,&repo.0]).await.map_err(|_|CodeBindingError::PersistenceFailure)?.get(0))?;tx.commit().await.map_err(|_|CodeBindingError::PersistenceFailure)?;Ok(updated)})
    }
    pub fn eligibility(&mut self, scope: &Scope) -> Result<CodeEligibility, CodeBindingError> {
        let key = scope_key(scope)?;
        runtime().block_on(async {
            let tx = self
                .connection
                .client
                .build_transaction()
                .isolation_level(IsolationLevel::RepeatableRead)
                .start()
                .await
                .map_err(|_| CodeBindingError::PersistenceFailure)?;
            let epoch = db(
                tx.query_opt(
                    "SELECT epoch FROM code_epochs WHERE tenant=$1 AND scope_key=$2",
                    &[&scope.tenant, &key],
                )
                .await,
            )?
            .map(|row| row.get::<_, i64>(0))
            .unwrap_or(0);
            let rows = db(
                tx.query(
                    "SELECT repo_id,active_generation FROM code_repo_bindings WHERE tenant=$1 AND scope_key=$2 AND active_generation IS NOT NULL ORDER BY repo_id",
                    &[&scope.tenant, &key],
                )
                .await,
            )?;
            let mut repository_ids = Vec::with_capacity(rows.len());
            let mut generations = Vec::with_capacity(rows.len());
            for row in rows {
                repository_ids.push(row.get(0));
                generations.push(row.get(1));
            }
            tx.commit()
                .await
                .map_err(|_| CodeBindingError::PersistenceFailure)?;
            Ok(CodeEligibility {
                scope: scope.clone(),
                epoch: u64::try_from(epoch).map_err(|_| CodeBindingError::PersistenceFailure)?,
                generations,
                repository_ids,
            })
        })
    }
    pub fn journal_reset(
        &self,
        j: &CodeResetJournal,
        checkpoint: &str,
    ) -> Result<CodeResetJournal, CodeBindingError> {
        j.preview.validate()?;
        if checkpoint.is_empty() {
            return Err(CodeBindingError::InvalidResetPreview);
        }
        let v = json(j)?;
        let key = scope_key(&j.preview.binding.scope)?;
        runtime().block_on(async{db(self.connection.client.execute("INSERT INTO code_reset_journals(preview_hash,tenant,scope_key,repo_id,journal_json,checkpoint) VALUES($1,$2,$3,$4,$5,$6) ON CONFLICT(preview_hash) DO NOTHING",&[&j.preview.preview_hash,&j.preview.binding.scope.tenant,&key,&j.preview.binding.repo_id.0,&v,&checkpoint]).await)?;let row=db(self.connection.client.query_one("SELECT journal_json,checkpoint FROM code_reset_journals WHERE preview_hash=$1",&[&j.preview.preview_hash]).await)?;if row.get::<_,serde_json::Value>(0)!=v||row.get::<_,String>(1)!=checkpoint{return Err(CodeBindingError::InvalidResetPreview)}Ok(j.clone())})
    }
    pub fn load_reset_journal(
        &self,
        reset_id: &str,
    ) -> Result<Option<CodeResetJournal>, CodeBindingError> {
        runtime().block_on(async {
            let rows = db(self
                .connection
                .client
                .query("SELECT journal_json FROM code_reset_journals", &[])
                .await)?;
            for row in rows {
                let journal: CodeResetJournal = serde_json::from_value(row.get(0))
                    .map_err(|_| CodeBindingError::PersistenceFailure)?;
                if journal.reset_id == reset_id {
                    let rows = db(self.connection.client.query(
                        "SELECT checkpoint FROM code_reset_journal_progress WHERE reset_id=$1 ORDER BY record_id",
                        &[&reset_id],
                    ).await)?;
                    let mut journal = journal;
                    for row in rows {
                        let checkpoint: String = row.get(0);
                        if !journal.checkpoints.contains(&checkpoint) {
                            journal.checkpoints.push(checkpoint);
                        }
                    }
                    return Ok(Some(journal));
                }
            }
            Ok(None)
        })
    }
    pub fn save_reset_journal(&self, journal: &CodeResetJournal) -> Result<(), CodeBindingError> {
        let value = json(journal)?;
        runtime().block_on(async {
            let changed = db(self.connection.client.execute("UPDATE code_reset_journals SET journal_json=$1, checkpoint='progress' WHERE preview_hash=$2", &[&value, &journal.preview.preview_hash]).await)?;
            if changed != 1 { return Err(CodeBindingError::InvalidResetPreview); }
            Ok(())
        })
    }
    pub fn reset_preview(
        &self,
        scope: &Scope,
        repo: &RepoId,
    ) -> Result<CodeResetPreview, CodeBindingError> {
        let binding = self
            .get_binding(scope, repo)?
            .ok_or(CodeBindingError::BindingRequired)?;
        let key = scope_key(scope)?;
        let targets=runtime().block_on(async{let rows=db(self.connection.client.query("SELECT record_id FROM code_lineage WHERE tenant=$1 AND scope_key=$2 AND repo_id=$3 ORDER BY record_id",&[&scope.tenant,&key,&repo.0]).await)?;Ok::<_,CodeBindingError>(rows.iter().map(|r|r.get::<_,String>(0)).collect::<Vec<_>>())})?;
        if targets.is_empty() {
            return Err(CodeBindingError::InvalidResetPreview);
        }
        let mut preview = CodeResetPreview {
            binding: binding.clone(),
            preview_hash: String::new(),
            target_ids: targets,
            targets: Vec::new(),
            ambiguity: Vec::new(),
            preservation_digests: vec![PreservationInventory::default().digest.clone()],
            preservation_inventory: PreservationInventory::default(),
            binding_versions: vec![(repo.clone(), binding.version)],
            code_epoch: binding.epoch,
        };
        preview.preview_hash = preview.content_hash()?;
        Ok(preview)
    }
    /// Persists one on-request ref observation keyed by (scope, repo)
    /// (TASK-018), engine-neutral twin of the SQLite cell: observed OID/time
    /// of the CONFIGURED branch ref, recorded separately from the indexed
    /// generation OID. Binding-keyed — never keyed by a root basename or
    /// source string.
    pub fn put_ref_observation(
        &self,
        scope: &Scope,
        repo: &str,
        observation: &engram_knowledge::CodeRefObservation,
    ) -> Result<engram_knowledge::CodeRefObservation, CodeBindingError> {
        observation.validate()?;
        let key = scope_key(scope)?;
        let value = json(observation)?;
        let policy = match observation.policy {
            engram_knowledge::RefObservationPolicy::OnRequest => "on_request",
        };
        runtime().block_on(async {
            db(self
                .connection
                .client
                .execute(
                    "INSERT INTO code_ref_observations(tenant,scope_key,repo_id,branch_ref,observed_oid,observed_at,policy,record_json) VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT(tenant,scope_key,repo_id) DO UPDATE SET branch_ref=$4,observed_oid=$5,observed_at=$6,policy=$7,record_json=$8",
                    &[
                        &scope.tenant,
                        &key,
                        &repo,
                        &observation.branch_ref,
                        &observation.observed_oid,
                        &observation.observed_at.to_rfc3339(),
                        &policy,
                        &value,
                    ],
                )
                .await)?;
            Ok(observation.clone())
        })
    }
    /// The durable last ref observation for one bound repository; `None`
    /// is the honest "no valid observation exists" state.
    pub fn get_ref_observation(
        &self,
        scope: &Scope,
        repo: &str,
    ) -> Result<Option<engram_knowledge::CodeRefObservation>, CodeBindingError> {
        let key = scope_key(scope)?;
        runtime().block_on(async {
            let row = db(self
                .connection
                .client
                .query_opt(
                    "SELECT record_json FROM code_ref_observations WHERE tenant=$1 AND scope_key=$2 AND repo_id=$3",
                    &[&scope.tenant, &key, &repo],
                )
                .await)?;
            row.map(|r| {
                serde_json::from_value(r.get(0))
                    .map_err(|_| CodeBindingError::PersistenceFailure)
            })
            .transpose()
        })
    }
}
fn jsonb_to_binding(v: serde_json::Value) -> Result<CodeBinding, CodeBindingError> {
    serde_json::from_value(v).map_err(|_| CodeBindingError::PersistenceFailure)
}
