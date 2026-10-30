//! Recipe-level integration test for the pgvector backend.
//!
//! Opens a provider via the `backends/pgvector` recipe (`open`) against the
//! Docker pgvector instance + verifies the hot-path capabilities (knowledge +
//! graph + vectors) are wired, and a knowledge write/read round-trips. This is
//! the recipe-moved twin of the old `core/integration/tests/pgvector_bootstrap`
//! — the only stranded host from removing pgvector from the SDK facade.
//!
//! Requires: docker compose -f docs/how-to-pg/docker-compose.yaml up -d
//! Run: cargo test -p engram-backend-pgvector -- --ignored pg

use engram_backend_pgvector::open;
use engram_domain::ScopeMappingStrategy;
use engram_integration::{
    CapabilityPolicy, EmbeddingProviderConfig, EngramConfig, EngramProvider, MigrationMode,
};
use engram_store_pgvector::PgConnection;
use futures::executor::block_on;
use std::path::PathBuf;

/// Run-unique scopes and identifiers prevent durable shared-fixture state from
/// changing any bootstrap assertion on a later invocation.
fn unique_suffix() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    format!("{}-{nanos}", std::process::id())
}

fn test_scope(workspace: &str) -> engram_domain::Scope {
    engram_domain::Scope {
        tenant: format!("pgvector-test-{}", unique_suffix()),
        subject: None,
        workspace: Some(workspace.to_owned()),
        session: None,
        environment: Some("test".to_owned()),
    }
}

fn pg_url() -> String {
    std::env::var("ENGRAM_TEST_PG_URL")
        .unwrap_or_else(|_| "postgres://engram:engram@localhost:5432/engram".to_owned())
}

const DISPOSABLE_FIXTURE_MARKER: &str =
    "engram.factory.job=010-project-bound-branch-indexing; disposable=true";

#[derive(Debug)]
enum ResetVectorStoreRefusal {
    ConnectionUnavailable,
    MarkerQueryFailed,
    DisposableMarkerMismatch,
    ResetFailed,
}

/// Drop the vector data and its durable embedding-space identity before every
/// recipe open, so prior ignored tests cannot constrain this test's model or
/// vector dimensions.
///
/// The exact database marker is checked before any destructive SQL executes.
fn reset_vector_store() -> Result<(), ResetVectorStoreRefusal> {
    let connection = PgConnection::connect(&pg_url())
        .map_err(|_| ResetVectorStoreRefusal::ConnectionUnavailable)?;
    let marker = connection
        .block_on(async {
            connection
                .client
                .query_one(
                    "SELECT shobj_description(oid,'pg_database') FROM pg_database WHERE datname=current_database()",
                    &[],
                )
                .await
        })
        .map_err(|_| ResetVectorStoreRefusal::MarkerQueryFailed)?
        .get::<_, Option<String>>(0);
    if marker.as_deref() != Some(DISPOSABLE_FIXTURE_MARKER) {
        return Err(ResetVectorStoreRefusal::DisposableMarkerMismatch);
    }
    connection
        .block_on(connection.client.batch_execute(
            "DROP TABLE IF EXISTS vectors; DROP TABLE IF EXISTS vector_embedding_space;",
        ))
        .map_err(|_| ResetVectorStoreRefusal::ResetFailed)
}

fn pg_config() -> EngramConfig {
    EngramConfig::new(
        PathBuf::from("/tmp/engram-pgvector-test"),
        PathBuf::from("/tmp"),
        ScopeMappingStrategy::Strict,
        EmbeddingProviderConfig {
            provider_type: "fastembed".to_owned(),
            model: "BGE-small-en-v1.5".to_owned(),
            dimensions: 384,
            prompt_profile: "passage".to_owned(),
            normalization: Some("l2".to_owned()),
            ..Default::default()
        },
        MigrationMode::DryRun,
        CapabilityPolicy::FailClosed,
    )
    .with_pgvector(pg_url())
}

fn open_bootstrap_recipe() -> EngramProvider {
    reset_vector_store().expect("typed refusal protects unmarked databases from vector reset");
    open(&pg_config()).expect("recipe opens against reset Docker pgvector")
}

