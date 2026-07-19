//! Integration tests for `SqlProvenanceQuery` (engram-host-sdk brief, S2).
//!
//! These tests exercise the SQLite `ProvenanceQuery` impl against an in-memory
//! `SqlKnowledgeStore`: provenance/evidence recovery by id, source-filtered and
//! scope-windowed listing, scope isolation, and the `CapabilityUnsupported`
//! short-circuit for v1-unsupported target kinds. They mirror the block_on
//! driving style of `adapters/knowledge/sqlite/tests/repository.rs` — no tokio.

use std::collections::BTreeMap;
use std::sync::Arc;

use engram_conformance::SqlProvenanceQuery;
use engram_domain::*;
use engram_integration::{ProvenanceQuery, TimeWindow};
use engram_knowledge::{KnowledgeGraphRepository, KnowledgeRepository};
use engram_runtime::{CoreError, CoreResult};
use engram_store_sqlite::SqlKnowledgeStore;
use futures::executor::block_on;

// ---------- helpers -------------------------------------------------------

fn actor() -> Actor {
    Actor {
        id: Id::from("agent-prov-test"),
        kind: ActorKind::User,
        display_name: None,
        metadata: None,
    }
}

fn scope_a() -> Scope {
    Scope {
        tenant: "tenant-a".to_string(),
        subject: Some("subject-a".to_string()),
        workspace: Some("workspace-a".to_string()),
        session: None,
        environment: Some("test".to_string()),
    }
}

fn scope_b() -> Scope {
    Scope {
        tenant: "tenant-b".to_string(),
        subject: Some("subject-b".to_string()),
        workspace: Some("workspace-b".to_string()),
        session: None,
        environment: Some("test".to_string()),
    }
}

fn evidence(target_id: &str) -> EvidenceRef {
    EvidenceRef {
        target_type: EvidenceTargetType::Entity,
        target_id: Some(target_id.to_string()),
        uri: None,
        quote: Some("supports the claim".to_string()),
        location: None,
    }
}

/// Evidence attached via the write-side op — made distinct (own `uri` + `quote`)
/// from the seeded `evidence(...)` helper so a test can assert "the attached one
/// landed" without ambiguity against the records' pre-existing evidence.
fn attached_evidence(target_id: &str) -> EvidenceRef {
    EvidenceRef {
        target_type: EvidenceTargetType::Entity,
        target_id: Some(target_id.to_string()),
        uri: Some("https://example/attached".to_string()),
        quote: Some("attached after the fact".to_string()),
        location: None,
    }
}

fn provenance_with(
    evidence: Vec<EvidenceRef>,
    observed_at: chrono::DateTime<chrono::Utc>,
) -> Provenance {
    Provenance {
        source: "conformance".to_string(),
        actor: actor(),
        observed_at,
        evidence,
        derivations: Vec::new(),
        confidence: Some(1.0),
        method: Some("manual".to_string()),
    }
}

fn graph(id: &str, stable_source_key: &str, scope: Scope) -> KnowledgeGraph {
    let mut metadata = BTreeMap::new();
    metadata.insert(
        "stableSourceKey".to_string(),
        serde_json::Value::String(stable_source_key.to_string()),
    );
    KnowledgeGraph {
        id: Id::from(id),
        scope,
        name: "prov graph".to_string(),
        uri: None,
        version: None,
        ontology_refs: Vec::new(),
        policy: Policy {
            visibility: Visibility::Workspace,
            retention: Retention::Durable,
            sensitivity: None,
            allowed_uses: vec![AllowedUse::Retrieval],
            expires_at: None,
            delete_mode: None,
        },
        provenance: provenance_with(Vec::new(), chrono::Utc::now()),
        created_at: chrono::Utc::now(),
        updated_at: None,
        metadata: Some(metadata),
    }
}

