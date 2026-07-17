//! Shim: the knowledge code has folded into `engram-store-sqlite` (consolidation
//! T2). Re-exports it so existing `engram_store_knowledge_sqlite::*` consumers
//! keep compiling until re-pointed (T7) and this crate is deleted (T8).
pub use engram_store_sqlite::*;