#[test]
#[ignore]
fn pg_recipe_opens_and_reports_capabilities() {
    use engram_domain::CapabilityState;

    let provider = open_bootstrap_recipe();

    // Hot-path capabilities must be wired.
    let caps = provider.capabilities();
    assert_eq!(caps.memory, CapabilityState::Supported, "memory Supported");
    assert_eq!(
        caps.knowledge,
        CapabilityState::Supported,
        "knowledge Supported"
    );
    assert_eq!(caps.graph, CapabilityState::Supported, "graph Supported");
    assert_eq!(
        caps.vectors,
        CapabilityState::Supported,
        "vectors Supported"
    );

    println!("pgvector recipe: provider opens, memory + knowledge + graph + vectors Supported ✓");
}

#[test]
#[ignore]
fn pg_recipe_knowledge_write_read_round_trip() {
    use engram_domain::*;

    let provider = open_bootstrap_recipe();
    let repo = provider.require_knowledge().expect("knowledge handle");

    let scope = test_scope("entity-round-trip");
    let entity_id = Id::from(format!("pg-rt-entity-{}", unique_suffix()));
    let entity = KnowledgeEntity {
        id: entity_id.clone(),
        graph_id: None,
        kind: EntityKind::Concept,
        name: "pgvector-round-trip".to_owned(),
        aliases: Vec::new(),
        scope: scope.clone(),
        source_refs: Vec::new(),
        concept_refs: Vec::new(),
        ontology_class_refs: Vec::new(),
        provenance: Provenance {
            source: "test".to_owned(),
            actor: Actor {
                id: Id::from("test"),
                kind: ActorKind::System,
                display_name: None,
                metadata: None,
            },
            observed_at: chrono::Utc::now(),
            evidence: Vec::new(),
            derivations: Vec::new(),
            confidence: Some(1.0),
            method: None,
        },
        created_at: chrono::Utc::now(),
        updated_at: None,
        valid_from: None,
        valid_until: None,
        metadata: None,

        archived_at: None,
    };

    block_on(repo.put_entity(entity)).expect("put_entity");

    // Read back — proves the entity persisted (get_entity now works after the
    // trait-default fix).
    let read_back = block_on(repo.get_entity(&entity_id, &scope)).expect("get_entity");
    assert!(read_back.is_some(), "entity must persist after put_entity");
    assert_eq!(
        read_back.unwrap().name,
        "pgvector-round-trip",
        "read-back entity matches"
    );

    block_on(repo.delete_entity(&entity_id, &scope)).expect("delete_entity");

    println!("pgvector recipe knowledge round-trip: put → delete ✓");
}

#[test]
#[ignore]
fn pg_recipe_relationship_round_trip() {
    use engram_domain::*;

    let provider = open_bootstrap_recipe();
    let repo = provider.require_knowledge().expect("knowledge handle");

    let scope = test_scope("relationship-round-trip");
    let suffix = unique_suffix();
    let relationship_id = Id::from(format!("pg-rt-rel-{suffix}"));
    let source_id = Id::from(format!("pg-rt-src-{suffix}"));
    let destination_id = Id::from(format!("pg-rt-dst-{suffix}"));
    let rel = KnowledgeRelationship {
        id: relationship_id.clone(),
        graph_id: None,
        subject: EntityRef {
            id: Some(source_id),
            kind: None,
            name: None,
            aliases: Vec::new(),
        },
        predicate: "depends_on".to_owned(),
        code_resolution: None,
        object: EntityRef {
            id: Some(destination_id),
            kind: None,
            name: None,
            aliases: Vec::new(),
        },
        scope: scope.clone(),
        evidence: Vec::new(),
        confidence: Some(1.0),
        provenance: Provenance {
            source: "test".to_owned(),
            actor: Actor {
                id: Id::from("test"),
                kind: ActorKind::System,
                display_name: None,
                metadata: None,
            },
            observed_at: chrono::Utc::now(),
            evidence: Vec::new(),
            derivations: Vec::new(),
            confidence: Some(1.0),
            method: None,
        },
        created_at: chrono::Utc::now(),
        updated_at: None,

        archived_at: None,
    };

    block_on(repo.put_relationship(rel)).expect("put_relationship");
    let read = block_on(repo.get_relationship(&relationship_id, &scope)).expect("get_relationship");
    assert!(read.is_some(), "relationship must persist after put");
    block_on(repo.delete_relationship(&relationship_id, &scope)).expect("delete_relationship");

    println!("pgvector recipe relationship round-trip: put → get → delete ✓");
}

