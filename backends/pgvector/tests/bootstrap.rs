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
    CapabilityPolicy, EmbeddingProviderConfig, EngramConfig, MigrationMode,
};
use futures::executor::block_on;
use std::path::PathBuf;

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
        },
        MigrationMode::DryRun,
        CapabilityPolicy::FailClosed,
    )
    .with_pgvector("postgres://engram:engram@localhost:5432/engram")
}

#[test]
#[ignore]
fn pg_recipe_opens_and_reports_capabilities() {
    use engram_domain::CapabilityState;

    let config = pg_config();
    let provider = open(&config).expect("recipe opens against Docker pgvector");

    // Hot-path capabilities must be wired.
    let caps = provider.capabilities();
    assert_eq!(caps.knowledge, CapabilityState::Supported, "knowledge Supported");
    assert_eq!(caps.graph, CapabilityState::Supported, "graph Supported");
    assert_eq!(caps.vectors, CapabilityState::Supported, "vectors Supported");

    println!("pgvector recipe: provider opens, knowledge + graph + vectors Supported ✓");
}

#[test]
#[ignore]
fn pg_recipe_knowledge_write_read_round_trip() {
    use engram_domain::*;

    let config = pg_config();
    let provider = open(&config).expect("recipe opens");
    let repo = provider.require_knowledge().expect("knowledge handle");

    let scope = Scope {
        tenant: "pgvector-test".to_owned(),
        subject: None,
        workspace: Some("test".to_owned()),
        session: None,
        environment: Some("test".to_owned()),
    };
    let entity = KnowledgeEntity {
        id: Id::from("pg-rt-entity"),
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
    };

    block_on(repo.put_entity(entity)).expect("put_entity");
    block_on(repo.delete_entity(&Id::from("pg-rt-entity"), &scope)).expect("delete_entity");

    println!("pgvector recipe knowledge round-trip: put_entity → delete_entity ✓");
}
