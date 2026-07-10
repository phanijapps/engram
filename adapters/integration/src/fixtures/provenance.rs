//! Provenance / evidence capability fixture (engram-host-sdk brief, S2).
//!
//! Writes a source, graph, entity, and relationship — each carrying real
//! `Provenance` and `EvidenceRef` — into an in-memory `SqlKnowledgeStore`, then
//! recovers them through [`SqlProvenanceQuery`]. The capability is only reported
//! `Supported` when this fixture passes during bootstrap. This is the
//! cross-cutting integration test for the `episodes_evidence` read half: it
//! proves the embedded provenance / evidence round-trips through the provider
//! handle and that scope isolation holds.

use std::collections::BTreeMap;

use engram_domain::*;
use engram_integration::{ProvenanceQuery, TimeWindow};
use engram_knowledge::{KnowledgeGraphRepository, KnowledgeRepository};
use engram_runtime::{CoreError, CoreResult};
use engram_store_knowledge_sqlite::SqlKnowledgeStore;
use futures::executor::block_on;

use super::support::{actor, policy, scope};
use crate::SqlProvenanceQuery;

/// Runs the provenance / evidence fixture.
///
/// Writes a graph (with a `stableSourceKey` matching its source so the
/// by-source listings resolve), an entity, a relationship, and the source
/// itself; then recovers their `Provenance` / `EvidenceRef` through the
/// `ProvenanceQuery` handle and verifies scope isolation.
///
/// # Errors
///
/// Returns `CoreError::Adapter` if any write/read fails or scope leaks.
pub fn run_provenance_fixture() -> CoreResult<()> {
    let store = SqlKnowledgeStore::open_in_memory()?;
    let store: std::sync::Arc<SqlKnowledgeStore> = std::sync::Arc::new(store);

    let graph_id = Id::from("graph-prov");
    let entity_id = Id::from("entity-prov");
    let relationship_id = Id::from("relationship-prov");
    let source_id = Id::from("source-prov");
    // The stableSourceKey lifted into the knowledge_graphs index column; the
    // by-source listings filter on it, so it must match the source id we query.
    let stable_source_key = source_id.to_string();

    block_on(store.put_graph(graph(graph_id.clone(), stable_source_key.as_str())))
        .map_err(err("put_graph"))?;
    block_on(store.put_entity(entity_with_evidence(entity_id.clone(), graph_id.clone())))
        .map_err(err("put_entity"))?;
    block_on(store.put_relationship(relationship_with_evidence(
        relationship_id.clone(),
        graph_id.clone(),
    )))
    .map_err(err("put_relationship"))?;
    block_on(store.put_source(source(source_id.clone()))).map_err(err("put_source"))?;

    let query = SqlProvenanceQuery::new(store.clone());
    let scope_a = scope("tenant-a");
    let scope_b = scope("tenant-b");

    // provenance_for recovers the embedded Provenance for each record kind.
    let entity_prov = block_on(query.provenance_for(
        EvidenceTargetType::Entity,
        &entity_id.to_string(),
        &scope_a,
    ))
    .map_err(err("provenance_for(entity)"))?
    .ok_or_else(|| {
        err("provenance_for(entity)")(CoreError::Conflict {
            reason: "entity provenance missing".to_string(),
        })
    })?;
    if entity_prov.confidence != Some(1.0) {
        return Err(err("provenance_for(entity)")(CoreError::Conflict {
            reason: "entity provenance should carry confidence".to_string(),
        }));
    }

    let rel_prov = block_on(query.provenance_for(
        EvidenceTargetType::Relationship,
        &relationship_id.to_string(),
        &scope_a,
    ))
    .map_err(err("provenance_for(relationship)"))?
    .ok_or_else(|| {
        err("provenance_for(relationship)")(CoreError::Conflict {
            reason: "relationship provenance missing".to_string(),
        })
    })?;

    let source_prov = block_on(query.provenance_for(
        EvidenceTargetType::Source,
        &source_id.to_string(),
        &scope_a,
    ))
    .map_err(err("provenance_for(source)"))?
    .ok_or_else(|| {
        err("provenance_for(source)")(CoreError::Conflict {
            reason: "source provenance missing".to_string(),
        })
    })?;
    let _ = source_prov;

    // evidence_for returns the embedded EvidenceRef list. The relationship
    // surface combines its own `evidence` slot with its provenance evidence.
    let entity_ev =
        block_on(query.evidence_for(EvidenceTargetType::Entity, &entity_id.to_string(), &scope_a))
            .map_err(err("evidence_for(entity)"))?;
    if entity_ev.is_empty() {
        return Err(err("evidence_for(entity)")(CoreError::Conflict {
            reason: "entity evidence not recovered".to_string(),
        }));
    }
    let rel_ev = block_on(query.evidence_for(
        EvidenceTargetType::Relationship,
        &relationship_id.to_string(),
        &scope_a,
    ))
    .map_err(err("evidence_for(relationship)"))?;
    // Relationship carries one evidence link plus one provenance evidence.
    if rel_ev.len() < 2 {
        return Err(err("evidence_for(relationship)")(CoreError::Conflict {
            reason: "relationship evidence not combined".to_string(),
        }));
    }

    // provenance_by_source filters by the stable_source_key and the window.
    let open =
        block_on(query.provenance_by_source(&stable_source_key, &scope_a, TimeWindow::open()))
            .map_err(err("provenance_by_source"))?;
    if !open.iter().any(|e| e.target_id == entity_id.to_string())
        || !open
            .iter()
            .any(|e| e.target_id == relationship_id.to_string())
    {
        return Err(err("provenance_by_source")(CoreError::Conflict {
            reason: "by-source did not return both records".to_string(),
        }));
    }

    // A disjoint observed_at window excludes every record.
    let future = TimeWindow::open().from(utc_far_future());
    let none = block_on(query.provenance_by_source(&stable_source_key, &scope_a, future))
        .map_err(err("provenance_by_source(disjoint)"))?;
    if !none.is_empty() {
        return Err(err("provenance_by_source(disjoint)")(CoreError::Conflict {
            reason: "disjoint window should return no records".to_string(),
        }));
    }

    // Scope isolation: tenant-b must not see tenant-a's records.
    let leaked = block_on(query.provenance_for(
        EvidenceTargetType::Entity,
        &entity_id.to_string(),
        &scope_b,
    ))
    .map_err(err("provenance_for(entity, tenant-b)"))?;
    if leaked.is_some() {
        return Err(err("scope_isolation")(CoreError::Conflict {
            reason: "tenant-b must not see tenant-a provenance".to_string(),
        }));
    }

    // Unsupported target kinds return CapabilityUnsupported, not a silent empty.
    let unsupported = block_on(query.provenance_for(EvidenceTargetType::Memory, "any", &scope_a));
    match unsupported {
        Err(CoreError::CapabilityUnsupported { .. }) => {}
        other => {
            return Err(err("unsupported_kind")(CoreError::Conflict {
                reason: format!("expected CapabilityUnsupported for Memory, got {other:?}"),
            }));
        }
    }

    let _ = (rel_prov, entity_ev, rel_ev);
    Ok(())
}

