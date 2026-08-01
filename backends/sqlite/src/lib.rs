//! SQLite backend recipe (ADR-0022).
//!
//! The **SQLite host entry**: opens a file-backed (or in-memory) SQLite provider
//! from a config. This recipe is the symmetric twin of `backends/pgvector` —
//! both engines expose `backends::<engine>::open(config)`.
//!
//! The SQLite adapter cells + bootstrap currently live in `engram-integration`
//! behind the `sqlite` feature (historical). This crate wraps
//! [`EngramProvider::open`] (which dispatches to `bootstrap_sqlite` for
//! non-pgvector configs) as the named recipe entry point. The full internal
//! extraction (moving the 15 files out of `core/integration/src/sqlite/` into
//! this crate + making `open` engine-neutral) is a larger follow-up — see
//! `docs/backlog.md#backends-sqlite-extraction`.

use engram_integration::{EngramConfig, EngramProvider};
use engram_runtime::CoreResult;

/// Opens a SQLite-backed [`EngramProvider`] from a config.
///
/// Delegates to [`EngramProvider::open`] (sqlite default). The config must NOT
/// carry a `pgvector_connection_string` (that routes to the pgvector recipe).
pub fn open(config: &EngramConfig) -> CoreResult<EngramProvider> {
    EngramProvider::open(config)
}
