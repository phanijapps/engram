//! Cross-file name resolution for non-code-source entities.
//!
//! Code-source symbols are resolved by the scanner's generation-aware
//! reference pass. This legacy name matcher must not synthesize unowned edges
//! from those symbols.

use engram_domain::{
    Actor, ActorKind, CodeSourceRecordKind, EntityRef, Id, KnowledgeRelationship, Provenance, Scope,
};
use engram_knowledge::KnowledgeRepository;
use engram_runtime::{CoreError, CoreResult};
use engram_store_sqlite::SqlKnowledgeStore;
use futures::executor::block_on;

use std::collections::{HashMap, HashSet};

/// Resolve same-name entities across graphs, excluding every entity owned by a
/// code-source manifest (including inactive manifests). Existing legacy
/// `rel-xfile-*` edges touching a code-source entity are removed so they cannot
/// outlive or bypass generation-aware scanner resolution. Returns store or
/// ownership errors to the scan host instead of silently dropping them.
pub fn resolve_cross_file_edges(store: &SqlKnowledgeStore, scope: &Scope) -> CoreResult<()> {
    // Read every input before mutating. If source ownership cannot be read, do
    // not fall back to treating code symbols as ordinary entities.
    let source_port = store
        .code_sources()
        .ok_or_else(|| CoreError::CapabilityUnsupported {
            capability: "code_source_preservation".into(),
            reason: "source ownership is required for cross-file resolution".into(),
        })?;
    let manifests = block_on(source_port.list_code_sources(scope))?;
    // Generic list ports permit partial scope queries; this post-scan write
    // pass must use the same exact tuple as its file ownership inventory.
    let entities: Vec<_> = block_on(store.list_entities(scope))?
        .into_iter()
        .filter(|e| e.scope == *scope)
        .collect();
    let relationships: Vec<_> = block_on(store.list_relationships(scope))?
        .into_iter()
        .filter(|r| r.scope == *scope)
        .collect();

    let mut source_entities = HashSet::new();
    let mut source_relationships = HashSet::new();
    for manifest in &manifests {
        // The adapter contract promises exact-scope results. Retain this check
        // at the caller boundary so a faulty implementation fails closed.
        if manifest.file.scope != *scope {
            return Err(CoreError::PolicyDenied {
                reason: "source ownership returned a different scope".into(),
            });
        }
        for record in &manifest.records {
            match record.kind {
                CodeSourceRecordKind::Entity => {
                    source_entities.insert(record.id.clone());
                }
                CodeSourceRecordKind::Relationship => {
                    source_relationships.insert(record.id.clone());
                }
                _ => {}
            }
        }
    }

    // Remove only legacy synthetic cross-file edges that now touch a
    // source-owned entity. Never delete a relation explicitly owned by a file
    // generation through the generic CRUD path.
    let mut removed = HashSet::new();
    for relationship in &relationships {
        if !relationship.id.as_str().starts_with("rel-xfile-") {
            continue;
        }
        let touches_source = relationship
            .subject
            .id
            .as_ref()
            .is_some_and(|id| source_entities.contains(id.as_str()))
            || relationship
                .object
                .id
                .as_ref()
                .is_some_and(|id| source_entities.contains(id.as_str()));
        if !touches_source {
            continue;
        }
        if source_relationships.contains(relationship.id.as_str()) {
            return Err(CoreError::Conflict {
                reason: "legacy cross-file edge is owned by a source generation".into(),
            });
        }
        block_on(store.delete_relationship(&relationship.id, scope))?;
        removed.insert(relationship.id.to_string());
    }

    // Group only entities that are not owned by any source generation.
    let mut by_name: HashMap<String, Vec<(String, String)>> = HashMap::new();
    for entity in &entities {
        if source_entities.contains(entity.id.as_str()) {
            continue;
        }
        let graph = entity
            .graph_id
            .as_ref()
            .map(ToString::to_string)
            .unwrap_or_default();
        by_name
            .entry(entity.name.to_lowercase())
            .or_default()
            .push((entity.id.to_string(), graph));
    }

    let mut existing = HashSet::new();
    for relationship in &relationships {
        if removed.contains(relationship.id.as_str()) {
            continue;
        }
        existing.insert((
            relationship
                .subject
                .id
                .as_ref()
                .map(ToString::to_string)
                .unwrap_or_default(),
            relationship.predicate.clone(),
            relationship
                .object
                .id
                .as_ref()
                .map(ToString::to_string)
                .unwrap_or_default(),
        ));
    }

    let now = chrono::Utc::now();
    let provenance = Provenance {
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

    for entries in by_name.values() {
        if entries.len() < 2 {
            continue;
        }
        for i in 0..entries.len() {
            for j in (i + 1)..entries.len() {
                let (id_a, graph_a) = &entries[i];
                let (id_b, graph_b) = &entries[j];
                if graph_a == graph_b {
                    continue;
                }

                let key_ab = (id_a.clone(), "defined_in".to_owned(), id_b.clone());
                if existing.contains(&key_ab) {
                    continue;
                }
                let relationship = KnowledgeRelationship {
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
                    scope: scope.clone(),
                    evidence: Vec::new(),
                    confidence: Some(0.8),
                    provenance: provenance.clone(),
                    created_at: now,
                    updated_at: None,
                    archived_at: None,
                };
                block_on(store.put_relationship(relationship))?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use engram_domain::{
        ChunkId, CodeByteRange, CodeChunkDescriptor, CodeChunkRole, CodeFileKey, CodeParseQuality,
        CodeSourceGeneration, CodeSourceRecord, CodeSourceState, DocumentId, EntityKind,
        KnowledgeChunk, KnowledgeChunkKind, KnowledgeEntity, KnowledgeSource, Policy, Retention,
        Sensitivity, SourceDocument, SourceDocumentKind, SourceId, SourceKind, Visibility,
    };
    use engram_knowledge::CodeSourceWrite;
    use engram_knowledge::KnowledgeRepository;

    fn scope() -> Scope {
        Scope {
            tenant: "cross-file-test".into(),
            subject: None,
            workspace: None,
            session: None,
            environment: None,
        }
    }

    fn actor() -> Actor {
        Actor {
            id: Id::from("cross-file-test-actor"),
            kind: ActorKind::Agent,
            display_name: None,
            metadata: None,
        }
    }

    fn policy() -> Policy {
        Policy {
            visibility: Visibility::Workspace,
            retention: Retention::Durable,
            sensitivity: Some(Sensitivity::Low),
            allowed_uses: vec![],
            expires_at: None,
            delete_mode: None,
        }
    }

    fn entity(id: &str, name: &str, graph: &str) -> KnowledgeEntity {
        let now = Utc::now();
        KnowledgeEntity {
            id: Id::from(id),
            graph_id: Some(Id::from(graph)),
            kind: EntityKind::Concept,
            name: name.into(),
            aliases: vec![],
            scope: scope(),
            source_refs: vec![],
            concept_refs: vec![],
            ontology_class_refs: vec![],
            provenance: Provenance {
                source: "test".into(),
                actor: actor(),
                observed_at: now,
                evidence: vec![],
                derivations: vec![],
                confidence: None,
                method: None,
            },
            created_at: now,
            updated_at: None,
            valid_from: None,
            valid_until: None,
            metadata: None,
            archived_at: None,
        }
    }

    fn relationship(id: &str, subject: &str, object: &str) -> KnowledgeRelationship {
        let now = Utc::now();
        KnowledgeRelationship {
            id: Id::from(id),
            graph_id: None,
            subject: EntityRef {
                id: Some(Id::from(subject)),
                kind: None,
                name: None,
                aliases: vec![],
            },
            predicate: "manual".into(),
            object: EntityRef {
                id: Some(Id::from(object)),
                kind: None,
                name: None,
                aliases: vec![],
            },
            scope: scope(),
            evidence: vec![],
            confidence: None,
            provenance: Provenance {
                source: "test".into(),
                actor: actor(),
                observed_at: now,
                evidence: vec![],
                derivations: vec![],
                confidence: None,
                method: None,
            },
            created_at: now,
            updated_at: None,
            archived_at: None,
        }
    }

    #[test]
    fn partial_scope_cannot_create_edges_from_an_unlisted_child_scope() {
        let store = SqlKnowledgeStore::open_in_memory().unwrap();
        block_on(store.put_entity(entity("a", "same", "ga"))).unwrap();
        let mut child = entity("b", "same", "gb");
        child.scope.session = Some("private-session".into());
        block_on(store.put_entity(child)).unwrap();
        resolve_cross_file_edges(&store, &scope()).unwrap();
        assert!(
            block_on(store.list_relationships(&scope()))
                .unwrap()
                .is_empty()
        );
    }

    fn own_entity(store: &SqlKnowledgeStore, id: &str) {
        let scope = scope();
        let now = Utc::now();
        let file = CodeFileKey {
            scope: scope.clone(),
            repository: "repo".into(),
            path: format!("{id}.ts"),
        };
        let port = store.code_sources().unwrap();
        let source_id = SourceId::from(format!("source-{id}"));
        let document_id = DocumentId::from(format!("document-{id}"));
        let snapshot_id = ChunkId::from(format!("snapshot-{id}"));
        let source_hash = "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
        let fingerprint = "test-fingerprint";
        let provenance = Provenance {
            source: "test".into(),
            actor: actor(),
            observed_at: now,
            evidence: vec![],
            derivations: vec![],
            confidence: None,
            method: Some("source_preservation_v1".into()),
        };
        let manifest = CodeSourceGeneration {
            file: file.clone(),
            generation: format!("generation-{id}"),
            attempt_id: format!("attempt-{id}"),
            source_hash: source_hash.into(),
            fingerprint: fingerprint.into(),
            state: CodeSourceState::Updating,
            byte_length: 0,
            document_id: Some(document_id.clone()),
            snapshot_ids: vec![snapshot_id.clone()],
            records: vec![
                CodeSourceRecord {
                    kind: CodeSourceRecordKind::Source,
                    id: source_id.to_string(),
                },
                CodeSourceRecord {
                    kind: CodeSourceRecordKind::Document,
                    id: document_id.to_string(),
                },
                CodeSourceRecord {
                    kind: CodeSourceRecordKind::Chunk,
                    id: snapshot_id.to_string(),
                },
                CodeSourceRecord {
                    kind: CodeSourceRecordKind::Entity,
                    id: id.into(),
                },
            ],
            policy: policy(),
            provenance: provenance.clone(),
        };
        block_on(port.begin_code_source(manifest.clone(), None)).unwrap();
        block_on(port.stage_code_records(
            &file,
            &manifest.attempt_id,
            &[CodeSourceRecord {
                kind: CodeSourceRecordKind::Entity,
                id: id.into(),
            }],
        ))
        .unwrap();
        block_on(port.stage_code_records(&file, &manifest.attempt_id, &manifest.records)).unwrap();
        block_on(port.write_code_record(
            &file,
            &manifest.attempt_id,
            CodeSourceWrite::Source(KnowledgeSource {
                id: source_id.clone(),
                kind: SourceKind::Filesystem,
                scope: scope.clone(),
                name: "test-source".into(),
                uri: None,
                version: None,
                policy: manifest.policy.clone(),
                provenance: provenance.clone(),
                created_at: now,
                updated_at: None,
                metadata: Some(std::collections::BTreeMap::from([(
                    "stableSourceKey".into(),
                    serde_json::json!("repo"),
                )])),
            }),
        ))
        .unwrap();
        block_on(port.write_code_record(
            &file,
            &manifest.attempt_id,
            CodeSourceWrite::Document(SourceDocument {
                id: document_id.clone(),
                source_id: source_id.clone(),
                kind: SourceDocumentKind::Code,
                uri: None,
                path: Some(file.path.clone()),
                title: None,
                mime_type: None,
                language: None,
                version: None,
                content_hash: source_hash.into(),
                provenance: provenance.clone(),
                policy: manifest.policy.clone(),
                created_at: now,
                updated_at: None,
                metadata: None,
            }),
        ))
        .unwrap();
        block_on(port.write_code_record(
            &file,
            &manifest.attempt_id,
            CodeSourceWrite::Chunk(KnowledgeChunk {
                code: Some(CodeChunkDescriptor {
                    role: CodeChunkRole::SourceSnapshot,
                    file: file.clone(),
                    generation: manifest.generation.clone(),
                    source_hash: source_hash.into(),
                    fingerprint: fingerprint.into(),
                    ranges: vec![CodeByteRange { start: 0, end: 0 }],
                    slice_separator: String::new(),
                    declaration_range: None,
                    complete: true,
                    parse_quality: CodeParseQuality::Clean,
                    unsupported_features: vec![],
                    unit: None,
                    parent_unit: None,
                }),
                document_kind: Some(SourceDocumentKind::Code),
                id: snapshot_id,
                document_id,
                source_id,
                kind: KnowledgeChunkKind::CodeBlock,
                text: String::new(),
                summary: None,
                location: None,
                entities: vec![],
                concepts: vec![],
                embedding_refs: vec![],
                content_hash: source_hash.into(),
                provenance: provenance.clone(),
                policy: manifest.policy.clone(),
                created_at: now,
                updated_at: None,
                metadata: None,
            }),
        ))
        .unwrap();
        let mut owned_entity = entity(id, "SharedName", "unused-graph");
        owned_entity.graph_id = None;
        block_on(port.write_code_record(
            &file,
            &manifest.attempt_id,
            CodeSourceWrite::Entity(owned_entity),
        ))
        .unwrap();
        let mut active = manifest;
        active.state = CodeSourceState::Active;
        block_on(port.activate_code_source(active)).unwrap();
    }

    #[test]
    fn source_owned_name_matches_do_not_emit_edges_and_stale_edges_are_removed() {
        let store = SqlKnowledgeStore::open_in_memory().unwrap();
        let a = entity("source-a", "SharedName", "graph-a");
        let b = entity("plain-b", "SharedName", "graph-b");
        block_on(store.put_entity(a)).unwrap();
        block_on(store.put_entity(b)).unwrap();
        own_entity(&store, "source-a"); // deliberately incomplete manifest

        let stale = relationship("rel-xfile-source-a-plain-b", "source-a", "plain-b");
        let keep = relationship("rel-xfile-unrelated", "plain-b", "other");
        block_on(store.put_relationship(stale)).unwrap();
        block_on(store.put_relationship(keep)).unwrap();

        resolve_cross_file_edges(&store, &scope()).unwrap();
        let edges = block_on(store.list_relationships(&scope())).unwrap();
        assert!(edges.iter().any(|r| r.id.as_str() == "rel-xfile-unrelated"));
        assert!(
            !edges
                .iter()
                .any(|r| r.id.as_str() == "rel-xfile-source-a-plain-b")
        );
        assert!(!edges.iter().any(|r| {
            r.predicate == "defined_in"
                && (r
                    .subject
                    .id
                    .as_ref()
                    .is_some_and(|id| id.as_str() == "source-a")
                    || r.object
                        .id
                        .as_ref()
                        .is_some_and(|id| id.as_str() == "source-a"))
        }));
    }

    #[test]
    fn non_source_name_matches_still_resolve() {
        let store = SqlKnowledgeStore::open_in_memory().unwrap();
        block_on(store.put_entity(entity("plain-a", "Same", "graph-a"))).unwrap();
        block_on(store.put_entity(entity("plain-b", "Same", "graph-b"))).unwrap();

        resolve_cross_file_edges(&store, &scope()).unwrap();
        let edges = block_on(store.list_relationships(&scope())).unwrap();
        assert!(edges.iter().any(|r| {
            r.predicate == "defined_in"
                && r.subject
                    .id
                    .as_ref()
                    .is_some_and(|id| id.as_str() == "plain-a")
                && r.object
                    .id
                    .as_ref()
                    .is_some_and(|id| id.as_str() == "plain-b")
        }));
    }
}
