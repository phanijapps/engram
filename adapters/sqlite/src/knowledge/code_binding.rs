//! Durable SQLite records and fenced publication for project code bindings.
use engram_domain::{
    CodeAttempt, CodeBinding, CodeBindingError, CodeGeneration, CodeLineage, CodeResetJournal,
};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use std::path::Path;

pub struct SqlCodeBindingStore {
    pub(super) connection: Connection,
}
impl SqlCodeBindingStore {
    pub fn open(path: impl AsRef<Path>) -> rusqlite::Result<Self> {
        let connection = Connection::open(path)?;
        connection.busy_timeout(std::time::Duration::from_secs(5))?;
        connection.execute_batch("PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL; CREATE TABLE IF NOT EXISTS code_repo_bindings (tenant TEXT NOT NULL, scope_key TEXT NOT NULL, repo_id TEXT NOT NULL, branch_ref TEXT NOT NULL, version INTEGER NOT NULL, active_generation TEXT, epoch INTEGER NOT NULL, record_json TEXT NOT NULL, PRIMARY KEY(tenant,scope_key,repo_id)); CREATE TABLE IF NOT EXISTS code_generations (generation_id TEXT PRIMARY KEY, record_json TEXT NOT NULL); CREATE TABLE IF NOT EXISTS code_attempts (attempt_id TEXT PRIMARY KEY, tenant TEXT NOT NULL, scope_key TEXT NOT NULL, repo_id TEXT NOT NULL, generation_id TEXT NOT NULL, binding_version INTEGER NOT NULL, expected_epoch INTEGER NOT NULL, expected_active_generation TEXT, fence INTEGER NOT NULL, idempotency_key TEXT NOT NULL, state TEXT NOT NULL, receipts_json TEXT NOT NULL DEFAULT '[]', record_json TEXT NOT NULL, UNIQUE(tenant,scope_key,repo_id,idempotency_key)); CREATE TABLE IF NOT EXISTS code_lineage (record_id TEXT PRIMARY KEY, repo_id TEXT NOT NULL, generation_id TEXT NOT NULL, lineage_json TEXT NOT NULL); CREATE TABLE IF NOT EXISTS code_reset_journals (preview_hash TEXT PRIMARY KEY, journal_json TEXT NOT NULL, checkpoint TEXT NOT NULL); CREATE TABLE IF NOT EXISTS code_attempt_receipts(attempt_id TEXT NOT NULL,lane TEXT NOT NULL,digest TEXT NOT NULL,PRIMARY KEY(attempt_id,lane));")?;
        connection.execute_batch("CREATE TABLE IF NOT EXISTS code_scope_epochs(scope_key TEXT PRIMARY KEY,epoch INTEGER NOT NULL); CREATE TABLE IF NOT EXISTS code_read_leases(lease_id TEXT PRIMARY KEY,scope_key TEXT NOT NULL,fence INTEGER NOT NULL,expires_ms INTEGER NOT NULL,record_json TEXT NOT NULL);")?;
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
        if binding.active_generation.is_some() || binding.epoch != 0 || binding.version != 1 {
            return Err(CodeBindingError::InvalidBindingVersion);
        }
        let json =
            serde_json::to_string(binding).map_err(|_| CodeBindingError::PersistenceFailure)?;
        tx.execute("INSERT INTO code_repo_bindings(tenant,scope_key,repo_id,branch_ref,version,active_generation,epoch,record_json) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",params![scope.tenant,scope_key,binding.repo_id.0,binding.branch_ref,binding.version,binding.active_generation,binding.epoch,json]).map_err(|_|CodeBindingError::BindingConflict)?;
        tx.execute("INSERT INTO code_scope_epochs(scope_key,epoch) VALUES(?1,1) ON CONFLICT(scope_key) DO UPDATE SET epoch=epoch+1",[&scope_key]).map_err(|_|CodeBindingError::PersistenceFailure)?;
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
        attempt: &CodeAttempt,
    ) -> Result<(), CodeBindingError> {
        let mut canonical = attempt.clone();
        generation.validate()?;
        let key = serde_json::to_string(scope).map_err(|_| CodeBindingError::PersistenceFailure)?;
        let generation_json =
            serde_json::to_string(generation).map_err(|_| CodeBindingError::PersistenceFailure)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| CodeBindingError::PersistenceFailure)?;
        let bound: Option<(i64,i64,Option<String>)> = tx.query_row("SELECT version,epoch,active_generation FROM code_repo_bindings WHERE tenant=?1 AND scope_key=?2 AND repo_id=?3", params![scope.tenant,key,repo], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional().map_err(|_|CodeBindingError::PersistenceFailure)?;
        let (version, epoch, active) = bound.ok_or(CodeBindingError::BindingRequired)?;
        let existing: Option<(String,String)> = tx.query_row("SELECT record_json,generation_id FROM code_attempts WHERE tenant=?1 AND scope_key=?2 AND repo_id=?3 AND idempotency_key=?4", params![scope.tenant,key,repo,attempt.idempotency_key], |r| Ok((r.get(0)?,r.get(1)?))).optional().map_err(|_|CodeBindingError::PersistenceFailure)?;
        if let Some((stored, generation_id)) = existing {
            let previous: CodeAttempt =
                serde_json::from_str(&stored).map_err(|_| CodeBindingError::PersistenceFailure)?;
            if canonical.fencing_token == 0 {
                canonical.fencing_token = previous.fencing_token;
            }
            canonical.validate()?;
            return if previous == canonical && generation_id == generation.id {
                Ok(())
            } else {
                Err(CodeBindingError::InvalidAttempt)
            };
        }
        if version as u64 != attempt.binding_version
            || generation.binding_version != attempt.binding_version
            || epoch as u64 != attempt.expected_epoch
            || active != attempt.expected_active_generation
        {
            return Err(CodeBindingError::StaleGeneration);
        }
        let latest: Option<i64> = tx.query_row("SELECT MAX(fence) FROM code_attempts WHERE tenant=?1 AND scope_key=?2 AND repo_id=?3",params![scope.tenant,key,repo],|r| r.get(0)).map_err(|_| CodeBindingError::PersistenceFailure)?;
        if canonical.fencing_token == 0 {
            canonical.fencing_token = latest
                .unwrap_or(0)
                .checked_add(1)
                .ok_or(CodeBindingError::InvalidAttempt)?
                as u64;
        }
        canonical.validate()?;
        let attempt = &canonical;
        let fence =
            i64::try_from(attempt.fencing_token).map_err(|_| CodeBindingError::InvalidAttempt)?;
        let json =
            serde_json::to_string(attempt).map_err(|_| CodeBindingError::PersistenceFailure)?;
        if latest.is_some_and(|old| fence <= old) {
            return Err(CodeBindingError::InvalidAttempt);
        }
        let stored: Option<String> = tx
            .query_row(
                "SELECT record_json FROM code_generations WHERE generation_id=?1",
                [&generation.id],
                |r| r.get(0),
            )
            .optional()
            .map_err(|_| CodeBindingError::PersistenceFailure)?;
        if stored.as_ref().is_some_and(|old| old != &generation_json) {
            return Err(CodeBindingError::InvalidSource);
        }
        tx.execute(
            "INSERT OR IGNORE INTO code_generations(generation_id,record_json) VALUES(?1,?2)",
            params![generation.id, generation_json],
        )
        .map_err(|_| CodeBindingError::PersistenceFailure)?;
        tx.execute("INSERT INTO code_attempts(attempt_id,tenant,scope_key,repo_id,generation_id,binding_version,expected_epoch,expected_active_generation,fence,idempotency_key,state,receipts_json,record_json) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)",params![attempt.id,scope.tenant,key,repo,generation.id,attempt.binding_version,attempt.expected_epoch,attempt.expected_active_generation,fence,attempt.idempotency_key,format!("{:?}",attempt.state),serde_json::to_string(&attempt.receipt_kinds).map_err(|_|CodeBindingError::InvalidAttempt)?,json]).map_err(|_|CodeBindingError::InvalidAttempt)?;
        tx.commit()
            .map_err(|_| CodeBindingError::PersistenceFailure)
    }

