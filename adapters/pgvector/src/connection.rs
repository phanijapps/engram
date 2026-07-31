//! Shared tokio runtime + Postgres connection management.
//!
//! The engram codebase drives async via `futures::executor::block_on`, not a
//! tokio runtime. tokio-postgres needs a tokio reactor for its I/O, so this
//! module provides a shared `tokio::runtime::Runtime` (created once, reused
//! for all Postgres calls) and a thin connection wrapper.

use std::sync::OnceLock;

use tokio::runtime::Runtime;
use tokio_postgres::NoTls;

/// Returns the shared tokio runtime (lazily initialized).
pub fn runtime() -> &'static Runtime {
    static RUNTIME: OnceLock<Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| Runtime::new().expect("failed to create tokio runtime"))
}

/// Connects to Postgres + returns a `(Client, ConnectionHandle)`. The handle
/// drives the connection's I/O on a background thread.
pub struct PgConnection {
    pub client: tokio_postgres::Client,
}

impl PgConnection {
    /// Connects using a libpq-style connection string.
    pub fn connect(connection_string: &str) -> Result<Self, String> {
        runtime().block_on(async {
            let (client, connection) = tokio_postgres::connect(connection_string, NoTls)
                .await
                .map_err(|e| e.to_string())?;
            // Drive the connection's I/O on a background thread.
            tokio::spawn(async move {
                let _ = connection.await;
            });
            Ok(Self { client })
        })
    }

    /// Runs an async closure on the shared runtime, blocking the caller.
    pub fn block_on<F, T>(&self, f: F) -> T
    where
        F: std::future::Future<Output = T> + Send,
        T: Send,
    {
        runtime().block_on(f)
    }
}
