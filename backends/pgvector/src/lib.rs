//! pgvector (Postgres) backend recipe (ADR-0022).
//!
//! This crate is the **pgvector host entry**: it owns Postgres connection
//! lifecycle, schema application, adapter-cell composition, and per-engine
//! conformance — the only place a "pgvector" backend identity exists. Hosts open
//! a Postgres-backed [`EngramProvider`] via [`open`]; the SDK facade
//! (`engram-integration`) stays engine-neutral and does not route pgvector (an
//! `integration → backends` dependency would be a cycle, so the recipe — not
//! `EngramProvider::open` — is the entry point).

mod recall;

use std::sync::Arc;

use engram_domain::{CapabilityState, EmbeddingSpace};
use engram_integration::{CapabilityReport, EngramConfig, EngramProvider, EngramProviderBuilder};
use engram_runtime::{CoreError, CoreResult};
use engram_store_pgvector::{
    PgBeliefStore, PgConnection, PgHierarchyStore, PgKnowledgeStore, PgMemoryService,
    PgProcedureStore, PgVectorIndex, schema,
};

use recall::PgUnifiedRecall;

/// Opens a Postgres (pgvector)-backed [`EngramProvider`] from a config carrying
/// a `pgvector_connection_string`.
///
/// Connects, applies the schema (idempotent), and composes the adapter cells —
/// memory / knowledge+graph / beliefs / hierarchy / procedures / vectors /
/// unified-recall — into one provider.
pub fn open(config: &EngramConfig) -> CoreResult<EngramProvider> {
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

    // Connect + apply schema (idempotent).
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

    // Construct cells (each gets its own connection; a pool follows).
    let mk_conn = || PgConnection::connect(conn_str).map_err(pg_err);

    let knowledge = Arc::new(PgKnowledgeStore::new(mk_conn()?));
    let memory = Arc::new(PgMemoryService::new(mk_conn()?));
    let beliefs = Arc::new(PgBeliefStore::new(mk_conn()?));
    let hierarchy = Arc::new(PgHierarchyStore::new(mk_conn()?));
    let procedures = Arc::new(PgProcedureStore::new(mk_conn()?));

    let space = EmbeddingSpace::new(
        &config.embedding_provider.provider_type,
        &config.embedding_provider.model,
        dims,
        &config.embedding_provider.prompt_profile,
        config.embedding_provider.normalization.clone(),
    );
    let vectors = Arc::new(PgVectorIndex::new(mk_conn()?, space));

    // Unified recall: composes memory.retrieve + beliefs via RRF.
    let recall = Arc::new(PgUnifiedRecall {
        memory: memory.clone(),
        beliefs: beliefs.clone(),
    });

    let report = CapabilityReport::builder()
        .memory(CapabilityState::Supported)
        .knowledge(CapabilityState::Supported)
        .graph(CapabilityState::Supported)
        .beliefs(CapabilityState::Supported)
        .contradiction(CapabilityState::Supported)
        .hierarchy(CapabilityState::Supported)
        .procedures(CapabilityState::Supported)
        .vectors(CapabilityState::Supported)
        .unified_recall(CapabilityState::Supported)
        .build();

    let provider = EngramProviderBuilder::new(report)
        .memory(memory)
        .knowledge(knowledge.clone())
        .graph(knowledge)
        .beliefs(beliefs)
        .hierarchy(hierarchy)
        .procedures(procedures)
        .vectors(vectors)
        .recall(recall);

    Ok(provider.build())
}
