//! Durable SQLite records and fenced publication for project code bindings.
use engram_domain::{
    CodeAttempt, CodeBinding, CodeBindingError, CodeGeneration, CodeLineage, CodeResetJournal,
};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use std::path::Path;

pub struct SqlCodeBindingStore {
    connection: Connection,
}
impl SqlCodeBindingStore {
    pub fn open(path: impl AsRef<Path>) -> rusqlite::Result<Self> {
        let connection = Connection::open(path)?;
        connection.busy_timeout(std::time::Duration::from_secs(5))?;
        connection.execute_batch("PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL; CREATE TABLE IF NOT EXISTS code_repo_bindings (tenant TEXT NOT NULL, scope_key TEXT NOT NULL, repo_id TEXT NOT NULL, branch_ref TEXT NOT NULL, version INTEGER NOT NULL, active_generation TEXT, epoch INTEGER NOT NULL, record_json TEXT NOT NULL, PRIMARY KEY(tenant,scope_key,repo_id)); CREATE TABLE IF NOT EXISTS code_generations (generation_id TEXT PRIMARY KEY, record_json TEXT NOT NULL); CREATE TABLE IF NOT EXISTS code_attempts (attempt_id TEXT PRIMARY KEY, tenant TEXT NOT NULL, scope_key TEXT NOT NULL, repo_id TEXT NOT NULL, generation_id TEXT NOT NULL, binding_version INTEGER NOT NULL, expected_epoch INTEGER NOT NULL, expected_active_generation TEXT, fence INTEGER NOT NULL, idempotency_key TEXT NOT NULL, state TEXT NOT NULL, receipts_json TEXT NOT NULL DEFAULT '[]', record_json TEXT NOT NULL, UNIQUE(tenant,scope_key,repo_id,idempotency_key)); CREATE TABLE IF NOT EXISTS code_lineage (record_id TEXT PRIMARY KEY, repo_id TEXT NOT NULL, generation_id TEXT NOT NULL, lineage_json TEXT NOT NULL); CREATE TABLE IF NOT EXISTS code_reset_journals (preview_hash TEXT PRIMARY KEY, journal_json TEXT NOT NULL, checkpoint TEXT NOT NULL); CREATE TABLE IF NOT EXISTS code_attempt_receipts(attempt_id TEXT NOT NULL,lane TEXT NOT NULL,digest TEXT NOT NULL,PRIMARY KEY(attempt_id,lane));")?;
        Ok(Self { connection })
    }
    pub fn bind(&mut self, binding: &CodeBinding) -> Result<CodeBinding, CodeBindingError> {
        binding.validate()?;
        let scope = &binding.scope;
        let scope_key =
            serde_json::to_string(scope).map_err(|_| CodeBindingError::PersistenceFailure)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| CodeBindingError::PersistenceFailure)?;
        let current:Option<(String,String)>=tx.query_row("SELECT branch_ref,record_json FROM code_repo_bindings WHERE tenant=?1 AND scope_key=?2 AND repo_id=?3",params![scope.tenant,scope_key,binding.repo_id.0],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(|_|CodeBindingError::BindingConflict)?;
        if let Some((branch, json)) = current {
            if branch != binding.branch_ref {
                return Err(CodeBindingError::BindingConflict);
            }
            let result =
                serde_json::from_str(&json).map_err(|_| CodeBindingError::PersistenceFailure)?;
            tx.commit()
                .map_err(|_| CodeBindingError::PersistenceFailure)?;
            return Ok(result);
        }
        let json =
            serde_json::to_string(binding).map_err(|_| CodeBindingError::PersistenceFailure)?;
        tx.execute("INSERT INTO code_repo_bindings(tenant,scope_key,repo_id,branch_ref,version,active_generation,epoch,record_json) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",params![scope.tenant,scope_key,binding.repo_id.0,binding.branch_ref,binding.version,binding.active_generation,binding.epoch,json]).map_err(|_|CodeBindingError::BindingConflict)?;
        tx.commit()
            .map_err(|_| CodeBindingError::PersistenceFailure)?;
        Ok(binding.clone())
    }
    pub fn binding(
        &self,
        scope: &engram_domain::Scope,
        repo: &str,
    ) -> rusqlite::Result<Option<CodeBinding>> {
        let key = serde_json::to_string(scope)
            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
        self.connection.query_row("SELECT record_json FROM code_repo_bindings WHERE tenant=?1 AND scope_key=?2 AND repo_id=?3",params![scope.tenant,key,repo],|r|r.get::<_,String>(0)).optional()?.map(|s|serde_json::from_str(&s).map_err(|e|rusqlite::Error::ToSqlConversionFailure(Box::new(e)))).transpose()
    }
    pub fn stage_generation(&mut self, g: &CodeGeneration) -> Result<(), CodeBindingError> {
        g.validate()?;
        let json = serde_json::to_string(g).map_err(|_| CodeBindingError::InvalidSource)?;
        let existing: Option<String> = self
            .connection
            .query_row(
                "SELECT record_json FROM code_generations WHERE generation_id=?1",
                [&g.id],
                |r| r.get(0),
            )
            .optional()
            .map_err(|_| CodeBindingError::PersistenceFailure)?;
        if let Some(stored) = existing {
            return if stored == json {
                Ok(())
            } else {
                Err(CodeBindingError::InvalidSource)
            };
        }
        self.connection
            .execute(
                "INSERT INTO code_generations(generation_id,record_json) VALUES(?1,?2)",
                params![g.id, json],
            )
            .map_err(|_| CodeBindingError::PersistenceFailure)?;
        Ok(())
    }
    pub fn begin_attempt(
        &mut self,
        scope: &engram_domain::Scope,
        repo: &str,
        generation: &CodeGeneration,
        a: &CodeAttempt,
    ) -> Result<(), CodeBindingError> {
        a.validate()?;
        let key = serde_json::to_string(scope).map_err(|_| CodeBindingError::PersistenceFailure)?;
        let json = serde_json::to_string(a).map_err(|_| CodeBindingError::PersistenceFailure)?;
        let bound: Option<(i64,i64,Option<String>)> = self.connection.query_row("SELECT version,epoch,active_generation FROM code_repo_bindings WHERE tenant=?1 AND scope_key=?2 AND repo_id=?3", params![scope.tenant,key,repo], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional().map_err(|_|CodeBindingError::PersistenceFailure)?;
        let (version, epoch, active) = bound.ok_or(CodeBindingError::BindingRequired)?;
        if version as u64 != a.binding_version
            || generation.binding_version != a.binding_version
            || epoch as u64 != a.expected_epoch
            || active != a.expected_active_generation
        {
            return Err(CodeBindingError::StaleGeneration);
        }
        let existing: Option<String> = self.connection.query_row("SELECT record_json FROM code_attempts WHERE tenant=?1 AND scope_key=?2 AND repo_id=?3 AND idempotency_key=?4", params![scope.tenant,key,repo,a.idempotency_key], |r| r.get(0)).optional().map_err(|_|CodeBindingError::PersistenceFailure)?;
        if let Some(record) = existing {
            return if record == json {
                Ok(())
            } else {
                Err(CodeBindingError::InvalidAttempt)
            };
        }
        self.stage_generation(generation)?;
        self.connection.execute("INSERT INTO code_attempts(attempt_id,tenant,scope_key,repo_id,generation_id,binding_version,expected_epoch,expected_active_generation,fence,idempotency_key,state,receipts_json,record_json) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)",params![a.id,scope.tenant,key,repo,generation.id,a.binding_version,a.expected_epoch,a.expected_active_generation,a.fencing_token,a.idempotency_key,format!("{:?}",a.state),serde_json::to_string(&a.receipt_kinds).unwrap(),json]).map_err(|_|CodeBindingError::InvalidAttempt)?;
        Ok(())
    }
    pub fn put_lineage(
        &mut self,
        record_id: &str,
        lineage: &CodeLineage,
    ) -> Result<(), CodeBindingError> {
        lineage.validate()?;
        let json =
            serde_json::to_string(lineage).map_err(|_| CodeBindingError::PersistenceFailure)?;
        let prior: Option<String> = self
            .connection
            .query_row(
                "SELECT lineage_json FROM code_lineage WHERE record_id=?1",
                [record_id],
                |r| r.get(0),
            )
            .optional()
            .map_err(|_| CodeBindingError::PersistenceFailure)?;
        if let Some(value) = prior {
            return if value == json {
                Ok(())
            } else {
                Err(CodeBindingError::InvalidSource)
            };
        }
        self.connection.execute("INSERT INTO code_lineage(record_id,repo_id,generation_id,lineage_json) VALUES(?1,?2,?3,?4)",params![record_id,lineage.repo_id.0,lineage.generation_id,json]).map_err(|_|CodeBindingError::PersistenceFailure)?;
        Ok(())
    }
    pub fn put_reset_journal(
        &mut self,
        journal: &CodeResetJournal,
        checkpoint: &str,
    ) -> Result<(), CodeBindingError> {
        journal.preview.validate()?;
        if checkpoint.trim().is_empty() {
            return Err(CodeBindingError::InvalidResetPreview);
        }
        let json =
            serde_json::to_string(journal).map_err(|_| CodeBindingError::PersistenceFailure)?;
        let prior: Option<(String, String)> = self
            .connection
            .query_row(
                "SELECT journal_json,checkpoint FROM code_reset_journals WHERE preview_hash=?1",
                [&journal.preview.preview_hash],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(|_| CodeBindingError::PersistenceFailure)?;
        if let Some((stored, old_checkpoint)) = prior {
            return if stored == json && old_checkpoint == checkpoint {
                Ok(())
            } else {
                Err(CodeBindingError::InvalidResetPreview)
            };
        }
        self.connection.execute("INSERT INTO code_reset_journals(preview_hash,journal_json,checkpoint) VALUES(?1,?2,?3)",params![journal.preview.preview_hash,json,checkpoint]).map_err(|_|CodeBindingError::PersistenceFailure)?;
        Ok(())
    }
    pub fn record_receipt(
        &mut self,
        attempt_id: &str,
        lane: &str,
        digest: &str,
    ) -> Result<(), CodeBindingError> {
        if lane.is_empty() || digest.is_empty() {
            return Err(CodeBindingError::InvalidAttempt);
        }
        let existing: Option<String> = self
            .connection
            .query_row(
                "SELECT digest FROM code_attempt_receipts WHERE attempt_id=?1 AND lane=?2",
                params![attempt_id, lane],
                |r| r.get(0),
            )
            .optional()
            .map_err(|_| CodeBindingError::PersistenceFailure)?;
        if let Some(value) = existing {
            return if value == digest {
                Ok(())
            } else {
                Err(CodeBindingError::InvalidAttempt)
            };
        }
        self.connection
            .execute(
                "INSERT INTO code_attempt_receipts(attempt_id,lane,digest) VALUES(?1,?2,?3)",
                params![attempt_id, lane, digest],
            )
            .map_err(|_| CodeBindingError::PersistenceFailure)?;
        Ok(())
    }
    pub fn publish(
        &mut self,
        scope: &engram_domain::Scope,
        repo: &str,
        attempt_id: &str,
    ) -> Result<CodeBinding, CodeBindingError> {
        let key = serde_json::to_string(scope).map_err(|_| CodeBindingError::StaleGeneration)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| CodeBindingError::PersistenceFailure)?;
        let row:Option<(i64,String,String,i64,i64,Option<String>,i64,String,String)>=tx.query_row("SELECT a.expected_epoch,a.generation_id,a.state,b.epoch,a.binding_version,a.expected_active_generation,a.fence,a.receipts_json,b.record_json FROM code_attempts a JOIN code_repo_bindings b ON b.tenant=a.tenant AND b.scope_key=a.scope_key AND b.repo_id=a.repo_id WHERE a.attempt_id=?1 AND a.tenant=?2 AND a.scope_key=?3 AND a.repo_id=?4",params![attempt_id,scope.tenant,key,repo],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?,r.get(7)?,r.get(8)?))).optional().map_err(|_|CodeBindingError::PersistenceFailure)?;
        let (
            expected,
            generation,
            state,
            epoch,
            attempt_version,
            expected_active,
            fence,
            receipts_json,
            binding_json,
        ) = row.ok_or(CodeBindingError::StaleGeneration)?;
        let bound: CodeBinding = serde_json::from_str(&binding_json)
            .map_err(|_| CodeBindingError::PersistenceFailure)?;
        if expected != epoch
            || attempt_version as u64 != bound.version
            || bound.active_generation != expected_active
            || fence <= 0
            || state != "Ready"
        {
            return Err(CodeBindingError::StaleGeneration);
        }
        let declared: Vec<String> = serde_json::from_str(&receipts_json)
            .map_err(|_| CodeBindingError::PersistenceFailure)?;
        let mut actual: Vec<String> = tx
            .prepare("SELECT lane FROM code_attempt_receipts WHERE attempt_id=?1")
            .map_err(|_| CodeBindingError::PersistenceFailure)?
            .query_map([attempt_id], |r| r.get(0))
            .map_err(|_| CodeBindingError::PersistenceFailure)?
            .collect::<Result<_, _>>()
            .map_err(|_| CodeBindingError::PersistenceFailure)?;
        actual.sort();
        let mut required = declared;
        required.sort();
        if required.is_empty() || required != actual {
            return Err(CodeBindingError::StaleGeneration);
        }
        let receipts: i64 = tx
            .query_row(
                "SELECT count(*) FROM code_attempt_receipts WHERE attempt_id=?1",
                [attempt_id],
                |r| r.get(0),
            )
            .unwrap_or(0);
        if receipts == 0 {
            return Err(CodeBindingError::StaleGeneration);
        }
        let n=tx.execute("UPDATE code_repo_bindings SET active_generation=?1,epoch=epoch+1 WHERE tenant=?2 AND scope_key=?3 AND repo_id=?4 AND epoch=?5",params![generation,scope.tenant,key,repo,expected]).map_err(|_|CodeBindingError::StaleGeneration)?;
        if n != 1 {
            return Err(CodeBindingError::StaleGeneration);
        }
        tx.execute(
            "UPDATE code_attempts SET state='Published' WHERE attempt_id=?1",
            [attempt_id],
        )
        .map_err(|_| CodeBindingError::StaleGeneration)?;
        let json:String=tx.query_row("SELECT record_json FROM code_repo_bindings WHERE tenant=?1 AND scope_key=?2 AND repo_id=?3",params![scope.tenant,key,repo],|r|r.get(0)).map_err(|_|CodeBindingError::StaleGeneration)?;
        let mut binding: CodeBinding =
            serde_json::from_str(&json).map_err(|_| CodeBindingError::StaleGeneration)?;
        binding.active_generation = Some(generation);
        binding.epoch = expected as u64 + 1;
        tx.execute("UPDATE code_repo_bindings SET record_json=?1 WHERE tenant=?2 AND scope_key=?3 AND repo_id=?4",params![serde_json::to_string(&binding).unwrap(),scope.tenant,key,repo]).map_err(|_|CodeBindingError::StaleGeneration)?;
        tx.commit().map_err(|_| CodeBindingError::StaleGeneration)?;
        Ok(binding)
    }
}
