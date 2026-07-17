//! Integration test: the associative-graph retrieval lane contributes to
//! `SqlUnifiedRecall` and dedups with the lexical graph lane.
//!
//! Verified at the `SqlUnifiedRecall` level — the spec's chosen level
//! (`core/integration` has no sqlite test infra by design). The bootstrap lane
//! push (`core/integration/src/sqlite/bootstrap.rs`) is compile-verified (the
//! push into `Vec<Arc<dyn RetrievalIndex>>` is type-checked) and mirrors the
//! lexical `GraphRetrievalIndex` lane.

use std::sync::Arc;

use engram_conformance::SqlUnifiedRecall;
use engram_domain::*;
use engram_integration::UnifiedRecall;
use engram_knowledge::KnowledgeRepository;
use engram_retrieval::RetrievalIndex;
use engram_store_sqlite::SqlBeliefStore;
use engram_store_sqlite::{GraphRetrievalIndex, SqlKnowledgeStore};
use engram_store_sqlite::SqlMemoryService;
use futures::executor::block_on;

fn scope() -> Scope {
    Scope {
        tenant: "t".to_string(),
        subject: None,
        workspace: None,
        session: None,
        environment: None,
    }
}

fn scope_tenant(tenant: &str) -> Scope {
    Scope {
        tenant: tenant.to_string(),
        subject: None,
        workspace: None,
        session: None,
        environment: None,
    }
}

fn actor() -> Actor {
    Actor {
        id: Id::from("associative-recall-test"),
        kind: ActorKind::Agent,
        display_name: None,
        metadata: None,
    }
}

fn provenance() -> Provenance {
    Provenance {
        source: "associative-recall-test".to_string(),
        actor: actor(),
        observed_at: chrono::Utc::now(),
        evidence: Vec::new(),
        derivations: Vec::new(),
        confidence: Some(1.0),
        method: Some("test".to_string()),
    }
}

fn ent(id: &str, name: &str) -> KnowledgeEntity {
    KnowledgeEntity {
        id: Id::from(id),
        graph_id: None,
        kind: EntityKind::Concept,
        name: name.to_string(),
        aliases: Vec::new(),
        scope: scope(),
        source_refs: Vec::new(),
        concept_refs: Vec::new(),
        ontology_class_refs: Vec::new(),
        provenance: provenance(),
        created_at: chrono::Utc::now(),
        updated_at: None,
        valid_from: None,
        valid_until: None,
        metadata: None,
    }
}

fn ref_of(key: &str) -> EntityRef {
    EntityRef {
        id: Some(Id::from(key)),
        kind: None,
        name: Some(key.to_string()),
        aliases: Vec::new(),
    }
}

fn rel(subject: &str, object: &str) -> KnowledgeRelationship {
    KnowledgeRelationship {
        id: Id::from(format!("rel-{subject}-{object}")),
        graph_id: None,
        subject: ref_of(subject),
        predicate: "related_to".to_string(),
        object: ref_of(object),
        scope: scope(),
        evidence: Vec::new(),
        confidence: None,
        provenance: provenance(),
        created_at: chrono::Utc::now(),
        updated_at: None,
    }
}

fn rel_tenant(subject: &str, object: &str, tenant: &str) -> KnowledgeRelationship {
    KnowledgeRelationship {
        scope: scope_tenant(tenant),
        ..rel(subject, object)
    }
}

fn request(query: &str) -> RetrievalRequest {
    RetrievalRequest {
        query: query.to_string(),
        scope: scope(),
        requester: Requester {
            actor: actor(),
            roles: Vec::new(),
            permissions: Vec::new(),
            on_behalf_of: None,
        },
        modes: Vec::new(),
        filters: None,
        cues: Vec::new(),
        limit: Some(10),
        budget: None,
        include_explanations: None,
    }
}

#[test]
fn associative_lane_contributes_and_dedups_with_lexical() {
    let memory = Arc::new(SqlMemoryService::open_in_memory().expect("memory open"));
    let beliefs = Arc::new(SqlBeliefStore::open_in_memory().expect("beliefs open"));
    let knowledge = Arc::new(SqlKnowledgeStore::open_in_memory().expect("knowledge open"));

    // Tenant "t": chain a - b - c - d, plus an isolated in-scope entity w.
    for (id, name) in [
        ("a", "Alice"),
        ("b", "Bob"),
        ("c", "Carol"),
        ("d", "Dave"),
        ("w", "Wendy"),
    ] {
        block_on(knowledge.put_entity(ent(id, name))).expect("put entity");
    }
    for (s, o) in [("a", "b"), ("b", "c"), ("c", "d")] {
        block_on(knowledge.put_relationship(rel(s, o))).expect("put relationship");
    }
    // Cross-scope relationship a -> w (tenant "other"). The seed `a` is in-scope
    // and `w` is an in-scope entity, so the walk WOULD reach `w` if this
    // relationship were admitted; its absence proves the relationship scope
    // filter at the read boundary.
    block_on(knowledge.put_relationship(rel_tenant("a", "w", "other")))
        .expect("put cross-scope rel");

    // Both graph-mode lanes: lexical (entity-name match) + associative (PPR).
    let lexical: Arc<dyn RetrievalIndex> = Arc::new(GraphRetrievalIndex::new(knowledge.clone()));
    let associative = engram_integration::sqlite::associative_recall_lane(knowledge.clone());
    let recall = SqlUnifiedRecall::new(memory, vec![lexical, associative], beliefs);
    let payload = block_on(recall.recall(request("Alice"))).expect("recall");

    // AC4 dedup: entity "a" is returned by BOTH lanes (lexical exact-name match
    // on "Alice" + associative seed) and must appear exactly once — RRF merges
    // same-(target_type, target_id) candidates (source becomes e.g.
    // "graph+associative_graph").
    let a_count = payload
        .items
        .iter()
        .filter(|i| i.target_id == "a" && i.target_type == RetrievalTargetType::Entity)
        .count();
    assert_eq!(
        a_count,
        1,
        "entity a (in both lanes) must appear exactly once after RRF merge, got {a_count}: {:?}",
        payload
            .items
            .iter()
            .map(|i| (
                i.target_type.clone(),
                i.target_id.clone(),
                i.fusion_trace.as_ref().map(|t| t.source.clone())
            ))
            .collect::<Vec<_>>()
    );

    // Associative contributed: at least one item sources associative_graph
    // (b/c/d are associative-only; "a" may be merged as "graph+associative_graph").
    assert!(
        payload.items.iter().any(|i| {
            i.fusion_trace
                .as_ref()
                .map(|t| t.source.contains("associative_graph"))
                .unwrap_or(false)
        }),
        "associative lane should contribute candidates"
    );

    let ids: Vec<&str> = payload.items.iter().map(|i| i.target_id.as_str()).collect();
    assert!(ids.contains(&"a"), "seed a present");
    assert!(ids.contains(&"b"), "1-hop neighbor b present");
    assert!(
        !ids.contains(&"w"),
        "cross-scope relationship a->w must be filtered out, so w is unreachable"
    );
}
