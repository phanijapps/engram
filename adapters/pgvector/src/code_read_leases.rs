//! Transactional PostgreSQL read leases for durable generation retention.
use super::code_binding_port::{storage_error, PgCodeBindingRepository};
use async_trait::async_trait;
use engram_domain::{Scope, Timestamp};
use engram_knowledge::{CodeEligibility, CodeReadLease, CodeReadLeases, CoreError, CoreResult};
use std::collections::BTreeSet;

fn changed() -> CoreError {
    CoreError::Conflict {
        reason: "GenerationChanged".into(),
    }
}

fn value<T: serde::Serialize>(value: &T) -> CoreResult<serde_json::Value> {
    serde_json::to_value(value).map_err(|_| storage_error())
}

fn fence(value: u64) -> CoreResult<i64> {
    i64::try_from(value).map_err(|_| changed())
}

async fn lease_authority(tx: &tokio_postgres::Transaction<'_>, lease_id: &str) -> CoreResult<()> {
    tx.execute(
        "SELECT pg_advisory_xact_lock(hashtextextended($1, 0))",
        &[&format!("engram-code-read-lease-id:{lease_id}")],
    )
    .await
    .map_err(|_| storage_error())?;
    Ok(())
}

async fn eligibility(
    tx: &tokio_postgres::Transaction<'_>,
    scope: &Scope,
) -> CoreResult<CodeEligibility> {
    let key = serde_json::to_string(scope).map_err(|_| storage_error())?;
    let epoch = tx
        .query_opt(
            "SELECT epoch FROM code_epochs WHERE tenant=$1 AND scope_key=$2 FOR SHARE",
            &[&scope.tenant, &key],
        )
        .await
        .map_err(|_| storage_error())?
        .map(|row| row.get::<_, i64>(0))
        .unwrap_or(0);
    let rows = tx
        .query(
            "SELECT repo_id,active_generation FROM code_repo_bindings WHERE tenant=$1 AND scope_key=$2 AND active_generation IS NOT NULL ORDER BY repo_id FOR SHARE",
            &[&scope.tenant, &key],
        )
        .await
        .map_err(|_| storage_error())?;
    let mut repository_ids = Vec::with_capacity(rows.len());
    let mut generations = Vec::with_capacity(rows.len());
    for row in rows {
        repository_ids.push(row.get(0));
        generations.push(row.get(1));
    }
    Ok(CodeEligibility {
        scope: scope.clone(),
        epoch: u64::try_from(epoch).map_err(|_| storage_error())?,
        generations,
        repository_ids,
    })
}

#[async_trait]
impl CodeReadLeases for PgCodeBindingRepository {
    async fn acquire_read_lease(
        &self,
        lease: CodeReadLease,
        now: Timestamp,
    ) -> CoreResult<CodeReadLease> {
        if lease.id.is_empty() || lease.fencing_token != 1 || lease.expires_at <= now {
            return Err(changed());
        }
        let scope = serde_json::to_string(&lease.eligibility.scope).map_err(|_| storage_error())?;
        let record = value(&lease)?;
        let lease_fence = fence(lease.fencing_token)?;
        self.blocking(move |store| crate::connection::runtime().block_on(async {
            let tx = store.connection.client.transaction().await.map_err(|_| storage_error())?;
            tx.execute("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))", &[&format!("{}:{scope}", lease.eligibility.scope.tenant)]).await.map_err(|_| storage_error())?;
            if eligibility(&tx, &lease.eligibility.scope).await? != lease.eligibility {
                return Err(changed());
            }
            lease_authority(&tx, &lease.id).await?;
            if tx.query_opt("SELECT 1 FROM code_read_lease_tombstones WHERE lease_id=$1", &[&lease.id]).await.map_err(|_| storage_error())?.is_some() {
                return Err(changed());
            }
            let previous = tx.query_opt("SELECT record_json FROM code_read_leases WHERE lease_id=$1 FOR UPDATE", &[&lease.id]).await.map_err(|_| storage_error())?;
            if previous.is_some_and(|row| row.get::<_, serde_json::Value>(0) != record) {
                return Err(changed());
            }
            tx.execute("INSERT INTO code_read_leases(lease_id,scope_key,fence,expires_ms,record_json) VALUES($1,$2,$3,$4,$5) ON CONFLICT(lease_id) DO NOTHING", &[&lease.id, &scope, &lease_fence, &lease.expires_at.timestamp_millis(), &record]).await.map_err(|_| storage_error())?;
            let durable = tx.query_opt("SELECT record_json FROM code_read_leases WHERE lease_id=$1 FOR UPDATE", &[&lease.id]).await.map_err(|_| storage_error())?.map(|row| row.get::<_, serde_json::Value>(0));
            if durable != Some(record) {
                return Err(changed());
            }
            tx.commit().await.map_err(|_| storage_error())?;
            Ok(lease)
        })).await
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
        let lease = lease.clone();
        let old = value(&lease)?;
        let scope = serde_json::to_string(&lease.eligibility.scope).map_err(|_| storage_error())?;
        let old_fence = fence(lease.fencing_token)?;
        self.blocking(move |store| crate::connection::runtime().block_on(async {
            let tx = store.connection.client.transaction().await.map_err(|_| storage_error())?;
            tx.execute("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))", &[&format!("{}:{scope}", lease.eligibility.scope.tenant)]).await.map_err(|_| storage_error())?;
            if eligibility(&tx, &lease.eligibility.scope).await? != lease.eligibility {
                return Err(changed());
            }
            lease_authority(&tx, &lease.id).await?;
            let previous = tx.query_opt("SELECT record_json FROM code_read_leases WHERE lease_id=$1 FOR UPDATE", &[&lease.id]).await.map_err(|_| storage_error())?;
            if previous.map(|row| row.get::<_, serde_json::Value>(0)) != Some(old) {
                return Err(changed());
            }
            let mut renewed = lease.clone();
            renewed.fencing_token = renewed.fencing_token.checked_add(1).ok_or_else(changed)?;
            renewed.expires_at = expires_at;
            let renewed_fence = fence(renewed.fencing_token)?;
            let changed_rows = tx.execute("UPDATE code_read_leases SET fence=$1,expires_ms=$2,record_json=$3 WHERE lease_id=$4 AND fence=$5", &[&renewed_fence, &expires_at.timestamp_millis(), &value(&renewed)?, &lease.id, &old_fence]).await.map_err(|_| storage_error())?;
            if changed_rows != 1 {
                return Err(changed());
            }
            tx.commit().await.map_err(|_| storage_error())?;
            Ok(renewed)
        })).await
    }