fn entity(
    id: &str,
    graph_id: &str,
    scope: Scope,
    observed_at: chrono::DateTime<chrono::Utc>,
) -> KnowledgeEntity {
    KnowledgeEntity {
        id: Id::from(id),
        graph_id: Some(Id::from(graph_id)),
        kind: EntityKind::Function,
        name: "fn".to_string(),
        aliases: Vec::new(),
        scope,
        source_refs: Vec::new(),
        concept_refs: Vec::new(),
        ontology_class_refs: Vec::new(),
        provenance: provenance_with(vec![evidence("doc-1")], observed_at),
        created_at: chrono::Utc::now(),
        updated_at: None,
        valid_from: None,
        valid_until: None,
        metadata: None,
    }
}

fn relationship(
    id: &str,
    graph_id: &str,
    scope: Scope,
    observed_at: chrono::DateTime<chrono::Utc>,
) -> KnowledgeRelationship {
    KnowledgeRelationship {
        id: Id::from(id),
        graph_id: Some(Id::from(graph_id)),
        subject: EntityRef {
            id: Some(Id::from("e-subject")),
            kind: Some("function".to_string()),
            name: Some("caller".to_string()),
            aliases: Vec::new(),
        },
        predicate: "calls".to_string(),
        object: EntityRef {
            id: Some(Id::from("e-object")),
            kind: Some("function".to_string()),
            name: Some("callee".to_string()),
            aliases: Vec::new(),
        },
        scope,
        evidence: vec![evidence("doc-rel")],
        confidence: Some(0.9),
        provenance: provenance_with(vec![evidence("doc-1")], observed_at),
        created_at: chrono::Utc::now(),
        updated_at: None,
    }
}

fn source(id: &str, scope: Scope) -> KnowledgeSource {
    KnowledgeSource {
        id: Id::from(id),
        kind: SourceKind::Filesystem,
        scope,
        name: "source".to_string(),
        uri: None,
        version: None,
        policy: Policy {
            visibility: Visibility::Workspace,
            retention: Retention::Durable,
            sensitivity: None,
            allowed_uses: vec![AllowedUse::Retrieval],
            expires_at: None,
            delete_mode: None,
        },
        provenance: provenance_with(vec![evidence("doc-src")], chrono::Utc::now()),
        created_at: chrono::Utc::now(),
        updated_at: None,
        metadata: None,
    }
}

/// Builds a store seeded with a graph (stable_source_key = source id), an
/// entity, a relationship, and a source — all in tenant-a — plus the shared
/// `SqlProvenanceQuery` handle.
fn seeded() -> (
    Arc<SqlKnowledgeStore>,
    SqlProvenanceQuery,
    chrono::DateTime<chrono::Utc>,
) {
    let store = SqlKnowledgeStore::open_in_memory().expect("open_in_memory");
    let store: Arc<SqlKnowledgeStore> = Arc::new(store);
    let observed_at = chrono::Utc::now();
    let stable_key = "source-prov";

    block_on(store.put_graph(graph("graph-1", stable_key, scope_a()))).expect("put_graph");
    block_on(store.put_entity(entity("entity-1", "graph-1", scope_a(), observed_at)))
        .expect("put_entity");
    block_on(store.put_relationship(relationship("rel-1", "graph-1", scope_a(), observed_at)))
        .expect("put_relationship");
    block_on(store.put_source(source(stable_key, scope_a()))).expect("put_source");

    let query = SqlProvenanceQuery::new(store.clone());
    (store, query, observed_at)
}

fn ts(epoch: i64) -> chrono::DateTime<chrono::Utc> {
    use chrono::TimeZone;
    chrono::Utc.timestamp_opt(epoch, 0).single().expect("ts")
}

// ---------- tests ---------------------------------------------------------

#[test]
fn provenance_for_recovers_embedded_provenance() {
    let (_store, query, _observed_at) = seeded();

    let entity_prov =
        block_on(query.provenance_for(EvidenceTargetType::Entity, "entity-1", &scope_a()))
            .expect("provenance_for entity");
    assert_eq!(
        entity_prov.as_ref().map(|p| p.confidence),
        Some(Some(1.0)),
        "entity provenance recovered"
    );

    let rel_prov =
        block_on(query.provenance_for(EvidenceTargetType::Relationship, "rel-1", &scope_a()))
            .expect("provenance_for relationship");
    assert_eq!(
        rel_prov.as_ref().map(|p| p.method.clone()),
        Some(Some("manual".to_string()))
    );

    let src_prov =
        block_on(query.provenance_for(EvidenceTargetType::Source, "source-prov", &scope_a()))
            .expect("provenance_for source");
    assert!(src_prov.is_some(), "source provenance recovered");
}

