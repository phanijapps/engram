//! Shared embedded SurrealKV connection for every Surreal cell.
//!
//! All cells (memory, knowledge, belief, …) talk to ONE Surreal instance — one
//! namespace/database — so the connection is opened once and shared via
//! `Arc<SurrealConnection>`. The open is LAZY: the Surreal SDK needs a Tokio
//! reactor, and the facade's `bootstrap_surreal` is sync, so the connection is
//! established on the first async cell call (under the consumer's runtime), via
//! a `tokio::sync::OnceCell`.

use engram_runtime::{CoreError, CoreResult};
use surrealdb::Surreal;
use surrealdb::engine::local::{Db, SurrealKv};
use tokio::sync::OnceCell;

/// Shared, lazily-opened embedded SurrealKV connection.
///
/// Cheap to share — clone the `Arc<SurrealConnection>` into each cell; they all
/// resolve `db().await` to the same underlying `Surreal<Db>` after the first
/// open.
pub struct SurrealConnection {
    path: String,
    namespace: String,
    database: String,
    db: OnceCell<Surreal<Db>>,
}

impl SurrealConnection {
    /// Creates a connection handle that opens the embedded store at `path` on
    /// first use. No I/O happens here.
    pub fn new(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            namespace: "engram".to_owned(),
            database: "engram".to_owned(),
            db: OnceCell::new(),
        }
    }

    /// Returns the shared `Surreal<Db>`, opening + selecting ns/db on first
    /// call. Runs under the caller's Tokio runtime.
    pub async fn db(&self) -> CoreResult<&Surreal<Db>> {
        let db = self
            .db
            .get_or_try_init(|| async {
                let db = Surreal::new::<SurrealKv>(&self.path)
                    .await
                    .map_err(surreal_err)?;
                db.use_ns(&self.namespace)
                    .use_db(&self.database)
                    .await
                    .map_err(surreal_err)?;
                Ok::<_, CoreError>(db)
            })
            .await?;
        Ok(db)
    }
}

fn surreal_err(error: surrealdb::Error) -> CoreError {
    CoreError::Adapter {
        adapter: "surreal.connection".to_owned(),
        message: error.to_string(),
    }
}
