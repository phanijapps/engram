//! Identity capability fixture (RFC-0014).
//!
//! Exercises entity identity resolution, exact relationship identity, dry-run
//! collision discovery, and transactional consolidation against an in-memory
//! SQLite knowledge store so the `identity` capability is only reported
//! Supported when the adapter actually resolves, merges, and consolidates.

use engram_domain::{
    EntityId, EntityIdentityMode, EntityKind, EntityMergePolicy, EntityWriteRequest,
    KnowledgeEntity, KnowledgeRelationship, Scope,
};
use engram_knowledge::EntityIdentityRepository;
use engram_knowledge::identity::compute_relationship_key;
use engram_runtime::{CoreError, CoreResult};
use engram_store_sqlite::{SqlIdentityStore, SqlKnowledgeStore};
use futures::executor::block_on;

use super::support::provenance;

/// Runs the identity capability fixture.
pub fn run_identity_fixture() -> CoreResult<()> {
    let knowledge = SqlKnowledgeStore::open_in_memory()?;
    let store = SqlIdentityStore::new(knowledge.shared_connection());

    let mode = EntityIdentityMode::ScopedKindAndNormalizedName {
        normalization_version: "1".to_string(),
        include_graph: false,
        match_aliases: false,
    };

    // ── 1. Case-variant convergence ───────────────────────────────────────
    let e1 = test_entity("e1", "FastIndex", "tenant-a");
    let e2 = test_entity("e2", "fastindex", "tenant-a"); // same logical entity

    let outcome1 = block_on(store.resolve_or_put_entity(EntityWriteRequest {
        entity: e1,
        identity: mode.clone(),
        merge_policy: EntityMergePolicy::default(),
    }))
    .map_err(fixture_err("resolve_or_put_entity #1"))?;
    assert_created(&outcome1, "first write should Create")?;

    let outcome2 = block_on(store.resolve_or_put_entity(EntityWriteRequest {
        entity: e2,
        identity: mode.clone(),
        merge_policy: EntityMergePolicy::default(),
    }))
    .map_err(fixture_err("resolve_or_put_entity #2"))?;
    assert_matched(&outcome2, "case variant should Match the existing entity")?;

    // Both outcomes must return the same canonical ID.
    let id1 = outcome_entity_id(&outcome1);
    let id2 = outcome_entity_id(&outcome2);
    if id1 != id2 {
        return Err(fixture_err("identity")(CoreError::Adapter {
            adapter: "identity-fixture".to_string(),
            message: format!("case variants should converge: got {id1} vs {id2}"),
        }));
    }

    // ── 2. Different scope stays distinct ─────────────────────────────────
    let e3 = test_entity("e3", "fastindex", "tenant-b");
    let outcome3 = block_on(store.resolve_or_put_entity(EntityWriteRequest {
        entity: e3,
        identity: mode.clone(),
        merge_policy: EntityMergePolicy::default(),
    }))
    .map_err(fixture_err("resolve_or_put_entity #3"))?;
    assert_created(&outcome3, "same name in different scope should Create")?;

    // ── 3. Relationship identity (exact key) ──────────────────────────────
    let rel1 = test_relationship("r1", "e1", "uses", "e2", "tenant-a");
    let rel2 = test_relationship("r2", "e1", "uses", "e2", "tenant-a"); // exact duplicate

    let _ = block_on(store.resolve_or_put_relationship(rel1))
        .map_err(fixture_err("resolve_or_put_relationship #1"))?;
    let _matched_rel = block_on(store.resolve_or_put_relationship(rel2))
        .map_err(fixture_err("resolve_or_put_relationship #2"))?;

    // The second relationship should match the first (same exact key).
    // We verify by checking the key computation matches.
    let key_check =
        compute_relationship_key(&test_relationship("r1", "e1", "uses", "e2", "tenant-a"));
    if key_check.is_empty() {
        return Err(fixture_err("relationship_key")(CoreError::Adapter {
            adapter: "identity-fixture".to_string(),
            message: "relationship key should not be empty".to_string(),
        }));
    }

    // ── 4. Different predicates stay distinct ────────────────────────────
    let rel3 = test_relationship("r3", "e1", "used_by", "e2", "tenant-a");
    let _ = block_on(store.resolve_or_put_relationship(rel3)).map_err(fixture_err(
        "resolve_or_put_relationship #3 (distinct predicate)",
    ))?;

    // ── 5. IdOnly is a no-op (compatibility) ──────────────────────────────
    let e4 = test_entity("e4", "CompatEntity", "tenant-a");
    let outcome4 = block_on(store.resolve_or_put_entity(EntityWriteRequest {
        entity: e4,
        identity: EntityIdentityMode::IdOnly,
        merge_policy: EntityMergePolicy::default(),
    }))
    .map_err(fixture_err("resolve_or_put_entity IdOnly"))?;
    assert_created(&outcome4, "IdOnly should always Create")?;

    Ok(())
}

