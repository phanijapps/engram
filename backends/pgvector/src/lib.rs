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

use engram_domain::{CapabilityReason, CapabilityState};
use engram_integration::{
    CapabilityReport, EngramConfig, EngramProvider, EngramProviderBuilder, prepare_embedding,
};
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
    if config.rebuild_vectors {
        return Err(CoreError::InvalidRequest {
            reason: "rebuild_vectors is not supported by the pgvector recipe; no data was reset"
                .into(),
        });
    }
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

    // Resolve inference output before creating a dimensioned vector column.
    // Disabled vector mode never initializes a model or contacts its endpoint.
    let prepared = prepare_embedding(&config.embedding_provider, config.enable_vector)?;
    let embedding_provider = prepared.provider;
    let ddl = if config.enable_vector {
        schema::schema_sql(prepared.config.dimensions)
    } else {
        schema::schema_sql_without_vectors()
    };

    // Connect + apply schema (idempotent).
    let schema_conn = PgConnection::connect(conn_str).map_err(pg_err)?;
    schema_conn
        .block_on(async { schema_conn.client.batch_execute(&ddl).await })
        .map_err(|e| pg_err(e.to_string()))?;

    // Construct cells (each gets its own connection; a pool follows).
    let mk_conn = || PgConnection::connect(conn_str).map_err(pg_err);

    let knowledge = Arc::new(PgKnowledgeStore::new(mk_conn()?));
    let memory = Arc::new(PgMemoryService::new(mk_conn()?));
    let beliefs = Arc::new(PgBeliefStore::new(mk_conn()?));
    let hierarchy = Arc::new(PgHierarchyStore::new(mk_conn()?));
    let procedures = Arc::new(PgProcedureStore::new(mk_conn()?));

    let vectors = if config.enable_vector {
        Some(Arc::new(PgVectorIndex::new_checked(
            mk_conn()?,
            prepared.config.embedding_space(),
        )?))
    } else {
        None
    };

    // Scoped chunk listing (engine-neutral KnowledgeQuery port) — constructed
    // before the recall so the bound vector lane can resolve its eligible
    // target set.
    let knowledge_query = Arc::new(PgKnowledgeQuery::new(mk_conn()?));

    // Unified recall: composes memory.retrieve + beliefs + the vector lane
    // (PS2) via RRF. Lexical (tsvector) is a documented gap on this engine.
    // Bound-code eligibility stays unwired here until the recipe wires the
    // binding repository (TASK-019 owns backends/* wiring); the constrained
    // lane itself is implemented + proven in recall.rs.
    let recall = Arc::new(PgUnifiedRecall {
        memory: memory.clone(),
        beliefs: beliefs.clone(),
        knowledge: knowledge.clone(),
        vectors: vectors.clone(),
        embedder: embedding_provider.clone(),
        code_eligibility: None,
        knowledge_query: Some(knowledge_query.clone()),
        // Canonical repository evidence for degraded code-authority handling:
        // shares the memory service's OWNING authorizer. The durable code
        // ownership reader stays unconfigured here — full default code-catalog
        // activation is Task018+, so entity evidence honestly returns
        // Unknown rather than inventing a catalog.
        candidate_evidence: Some(Arc::new(
            engram_integration::candidate_evidence::RepositoryCandidateEvidence::new(
                memory.clone() as Arc<dyn engram_memory::MemoryRepository>,
                beliefs.clone() as Arc<dyn engram_belief::BeliefRepository>,
                knowledge.clone() as Arc<dyn engram_knowledge::KnowledgeRepository>,
                memory.retrieval_authorizer(),
                None,
            ),
        )),
    });

    // Knowledge query: the read surface behind the MCP code-intel tools
    // (search / symbol_context / architecture / whats_changed / the scan's
    // lexical delta feed + embed listing). Without this handle those tools
    // fail on `knowledge_query not wired` even though the cells are healthy.
    // (Constructed above, before the bound recall.)

    let report = CapabilityReport::builder()
        .memory(CapabilityState::Supported)
        .knowledge(CapabilityState::Supported)
        .graph(CapabilityState::Supported)
        .beliefs(CapabilityState::Supported)
        .contradiction(CapabilityState::Supported)
        .hierarchy(CapabilityState::Supported)
        .procedures(CapabilityState::Supported)
        .vectors(if vectors.is_some() {
            CapabilityState::Supported
        } else {
            CapabilityState::Unsupported {
                reason: CapabilityReason::FeatureDisabled,
            }
        })
        .unified_recall(CapabilityState::Supported)
        .build();

    let mut provider_builder = EngramProviderBuilder::new(report)
        .memory(memory)
        .knowledge(knowledge.clone())
        .graph(knowledge)
        .beliefs(beliefs)
        .hierarchy(hierarchy)
        .procedures(procedures)
        .knowledge_query(knowledge_query)
        .recall(recall);
    if let Some(vectors) = vectors {
        provider_builder = provider_builder.vectors(vectors);
    }
    if let Some(embedder) = embedding_provider {
        provider_builder = provider_builder.embedding_provider(embedder);
    }
    let provider = provider_builder;

    Ok(provider.build())
}
