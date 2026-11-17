//! Memory persistence; lifecycle orchestration remains in the canonical service.
use crate::{HelixStore, records::*};
use async_trait::async_trait;
use engram_domain::*;
use engram_memory::{MemoryEventRepository, MemoryRepository};
use engram_runtime::{CoreError, CoreResult};
#[async_trait]
impl MemoryRepository for HelixStore {
    async fn put_memory(&self, record: MemoryRecord) -> CoreResult<MemoryRecord> {
        self.check_scope(&record.scope).map_err(core)?;
        self.put(MEMORY, &record).map_err(core)
    }
    async fn get_memory(&self, id: &MemoryId, scope: &Scope) -> CoreResult<Option<MemoryRecord>> {
        if !self.visible_scope(scope) {
            return Ok(None);
        }
        self.get(MEMORY, &id.to_string()).map_err(core)
    }
    async fn append_event(&self, event: MemoryEvent) -> CoreResult<MemoryEvent> {
        self.check_scope(&event.scope).map_err(core)?;
        self.atomic(|s| {
            if let Some(existing) = s.get::<MemoryEvent>(EVENT, &event.id.to_string())? {
                if existing != event {
                    return Err(crate::error::Error::InvalidConfig(
                        "event identity is immutable",
                    ));
                }
                return Ok(existing);
            }
            s.put(EVENT, &event)
        })
        .map_err(core)
    }
    async fn update_memory_status(
        &self,
        id: &MemoryId,
        scope: &Scope,
        status: MemoryStatus,
    ) -> CoreResult<MemoryRecord> {
        if !self.visible_scope(scope) {
            return Err(missing("memory", id));
        }
        self.atomic(|s| {
            let Some(mut record) = s.get::<MemoryRecord>(MEMORY, &id.to_string())? else {
                return Ok(None);
            };
            record.status = status;
            record.updated_at = Some(chrono::Utc::now());
            Ok(Some(s.put(MEMORY, &record)?))
        })
        .map_err(core)?
        .ok_or_else(|| missing("memory", id))
    }
    async fn list_memories_paged(
        &self,
        scope: &Scope,
        after: Option<&Cursor>,
        limit: usize,
    ) -> CoreResult<Page<MemoryRecord>> {
        if limit == 0 || limit > 1000 {
            return Err(CoreError::InvalidRequest {
                reason: "memory page limit must be 1..=1000".into(),
            });
        }
        if !self.visible_scope(scope) {
            return Ok(Page::last(vec![]));
        }
        let prefix = format!("{}:", self.inner.partition.discriminator().map_err(core)?);
        let after = after
            .map(|c| {
                c.as_str()
                    .strip_prefix(&prefix)
                    .ok_or(CoreError::InvalidRequest {
                        reason: "cursor belongs to another partition".into(),
                    })
            })
            .transpose()?;
        let mut rows = self
            .scan_window(MEMORY, None, after, Some((0, limit + 1)))
            .map_err(core)?
            .into_iter()
            .map(decode::<MemoryRecord>)
            .collect::<crate::error::Result<Vec<_>>>()
            .map_err(core)?;
        let more = rows.len() > limit;
        rows.truncate(limit);
        let next = if more {
            rows.last()
                .map(|r| Cursor::new(format!("{prefix}{}", r.id)))
        } else {
            None
        };
        Ok(Page::new(rows, next))
    }
}
#[async_trait]
impl MemoryEventRepository for HelixStore {
    async fn get_event(&self, id: &EventId, scope: &Scope) -> CoreResult<Option<MemoryEvent>> {
        if !self.visible_scope(scope) {
            return Ok(None);
        }
        self.get(EVENT, &id.to_string()).map_err(core)
    }
    async fn list_events_for_memory(
        &self,
        id: &MemoryId,
        scope: &Scope,
    ) -> CoreResult<Vec<MemoryEvent>> {
        if !self.visible_scope(scope) {
            return Ok(vec![]);
        }
        let mut rows = self
            .filtered::<MemoryEvent>(EVENT, "memoryId", &id.to_string())
            .map_err(core)?;
        rows.sort_by_key(|e| (e.recorded_at, e.id.to_string()));
        Ok(rows)
    }
    async fn list_events_for_scope(&self, scope: &Scope) -> CoreResult<Vec<MemoryEvent>> {
        if !self.visible_scope(scope) {
            return Ok(vec![]);
        }
        let mut rows = self.all::<MemoryEvent>(EVENT).map_err(core)?;
        rows.sort_by_key(|e| (e.recorded_at, e.id.to_string()));
        Ok(rows)
    }
}

impl HelixStore {
    /// Hard-delete only the governed memory record; retained lifecycle events remain immutable.
    pub fn delete_memory_record(&self, id: &MemoryId, scope: &Scope) -> CoreResult<bool> {
        if !self.visible_scope(scope) {
            return Ok(false);
        }
        self.remove(MEMORY, &id.to_string()).map_err(core)
    }
}
