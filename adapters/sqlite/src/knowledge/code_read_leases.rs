//! Transactional read leases and conservative source-retention references.
use super::code_binding_port::{SqlCodeBindingRepository, read_eligibility, storage_error};
use async_trait::async_trait;
use engram_domain::{Scope, Timestamp};
use engram_knowledge::{CodeReadLease, CodeReadLeases, CoreError, CoreResult};
use rusqlite::{OptionalExtension, TransactionBehavior, params};
use std::collections::BTreeSet;
fn changed() -> CoreError {
    CoreError::Conflict {
        reason: "GenerationChanged".into(),
    }
}
fn value<T: serde::Serialize>(v: &T) -> CoreResult<String> {
    serde_json::to_string(v).map_err(|_| storage_error())
}
#[async_trait]
impl CodeReadLeases for SqlCodeBindingRepository {
    async fn acquire_read_lease(
        &self,
        lease: CodeReadLease,
        now: Timestamp,
    ) -> CoreResult<CodeReadLease> {
        if lease.id.is_empty() || lease.fencing_token != 1 || lease.expires_at <= now {
            return Err(changed());
        }
        let mut store = self.lock()?;
        let tx = store
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| storage_error())?;
        if read_eligibility(&tx, &lease.eligibility.scope)? != lease.eligibility {
            return Err(changed());
        }
        let scope = value(&lease.eligibility.scope)?;
        let json = value(&lease)?;
        let previous: Option<String> = tx
            .query_row(
                "SELECT record_json FROM code_read_leases WHERE lease_id=?1",
                [&lease.id],
                |r| r.get(0),
            )
            .optional()
            .map_err(|_| storage_error())?;
        if previous.as_ref().is_some_and(|old| old != &json) {
            return Err(changed());
        }
        tx.execute("INSERT OR IGNORE INTO code_read_leases(lease_id,scope_key,fence,expires_ms,record_json) VALUES(?1,?2,?3,?4,?5)",params![lease.id,scope,lease.fencing_token,lease.expires_at.timestamp_millis(),json]).map_err(|_|storage_error())?;
        tx.commit().map_err(|_| storage_error())?;
        Ok(lease)
    }
    async fn renew_read_lease(
        &self,
        lease: &CodeReadLease,
        now: Timestamp,
        expires_at: Timestamp,
    ) -> CoreResult<CodeReadLease> {
        if lease.expires_at <= now || expires_at <= now || expires_at <= lease.expires_at {
            return Err(changed());
        }
        let mut store = self.lock()?;
        let tx = store
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| storage_error())?;
        let previous: Option<String> = tx
            .query_row(
                "SELECT record_json FROM code_read_leases WHERE lease_id=?1",
                [&lease.id],
                |r| r.get(0),
            )
            .optional()
            .map_err(|_| storage_error())?;
        if previous.as_deref() != Some(&value(lease)?) {
            return Err(changed());
        }
        let mut renewed = lease.clone();
        renewed.fencing_token = renewed.fencing_token.checked_add(1).ok_or_else(changed)?;
        renewed.expires_at = expires_at;
        let changed_rows = tx.execute("UPDATE code_read_leases SET fence=?1,expires_ms=?2,record_json=?3 WHERE lease_id=?4 AND fence=?5",params![renewed.fencing_token,expires_at.timestamp_millis(),value(&renewed)?,lease.id,lease.fencing_token]).map_err(|_|storage_error())?;
        if changed_rows != 1 {
            return Err(changed());
        }
        tx.commit().map_err(|_| storage_error())?;
        Ok(renewed)
    }
    async fn release_read_lease(&self, lease: &CodeReadLease) -> CoreResult<()> {
        let store = self.lock()?;
        let changed_rows = store
            .connection
            .execute(
                "DELETE FROM code_read_leases WHERE lease_id=?1 AND scope_key=?2 AND fence=?3",
                params![
                    lease.id,
                    value(&lease.eligibility.scope)?,
                    lease.fencing_token
                ],
            )
            .map_err(|_| storage_error())?;
        if changed_rows != 1 {
            return Err(changed());
        }
        Ok(())
    }
    async fn retained_generations(
        &self,
        scope: &Scope,
        now: Timestamp,
        grace_millis: u64,
    ) -> CoreResult<Vec<String>> {
        let grace = i64::try_from(grace_millis).map_err(|_| changed())?;
        let cutoff = now.timestamp_millis().saturating_sub(grace);
        let mut store = self.lock()?;
        let tx = store
            .connection
            .transaction()
            .map_err(|_| storage_error())?;
        let key = value(scope)?;
        let mut retained: BTreeSet<String> = read_eligibility(&tx, scope)?
            .generations
            .into_iter()
            .collect();
        {
            let mut statement = tx.prepare("SELECT a.generation_id FROM code_attempts a JOIN code_repo_bindings b ON b.tenant=a.tenant AND b.scope_key=a.scope_key AND b.repo_id=a.repo_id WHERE a.tenant=?1 AND a.scope_key=?2 AND (a.state IN ('Created','Acquiring','Staging','Ready','CleanupPending') OR (a.state='Published' AND a.generation_id IS NOT b.active_generation AND a.expected_epoch=(SELECT MAX(p.expected_epoch) FROM code_attempts p WHERE p.tenant=a.tenant AND p.scope_key=a.scope_key AND p.repo_id=a.repo_id AND p.state='Published' AND p.generation_id IS NOT b.active_generation)))").map_err(|_|storage_error())?;
            for row in statement
                .query_map(params![scope.tenant, key], |r| r.get::<_, String>(0))
                .map_err(|_| storage_error())?
            {
                retained.insert(row.map_err(|_| storage_error())?);
            }
        }
        {
            let mut statement = tx
                .prepare(
                    "SELECT record_json FROM code_read_leases WHERE scope_key=?1 AND expires_ms>?2",
                )
                .map_err(|_| storage_error())?;
            for row in statement
                .query_map(params![key, cutoff], |r| r.get::<_, String>(0))
                .map_err(|_| storage_error())?
            {
                let lease: CodeReadLease = serde_json::from_str(&row.map_err(|_| storage_error())?)
                    .map_err(|_| storage_error())?;
                retained.extend(lease.eligibility.generations);
            }
        }
        tx.commit().map_err(|_| storage_error())?;
        Ok(retained.into_iter().collect())
    }
}