    async fn release_read_lease(&self, lease: &CodeReadLease) -> CoreResult<()> {
        let lease = lease.clone();
        let scope = serde_json::to_string(&lease.eligibility.scope).map_err(|_| storage_error())?;
        let lease_fence = fence(lease.fencing_token)?;
        let record = value(&lease)?;
        self.blocking(move |store| {
            crate::connection::runtime().block_on(async {
                let tx = store.connection.client.transaction().await.map_err(|_| storage_error())?;
                lease_authority(&tx, &lease.id).await?;
                let durable = tx.query_opt("SELECT record_json FROM code_read_leases WHERE lease_id=$1 FOR UPDATE", &[&lease.id]).await.map_err(|_| storage_error())?;
                if durable.map(|row| row.get::<_, serde_json::Value>(0)) != Some(record) {
                    return Err(changed());
                }
                let changed_rows = tx.execute("DELETE FROM code_read_leases WHERE lease_id=$1 AND scope_key=$2 AND fence=$3", &[&lease.id, &scope, &lease_fence]).await.map_err(|_| storage_error())?;
                if changed_rows != 1 {
                    return Err(changed());
                }
                tx.execute("INSERT INTO code_read_lease_tombstones(lease_id) VALUES($1) ON CONFLICT(lease_id) DO NOTHING", &[&lease.id]).await.map_err(|_| storage_error())?;
                tx.commit().await.map_err(|_| storage_error())?;
                Ok(())
            })
        }).await
    }

    async fn retained_generations(
        &self,
        scope: &Scope,
        now: Timestamp,
        grace_millis: u64,
    ) -> CoreResult<Vec<String>> {
        let scope = scope.clone();
        let cutoff = now
            .timestamp_millis()
            .saturating_sub(i64::try_from(grace_millis).map_err(|_| changed())?);
        let key = serde_json::to_string(&scope).map_err(|_| storage_error())?;
        self.blocking(move |store| crate::connection::runtime().block_on(async {
            let tx = store.connection.client.transaction().await.map_err(|_| storage_error())?;
            tx.execute("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))", &[&format!("{}:{key}", scope.tenant)]).await.map_err(|_| storage_error())?;
            let mut retained: BTreeSet<String> = eligibility(&tx, &scope).await?.generations.into_iter().collect();
            let attempts = tx.query("SELECT a.generation_id FROM code_attempts a JOIN code_repo_bindings b ON b.tenant=a.tenant AND b.scope_key=a.scope_key AND b.repo_id=a.repo_id WHERE a.tenant=$1 AND a.scope_key=$2 AND (a.state IN ('created','acquiring','staging','ready','cleanup_pending') OR (a.state='published' AND a.generation_id IS DISTINCT FROM b.active_generation AND a.expected_epoch=(SELECT MAX(p.expected_epoch) FROM code_attempts p WHERE p.tenant=a.tenant AND p.scope_key=a.scope_key AND p.repo_id=a.repo_id AND p.state='published' AND p.generation_id IS DISTINCT FROM b.active_generation)))", &[&scope.tenant, &key]).await.map_err(|_| storage_error())?;
            retained.extend(attempts.into_iter().map(|row| row.get::<_, String>(0)));
            let leases = tx.query("SELECT record_json FROM code_read_leases WHERE scope_key=$1 AND expires_ms>$2", &[&key, &cutoff]).await.map_err(|_| storage_error())?;
            for row in leases {
                let lease: CodeReadLease = serde_json::from_value(row.get(0)).map_err(|_| storage_error())?;
                retained.extend(lease.eligibility.generations);
            }
            tx.commit().await.map_err(|_| storage_error())?;
            Ok(retained.into_iter().collect())
        })).await
    }
}
