//! Postgres (pgvector) backend bootstrap.
//!
//! Constructs a wired [`EngramProvider`] from a Postgres connection string.
//! P0 cells: knowledge/graph + vector. Memory + belief + hierarchy +
//! procedures are feature-disabled (default Unsupported) — they attach in
//! follow-on cells.
//!
//! Conformance: the schema application is the gate — if Postgres is reachable
//! and the DDL applies (tables + pgvector extension + HNSW index), the cells
//! are wired. If the connection or schema fails, the bootstrap errors. This
//! mirrors the SQLite pattern (where in-memory probes test each cell) but is
//! appropriate for Postgres (there is no in-memory Postgres; a reachable +
//! schema'd instance is the precondition).
//!
//! ADR-0022: engine-specific zone — exempt from the neutrality gate.

use std::sync::Arc;

use engram_domain::{CapabilityState, EmbeddingSpace};
use engram_runtime::{CoreError, CoreResult};
use engram_store_pgvector::{PgConnection, PgKnowledgeStore, PgVectorIndex, schema};

use crate::{CapabilityReport, EngramConfig, EngramProvider, EngramProviderBuilder};

/// Bootstraps a Postgres-backed provider from the config's
/// `pgvector_connection_string`. Applies the schema, constructs the
/// knowledge/graph + vector cells, and returns a wired `EngramProvider`.
pub fn bootstrap_pgvector(config: &EngramConfig) -> CoreResult<EngramProvider> {
    let conn_str = config
        .pgvector_connection_string
        .as_deref()
        .ok_or_else(|| CoreError::InvalidRequest {
            reason: "pgvector_connection_string is required for the pgvector backend".to_owned(),
        })?;

    let pg_err = |e: String| CoreError::Adapter {
        adapter: "engram-store-pgvector".to_owned(),
        message: e,
    };

    // Connect + apply schema (idempotent). This is the conformance gate:
    // if Postgres is unreachable or the DDL fails, the bootstrap errors.
    let schema_conn = PgConnection::connect(conn_str).map_err(pg_err)?;
    let dims = config.embedding_provider.dimensions;
    schema_conn
        .block_on(async {
            schema_conn
                .client
                .batch_execute(&schema::schema_sql(dims))
                .await
        })
        .map_err(|e| pg_err(e.to_string()))?;

    // Knowledge + graph cell (PgKnowledgeStore implements both traits).
    let k_conn = PgConnection::connect(conn_str).map_err(pg_err)?;
    let knowledge = Arc::new(PgKnowledgeStore::new(k_conn));

    // Vector cell.
    let space = EmbeddingSpace::new(
        &config.embedding_provider.provider_type,
        &config.embedding_provider.model,
        dims,
        &config.embedding_provider.prompt_profile,
        config.embedding_provider.normalization.clone(),
    );
    let v_conn = PgConnection::connect(conn_str).map_err(pg_err)?;
    let vectors = Arc::new(PgVectorIndex::new(v_conn, space));

    // Build a capability report marking the P0 cells Supported.
    let report = CapabilityReport::builder()
        .knowledge(CapabilityState::Supported)
        .graph(CapabilityState::Supported)
        .vectors(CapabilityState::Supported)
        .build();

    // Wire the cells.
    let provider = EngramProviderBuilder::new(report)
        .knowledge(knowledge.clone())
        .graph(knowledge)
        .vectors(vectors)
        .build();

    Ok(provider)
}
