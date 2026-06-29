//! Process-local state for the in-memory adapter.
//!
//! The state shape is private to this crate. Public callers interact through
//! core repository and service traits, which keeps future SQL or vector adapters
//! free to choose different internal tables and indexes.

use std::collections::BTreeMap;

use engram_domain::{MemoryEvent, MemoryRecord, WriteMemoryResponse};

#[derive(Debug, Default)]
pub(crate) struct InMemoryState {
    pub(crate) memories: BTreeMap<String, MemoryRecord>,
    pub(crate) events: Vec<MemoryEvent>,
    pub(crate) idempotency: BTreeMap<String, WriteMemoryResponse>,
}
