//! pgvector (Postgres) backend recipe (ADR-0022).
//!
//! This crate is the **pgvector host entry**: it owns Postgres connection
//! lifecycle, schema application, adapter-cell composition, and per-engine
//! conformance — the only place a "pgvector" backend identity exists. Hosts open
//! a Postgres-backed [`EngramProvider`] via [`open`]; the SDK facade
//! (`engram-integration`) stays engine-neutral and does not route pgvector (an
//! `integration → backends` dependency would be a cycle, so the recipe — not
//! `EngramProvider::open` — is the entry point).

mod query;
mod recall;

use std::sync::Arc;

use engram_domain::{CapabilityState, EmbeddingSpace};
use engram_integration::{CapabilityReport, EngramConfig, EngramProvider, EngramProviderBuilder};
use engram_runtime::{CoreError, CoreResult};
use engram_store_pgvector::{
    PgBeliefStore, PgConnection, PgHierarchyStore, PgKnowledgeStore, PgMemoryService,
    PgProcedureStore, PgVectorIndex, schema,
};

use query::PgKnowledgeQuery;
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

    // The vector space: the config's embedding block by default; overridden
    // to the model's TRUE space when fastembed is compiled in (the index and
    // the embedder must agree on one space for insert/search to match).
    #[cfg_attr(not(feature = "fastembed"), allow(unused_variables))]
    let space = EmbeddingSpace::new(
        &config.embedding_provider.provider_type,
        &config.embedding_provider.model,
        dims,
        &config.embedding_provider.prompt_profile,
        config.embedding_provider.normalization.clone(),
    );
    // PS2: when fastembed is compiled in, the vector space is the model's
    // true space (BGE-small/384/query profile) — the config's
    // `embedding_provider` block may say "none" on a pg-only deployment, and
    // the index + embedder MUST agree on one space for insert/search to match.
    #[cfg(feature = "fastembed")]
    let space = EmbeddingSpace::new(
        "fastembed",
        "BAAI/bge-small-en-v1.5",
        384,
        "query",
        None::<&str>,
    );
    let vectors = Arc::new(PgVectorIndex::new(mk_conn()?, space.clone()));

    // PS2: the vector recall lane needs a query-vector provider. Wired only
    // when the `fastembed` feature is on AND the model loads — absent
    // embedder ⇒ degraded recall (facts + beliefs), never a boot failure.
    // The embedder is also exposed on the provider so the MCP scan path can
    // embed chunks into PgVectorIndex (scan_repo's incremental embed step
    // requires `embedding_provider()` + `require_vectors()`).
    let embedding_provider: Option<Arc<dyn engram_integration::EmbeddingProvider>> = None;
    #[cfg(feature = "fastembed")]
    {
        // Model-load failure DEGRADES (no embedder ⇒ facts+beliefs recall, the
        // capability report shows vector unsupported) — a missing model cache
        // must never take a production deployment's boot down.
        match engram_store_sqlite::FastEmbedBgeSmallQueryProvider::new() {
            Ok(inner) => {
                let embedder: Arc<dyn engram_integration::EmbeddingProvider> =
                    Arc::new(engram_integration::FastEmbedEmbeddingProvider::new(
                        std::sync::Arc::new(inner),
                        space.clone(),
                    ));
                embedding_provider = Some(embedder);
            }
            Err(e) => {
                eprintln!(
                    "engram-backend-pgvector: fastembed model unavailable — vector lane disabled (degraded recall): {e}"
                );
            }
        }
    }

    // Unified recall: composes memory.retrieve + beliefs + the vector lane
    // (PS2) via RRF. Lexical (tsvector) is a documented gap on this engine.
    let recall = Arc::new(PgUnifiedRecall {
        memory: memory.clone(),
        beliefs: beliefs.clone(),
        knowledge: knowledge.clone(),
        vectors: Some(vectors.clone()),
        embedder: embedding_provider.clone(),
    });

    // Knowledge query: the read surface behind the MCP code-intel tools
    // (search / symbol_context / architecture / whats_changed / the scan's
    // lexical delta feed + embed listing). Without this handle those tools
    // fail on `knowledge_query not wired` even though the cells are healthy.
    let knowledge_query = Arc::new(PgKnowledgeQuery::new(mk_conn()?));

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

    let mut provider_builder = EngramProviderBuilder::new(report)
        .memory(memory)
        .knowledge(knowledge.clone())
        .graph(knowledge)
        .beliefs(beliefs)
        .hierarchy(hierarchy)
        .procedures(procedures)
        .vectors(vectors)
        .knowledge_query(knowledge_query)
        .recall(recall);
    if let Some(embedder) = embedding_provider {
        provider_builder = provider_builder.embedding_provider(embedder);
    }
    let provider = provider_builder;

    Ok(provider.build())
}