    pub fn attempt(
        &self,
        scope: &engram_domain::Scope,
        repo: &str,
        attempt_id: &str,
    ) -> Result<Option<(CodeGeneration, CodeAttempt)>, CodeBindingError> {
        let key = serde_json::to_string(scope).map_err(|_| CodeBindingError::PersistenceFailure)?;
        let row: Option<(String,String,String)> = self.connection.query_row("SELECT g.record_json,a.record_json,a.state FROM code_attempts a JOIN code_generations g ON g.generation_id=a.generation_id WHERE a.attempt_id=?1 AND a.tenant=?2 AND a.scope_key=?3 AND a.repo_id=?4",params![attempt_id,scope.tenant,key,repo],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional().map_err(|_|CodeBindingError::PersistenceFailure)?;
        row.map(|(g, a, state)| {
            let generation =
                serde_json::from_str(&g).map_err(|_| CodeBindingError::PersistenceFailure)?;
            let mut attempt: CodeAttempt =
                serde_json::from_str(&a).map_err(|_| CodeBindingError::PersistenceFailure)?;
            attempt.state = match state.as_str() {
                "Created" => engram_domain::CodeAttemptState::Created,
                "Acquiring" => engram_domain::CodeAttemptState::Acquiring,
                "Staging" => engram_domain::CodeAttemptState::Staging,
                "Ready" => engram_domain::CodeAttemptState::Ready,
                "Published" => engram_domain::CodeAttemptState::Published,
                "Failed" => engram_domain::CodeAttemptState::Failed,
                "Superseded" => engram_domain::CodeAttemptState::Superseded,
                "CleanupPending" => engram_domain::CodeAttemptState::CleanupPending,
                _ => return Err(CodeBindingError::PersistenceFailure),
            };
            Ok((generation, attempt))
        })
        .transpose()
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
        let declared: Option<String> = self
            .connection
            .query_row(
                "SELECT receipts_json FROM code_attempts WHERE attempt_id=?1",
                [attempt_id],
                |r| r.get(0),
            )
            .optional()
            .map_err(|_| CodeBindingError::PersistenceFailure)?;
        let required: Vec<String> =
            serde_json::from_str(&declared.ok_or(CodeBindingError::InvalidAttempt)?)
                .map_err(|_| CodeBindingError::PersistenceFailure)?;
        if !required.iter().any(|required| required == lane) {
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
        if state == "Published"
            && bound.active_generation.as_deref() == Some(generation.as_str())
            && epoch == expected + 1
            && attempt_version as u64 == bound.version
        {
            tx.commit()
                .map_err(|_| CodeBindingError::PersistenceFailure)?;
            return Ok(bound);
        }
        let latest: i64 = tx.query_row("SELECT MAX(fence) FROM code_attempts WHERE tenant=?1 AND scope_key=?2 AND repo_id=?3",params![scope.tenant,key,repo],|r|r.get(0)).map_err(|_|CodeBindingError::PersistenceFailure)?;
        if fence != latest {
            return Err(CodeBindingError::StaleGeneration);
        }
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
        tx.execute("INSERT INTO code_scope_epochs(scope_key,epoch) VALUES(?1,1) ON CONFLICT(scope_key) DO UPDATE SET epoch=epoch+1",[&key]).map_err(|_|CodeBindingError::PersistenceFailure)?;
        tx.commit().map_err(|_| CodeBindingError::StaleGeneration)?;
        Ok(binding)
    }
}
