//! Shim: the hierarchy code has folded into `engram-store-sqlite` (consolidation
//! T4). Re-exports it so existing `engram_store_hierarchy_sqlite::*` consumers
//! keep compiling until re-pointed (T7) and this crate is deleted (T8).
pub use engram_store_sqlite::*;