/// PS1: the recipe wires a `KnowledgeQuery` handle — the read surface the MCP
/// code-intel tools (`search`, `symbol_context`, `architecture`, the scan's
/// lexical delta feed + embed listing) fail without. Writes entities +
/// relationships through the knowledge port, reads them back through the
/// query handle, and checks scope matching.
#[test]
#[ignore]
fn pg_recipe_knowledge_query_lists_scope() {
    use engram_domain::*;

    let provider = open_bootstrap_recipe();
    let repo = provider.require_knowledge().expect("knowledge handle");
    let query = provider
        .require_knowledge_query()
        .expect("knowledge_query handle wired (PS1)");

    let scope = test_scope("query-surface");
    let suffix = unique_suffix();
    let entity_id = Id::from(format!("pg-q-entity-{suffix}"));
    let relationship_id = Id::from(format!("pg-q-rel-{suffix}"));
    let target_id = Id::from(format!("pg-q-entity-2-{suffix}"));
    let entity = KnowledgeEntity {
        id: entity_id.clone(),
        graph_id: None,
        kind: EntityKind::Concept,
        name: "query-surface-probe".to_owned(),
        aliases: Vec::new(),
        scope: scope.clone(),
        source_refs: Vec::new(),
        concept_refs: Vec::new(),
        ontology_class_refs: Vec::new(),
        provenance: Provenance {
            source: "test".to_owned(),
            actor: Actor {
                id: Id::from("test"),
                kind: ActorKind::System,
                display_name: None,
                metadata: None,
            },
            observed_at: chrono::Utc::now(),
            evidence: Vec::new(),
            derivations: Vec::new(),
            confidence: Some(1.0),
            method: None,
        },
        created_at: chrono::Utc::now(),
        updated_at: None,
        metadata: Default::default(),
        valid_from: None,
        valid_until: None,
        archived_at: None,
    };
    let rel = KnowledgeRelationship {
        id: relationship_id.clone(),
        graph_id: None,
        subject: EntityRef {
            id: Some(entity.id.clone()),
            kind: None,
            name: Some("query-surface-probe".to_owned()),
            aliases: Vec::new(),
        },
        predicate: "calls".to_owned(),
        code_resolution: None,
        object: EntityRef {
            id: Some(target_id),
            kind: None,
            name: Some("callee-probe".to_owned()),
            aliases: Vec::new(),
        },
        scope: scope.clone(),
        evidence: Vec::new(),
        confidence: Some(1.0),
        provenance: Provenance {
            source: "test".to_owned(),
            actor: Actor {
                id: Id::from("test"),
                kind: ActorKind::System,
                display_name: None,
                metadata: None,
            },
            observed_at: chrono::Utc::now(),
            evidence: Vec::new(),
            derivations: Vec::new(),
            confidence: Some(1.0),
            method: None,
        },
        created_at: chrono::Utc::now(),
        updated_at: None,
        archived_at: None,
    };

    block_on(repo.put_entity(entity)).expect("put_entity");
    block_on(repo.put_relationship(rel)).expect("put_relationship");

    let entities = block_on(query.list_entities(&scope)).expect("list_entities");
    assert!(
        entities.iter().any(|e| e.name == "query-surface-probe"),
        "entity must appear in list_entities: {entities:?}"
    );
    let rels = block_on(query.list_relationships(&scope)).expect("list_relationships");
    assert!(
        rels.iter()
            .any(|r| r.predicate == "calls" && r.object.name.as_deref() == Some("callee-probe")),
        "relationship must appear in list_relationships: {rels:?}"
    );

    // Cleanup so the test is re-runnable.
    block_on(repo.delete_relationship(&relationship_id, &scope)).expect("delete_relationship");
    block_on(repo.delete_entity(&entity_id, &scope)).expect("delete_entity");

    println!(
        "pgvector recipe knowledge_query: entities + relationships listed via the query handle ✓"
    );
}

