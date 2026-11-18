//! Typed storage contributions; deterministic memory behavior belongs to core/memory.
use crate::{HelixStore, error::Error, records::*};
use async_trait::async_trait;
use engram_domain::*;
use engram_memory::MemoryStorage;
use engram_runtime::{CoreResult, effect::ObservedResult};
use serde_json::{Value, json};

fn response_id(store: &HelixStore, key: &str) -> crate::error::Result<String> {
    // Actual operation receipts use canonical UUIDs; this reserved namespace cannot alias them.
    Ok(format!(
        "memory-write-response:{}",
        store.inner.partition.record_key(RECEIPT, key)?
    ))
}
fn response(store: &HelixStore, key: &str) -> crate::error::Result<Option<WriteMemoryResponse>> {
    let id = response_id(store, key)?;
    let Some(envelope) = store.get::<Value>(RECEIPT, &id)? else {
        return Ok(None);
    };
    if envelope.get("kind").and_then(Value::as_str) != Some("memory-write-response-v1")
        || envelope.get("key").and_then(Value::as_str) != Some(key)
    {
        return Err(Error::IncompatibleSchema);
    }
    let response: WriteMemoryResponse = decode(
        envelope
            .get("response")
            .cloned()
            .ok_or(Error::IncompatibleSchema)?,
    )?;
    store.check_scope(&response.record.scope)?;
    store.check_scope(&response.event.scope)?;
    Ok(Some(response))
}
#[async_trait]
impl MemoryStorage for HelixStore {
    async fn enumerate(&self, scope: &Scope) -> CoreResult<Vec<MemoryRecord>> {
        if !self.visible_scope(scope) {
            return Ok(vec![]);
        }
        self.all(MEMORY).map_err(core)
    }
    async fn idempotent_response(
        &self,
        scope: &Scope,
        key: &str,
    ) -> CoreResult<Option<WriteMemoryResponse>> {
        self.check_scope(scope).map_err(core)?;
        response(self, key).map_err(core)
    }
    async fn commit_write(
        &self,
        key: Option<&str>,
        record: MemoryRecord,
        event: MemoryEvent,
    ) -> ObservedResult<WriteMemoryResponse> {
        let (result,observation)=self.observed_atomic(&event.actor.id.to_string(), &record.id.to_string(), |store| {
            store.check_scope(&record.scope)?;store.check_scope(&event.scope)?;
            if event.memory_id.as_ref()!=Some(&record.id) || event.kind!=MemoryEventKind::Written {
                return Err(Error::InvalidConfig("memory write event association mismatch"));
            }
            if let Some(key)=key {
                if let Some(mut prior)=response(store,key)? { prior.deduplicated=Some(true); return Ok(prior); }
            }
            if store.get::<Value>(MEMORY,&record.id.to_string())?.is_some()
                || store.get::<Value>(EVENT,&event.id.to_string())?.is_some() {
                return Err(Error::InvalidConfig("memory write identity already exists"));
            }
            store.put(MEMORY,&record)?;store.put(EVENT,&event)?;
            let written=WriteMemoryResponse {record,event,deduplicated:Some(false)};
            if let Some(key)=key {
                let id=response_id(store,key)?;
                store.put(RECEIPT,&json!({"id":id,"kind":"memory-write-response-v1","key":key,"response":written}))?;
            }
            Ok(written)
        });
        ObservedResult {
            result: result.map_err(core),
            observation,
        }
    }
    async fn remove(&self, id: &MemoryId, scope: &Scope) -> CoreResult<bool> {
        self.delete_memory_record(id, scope)
    }
}
