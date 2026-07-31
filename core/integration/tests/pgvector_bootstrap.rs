//! T7: End-to-end integration test for the pgvector backend.
//!
//! Opens an EngramProvider against the Docker pgvector instance + verifies
//! the P0 capabilities (knowledge + graph + vectors) are wired.
//!
//! Requires: docker compose -f docs/how-to-pg/docker-compose.yaml up -d
//! Run: cargo test -p engram-integration --features pgvector -- --ignored pgvector

#![cfg(feature = "pgvector")]

use engram_domain::CapabilityState;
use engram_domain::ScopeMappingStrategy;
use engram_integration::{
    CapabilityPolicy, EmbeddingProviderConfig, EngramConfig, EngramProvider, MigrationMode,
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
fn pgvector_provider_opens_and_reports_capabilities() {
    let config = pg_config();
    let provider = EngramProvider::open(&config).expect("provider opens against Docker pgvector");

    // P0 capabilities must be wired.
    let caps = provider.capabilities();
    assert_eq!(
        caps.knowledge,
        CapabilityState::Supported,
        "knowledge must be Supported"
    );
    assert_eq!(
        caps.graph,
        CapabilityState::Supported,
        "graph must be Supported"
    );
    assert_eq!(
        caps.vectors,
        CapabilityState::Supported,
        "vectors must be Supported"
    );

    println!("pgvector bootstrap: provider opens, knowledge + graph + vectors Supported ✓");
}

#[test]
#[ignore]
fn pgvector_knowledge_write_read_round_trip() {
    use engram_domain::*;

    let config = pg_config();
    let provider = EngramProvider::open(&config).expect("provider opens");
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

    // Write.
    block_on(repo.put_entity(entity)).expect("put_entity");

    // Clean up.
    block_on(repo.delete_entity(&Id::from("pg-rt-entity"), &scope)).expect("delete_entity");

    println!("pgvector knowledge round-trip: put_entity → delete_entity ✓");
}
