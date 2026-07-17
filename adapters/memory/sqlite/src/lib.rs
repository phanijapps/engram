//! Shim: the memory code has folded into `engram-store-sqlite` (consolidation
//! T1). This crate now only re-exports it so existing `engram_store_sql::*`
//! consumers keep compiling until they are re-pointed (T7) and this crate is
//! deleted (T8).
pub use engram_store_sqlite::*;
