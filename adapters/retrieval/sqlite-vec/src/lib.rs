//! Shim: the vector code has folded into `engram-store-sqlite` (consolidation
//! T5). Re-exports it so existing `engram_store_vector::*` consumers keep
//! compiling until re-pointed (T7) and this crate is deleted (T8).
pub use engram_store_sqlite::*;
