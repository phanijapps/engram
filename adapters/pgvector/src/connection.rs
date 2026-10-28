//! Shared tokio runtime + Postgres connection management.
//!
//! The engram codebase drives async via `futures::executor::block_on`, not a
//! tokio runtime. tokio-postgres needs a tokio reactor for its I/O, so this
//! module provides a shared `tokio::runtime::Runtime` (created once, reused
//! for all Postgres calls) and a thin connection wrapper.

use std::{error::Error, io, sync::OnceLock};

use tokio::runtime::Runtime;
use tokio_postgres::NoTls;

/// Returns the shared tokio runtime (lazily initialized).
pub fn runtime() -> &'static Runtime {
    static RUNTIME: OnceLock<Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| Runtime::new().expect("failed to create tokio runtime"))
}

fn sanitized_connect_error(error: &tokio_postgres::Error) -> String {
    if let Some(database) = error.as_db_error() {
        return format!(
            "postgres connection rejected (sqlstate={})",
            database.code().code()
        );
    }
    if let Some(io) = error
        .source()
        .and_then(|source| source.downcast_ref::<io::Error>())
    {
        if let Some(code) = io.raw_os_error() {
            return format!("postgres transport failed (os_error={code})");
        }
    }
    "postgres connection failed (stage=connect)".into()
}

/// Connects to Postgres + returns a `(Client, ConnectionHandle)`. The handle
/// drives the connection's I/O on a background thread.
pub struct PgConnection {
    pub client: tokio_postgres::Client,
}

impl PgConnection {
    /// Connects using a libpq-style connection string.
    ///
    /// Failure text intentionally excludes the supplied URL and all server-provided detail.
    pub fn connect(connection_string: &str) -> Result<Self, String> {
        if !connection_string.starts_with("postgres://")
            && !connection_string.starts_with("postgresql://")
        {
            return Err("invalid pgvector connection string (expected PostgreSQL URL)".into());
        }
        runtime().block_on(async {
            let (client, connection) = tokio_postgres::connect(connection_string, NoTls)
                .await
                .map_err(|error| sanitized_connect_error(&error))?;
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

#[cfg(test)]
mod tests {
    use super::PgConnection;

    #[test]
    fn invalid_prefix_never_echoes_credentials() {
        let error = match PgConnection::connect("mysql://credential-sentinel@host/private-db") {
            Ok(_) => panic!("non-PostgreSQL URL accepts"),
            Err(error) => error,
        };
        assert_eq!(
            error,
            "invalid pgvector connection string (expected PostgreSQL URL)"
        );
        assert!(!error.contains("credential-sentinel"));
        assert!(!error.contains("private-db"));
    }

    #[test]
    fn failed_connect_never_echoes_url_or_credentials() {
        let error = match PgConnection::connect(
            "postgresql://credential-sentinel@127.0.0.1:1/private-db",
        ) {
            Ok(_) => panic!("unreachable loopback port accepts"),
            Err(error) => error,
        };
        assert!(!error.contains("credential-sentinel"));
        assert!(!error.contains("private-db"));
        assert!(!error.contains("postgresql://"));
    }
}
