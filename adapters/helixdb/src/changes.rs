//! Transaction-bound invalidation metadata and durable scan progress.
use crate::{
    HelixStore,
    error::{Error, Result},
    records::*,
    transport::Operation,
};
use async_trait::async_trait;
use engram_domain::*;
use engram_runtime::{CoreError, CoreResult};
use helix_db::dsl::prelude::*;
use serde_json::{Value, json};
use std::collections::BTreeMap;
type Mutations = BTreeMap<(String, String), Option<Value>>;
fn before(store: &HelixStore, family: &str, id: &str) -> Result<Option<Value>> {
    let partition = store.inner.partition.discriminator()?;
    let key = store.inner.partition.record_key(family, id)?;
    let request = QueryRequest::read(
        read_batch()
            .var_as(
                "rows",
                g().n_with_label_where(
                    family,
                    SourcePredicate::and(vec![
                        SourcePredicate::eq("partition", partition.clone()),
                        SourcePredicate::eq("key", key.clone()),
                    ]),
                )
                .value_map(None::<Vec<String>>),
            )
            .returning(["rows"]),
    );
    let response = store.inner.transport.execute(&Operation::new(request)?)?;
    let rows = crate::schema::rows(&response, "rows")?;
    if rows.len() > 1 {
        return Err(Error::IncompatibleSchema);
    }
    rows.first()
        .map(|row| {
            if row.get("partition").and_then(Value::as_str) != Some(partition.as_str())
                || row.get("key").and_then(Value::as_str) != Some(key.as_str())
                || row.get("public_id").and_then(Value::as_str) != Some(id)
            {
                return Err(Error::IncompatibleSchema);
            }
            let v: Value = serde_json::from_str(
                row.get("payload")
                    .and_then(Value::as_str)
                    .ok_or(Error::IncompatibleSchema)?,
            )
            .map_err(|_| Error::IncompatibleSchema)?;
            if crate::records::id(&v)? != id {
                return Err(Error::IncompatibleSchema);
            }
            Ok(v)
        })
        .transpose()
}
/// Called before request construction under the already-owned scope/session guard.
/// It appends metadata to the SAME guarded mutation map, never a second transaction.
pub(crate) fn augment(store: &HelixStore, mutations: &mut Mutations) -> Result<()> {
    if mutations
        .keys()
        .any(|(family, _)| family == CHANGE || family == CHANGE_STATE)
    {
        return Err(Error::InvalidConfig(
            "derived change families cannot overwrite explicit staged effects",
        ));
    }
    let original = mutations.clone();
    let mut state = before(store, CHANGE_STATE, "state")?.unwrap_or_else(
        || json!({"id":"state","generation":uuid::Uuid::new_v4().to_string(),"sequence":0}),
    );
    let mut sequence = state["sequence"]
        .as_u64()
        .ok_or(Error::IncompatibleSchema)?;
    let start = sequence;
    for ((family, id), new) in original {
        let target = match family.as_str() {
            GRAPH => KnowledgeChangeTarget::Graph,
            ENTITY => KnowledgeChangeTarget::Entity,
            RELATIONSHIP => KnowledgeChangeTarget::Relationship,
            DOCUMENT => KnowledgeChangeTarget::Document,
            CHUNK => KnowledgeChangeTarget::Chunk,
            _ => continue,
        };
        let old = before(store, &family, &id)?;
        if old == new {
            continue;
        }
        let value = new.as_ref().or(old.as_ref()).ok_or(Error::Storage)?;
        sequence = sequence
            .checked_add(1)
            .filter(|n| *n <= i64::MAX as u64)
            .ok_or(Error::Storage)?;
        let archived = |v: &Value| {
            v.get("archivedAt").is_some_and(|v| !v.is_null())
                || v.get("status").and_then(Value::as_str) == Some("archived")
                || v.pointer("/metadata/archived").and_then(Value::as_bool) == Some(true)
        };
        let op = if new.is_none() {
            KnowledgeChangeOp::Remove
        } else if new.as_ref().is_some_and(archived) && !old.as_ref().is_some_and(archived) {
            KnowledgeChangeOp::Archive
        } else if old.as_ref().is_some_and(archived) && !new.as_ref().is_some_and(archived) {
            KnowledgeChangeOp::Restore
        } else {
            KnowledgeChangeOp::Upsert
        };
        let record = KnowledgeChange {
            seq: sequence.to_string(),
            scope: store.inner.partition.scope.clone(),
            stable_source_key: value
                .pointer("/metadata/stableSourceKey")
                .and_then(Value::as_str)
                .map(str::to_owned),
            graph_id: value
                .get("graphId")
                .and_then(Value::as_str)
                .map(Id::from)
                .or_else(|| (family == GRAPH).then(|| Id::from(id.clone()))),
            target_type: target,
            target_id: Id::from(id),
            op,
            occurred_at: chrono::Utc::now(),
            scan_run_id: value
                .pointer("/metadata/scanRunId")
                .and_then(Value::as_str)
                .map(str::to_owned),
        };
        let mut payload = crate::records::value(&record)?;
        let key = format!("{sequence:020}");
        payload["id"] = key.clone().into();
        mutations.insert((CHANGE.into(), key), Some(payload));
    }
    if sequence != start {
        state["sequence"] = sequence.into();
        mutations.insert((CHANGE_STATE.into(), "state".into()), Some(state));
    }
    Ok(())
}
fn invalid(reason: &str) -> CoreError {
    CoreError::InvalidRequest {
        reason: reason.into(),
    }
}
#[async_trait]
impl engram_knowledge::KnowledgeChangeLog for HelixStore {
    async fn changes_since(&self, q: &KnowledgeChangesRequest) -> CoreResult<KnowledgeChangesPage> {
        if q.limit == 0 || q.limit > 500 {
            return Err(invalid("invalid bounded change page"));
        }
        self.check_scope(&q.scope).map_err(core)?;
        let state = self.get::<Value>(CHANGE_STATE, "state").map_err(core)?;
        let generation = state
            .as_ref()
            .and_then(|v| v["generation"].as_str())
            .unwrap_or("");
        let high = state
            .as_ref()
            .and_then(|v| v["sequence"].as_u64())
            .unwrap_or(0);
        let partition = self.inner.partition.discriminator().map_err(core)?;
        let cursor = |seq: u64| {
            serde_json::to_string(&(partition.as_str(), q.source_key.as_deref(), generation, seq))
                .expect("cursor serializes")
        };
        let high_watermark = Some(cursor(high));
        let mut reset = false;
        let after = if let Some(c) = &q.after {
            match serde_json::from_str::<(String, Option<String>, String, u64)>(c) {
                Ok((p, f, g, n))
                    if p == partition && f == q.source_key && g == generation && n <= high =>
                {
                    n
                }
                _ => {
                    reset = true;
                    0
                }
            }
        } else {
            0
        };
        if reset {
            return Ok(KnowledgeChangesPage {
                items: vec![],
                next_cursor: None,
                high_watermark,
                has_more: false,
                reset_required: true,
            });
        }
        let mut items = self.all::<KnowledgeChange>(CHANGE).map_err(core)?;
        items.retain(|v| {
            v.seq.parse::<u64>().is_ok_and(|n| n > after)
                && q.source_key
                    .as_ref()
                    .is_none_or(|k| v.stable_source_key.as_ref() == Some(k))
        });
        items.sort_by_key(|v| v.seq.parse::<u64>().unwrap_or(0));
        let has_more = items.len() > q.limit as usize;
        items.truncate(q.limit as usize);
        let next_cursor = items
            .last()
            .map(|v| cursor(v.seq.parse().expect("validated sequence")));
        Ok(KnowledgeChangesPage {
            items,
            next_cursor,
            high_watermark,
            has_more,
            reset_required: false,
        })
    }
    async fn latest_checkpoint(
        &self,
        scope: &Scope,
        key: &str,
    ) -> CoreResult<Option<ScanCheckpoint>> {
        if !self.visible_scope(scope) {
            return Ok(None);
        }
        self.get(CHECKPOINT, key).map_err(core)
    }
    async fn latest_checkpoints(&self, scope: &Scope) -> CoreResult<Vec<ScanCheckpoint>> {
        if !self.visible_scope(scope) {
            return Ok(vec![]);
        }
        let mut rows = self.all::<ScanCheckpoint>(CHECKPOINT).map_err(core)?;
        rows.sort_by(|a, b| {
            b.started_at
                .cmp(&a.started_at)
                .then_with(|| a.stable_source_key.cmp(&b.stable_source_key))
        });
        Ok(rows)
    }
}
#[async_trait]
impl engram_knowledge::ScanCheckpointStore for HelixStore {
    async fn put_checkpoint(&self, v: ScanCheckpoint) -> CoreResult<ScanCheckpoint> {
        self.check_scope(&v.scope).map_err(core)?;
        if v.stable_source_key.is_empty() || v.scan_run_id.is_empty() {
            return Err(invalid("checkpoint source/run identity required"));
        }
        self.domain_atomic(|s| {
            if let Some(old) = s
                .get::<ScanCheckpoint>(CHECKPOINT, &v.stable_source_key)
                .map_err(core)?
            {
                if old.started_at > v.started_at
                    || (old.started_at == v.started_at && old.scan_run_id != v.scan_run_id)
                {
                    return Ok(old);
                }
            }
            let mut payload = crate::records::value(&v).map_err(core)?;
            payload["id"] = v.stable_source_key.clone().into();
            s.stage(CHECKPOINT, &v.stable_source_key, Some(payload))
                .map_err(core)?;
            Ok(v)
        })
    }
}