/// PS2: the vector recall lane. Scan-embeds a chunk (via the provider's
/// embedding provider + vector index), then verifies recall surfaces it
/// through semantic similarity — the lane the recipe was missing. Requires
/// Docker Postgres + the fastembed model cache.
#[test]
#[ignore]
#[cfg(feature = "fastembed")]
fn pg_recipe_recall_fuses_vector_lane() {
    use engram_domain::*;
    use engram_integration::UnifiedRecall as _;

    // fastembed's model cache is cwd-relative (.fastembed_cache at the repo
    // root); cargo tests run from the crate dir.
    let repo_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    std::env::set_current_dir(&repo_root).expect("cd to repo root");

    let provider = open_bootstrap_recipe();

    // The embedding provider must be wired for the vector lane to exist.
    let embedder = provider
        .require_embedding_provider()
        .expect("embedding provider wired under the fastembed feature");
    let vectors = provider.require_vectors().expect("vector index wired");
    let knowledge = provider.require_knowledge().expect("knowledge handle");

    let scope = test_scope("vector-lane");
    let suffix = unique_suffix();
    let chunk_id = Id::from(format!("pg-vec-chunk-{suffix}"));
    let document_id = Id::from(format!("pg-vec-doc-{suffix}"));
    let source_id = Id::from(format!("pg-vec-src-{suffix}"));

    // A chunk whose text shares vocabulary with the query but no exact match.
    let chunk = KnowledgeChunk {
        id: chunk_id.clone(),
        document_id: document_id.clone(),
        source_id: source_id.clone(),
        kind: KnowledgeChunkKind::CodeBlock,
        text: "pub fn fibonacci(n: u64) -> u64 { match n { 0 => 0, 1 => 1, _ => fibonacci(n-1) + fibonacci(n-2) } }"
            .to_owned(),
        summary: None,
        location: None,
        entities: Vec::new(),
        concepts: Vec::new(),
        embedding_refs: Vec::new(),
        content_hash: "hash-1".to_owned(),
        provenance: Provenance {
            source: "test".to_owned(),
            actor: Actor {
                id: Id::from("test"),
                kind: ActorKind::System,
                display_name: None,
                metadata: None,
            },
            observed_at: chrono::Utc::now(),
            evidence: Vec::new(),
            derivations: Vec::new(),
            confidence: Some(1.0),
            method: None,
        },
        policy: Policy {
            visibility: Visibility::Workspace,
            retention: Retention::Durable,
            sensitivity: Some(Sensitivity::Medium),
            allowed_uses: vec![AllowedUse::Retrieval],
            expires_at: None,
            delete_mode: Some(DeleteMode::Tombstone),
        },
        created_at: chrono::Utc::now(),
        updated_at: None,
        metadata: None,
    };
    // The chunks table is FK-bound: parent source + document must exist first.
    let src = KnowledgeSource {
        id: source_id,
        kind: SourceKind::Filesystem,
        scope: scope.clone(),
        name: "vector-lane-test".to_owned(),
        uri: None,
        version: None,
        policy: chunk.policy.clone(),
        provenance: chunk.provenance.clone(),
        created_at: chrono::Utc::now(),
        updated_at: None,
        metadata: None,
    };
    let doc = SourceDocument {
        id: document_id,
        source_id: src.id.clone(),
        kind: SourceDocumentKind::Code,
        uri: None,
        path: Some("vector_lane.rs".to_owned()),
        title: None,
        mime_type: None,
        language: Some("rust".to_owned()),
        version: None,
        content_hash: "hash-doc".to_owned(),
        provenance: chunk.provenance.clone(),
        policy: chunk.policy.clone(),
        created_at: chrono::Utc::now(),
        updated_at: None,
        metadata: None,
    };
    block_on(knowledge.put_source(src)).expect("put_source");
    block_on(knowledge.put_document(doc)).expect("put_document");
    block_on(knowledge.put_chunk(chunk.clone())).expect("put_chunk");

    // Embed + index the chunk (the scan path's embed step, done by hand here).
    let space = embedder.embedding_space();
    let passage = embedder.embed_passage(&chunk.text).expect("embed passage");
    block_on(vectors.insert(&chunk.id, &space, passage)).expect("vector insert");

    // Recall: the query shares semantics ("recursive number sequence") with
    // the chunk but no exact tokens — only the vector lane can find it.
    let recall = provider.require_recall().expect("recall handle");
    let payload = block_on(recall.recall(RetrievalRequest {
        query: "recursive number sequence computation".to_owned(),
        scope: scope.clone(),
        modes: vec![],
        filters: None,
        cues: vec![],
        limit: None,
        budget: None,
        include_explanations: None,
        requester: Requester {
            actor: Actor {
                id: Id::from("test"),
                kind: ActorKind::System,
                display_name: None,
                metadata: None,
            },
            roles: Vec::new(),
            permissions: Vec::new(),
            on_behalf_of: None,
        },
    }))
    .expect("recall");

    assert!(
        payload
            .items
            .iter()
            .any(|r| r.target_id == chunk_id.to_string()),
        "vector lane must surface the chunk: {:?}",
        payload
            .items
            .iter()
            .map(|r| r.target_id.clone())
            .collect::<Vec<_>>()
    );
    assert!(
        payload.source_failures.iter().all(|f| f.source != "vector"),
        "vector lane must not fail: {:?}",
        payload.source_failures
    );

    // Cleanup.
    block_on(vectors.delete_target(&chunk.id)).expect("vector delete");
    println!("pgvector recipe recall: vector lane fused ✓");
}

