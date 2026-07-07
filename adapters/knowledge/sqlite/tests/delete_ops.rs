//! TDD unit tests for fine-grained knowledge delete ports (T1, T2).
//!
//! Verifies:
//! - `delete_entity` and `delete_relationship` are hard-deletes (row removed,
//!   returns `true`); a second delete returns `false`; a mismatched-scope delete
//!   is a no-op returning `false` (AC-1, AC-3, AC-7).
//! - `delete_graph` cascades to all entities and relationships with that
//!   `graph_id` (row counts go to zero — hard delete); other graphs' records
//!   are untouched; scope-mismatch is a no-op returning `false` (AC-1, AC-2,
//!   AC-3, AC-7).

use chrono::Utc;
use engram_domain::*;
use engram_knowledge::{KnowledgeGraphRepository, KnowledgeRepository};
use engram_store_knowledge_sqlite::SqlKnowledgeStore;
use futures::executor::block_on;

fn scope(tenant: &str) -> Scope {
    Scope {
        tenant: tenant.to_owned(),
        subject: None,
        workspace: Some("test".to_owned()),
        session: None,
        environment: Some("test".to_owned()),
    }
}

fn policy() -> Policy {
    Policy {
        visibility: Visibility::Workspace,
        retention: Retention::Durable,
        sensitivity: None,
        allowed_uses: vec![AllowedUse::Retrieval],
        expires_at: None,
        delete_mode: None,
    }
}

fn actor() -> Actor {
    Actor {
        id: Id::from("test-agent"),
        kind: ActorKind::Agent,
        display_name: None,
        metadata: None,
    }
}

fn provenance() -> Provenance {
    Provenance {
        source: "delete-ops-test".to_owned(),
        actor: actor(),
        observed_at: Utc::now(),
        evidence: Vec::new(),
        derivations: Vec::new(),
        confidence: Some(1.0),
        method: Some("manual".to_owned()),
    }
}

fn make_graph(id: &str, tenant: &str) -> KnowledgeGraph {
    KnowledgeGraph {
        id: Id::from(id),
        scope: scope(tenant),
        name: format!("Graph {id}"),
        uri: None,
        version: None,
        ontology_refs: Vec::new(),
        policy: policy(),
        provenance: provenance(),
        created_at: Utc::now(),
        updated_at: None,
        metadata: None,
    }
}

fn make_entity(id: &str, graph_id: Option<&str>, tenant: &str) -> KnowledgeEntity {
    KnowledgeEntity {
        id: Id::from(id),
        graph_id: graph_id.map(Id::from),
        kind: EntityKind::Function,
        name: id.to_owned(),
        aliases: Vec::new(),
        scope: scope(tenant),
        source_refs: Vec::new(),
        concept_refs: Vec::new(),
        provenance: provenance(),
        created_at: Utc::now(),
        updated_at: None,
        metadata: None,
    }
}

fn make_relationship(
    id: &str,
    graph_id: Option<&str>,
    subject_id: &str,
    tenant: &str,
) -> KnowledgeRelationship {
    KnowledgeRelationship {
        id: Id::from(id),
        graph_id: graph_id.map(Id::from),
        subject: EntityRef {
            id: Some(Id::from(subject_id)),
            kind: Some("function".to_owned()),
            name: Some(subject_id.to_owned()),
            aliases: Vec::new(),
        },
        predicate: "calls".to_owned(),
        object: EntityRef {
            id: Some(Id::from("target")),
            kind: Some("function".to_owned()),
            name: Some("target".to_owned()),
            aliases: Vec::new(),
        },
        scope: scope(tenant),
        evidence: Vec::new(),
        confidence: Some(0.9),
        provenance: provenance(),
        created_at: Utc::now(),
        updated_at: None,
    }
}

// ---------------------------------------------------------------------------
// T1: delete_entity
// ---------------------------------------------------------------------------

#[test]
fn delete_entity_removes_row_and_returns_true() {
    let store = SqlKnowledgeStore::open_in_memory().expect("open store");
    let entity = make_entity("entity-1", Some("graph-1"), "tenant-a");
    block_on(store.put_entity(entity.clone())).expect("put entity");

    // Confirm the entity exists via get.
    let before = block_on(store.get_entity(&entity.id, &scope("tenant-a"))).expect("get before");
    assert!(before.is_some(), "entity must exist before delete");

    let deleted =
        block_on(store.delete_entity(&entity.id, &scope("tenant-a"))).expect("delete_entity");
    assert!(deleted, "delete must return true when row existed");

    // Confirm hard delete: entity is gone.
    let after = block_on(store.get_entity(&entity.id, &scope("tenant-a"))).expect("get after");
    assert!(after.is_none(), "entity must be gone after hard delete");
}

