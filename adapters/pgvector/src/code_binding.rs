//! PostgreSQL persistence for project-bound code identity and fenced publication.
use crate::connection::{runtime, PgConnection};
use engram_domain::{
    CodeAttempt, CodeBinding, CodeBindingError, CodeGeneration, CodeLineage, CodeResetJournal,
    CodeResetPreview, IndexedSource, RepoId, Scope,
};
use engram_knowledge::{AcquisitionMode, CodeEligibility, RepositoryRegistration, StageReceipt};
use sha2::{Digest, Sha256};

pub struct PgCodeBindingStore {
    connection: PgConnection,
}
fn db<T>(r: Result<T, tokio_postgres::Error>) -> Result<T, CodeBindingError> {
    r.map_err(|_| CodeBindingError::PersistenceFailure)
}
fn scope_key(scope: &Scope) -> Result<String, CodeBindingError> {
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
        runtime().block_on(async{db(self.connection.client.batch_execute("CREATE TABLE IF NOT EXISTS code_repository_registrations(repo_id TEXT PRIMARY KEY, locator_id TEXT NOT NULL, acquisition TEXT NOT NULL, record_json JSONB NOT NULL); CREATE TABLE IF NOT EXISTS code_repo_bindings(tenant TEXT NOT NULL,scope_key TEXT NOT NULL,repo_id TEXT NOT NULL,branch_ref TEXT NOT NULL,version BIGINT NOT NULL,active_generation TEXT,epoch BIGINT NOT NULL,record_json JSONB NOT NULL,PRIMARY KEY(tenant,scope_key,repo_id)); CREATE TABLE IF NOT EXISTS code_generations(generation_id TEXT PRIMARY KEY,record_json JSONB NOT NULL); CREATE TABLE IF NOT EXISTS code_attempts(attempt_id TEXT PRIMARY KEY,tenant TEXT NOT NULL,scope_key TEXT NOT NULL,repo_id TEXT NOT NULL,generation_id TEXT NOT NULL,binding_version BIGINT NOT NULL,expected_epoch BIGINT NOT NULL,expected_active_generation TEXT,fence BIGINT NOT NULL,idempotency_key TEXT NOT NULL,state TEXT NOT NULL,required_receipts JSONB NOT NULL,record_json JSONB NOT NULL,UNIQUE(tenant,scope_key,repo_id,idempotency_key)); CREATE TABLE IF NOT EXISTS code_attempt_receipts(attempt_id TEXT NOT NULL,lane TEXT NOT NULL,digest TEXT NOT NULL,PRIMARY KEY(attempt_id,lane)); CREATE TABLE IF NOT EXISTS code_lineage(tenant TEXT NOT NULL,scope_key TEXT NOT NULL,repo_id TEXT NOT NULL,generation_id TEXT NOT NULL,record_id TEXT NOT NULL,lineage_json JSONB NOT NULL,PRIMARY KEY(tenant,scope_key,repo_id,generation_id,record_id)); CREATE TABLE IF NOT EXISTS code_sources(tenant TEXT NOT NULL,scope_key TEXT NOT NULL,repo_id TEXT NOT NULL,generation_id TEXT NOT NULL,path TEXT NOT NULL,source_json JSONB NOT NULL,PRIMARY KEY(tenant,scope_key,repo_id,generation_id,path)); CREATE TABLE IF NOT EXISTS code_reset_journals(preview_hash TEXT PRIMARY KEY,tenant TEXT NOT NULL,scope_key TEXT NOT NULL,repo_id TEXT NOT NULL,journal_json JSONB NOT NULL,checkpoint TEXT NOT NULL); CREATE TABLE IF NOT EXISTS code_epochs(tenant TEXT NOT NULL,scope_key TEXT NOT NULL,epoch BIGINT NOT NULL,PRIMARY KEY(tenant,scope_key));").await).map_err(|e|e)})
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
        runtime().block_on(async{let tx=self.connection.client.transaction().await.map_err(|_|CodeBindingError::PersistenceFailure)?;if let Some(row)=db(tx.query_opt("SELECT branch_ref,record_json FROM code_repo_bindings WHERE tenant=$1 AND scope_key=$2 AND repo_id=$3 FOR UPDATE",&[&b.scope.tenant,&key,&b.repo_id.0]).await)?{let branch:String=row.get(0);if branch!=b.branch_ref{return Err(CodeBindingError::BindingConflict)}let stored:serde_json::Value=row.get(1);let existing=serde_json::from_value(stored).map_err(|_|CodeBindingError::PersistenceFailure)?;tx.commit().await.map_err(|_|CodeBindingError::PersistenceFailure)?;return Ok(existing)}let inserted=db(tx.execute("INSERT INTO code_repo_bindings(tenant,scope_key,repo_id,branch_ref,version,active_generation,epoch,record_json) VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT DO NOTHING",&[&b.scope.tenant,&key,&b.repo_id.0,&b.branch_ref,&(b.version as i64),&b.active_generation,&(b.epoch as i64),&value]).await)?;if inserted==0{let row=db(tx.query_one("SELECT branch_ref,record_json FROM code_repo_bindings WHERE tenant=$1 AND scope_key=$2 AND repo_id=$3",&[&b.scope.tenant,&key,&b.repo_id.0]).await)?;let branch:String=row.get(0);if branch!=b.branch_ref{return Err(CodeBindingError::BindingConflict)}let v:serde_json::Value=row.get(1);let existing=serde_json::from_value(v).map_err(|_|CodeBindingError::PersistenceFailure)?;tx.commit().await.map_err(|_|CodeBindingError::PersistenceFailure)?;return Ok(existing)}tx.commit().await.map_err(|_|CodeBindingError::PersistenceFailure)?;Ok(b.clone())})
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
        &self,
        scope: &Scope,
        repo: &RepoId,
        g: &CodeGeneration,
        a: &CodeAttempt,
    ) -> Result<CodeAttempt, CodeBindingError> {
        a.validate()?;
        g.validate()?;
        if g.binding_version != a.binding_version {
            return Err(CodeBindingError::StaleGeneration);
        }
        let key = scope_key(scope)?;
        let binding = self
            .get_binding(scope, repo)?
            .ok_or(CodeBindingError::BindingRequired)?;
        if binding.version != a.binding_version
            || binding.epoch != a.expected_epoch
            || binding.active_generation != a.expected_active_generation
        {
            return Err(CodeBindingError::StaleGeneration);
        }
        let av = json(a)?;
        let existing: Option<serde_json::Value> = runtime().block_on(async { let row=db(self.connection.client.query_opt("SELECT record_json FROM code_attempts WHERE tenant=$1 AND scope_key=$2 AND repo_id=$3 AND idempotency_key=$4",&[&scope.tenant,&key,&repo.0,&a.idempotency_key]).await)?; Ok::<_,CodeBindingError>(row.map(|r|r.get(0))) })?;
        if let Some(previous) = existing {
            return if previous == av {
                Ok(a.clone())
            } else {
                Err(CodeBindingError::InvalidAttempt)
            };
        }
        let max_fence: Option<i64> = runtime().block_on(async { let row=db(self.connection.client.query_one("SELECT MAX(fence) FROM code_attempts WHERE tenant=$1 AND scope_key=$2 AND repo_id=$3",&[&scope.tenant,&key,&repo.0]).await)?; Ok::<_,CodeBindingError>(row.get(0)) })?;
        if max_fence.is_some_and(|fence| a.fencing_token as i64 <= fence) {
            return Err(CodeBindingError::InvalidAttempt);
        }
        self.put_generation(g)?;
        let gv = json(g)?;
        let required = json(&a.receipt_kinds)?;
        runtime().block_on(async{let inserted=db(self.connection.client.execute("INSERT INTO code_attempts(attempt_id,tenant,scope_key,repo_id,generation_id,binding_version,expected_epoch,expected_active_generation,fence,idempotency_key,state,required_receipts,record_json) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13) ON CONFLICT(tenant,scope_key,repo_id,idempotency_key) DO NOTHING",&[&a.id,&scope.tenant,&key,&repo.0,&g.id,&(a.binding_version as i64),&(a.expected_epoch as i64),&a.expected_active_generation,&(a.fencing_token as i64),&a.idempotency_key,&serde_json::to_value(&a.state).unwrap().as_str().unwrap_or(""),&required,&av]).await)?;if inserted==0{let row=db(self.connection.client.query_opt("SELECT record_json FROM code_attempts WHERE tenant=$1 AND scope_key=$2 AND repo_id=$3 AND idempotency_key=$4",&[&scope.tenant,&key,&repo.0,&a.idempotency_key]).await)?.ok_or(CodeBindingError::PersistenceFailure)?;let prior:serde_json::Value=row.get(0);if prior!=av{return Err(CodeBindingError::InvalidAttempt)}}let _=gv;Ok(a.clone())})
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
    pub fn publish(
        &mut self,
        scope: &Scope,
        repo: &RepoId,
        attempt_id: &str,
        expected_epoch: u64,
    ) -> Result<CodeBinding, CodeBindingError> {
        let key = scope_key(scope)?;
        runtime().block_on(async{if let Some(done)=db(self.connection.client.query_opt("SELECT a.state,a.generation_id,b.epoch,b.record_json FROM code_attempts a JOIN code_repo_bindings b ON b.tenant=a.tenant AND b.scope_key=a.scope_key AND b.repo_id=a.repo_id WHERE a.attempt_id=$1 AND a.tenant=$2 AND a.scope_key=$3 AND a.repo_id=$4",&[&attempt_id,&scope.tenant,&key,&repo.0]).await)?{let state:String=done.get(0);let generation:String=done.get(1);let epoch:i64=done.get(2);if state=="published"&&epoch as u64==expected_epoch+1{let binding=jsonb_to_binding(done.get(3))?;if binding.active_generation.as_deref()==Some(generation.as_str()){return Ok(binding)}}}let tx=self.connection.client.transaction().await.map_err(|_|CodeBindingError::PersistenceFailure)?;let row=db(tx.query_opt("SELECT a.expected_epoch,a.binding_version,a.expected_active_generation,a.generation_id,a.fence,a.state,a.required_receipts,b.epoch,b.version,b.active_generation,b.record_json FROM code_attempts a JOIN code_repo_bindings b ON b.tenant=a.tenant AND b.scope_key=a.scope_key AND b.repo_id=a.repo_id WHERE a.attempt_id=$1 AND a.tenant=$2 AND a.scope_key=$3 AND a.repo_id=$4 FOR UPDATE OF b",&[&attempt_id,&scope.tenant,&key,&repo.0]).await)?.ok_or(CodeBindingError::StaleGeneration)?;let (epoch,ver,prev,gen,fence,state,required,current_epoch,binding_ver,current_active,_bjson):(i64,i64,Option<String>,String,i64,String,serde_json::Value,i64,i64,Option<String>,serde_json::Value)=(row.get(0),row.get(1),row.get(2),row.get(3),row.get(4),row.get(5),row.get(6),row.get(7),row.get(8),row.get(9),row.get(10));if epoch as u64!=expected_epoch||epoch!=current_epoch||ver!=binding_ver||prev!=current_active||fence<=0||state!="ready"{return Err(CodeBindingError::StaleGeneration)}let mut expected:Vec<String>=serde_json::from_value(required).map_err(|_|CodeBindingError::PersistenceFailure)?;expected.sort();let rows=db(tx.query("SELECT lane FROM code_attempt_receipts WHERE attempt_id=$1",&[&attempt_id]).await)?;let mut actual:Vec<String>=rows.iter().map(|r|r.get(0)).collect();actual.sort();if expected.is_empty()||expected!=actual{return Err(CodeBindingError::StaleGeneration)}let n=db(tx.execute("UPDATE code_repo_bindings SET active_generation=$1,epoch=epoch+1,record_json=jsonb_set(jsonb_set(record_json,'{activeGeneration}',to_jsonb($1::text)),'{epoch}',to_jsonb(epoch+1)) WHERE tenant=$2 AND scope_key=$3 AND repo_id=$4 AND epoch=$5 AND version=$6 AND active_generation IS NOT DISTINCT FROM $7",&[&gen,&scope.tenant,&key,&repo.0,&(expected_epoch as i64),&ver,&prev]).await)?;if n!=1{return Err(CodeBindingError::StaleGeneration)}db(tx.execute("UPDATE code_attempts SET state='published' WHERE attempt_id=$1 AND fence=$2",&[&attempt_id,&fence]).await)?;let updated=jsonb_to_binding(tx.query_one("SELECT record_json FROM code_repo_bindings WHERE tenant=$1 AND scope_key=$2 AND repo_id=$3",&[&scope.tenant,&key,&repo.0]).await.map_err(|_|CodeBindingError::PersistenceFailure)?.get(0))?;tx.commit().await.map_err(|_|CodeBindingError::PersistenceFailure)?;Ok(updated)})
    }
    pub fn eligibility(&self, scope: &Scope) -> Result<CodeEligibility, CodeBindingError> {
        let key = scope_key(scope)?;
        runtime().block_on(async{let rows=db(self.connection.client.query("SELECT repo_id,epoch FROM code_repo_bindings WHERE tenant=$1 AND scope_key=$2 AND active_generation IS NOT NULL",&[&scope.tenant,&key]).await)?;let mut ids=Vec::new();let mut gens=Vec::new();let mut epoch=0u64;for r in rows{ids.push(r.get::<_,String>(0));epoch=epoch.max(r.get::<_,i64>(1) as u64);}let rows=db(self.connection.client.query("SELECT active_generation FROM code_repo_bindings WHERE tenant=$1 AND scope_key=$2 AND active_generation IS NOT NULL",&[&scope.tenant,&key]).await)?;for r in rows{gens.push(r.get::<_,String>(0));}Ok(CodeEligibility{scope:scope.clone(),epoch,generations:gens,repository_ids:ids})})
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
        let bytes = serde_json::to_vec(&(scope, repo, &targets, binding.version, binding.epoch))
            .map_err(|_| CodeBindingError::PersistenceFailure)?;
        let hash = Sha256::digest(bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        Ok(CodeResetPreview {
            binding,
            preview_hash: hash,
            target_ids: targets,
        })
    }
}
fn jsonb_to_binding(v: serde_json::Value) -> Result<CodeBinding, CodeBindingError> {
    serde_json::from_value(v).map_err(|_| CodeBindingError::PersistenceFailure)
}
