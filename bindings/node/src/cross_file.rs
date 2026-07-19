//! Cross-file entity resolution for repository scans.
//!
//! After a scan, connects entities that share a name across different graphs.
//! For each entity whose name appears in multiple graphs, creates a "defined_in"
//! relationship so the Q&A + explorer see cross-file edges. Best-effort — errors
//! are silently ignored (the scan summary is already captured).

use engram_domain::{
    Actor, ActorKind, AllowedUse, DeleteMode, EntityRef, Id, KnowledgeRelationship, Policy,
    Retention, Scope, Sensitivity, Visibility,
};
use engram_knowledge::KnowledgeRepository;
use engram_store_sqlite::SqlKnowledgeStore;
use futures::executor::block_on;

use std::collections::{HashMap, HashSet};

/// After a scan, connects entities that share a name across different graphs.
/// For each entity whose name appears in multiple graphs, creates a "defined_in"
/// relationship so the Q&A + explorer see cross-file edges. Best-effort — errors
/// are silently ignored (the scan summary is already captured).
pub fn resolve_cross_file_edges(store: &SqlKnowledgeStore, scope: &Scope) {
    let entities = match block_on(store.list_entities(scope)) {
        Ok(e) => e,
        Err(_) => return,
    };
    let relationships = match block_on(store.list_relationships(scope)) {
        Ok(r) => r,
        Err(_) => return,
    };

    // Group entity IDs by name (lowercased) → Vec<(entity_id, graph_id)>.
    let mut by_name: HashMap<String, Vec<(String, String)>> = HashMap::new();
    for e in &entities {
        let name = e.name.to_lowercase();
        let gid = e
            .graph_id
            .as_ref()
            .map(|g| g.to_string())
            .unwrap_or_default();
        by_name
            .entry(name)
            .or_default()
            .push((e.id.to_string(), gid));
    }

    // Collect existing relationship keys to avoid duplicates.
    let mut existing: HashSet<(String, String, String)> = HashSet::new();
    for r in &relationships {
        existing.insert((
            r.subject
                .id
                .as_ref()
                .map(|i| i.to_string())
                .unwrap_or_default(),
            r.predicate.clone(),
            r.object
                .id
                .as_ref()
                .map(|i| i.to_string())
                .unwrap_or_default(),
        ));
    }

    // For each name that appears in multiple graphs, create cross-graph "defined_in" edges.
    let now = chrono::Utc::now();
    let prov = engram_domain::Provenance {
        source: "cross-file-resolver".to_owned(),
        actor: Actor {
            id: Id::from("engram-cross-file"),
            kind: ActorKind::System,
            display_name: Some("Cross-file resolver".to_owned()),
            metadata: None,
        },
        observed_at: now,
        evidence: Vec::new(),
        derivations: Vec::new(),
        confidence: Some(0.8),
        method: Some("name_match_resolution".to_owned()),
    };

    let scope_owned = scope.clone();
    let _policy = Policy {
        visibility: Visibility::Workspace,
        retention: Retention::Durable,
        sensitivity: Some(Sensitivity::Low),
        allowed_uses: vec![AllowedUse::Retrieval],
        expires_at: None,
        delete_mode: Some(DeleteMode::Tombstone),
    };

    for entries in by_name.values() {
        if entries.len() < 2 {
            continue;
        }

        // Create bidirectional "defined_in" edges between all pairs.
        for i in 0..entries.len() {
            for j in (i + 1)..entries.len() {
                let (id_a, graph_a) = &entries[i];
                let (id_b, graph_b) = &entries[j];

                if graph_a == graph_b {
                    continue;
                }

                let key_ab = (id_a.clone(), "defined_in".to_owned(), id_b.clone());
                if !existing.contains(&key_ab) {
                    let rel = KnowledgeRelationship {
                        id: Id::from(format!("rel-xfile-{id_a}-{id_b}")),
                        graph_id: None,
                        subject: EntityRef {
                            id: Some(Id::from(id_a.clone())),
                            kind: None,
                            name: None,
                            aliases: Vec::new(),
                        },
                        predicate: "defined_in".to_owned(),
                        object: EntityRef {
                            id: Some(Id::from(id_b.clone())),
                            kind: None,
                            name: None,
                            aliases: Vec::new(),
                        },
                        scope: scope_owned.clone(),
                        evidence: Vec::new(),
                        confidence: Some(0.8),
                        provenance: prov.clone(),
                        created_at: now,
                        updated_at: None,
                    };
                    let _ = block_on(store.put_relationship(rel));
                }
            }
        }
    }
}