#[test]
fn evidence_for_combines_relationship_evidence_and_provenance() {
    let (_store, query, _observed_at) = seeded();

    let entity_ev =
        block_on(query.evidence_for(EvidenceTargetType::Entity, "entity-1", &scope_a()))
            .expect("evidence_for entity");
    assert_eq!(entity_ev.len(), 1, "entity carries one provenance evidence");

    // Relationship: 1 relationship-level + 1 provenance-level evidence.
    let rel_ev =
        block_on(query.evidence_for(EvidenceTargetType::Relationship, "rel-1", &scope_a()))
            .expect("evidence_for relationship");
    assert_eq!(rel_ev.len(), 2, "relationship combines both evidence slots");

    let src_ev =
        block_on(query.evidence_for(EvidenceTargetType::Source, "source-prov", &scope_a()))
            .expect("evidence_for source");
    assert_eq!(src_ev.len(), 1, "source carries one provenance evidence");
}

#[test]
fn provenance_by_source_filters_by_stable_source_key() {
    let (_store, query, _observed_at) = seeded();

    let entries =
        block_on(query.provenance_by_source("source-prov", &scope_a(), TimeWindow::open()))
            .expect("provenance_by_source");
    let ids: Vec<&str> = entries.iter().map(|e| e.target_id.as_str()).collect();
    assert!(ids.contains(&"entity-1"), "entity returned by source");
    assert!(ids.contains(&"rel-1"), "relationship returned by source");
}

#[test]
fn disjoint_observed_at_window_returns_empty() {
    let (_store, query, _observed_at) = seeded();

    let future = TimeWindow::open().from(ts(2_000_000_000));
    let entries = block_on(query.provenance_by_source("source-prov", &scope_a(), future))
        .expect("provenance_by_source future");
    assert!(
        entries.is_empty(),
        "disjoint observed_at window excludes all records"
    );
}

#[test]
fn evidence_by_scope_respects_window_and_limit() {
    let (_store, query, _observed_at) = seeded();

    let all = block_on(query.evidence_by_scope(&scope_a(), TimeWindow::open(), 100))
        .expect("evidence_by_scope all");
    assert!(all.len() >= 2, "both entity and relationship in scope");

    let limited = block_on(query.evidence_by_scope(&scope_a(), TimeWindow::open(), 1))
        .expect("evidence_by_scope limit");
    assert_eq!(limited.len(), 1, "limit bounds the result");
}

#[test]
fn scope_isolation_tenant_b_does_not_see_tenant_a() {
    let (_store, query, _observed_at) = seeded();

    let leaked = block_on(query.provenance_for(EvidenceTargetType::Entity, "entity-1", &scope_b()))
        .expect("provenance_for tenant-b");
    assert!(
        leaked.is_none(),
        "tenant-b must not see tenant-a provenance"
    );

    let leaked_by_source =
        block_on(query.provenance_by_source("source-prov", &scope_b(), TimeWindow::open()))
            .expect("provenance_by_source tenant-b");
    assert!(
        leaked_by_source.is_empty(),
        "tenant-b by-source listing must be empty"
    );
}

#[test]
fn unsupported_target_kind_returns_capability_unsupported() {
    let (_store, query, _observed_at) = seeded();

    for kind in [
        EvidenceTargetType::Memory,
        EvidenceTargetType::Belief,
        EvidenceTargetType::Document,
        EvidenceTargetType::Chunk,
        EvidenceTargetType::Concept,
        EvidenceTargetType::Event,
        EvidenceTargetType::Url,
    ] {
        let result: CoreResult<_> = block_on(query.provenance_for(kind.clone(), "any", &scope_a()));
        match result {
            Err(CoreError::CapabilityUnsupported { capability, .. }) => {
                assert_eq!(
                    capability, "episodes_evidence",
                    "unsupported kind {kind:?} returns episodes_evidence capability error"
                );
            }
            other => panic!("expected CapabilityUnsupported for {kind:?}, got {other:?}"),
        }
    }
}

