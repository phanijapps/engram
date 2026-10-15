//! Schema and durable embedding identity for vector namespaces.
use super::{index::sql_error, namespace::VectorNamespace};
use engram_domain::EmbeddingSpace;
use engram_runtime::{CoreError, CoreResult};
use rusqlite::{Connection, OptionalExtension, params};

/// Creates the `vectors` vec0 virtual table if it does not already exist.
///
/// Shared by the in-memory and file-backed constructors so the schema stays in
/// one place. `IF NOT EXISTS` makes reopen idempotent for the persistent path.
pub(super) fn create_vectors_table(
    connection: &Connection,
    dimensions: u32,
    namespace: &VectorNamespace,
) -> CoreResult<()> {
    connection
        .execute(
            &namespace.sql(&format!(
                r#"
                CREATE VIRTUAL TABLE IF NOT EXISTS vectors USING vec0(
                    id text primary key,
                    embedding float[{dimensions}],
                    target_type text,
                    target_id text,
                    model text,
                    dimensions integer,
                    content_hash text
                )
                "#
            )),
            [],
        )
        .map_err(sql_error)?;
    // Metadata table records the embedding space the index was built with, so
    // a later reopen with a different configured space can signal RequiresReindex.
    connection
        .execute(
            &namespace.sql(
                r#"
            CREATE TABLE IF NOT EXISTS vector_meta (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            )
            "#,
            ),
            [],
        )
        .map_err(sql_error)?;
    Ok(())
}

pub(super) fn resolve_meta_space(
    connection: &Connection,
    requested: &EmbeddingSpace,
    namespace: &VectorNamespace,
) -> CoreResult<(EmbeddingSpace, bool)> {
    if namespace.is_legacy() {
        return Ok(resolve_legacy_space(connection, requested));
    }
    let read = || {
        connection
            .query_row(
                &namespace.sql("SELECT value FROM vector_meta WHERE key = 'embedding_space_v1'"),
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(sql_error)
    };
    if read()?.is_none() {
        let occupied: bool = connection
            .query_row(
                &namespace.sql("SELECT EXISTS(SELECT 1 FROM vectors_rowids LIMIT 1)"),
                [],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        if occupied {
            return Err(CoreError::ProviderUnavailable {
                provider:
                    "vector namespace has vectors without an embedding identity; rebuild required"
                        .into(),
            });
        }
        let identity = serde_json::to_string(requested).map_err(|e| CoreError::Adapter {
            adapter: "engram-store-vector".into(),
            message: e.to_string(),
        })?;
        connection.execute(
            &namespace.sql("INSERT OR IGNORE INTO vector_meta (key, value) VALUES ('embedding_space_v1', ?1)"),
            [identity],
        ).map_err(sql_error)?;
    }
    let identity = read()?.ok_or_else(|| CoreError::ProviderUnavailable {
        provider: "vector namespace embedding identity missing".into(),
    })?;
    let persisted: EmbeddingSpace =
        serde_json::from_str(&identity).map_err(|e| CoreError::Adapter {
            adapter: "engram-store-vector".into(),
            message: format!("invalid embedding identity: {e}"),
        })?;
    let incompatible = &persisted != requested;
    Ok((persisted, incompatible))
}

fn resolve_legacy_space(
    connection: &Connection,
    requested: &EmbeddingSpace,
) -> (EmbeddingSpace, bool) {
    let get = |key: &str| -> Option<String> {
        connection
            .query_row(
                "SELECT value FROM vector_meta WHERE key = ?1",
                params![key],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .ok()
            .flatten()
    };
    let set = |key: &str, value: &str| {
        let _ = connection.execute(
            "INSERT OR REPLACE INTO vector_meta (key, value) VALUES (?1, ?2)",
            params![key, value],
        );
    };

    match get("provider") {
        Some(provider) => {
            let model = get("model").unwrap_or_default();
            let dims = get("dimensions")
                .and_then(|s| s.parse::<u32>().ok())
                .unwrap_or(requested.dimensions);
            let profile = get("prompt_profile").unwrap_or_default();
            let normalization = get("normalization");
            let persisted = EmbeddingSpace::new(provider, model, dims, profile, normalization);
            let requires_reindex = &persisted != requested;
            (persisted, requires_reindex)
        }
        None => {
            set("provider", &requested.provider);
            set("model", &requested.model);
            set("dimensions", &requested.dimensions.to_string());
            set("prompt_profile", &requested.prompt_profile);
            if let Some(norm) = &requested.normalization {
                set("normalization", norm);
            }
            (requested.clone(), false)
        }
    }
}