fn utc_far_future() -> Timestamp {
    use chrono::TimeZone;
    // Year 3000 — safely after any fixture observed_at.
    chrono::Utc
        .with_ymd_and_hms(3000, 1, 1, 0, 0, 0)
        .single()
        .expect("valid future timestamp")
}

// ---------- domain constructors -------------------------------------------

fn graph(id: Id, stable_source_key: &str) -> KnowledgeGraph {
    let mut metadata = BTreeMap::new();
    metadata.insert(
        "stableSourceKey".to_string(),
        serde_json::Value::String(stable_source_key.to_string()),
    );
    KnowledgeGraph {
        id,
        scope: scope("tenant-a"),
        name: "Provenance Graph".to_owned(),
        uri: None,
        version: None,
        ontology_refs: Vec::new(),
        policy: policy(),
        provenance: provenance(None),
        created_at: chrono::Utc::now(),
        updated_at: None,
        metadata: Some(metadata),
    }
}

fn evidence_ref(target_id: &str) -> EvidenceRef {
    EvidenceRef {
        target_type: EvidenceTargetType::Entity,
        target_id: Some(target_id.to_string()),
        uri: None,
        quote: Some("supports the claim".to_string()),
        location: None,
    }
}

fn entity_with_evidence(id: Id, graph_id: Id) -> KnowledgeEntity {
    KnowledgeEntity {
        id,
        graph_id: Some(graph_id),
        kind: EntityKind::Function,
        name: "provenanced_fn".to_owned(),
        aliases: Vec::new(),
        scope: scope("tenant-a"),
        source_refs: Vec::new(),
        concept_refs: Vec::new(),
        provenance: provenance(Some(evidence_ref("doc-1"))),
        created_at: chrono::Utc::now(),
        updated_at: None,
        valid_from: None,
        valid_until: None,
        metadata: None,
    }
}

fn relationship_with_evidence(id: Id, graph_id: Id) -> KnowledgeRelationship {
    KnowledgeRelationship {
        id,
        graph_id: Some(graph_id),
        subject: EntityRef {
            id: Some(Id::from("entity-prov")),
            kind: Some("function".to_owned()),
            name: Some("caller".to_owned()),
            aliases: Vec::new(),
        },
        predicate: "calls".to_owned(),
        object: EntityRef {
            id: Some(Id::from("entity-other")),
            kind: Some("function".to_owned()),
            name: Some("callee".to_owned()),
            aliases: Vec::new(),
        },
        scope: scope("tenant-a"),
        // A relationship-level evidence link distinct from provenance evidence.
        evidence: vec![evidence_ref("doc-rel")],
        confidence: Some(0.9),
        provenance: provenance(Some(evidence_ref("doc-1"))),
        created_at: chrono::Utc::now(),
        updated_at: None,
    }
}

fn source(id: Id) -> KnowledgeSource {
    KnowledgeSource {
        id,
        kind: SourceKind::Filesystem,
        scope: scope("tenant-a"),
        name: "provenance source".to_owned(),
        uri: None,
        version: None,
        policy: policy(),
        provenance: provenance(None),
        created_at: chrono::Utc::now(),
        updated_at: None,
        metadata: None,
    }
}

/// Provenance carrying optional embedded evidence.
fn provenance(evidence: Option<EvidenceRef>) -> Provenance {
    Provenance {
        source: "conformance".to_owned(),
        actor: actor(),
        observed_at: chrono::Utc::now(),
        evidence: evidence.into_iter().collect(),
        derivations: Vec::new(),
        confidence: Some(1.0),
        method: Some("manual".to_owned()),
    }
}

fn err<'a>(op: &'a str) -> impl Fn(CoreError) -> CoreError + 'a {
    move |e| CoreError::Adapter {
        adapter: "conformance.provenance".to_string(),
        message: format!("{op}: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provenance_fixture_passes() {
        if let Err(e) = run_provenance_fixture() {
            panic!("provenance fixture failed: {e}");
        }
    }
}