#[test]
fn missing_record_returns_none_not_an_error() {
    let (_store, query, _observed_at) = seeded();

    let missing =
        block_on(query.provenance_for(EvidenceTargetType::Entity, "does-not-exist", &scope_a()))
            .expect("provenance_for missing");
    assert!(
        missing.is_none(),
        "missing record yields None, not an error"
    );

    let missing_ev =
        block_on(query.evidence_for(EvidenceTargetType::Entity, "does-not-exist", &scope_a()))
            .expect("evidence_for missing");
    assert!(
        missing_ev.is_empty(),
        "missing record yields empty evidence"
    );
}

// ---------- attach_evidence (write-side, ADR-0023) -----------------------

#[test]
fn attach_evidence_appends_to_entity_provenance() {
    let (_store, query, _observed_at) = seeded();
    let attached = attached_evidence("entity-1");

    let updated = block_on(query.attach_evidence(
        EvidenceTargetType::Entity,
        "entity-1",
        attached.clone(),
        &scope_a(),
    ))
    .expect("attach_evidence entity");
    assert!(
        updated.evidence.contains(&attached),
        "attach_evidence returns the provenance carrying the attached evidence"
    );

    // Re-read through the query: the evidence persisted into provenance.evidence.
    let reread = block_on(query.provenance_for(EvidenceTargetType::Entity, "entity-1", &scope_a()))
        .expect("provenance_for entity")
        .expect("entity provenance present");
    assert!(
        reread.evidence.contains(&attached),
        "attached evidence persisted in entity provenance.evidence"
    );
}

#[test]
fn attach_evidence_appends_to_relationship_both_slots() {
    // ADR-0023: a relationship carries both its own `evidence` vec and a
    // `Provenance.evidence` list; attach appends to BOTH.
    let (store, query, _observed_at) = seeded();
    let attached = attached_evidence("rel-1");

    let updated = block_on(query.attach_evidence(
        EvidenceTargetType::Relationship,
        "rel-1",
        attached.clone(),
        &scope_a(),
    ))
    .expect("attach_evidence relationship");
    assert!(
        updated.evidence.contains(&attached),
        "attach_evidence returns provenance carrying the attached evidence"
    );

    // Re-read the relationship directly: both slots must carry the evidence.
    let rel = block_on(store.get_relationship(&RelationshipId::from("rel-1"), &scope_a()))
        .expect("get_relationship")
        .expect("relationship present");
    assert!(
        rel.evidence.contains(&attached),
        "relationship.evidence carries the attached evidence"
    );
    assert!(
        rel.provenance.evidence.contains(&attached),
        "relationship.provenance.evidence carries the attached evidence"
    );
}

#[test]
fn attach_evidence_unsupported_target_returns_capability_unsupported() {
    let (_store, query, _observed_at) = seeded();

    let result = block_on(query.attach_evidence(
        EvidenceTargetType::Memory,
        "any",
        attached_evidence("any"),
        &scope_a(),
    ));
    match result {
        Err(CoreError::CapabilityUnsupported { capability, reason }) => {
            assert_eq!(
                capability, "episodes_evidence",
                "write-side unsupported uses the same capability key"
            );
            assert!(
                reason.contains("attach_evidence"),
                "reason names attach_evidence so the short-circuit is distinguishable: {reason}"
            );
        }
        other => panic!("expected CapabilityUnsupported, got {other:?}"),
    }
}

#[test]
fn attach_evidence_missing_record_returns_not_found() {
    let (_store, query, _observed_at) = seeded();

    let result = block_on(query.attach_evidence(
        EvidenceTargetType::Entity,
        "does-not-exist",
        attached_evidence("does-not-exist"),
        &scope_a(),
    ));
    match result {
        Err(CoreError::NotFound { target_id, .. }) => {
            assert_eq!(
                target_id, "does-not-exist",
                "NotFound names the missing record id"
            );
        }
        other => panic!("expected NotFound, got {other:?}"),
    }
}
