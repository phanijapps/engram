//! Scope-bounded source membership rows; supersession decisions remain canonical.
use crate::{HelixStore, error::Error, records::*};
use async_trait::async_trait;
use engram_domain::{Scope, Timestamp};
use engram_knowledge::{SourceBatchLedger, SourceBatchRecord, SourceBatchRecordKind};
use engram_runtime::CoreResult;
use serde::{Deserialize, Serialize};
#[derive(Clone, Serialize, Deserialize)]
struct Row {
    id: String,
    source_id: String,
    batch_key: String,
    kind: String,
    record_id: String,
    superseded_at: Option<Timestamp>,
}
fn kind(k: SourceBatchRecordKind) -> &'static str {
    match k {
        SourceBatchRecordKind::Entity => "entity",
        SourceBatchRecordKind::Relationship => "relationship",
        SourceBatchRecordKind::MemoryRecord => "memory",
    }
}
fn key(source: &str, batch: &str, k: SourceBatchRecordKind, id: &str) -> Result<String, Error> {
    serde_json::to_string(&(source, batch, kind(k), id)).map_err(|_| Error::Storage)
}
#[async_trait]
impl SourceBatchLedger for HelixStore {
    async fn list_source_batch_records(
        &self,
        scope: &Scope,
        source: &str,
    ) -> CoreResult<Vec<SourceBatchRecord>> {
        if !self.visible_scope(scope) {
            return Ok(vec![]);
        }
        self.filtered::<Row>(LEDGER, "source_id", source)
            .map_err(core)?
            .into_iter()
            .filter(|r| r.superseded_at.is_none())
            .map(|r| {
                Ok(SourceBatchRecord {
                    source_id: r.source_id,
                    batch_key: r.batch_key,
                    kind: match r.kind.as_str() {
                        "entity" => SourceBatchRecordKind::Entity,
                        "relationship" => SourceBatchRecordKind::Relationship,
                        "memory" => SourceBatchRecordKind::MemoryRecord,
                        _ => return Err(core(Error::IncompatibleSchema)),
                    },
                    record_id: r.record_id,
                    superseded_at: r.superseded_at,
                })
            })
            .collect()
    }
    async fn record_source_batch_writes(
        &self,
        scope: &Scope,
        source: &str,
        batch: &str,
        k: SourceBatchRecordKind,
        ids: &[String],
    ) -> CoreResult<()> {
        self.check_scope(scope).map_err(core)?;
        if source.trim().is_empty() || batch.trim().is_empty() {
            return Err(core(Error::InvalidConfig(
                "source and batch identity required",
            )));
        }
        self.atomic(|s| {
            for id in ids {
                let family = match k {
                    SourceBatchRecordKind::Entity => ENTITY,
                    SourceBatchRecordKind::Relationship => RELATIONSHIP,
                    SourceBatchRecordKind::MemoryRecord => MEMORY,
                };
                if s.get::<serde_json::Value>(family, id)?.is_none() {
                    return Err(Error::InvalidConfig("ledger target absent in partition"));
                }
                let row = Row {
                    id: key(source, batch, k, id)?,
                    source_id: source.into(),
                    batch_key: batch.into(),
                    kind: kind(k).into(),
                    record_id: id.clone(),
                    superseded_at: None,
                };
                s.put(LEDGER, &row)?;
            }
            Ok(())
        })
        .map_err(core)
    }
    async fn mark_source_batch_superseded(
        &self,
        scope: &Scope,
        source: &str,
        batch: &str,
        k: SourceBatchRecordKind,
        id: &str,
    ) -> CoreResult<()> {
        self.check_scope(scope).map_err(core)?;
        self.atomic(|s| {
            let key = key(source, batch, k, id)?;
            let Some(mut row) = s.get::<Row>(LEDGER, &key)? else {
                return Ok(());
            };
            if row.superseded_at.is_none() {
                row.superseded_at = Some(chrono::Utc::now());
                s.put(LEDGER, &row)?;
            }
            Ok(())
        })
        .map_err(core)
    }
}