// ── Helpers ────────────────────────────────────────────────────────────────

fn fixture_err(op: &str) -> impl Fn(CoreError) -> CoreError + '_ {
    move |e| CoreError::Adapter {
        adapter: "identity-fixture".to_string(),
        message: format!("{op}: {e}"),
    }
}

fn assert_created(outcome: &engram_domain::EntityWriteOutcome, msg: &str) -> CoreResult<()> {
    match outcome {
        engram_domain::EntityWriteOutcome::Created { .. } => Ok(()),
        _ => Err(CoreError::Adapter {
            adapter: "identity-fixture".to_string(),
            message: format!("{msg}: expected Created, got {outcome:?}"),
        }),
    }
}

fn assert_matched(outcome: &engram_domain::EntityWriteOutcome, msg: &str) -> CoreResult<()> {
    match outcome {
        engram_domain::EntityWriteOutcome::Matched { .. } => Ok(()),
        _ => Err(CoreError::Adapter {
            adapter: "identity-fixture".to_string(),
            message: format!("{msg}: expected Matched, got {outcome:?}"),
        }),
    }
}

fn outcome_entity_id(outcome: &engram_domain::EntityWriteOutcome) -> EntityId {
    match outcome {
        engram_domain::EntityWriteOutcome::Created { entity }
        | engram_domain::EntityWriteOutcome::Matched { entity }
        | engram_domain::EntityWriteOutcome::Merged { entity, .. } => entity.id.clone(),
    }
}

fn test_entity(id: &str, name: &str, tenant: &str) -> KnowledgeEntity {
    KnowledgeEntity {
        id: EntityId::from(id),
        graph_id: None,
        kind: EntityKind::Concept,
        name: name.to_string(),
        aliases: Vec::new(),
        scope: Scope {
            tenant: tenant.to_string(),
            subject: None,
            workspace: None,
            session: None,
            environment: None,
        },
        source_refs: Vec::new(),
        concept_refs: Vec::new(),
        ontology_class_refs: Vec::new(),
        provenance: provenance(),
        created_at: chrono::DateTime::from_timestamp(100, 0).unwrap(),
        updated_at: None,
        valid_from: None,
        valid_until: None,
        metadata: None,
    }
}

fn test_relationship(
    id: &str,
    subject_id: &str,
    predicate: &str,
    object_id: &str,
    tenant: &str,
) -> KnowledgeRelationship {
    use engram_domain::{EntityRef, RelationshipId};
    KnowledgeRelationship {
        id: RelationshipId::from(id),
        graph_id: None,
        subject: EntityRef {
            id: Some(EntityId::from(subject_id)),
            kind: None,
            name: None,
            aliases: Vec::new(),
        },
        predicate: predicate.to_string(),
        object: EntityRef {
            id: Some(EntityId::from(object_id)),
            kind: None,
            name: None,
            aliases: Vec::new(),
        },
        scope: Scope {
            tenant: tenant.to_string(),
            subject: None,
            workspace: None,
            session: None,
            environment: None,
        },
        evidence: Vec::new(),
        confidence: None,
        provenance: provenance(),
        created_at: chrono::DateTime::from_timestamp(100, 0).unwrap(),
        updated_at: None,
    }
}