/// PS4: the SQLite → Postgres migration round-trip, executable. Seeds a
/// SQLite provider (temp store), exports via the facade's ExportImport,
/// writes the records into the Postgres recipe provider through its ports,
/// then recall-parity spot-checks: the same query must surface the same
/// memory on both engines. Requires Docker Postgres.
#[test]
#[ignore]
fn pg_recipe_sqlite_export_import_round_trip() {
    use engram_domain::*;
    use engram_integration::{
        CapabilityPolicy, EmbeddingProviderConfig, EngramConfig, EngramProvider, MigrationMode,
    };

    // ---- 1. Seed a SQLite provider with one memory (typed full request).
    let dir = tempfile::tempdir().expect("tempdir");
    let sqlite_config = EngramConfig::new(
        dir.path().join("mig.db"),
        dir.path().to_path_buf(),
        engram_domain::ScopeMappingStrategy::Strict,
        EmbeddingProviderConfig {
            provider_type: "none".to_owned(),
            model: "none".to_owned(),
            dimensions: 384,
            prompt_profile: "query".to_owned(),
            normalization: None,
            ..Default::default()
        },
        MigrationMode::Apply,
        CapabilityPolicy::FailClosed,
    );
    let sqlite = EngramProvider::open(&sqlite_config).expect("sqlite provider");
    let scope = test_scope("migration-demo");
    let now = chrono::Utc::now();
    let policy = Policy {
        visibility: Visibility::Workspace,
        retention: Retention::Durable,
        sensitivity: Some(Sensitivity::Medium),
        allowed_uses: vec![AllowedUse::Retrieval],
        expires_at: None,
        delete_mode: Some(DeleteMode::Tombstone),
    };
    let provenance = Provenance {
        source: "migration-test".to_owned(),
        actor: Actor {
            id: Id::from("migrator"),
            kind: ActorKind::Agent,
            display_name: None,
            metadata: None,
        },
        observed_at: now,
        evidence: Vec::new(),
        derivations: Vec::new(),
        confidence: Some(1.0),
        method: Some("migration-round-trip".to_owned()),
    };
    let request = engram_domain::WriteMemoryRequest {
        kind: MemoryKind::Fact,
        content: MemoryContent {
            text: "The migration runbook demo memory: engram moves stores engine-neutrally"
                .to_owned(),
            summary: None,
            entities: Vec::new(),
            language: None,
            format: None,
            structured: None,
            hash: None,
        },
        scope: scope.clone(),
        requester: Requester {
            actor: Actor {
                id: Id::from("migrator"),
                kind: ActorKind::Agent,
                display_name: None,
                metadata: None,
            },
            roles: Vec::new(),
            permissions: Vec::new(),
            on_behalf_of: None,
        },
        provenance: provenance.clone(),
        policy: policy.clone(),
        links: Vec::new(),
        verdict: None,
        idempotency_key: None,
    };
    let memory = sqlite.require_memory().expect("memory handle");
    block_on(async { memory.write_memory(request).await }).expect("write memory to sqlite");

    // ---- 2. Export from SQLite (the facade's engine-neutral ExportImport).
    let export = sqlite.require_export_import().expect("export handle");
    let data = block_on(async { export.export(&scope).await }).expect("sqlite export");
    assert!(
        !data.memories.is_empty(),
        "export must carry the seeded memory"
    );

    // ---- 3. Import into Postgres through the recipe's ports (typed records
    //      rebuilt from the export's JSON strings — same domain contracts).
    let pg = open_bootstrap_recipe();
    let pg_memory = pg.require_memory().expect("pg memory");
    for record in &data.memories {
        // The import-record shape is the flattened export (text + scope JSON +
        // policy JSON + timestamp) — rebuild a typed request from those fields.
        let scope_json: engram_domain::Scope =
            serde_json::from_str(&record.scope).expect("scope json");
        let policy_json: engram_domain::Policy =
            serde_json::from_str(&record.policy).expect("policy json");
        block_on(async {
            pg_memory
                .write_memory(engram_domain::WriteMemoryRequest {
                    kind: MemoryKind::Observation,
                    content: MemoryContent {
                        text: record.content.clone(),
                        summary: None,
                        entities: Vec::new(),
                        language: None,
                        format: None,
                        structured: None,
                        hash: None,
                    },
                    scope: scope_json,
                    requester: Requester {
                        actor: provenance.actor.clone(),
                        roles: Vec::new(),
                        permissions: Vec::new(),
                        on_behalf_of: None,
                    },
                    provenance: provenance.clone(),
                    policy: policy_json,
                    links: Vec::new(),
                    verdict: None,
                    idempotency_key: None,
                })
                .await
        })
        .expect("write memory to pg");
    }

    // ---- 4. Recall parity: the same query surfaces the memory on BOTH engines.
    let query = |provider: &EngramProvider| {
        let recall = provider.require_recall().expect("recall");
        block_on(async {
            recall
                .recall(RetrievalRequest {
                    query: "migration runbook engine-neutral".to_owned(),
                    scope: scope.clone(),
                    requester: Requester {
                        actor: Actor {
                            id: Id::from("migrator"),
                            kind: ActorKind::Agent,
                            display_name: None,
                            metadata: None,
                        },
                        roles: Vec::new(),
                        permissions: Vec::new(),
                        on_behalf_of: None,
                    },
                    modes: vec![],
                    filters: None,
                    cues: vec![],
                    limit: None,
                    budget: None,
                    include_explanations: None,
                })
                .await
        })
        .expect("recall")
    };
    let from_sqlite = query(&sqlite);
    let from_pg = query(&pg);
    let needle = "migration runbook demo memory";
    assert!(
        from_sqlite.items.iter().any(|r| r.content.contains(needle)),
        "sqlite recall must surface the memory"
    );
    assert!(
        from_pg.items.iter().any(|r| r.content.contains(needle)),
        "pg recall must surface the SAME memory after import: {:?}",
        from_pg
            .items
            .iter()
            .map(|r| r.content.clone())
            .collect::<Vec<_>>()
    );

    println!("pgvector recipe migration: sqlite export → pg import → recall parity ✓");
}