#[test]
fn delete_entity_returns_false_when_already_deleted() {
    let store = SqlKnowledgeStore::open_in_memory().expect("open store");
    let entity = make_entity("entity-idempotent", Some("graph-1"), "tenant-a");
    block_on(store.put_entity(entity.clone())).expect("put entity");

    block_on(store.delete_entity(&entity.id, &scope("tenant-a"))).expect("first delete");
    // Second delete must return false (already gone).
    let second =
        block_on(store.delete_entity(&entity.id, &scope("tenant-a"))).expect("second delete");
    assert!(!second, "second delete must return false");
}

#[test]
fn delete_entity_mismatched_scope_is_noop() {
    let store = SqlKnowledgeStore::open_in_memory().expect("open store");
    let entity = make_entity("entity-scope", Some("graph-1"), "tenant-a");
    block_on(store.put_entity(entity.clone())).expect("put entity");

    // Delete under a different tenant → no-op.
    let deleted =
        block_on(store.delete_entity(&entity.id, &scope("tenant-b"))).expect("mismatched delete");
    assert!(!deleted, "mismatched-scope delete must return false");

    // Entity must still be visible under its own scope.
    let still_there =
        block_on(store.get_entity(&entity.id, &scope("tenant-a"))).expect("get after noop");
    assert!(
        still_there.is_some(),
        "entity must survive a mismatched-scope delete"
    );
}

// ---------------------------------------------------------------------------
// T1: delete_relationship
// ---------------------------------------------------------------------------

#[test]
fn delete_relationship_removes_row_and_returns_true() {
    let store = SqlKnowledgeStore::open_in_memory().expect("open store");
    let rel = make_relationship("rel-1", Some("graph-1"), "fn-a", "tenant-a");
    block_on(store.put_relationship(rel.clone())).expect("put relationship");

    let before = block_on(store.get_relationship(&rel.id, &scope("tenant-a"))).expect("get before");
    assert!(before.is_some());

    let deleted = block_on(store.delete_relationship(&rel.id, &scope("tenant-a")))
        .expect("delete_relationship");
    assert!(deleted, "delete must return true when row existed");

    let after = block_on(store.get_relationship(&rel.id, &scope("tenant-a"))).expect("get after");
    assert!(
        after.is_none(),
        "relationship must be gone after hard delete"
    );
}

#[test]
fn delete_relationship_returns_false_when_already_deleted() {
    let store = SqlKnowledgeStore::open_in_memory().expect("open store");
    let rel = make_relationship("rel-idempotent", Some("graph-1"), "fn-b", "tenant-a");
    block_on(store.put_relationship(rel.clone())).expect("put relationship");

    block_on(store.delete_relationship(&rel.id, &scope("tenant-a"))).expect("first delete");
    let second =
        block_on(store.delete_relationship(&rel.id, &scope("tenant-a"))).expect("second delete");
    assert!(!second, "second delete must return false");
}

#[test]
fn delete_relationship_mismatched_scope_is_noop() {
    let store = SqlKnowledgeStore::open_in_memory().expect("open store");
    let rel = make_relationship("rel-scope", Some("graph-1"), "fn-c", "tenant-a");
    block_on(store.put_relationship(rel.clone())).expect("put relationship");

    let deleted = block_on(store.delete_relationship(&rel.id, &scope("tenant-b")))
        .expect("mismatched delete");
    assert!(!deleted, "mismatched-scope delete must return false");

    let still_there =
        block_on(store.get_relationship(&rel.id, &scope("tenant-a"))).expect("get after noop");
    assert!(
        still_there.is_some(),
        "relationship must survive a mismatched-scope delete"
    );
}

// ---------------------------------------------------------------------------
// T2: delete_graph cascade
// ---------------------------------------------------------------------------

#[test]
fn delete_graph_cascades_to_entities_and_relationships() {
    let store = SqlKnowledgeStore::open_in_memory().expect("open store");

    // Insert a graph with two entities and one relationship.
    let graph = make_graph("graph-cascade", "tenant-a");
    let entity_a = make_entity("entity-ca", Some("graph-cascade"), "tenant-a");
    let entity_b = make_entity("entity-cb", Some("graph-cascade"), "tenant-a");
    let rel = make_relationship(
        "rel-cascade",
        Some("graph-cascade"),
        "entity-ca",
        "tenant-a",
    );

    block_on(store.put_graph(graph.clone())).expect("put graph");
    block_on(store.put_entity(entity_a.clone())).expect("put entity_a");
    block_on(store.put_entity(entity_b.clone())).expect("put entity_b");
    block_on(store.put_relationship(rel.clone())).expect("put relationship");

    // Confirm all exist before delete.
    assert!(
        block_on(store.get_graph(&graph.id, &scope("tenant-a")))
            .unwrap()
            .is_some()
    );
    assert!(
        block_on(store.get_entity(&entity_a.id, &scope("tenant-a")))
            .unwrap()
            .is_some()
    );
    assert!(
        block_on(store.get_relationship(&rel.id, &scope("tenant-a")))
            .unwrap()
            .is_some()
    );

    let deleted =
        block_on(store.delete_graph(&graph.id, &scope("tenant-a"))).expect("delete_graph");
    assert!(deleted, "delete_graph must return true when graph existed");

    // Graph row is gone.
    assert!(
        block_on(store.get_graph(&graph.id, &scope("tenant-a")))
            .unwrap()
            .is_none(),
        "graph must be deleted"
    );
    // All entities with that graph_id are gone.
    assert!(
        block_on(store.get_entity(&entity_a.id, &scope("tenant-a")))
            .unwrap()
            .is_none(),
        "entity_a must be deleted"
    );
    assert!(
        block_on(store.get_entity(&entity_b.id, &scope("tenant-a")))
            .unwrap()
            .is_none(),
        "entity_b must be deleted"
    );
    // Relationship with that graph_id is gone.
    assert!(
        block_on(store.get_relationship(&rel.id, &scope("tenant-a")))
            .unwrap()
            .is_none(),
        "relationship must be deleted"
    );
}

#[test]
fn delete_graph_does_not_touch_other_graphs() {
    let store = SqlKnowledgeStore::open_in_memory().expect("open store");

    // Two graphs, each with one entity and one relationship.
    let graph_x = make_graph("graph-x", "tenant-a");
    let graph_y = make_graph("graph-y", "tenant-a");
    let entity_x = make_entity("entity-x", Some("graph-x"), "tenant-a");
    let entity_y = make_entity("entity-y", Some("graph-y"), "tenant-a");
    let rel_x = make_relationship("rel-x", Some("graph-x"), "entity-x", "tenant-a");
    let rel_y = make_relationship("rel-y", Some("graph-y"), "entity-y", "tenant-a");

    block_on(store.put_graph(graph_x.clone())).expect("put graph_x");
    block_on(store.put_graph(graph_y.clone())).expect("put graph_y");
    block_on(store.put_entity(entity_x.clone())).expect("put entity_x");
    block_on(store.put_entity(entity_y.clone())).expect("put entity_y");
    block_on(store.put_relationship(rel_x.clone())).expect("put rel_x");
    block_on(store.put_relationship(rel_y.clone())).expect("put rel_y");

    // Delete only graph-x.
    let deleted =
        block_on(store.delete_graph(&graph_x.id, &scope("tenant-a"))).expect("delete_graph");
    assert!(deleted);

    // graph-x and its records are gone.
    assert!(
        block_on(store.get_graph(&graph_x.id, &scope("tenant-a")))
            .unwrap()
            .is_none()
    );
    assert!(
        block_on(store.get_entity(&entity_x.id, &scope("tenant-a")))
            .unwrap()
            .is_none()
    );
    assert!(
        block_on(store.get_relationship(&rel_x.id, &scope("tenant-a")))
            .unwrap()
            .is_none()
    );

    // graph-y and its records are intact.
    assert!(
        block_on(store.get_graph(&graph_y.id, &scope("tenant-a")))
            .unwrap()
            .is_some(),
        "graph-y must survive"
    );
    assert!(
        block_on(store.get_entity(&entity_y.id, &scope("tenant-a")))
            .unwrap()
            .is_some(),
        "entity_y must survive"
    );
    assert!(
        block_on(store.get_relationship(&rel_y.id, &scope("tenant-a")))
            .unwrap()
            .is_some(),
        "rel_y must survive"
    );
}

#[test]
fn delete_graph_mismatched_scope_is_noop() {
    let store = SqlKnowledgeStore::open_in_memory().expect("open store");
    let graph = make_graph("graph-scope-guard", "tenant-a");
    let entity = make_entity("entity-sg", Some("graph-scope-guard"), "tenant-a");

    block_on(store.put_graph(graph.clone())).expect("put graph");
    block_on(store.put_entity(entity.clone())).expect("put entity");

    // Delete under a different tenant → no-op.
    let deleted = block_on(store.delete_graph(&graph.id, &scope("tenant-b")))
        .expect("mismatched delete_graph");
    assert!(!deleted, "mismatched-scope delete_graph must return false");

    // Graph and entity are still present.
    assert!(
        block_on(store.get_graph(&graph.id, &scope("tenant-a")))
            .unwrap()
            .is_some(),
        "graph must survive mismatched-scope delete"
    );
    assert!(
        block_on(store.get_entity(&entity.id, &scope("tenant-a")))
            .unwrap()
            .is_some(),
        "entity must survive mismatched-scope delete"
    );
}

#[test]
fn delete_graph_returns_false_when_graph_does_not_exist() {
    let store = SqlKnowledgeStore::open_in_memory().expect("open store");
    let missing_id = Id::from("graph-nonexistent");
    let result = block_on(store.delete_graph(&missing_id, &scope("tenant-a")))
        .expect("delete_graph missing");
    assert!(
        !result,
        "delete_graph must return false for a nonexistent graph"
    );
}
