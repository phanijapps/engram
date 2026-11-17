#![cfg(any(feature = "embedded", feature = "service"))]
#[path = "support/data_fixture.rs"]
mod fixture;
use engram_domain::*;
use engram_knowledge::{
    EntityIdentityRepository, KnowledgeGraphRepository, KnowledgeRepository, OntologyRepository,
    TaxonomyRepository,
};
use engram_memory::{MemoryEventRepository, MemoryRepository};
use fixture::*;
use futures::executor::block_on;
use serde_json::json;
#[cfg(feature = "embedded")]
#[test]
fn embedded_complete_data_ports() {
    exercise(Mode::Embedded)
}
#[cfg(feature = "service")]
#[test]
fn service_complete_data_ports() {
    exercise(Mode::Service)
}
fn exercise(mode: Mode) {
    block_on(async {
        let mut f = Fixture::new(mode);
        let s = &f.store;
        let scope = scope();
        let mut other = scope.clone();
        other.subject = Some(String::new());
        let m = memory("memory");
        assert_eq!(s.put_memory(m.clone()).await.unwrap(), m);
        assert_eq!(s.get_memory(&m.id, &other).await.unwrap(), None);
        let mut foreign = m.clone();
        foreign.scope = other.clone();
        assert!(s.put_memory(foreign).await.is_err());
        let event: MemoryEvent = make(
            "event",
            json!({"kind":"written","actor":{"id":"actor","kind":"agent"},"memoryId":"memory","payload":{"reason":"write"},"occurredAt":"2026-11-17T00:00:00Z","recordedAt":"2026-11-17T00:00:01Z"}),
        );
        s.append_event(event.clone()).await.unwrap();
        s.append_event(event.clone()).await.unwrap();
        let mut changed = event.clone();
        changed.kind = MemoryEventKind::Forgotten;
        assert!(s.append_event(changed).await.is_err());
        assert_eq!(
            s.get_event(&event.id, &scope).await.unwrap(),
            Some(event.clone())
        );
        assert_eq!(
            s.list_events_for_memory(&m.id, &scope).await.unwrap(),
            vec![event.clone()]
        );
        assert_eq!(s.list_events_for_scope(&other).await.unwrap(), vec![]);
        assert_eq!(
            s.update_memory_status(&m.id, &scope, MemoryStatus::Archived)
                .await
                .unwrap()
                .status,
            MemoryStatus::Archived
        );
        s.put_memory(memory("memory-2")).await.unwrap();
        let page = s.list_memories_paged(&scope, None, 1).await.unwrap();
        assert_eq!(page.items.len(), 1);
        let next = s
            .list_memories_paged(&scope, page.next_cursor.as_ref(), 1)
            .await
            .unwrap();
        assert_eq!(next.items.len(), 1);
        assert!(next.next_cursor.is_none());
        assert_ne!(page.items[0].id, next.items[0].id);
        assert!(s.list_memories_paged(&scope, None, 0).await.is_err());
        let source: KnowledgeSource = make("source", json!({"kind":"generated"}));
        s.put_source(source.clone()).await.unwrap();
        assert_eq!(
            s.get_source(&source.id, &scope).await.unwrap(),
            Some(source)
        );
        assert_eq!(s.list_sources(&scope).await.unwrap().len(), 1);
        let doc: SourceDocument = make(
            "doc",
            json!({"sourceId":"source","kind":"text","contentHash":"hash"}),
        );
        s.put_document(doc.clone()).await.unwrap();
        assert_eq!(s.list_documents(&scope).await.unwrap(), vec![doc.clone()]);
        let chunk: KnowledgeChunk = make(
            "chunk",
            json!({"sourceId":"source","documentId":"doc","kind":"paragraph","text":"chunk","contentHash":"hash"}),
        );
        s.put_chunk(chunk.clone()).await.unwrap();
        assert_eq!(
            s.get_chunk(&chunk.id, &scope).await.unwrap(),
            Some(chunk.clone())
        );
        assert_eq!(
            s.list_chunks_by_document(&doc.id, &scope).await.unwrap(),
            vec![chunk.clone()]
        );
        let mut bad = chunk.clone();
        bad.source_id = "missing".into();
        assert!(s.put_chunk(bad).await.is_err());
        let graph: KnowledgeGraph = make(
            "graph",
            json!({"metadata":{"stableSourceKey":"stable-source","path":"file"}}),
        );
        s.put_graph(graph.clone()).await.unwrap();
        assert_eq!(
            s.list_graphs_by_source(&scope, "stable-source")
                .await
                .unwrap(),
            vec![graph.clone()]
        );
        assert!(
            s.list_graphs_by_source(&other, "stable-source")
                .await
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            s.get_graph(&graph.id, &scope).await.unwrap(),
            Some(graph.clone())
        );
        for id in ["left", "right"] {
            s.put_entity(entity(id, &["source"])).await.unwrap();
        }
        let edge = relation("edge", &["source"]);
        s.put_relationship(edge.clone()).await.unwrap();
        assert_eq!(
            s.get_relationship(&edge.id, &scope).await.unwrap(),
            Some(edge.clone())
        );
        assert_eq!(
            s.neighbors(&graph.id, &"left".into(), &scope, None)
                .await
                .unwrap(),
            vec![edge.clone()]
        );
        assert!(
            s.neighbors(&graph.id, &"right".into(), &scope, None)
                .await
                .unwrap()
                .is_empty()
        );
        assert!(
            s.neighbors(&graph.id, &"left".into(), &other, None)
                .await
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            s.list_entities_by_graph(&graph.id, &scope)
                .await
                .unwrap()
                .len(),
            2
        );
        assert_eq!(
            s.list_entities_by_source_ref("source", &scope)
                .await
                .unwrap()
                .len(),
            2
        );
        assert_eq!(
            s.list_relationships_by_source_ref("source", &scope)
                .await
                .unwrap(),
            vec![edge.clone()]
        );
        let mut wrong_type = entity("foreign-ref", &["source"]);
        wrong_type.source_refs[0].target_type = EvidenceTargetType::Document;
        s.put_entity(wrong_type).await.unwrap();
        assert_eq!(
            s.list_entities_by_source_ref("source", &scope)
                .await
                .unwrap()
                .len(),
            2
        );
        let identity = EntityIdentityMode::StableKey {
            key: "stable".into(),
        };
        let a = entity("identity-a", &[]);
        let mut b = a.clone();
        b.id = "identity-b".into();
        assert!(matches!(
            s.resolve_or_put_entity(EntityWriteRequest {
                entity: a.clone(),
                identity: identity.clone(),
                merge_policy: Default::default()
            })
            .await
            .unwrap(),
            EntityWriteOutcome::Created { .. }
        ));
        assert!(matches!(
            s.resolve_or_put_entity(EntityWriteRequest {
                entity: b,
                identity,
                merge_policy: Default::default()
            })
            .await
            .unwrap(),
            EntityWriteOutcome::Matched { .. }
        ));
        let mut same = edge.clone();
        same.id = "other-edge-id".into();
        assert_eq!(
            s.resolve_or_put_relationship(edge.clone())
                .await
                .unwrap()
                .id,
            edge.id
        );
        assert_eq!(
            s.resolve_or_put_relationship(same).await.unwrap().id,
            edge.id
        );
        s.put_entity(entity("duplicate-left", &["second"]))
            .await
            .unwrap();
        let mut dup_edge = edge.clone();
        dup_edge.id = "redirect".into();
        dup_edge.subject.id = Some("duplicate-left".into());
        s.put_relationship(dup_edge).await.unwrap();
        let merged = s
            .consolidate_entities(EntityMergeRequest {
                canonical_id: "left".into(),
                duplicate_ids: vec!["duplicate-left".into()],
                scope: scope.clone(),
                policy: Default::default(),
            })
            .await
            .unwrap();
        assert_eq!(merged.deleted_entities, 1);
        assert_eq!(merged.redirected_relationships, 1);
        assert_eq!(merged.coalesced_relationships, 1);
        assert!(
            s.get_entity(&"duplicate-left".into(), &scope)
                .await
                .unwrap()
                .is_none()
        );
        assert_eq!(
            s.neighbors(&graph.id, &"left".into(), &scope, None)
                .await
                .unwrap()
                .len(),
            1
        );
        let collisions = s
            .discover_collisions(&scope, &EntityIdentityMode::IdOnly)
            .await
            .unwrap();
        assert!(collisions.is_empty());
        let mut changing = relation("changing-key", &[]);
        changing.predicate = "before".into();
        s.resolve_or_put_relationship(changing.clone())
            .await
            .unwrap();
        changing.predicate = "after".into();
        s.put_relationship(changing.clone()).await.unwrap();
        let mut old_key = changing.clone();
        old_key.id = "new-before-key".into();
        old_key.predicate = "before".into();
        assert_eq!(
            s.resolve_or_put_relationship(old_key.clone())
                .await
                .unwrap()
                .id,
            old_key.id
        );
        assert_eq!(
            s.get_relationship(&changing.id, &scope)
                .await
                .unwrap()
                .unwrap()
                .predicate,
            "after"
        );
        s.delete_relationship(&changing.id, &scope).await.unwrap();
        s.delete_relationship(&old_key.id, &scope).await.unwrap();
        assert!(matches!(
            s.update_memory_status(&"absent".into(), &scope, MemoryStatus::Forgotten)
                .await,
            Err(engram_runtime::CoreError::NotFound { .. })
        ));
        for _ in 0..8 {
            let canonical = entity("ordered-canonical", &[]);
            let mut a = entity("ordered-a", &["shared-source"]);
            a.aliases = vec!["Alpha".into()];
            a.source_refs[0].quote = Some("source-A".into());
            let mut b = entity("ordered-b", &["shared-source"]);
            b.aliases = vec!["ALPHA".into()];
            b.source_refs[0].quote = Some("source-B".into());
            for record in [canonical, a, b] {
                s.put_entity(record).await.unwrap();
            }
            let result = s
                .consolidate_entities(EntityMergeRequest {
                    canonical_id: "ordered-canonical".into(),
                    duplicate_ids: vec![
                        "ordered-a".into(),
                        "ordered-b".into(),
                        "ordered-a".into(),
                        "ordered-canonical".into(),
                    ],
                    scope: scope.clone(),
                    policy: Default::default(),
                })
                .await
                .unwrap();
            assert_eq!(result.deleted_entities, 2);
            assert_eq!(result.canonical_entity.aliases, vec!["Alpha"]);
            assert_eq!(
                result.canonical_entity.source_refs[0].quote.as_deref(),
                Some("source-A")
            );
        }
        let unresolved: UnresolvedReference = make(
            "unresolved",
            json!({"graphId":"graph","fromEntityId":"left","referenceName":"unknown","status":"pending","path":"file"}),
        );
        s.put_unresolved_refs(vec![unresolved.clone()])
            .await
            .unwrap();
        assert_eq!(
            s.list_unresolved_refs(&scope, UnresolvedReferenceStatus::Pending)
                .await
                .unwrap()
                .len(),
            1
        );
        s.update_unresolved_status(&unresolved.id, UnresolvedReferenceStatus::Resolved, &scope)
            .await
            .unwrap();
        assert!(
            s.list_unresolved_refs(&scope, UnresolvedReferenceStatus::Pending)
                .await
                .unwrap()
                .is_empty()
        );
        let ontology: Ontology = make(
            "ontology",
            json!({"uri":"urn:ontology","language":"custom","version":"1","status":"active"}),
        );
        s.put_ontology(ontology.clone()).await.unwrap();
        assert_eq!(
            s.get_ontology(&ontology.id, &scope).await.unwrap(),
            Some(ontology.clone())
        );
        let class: OntologyClass = make(
            "class",
            json!({"ontologyId":"ontology","uri":"urn:class","label":"class","status":"active"}),
        );
        s.put_class(class).await.unwrap();
        let prop: OntologyProperty = make(
            "property",
            json!({"ontologyId":"ontology","uri":"urn:property","label":"ReLaTeD","kind":"object","status":"active"}),
        );
        s.put_property(prop).await.unwrap();
        let axiom: OntologyAxiom = make(
            "axiom",
            json!({"ontologyId":"ontology","kind":"transitive"}),
        );
        s.put_axiom(axiom).await.unwrap();
        assert!(
            s.validate_graph(&graph.id, &ontology.id, &other)
                .await
                .unwrap()
                .is_empty()
        );
        assert!(
            s.validate_graph(&graph.id, &"absent-ontology".into(), &scope)
                .await
                .unwrap()
                .is_empty()
        );
        assert!(
            s.validate_graph(&graph.id, &ontology.id, &scope)
                .await
                .unwrap()
                .is_empty()
        );
        let scheme: ConceptScheme = make("scheme", json!({"uri":"urn:scheme","version":"1"}));
        s.put_concept_scheme(scheme.clone()).await.unwrap();
        assert_eq!(
            s.get_concept_scheme(&scheme.id, &scope).await.unwrap(),
            Some(scheme.clone())
        );
        for id in ["concept-a", "concept-b"] {
            let concept: Concept = make(
                id,
                json!({"schemeId":"scheme","uri":format!("urn:{id}"),"prefLabel":{"value":id},"status":"active"}),
            );
            s.put_concept(concept).await.unwrap();
        }
        let cr: ConceptRelation = make(
            "concept-rel",
            json!({"schemeId":"scheme","subjectId":"concept-a","objectId":"concept-b","predicate":"broader"}),
        );
        s.put_concept_relation(cr).await.unwrap();
        assert_eq!(s.list_concepts(&scheme.id, &scope).await.unwrap().len(), 2);
        assert_eq!(s.count_memory_partitions().unwrap()[0].count, 2);
        assert_eq!(s.partition_entities(&scope, 1, 0).unwrap().len(), 1);
        assert!(s.partition_relationships(&other, 10, 0).unwrap().is_empty());
        assert_eq!(s.partition_recent_memories(&scope, 1).unwrap().len(), 1);
        let read = f.reader();
        assert!(read.put_memory(memory("forbidden-reader")).await.is_err());
        read.close().unwrap();
        f.reopen();
        assert_eq!(
            f.store
                .get_memory(&m.id, &scope)
                .await
                .unwrap()
                .unwrap()
                .status,
            MemoryStatus::Archived
        );
        assert_eq!(
            f.store.list_events_for_scope(&scope).await.unwrap(),
            vec![event]
        );
        assert_eq!(
            f.store
                .list_concepts(&scheme.id, &scope)
                .await
                .unwrap()
                .len(),
            2
        );
        assert!(!f.store.delete_graph(&graph.id, &other).await.unwrap());
        assert!(f.store.delete_graph(&graph.id, &scope).await.unwrap());
        assert!(
            f.store
                .get_relationship(&edge.id, &scope)
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            f.store
                .list_unresolved_refs(&scope, UnresolvedReferenceStatus::Resolved)
                .await
                .unwrap()
                .is_empty()
        );
        assert!(f.store.delete_memory_record(&m.id, &scope).unwrap());
        assert_eq!(
            f.store
                .get_event(&"event".into(), &scope)
                .await
                .unwrap()
                .unwrap()
                .kind,
            MemoryEventKind::Written
        );
        assert!(f.store.delete_chunk(&chunk.id, &scope).await.unwrap());
        assert!(f.store.delete_document(&doc.id, &scope).await.unwrap());
        let original = f.partition.clone();
        let mut empty = scope.clone();
        empty.subject = Some(String::new());
        f.switch_partition(empty.clone(), original.logical_store.clone());
        let mut em = memory("empty-subject-memory");
        em.scope = empty.clone();
        f.store.put_memory(em).await.unwrap();
        f.switch_partition(
            scope.clone(),
            format!("{}-other-store", original.logical_store),
        );
        f.store
            .put_memory(memory("other-store-memory"))
            .await
            .unwrap();
        f.switch_partition(original.scope.clone(), original.logical_store.clone());
        let counts = f.store.count_memory_partitions().unwrap();
        assert_eq!(counts.len(), 2);
        assert_eq!(counts.iter().map(|r| r.count).sum::<u64>(), 2);
        assert!(counts.iter().any(|r| r.scope.subject.is_none()));
        assert!(
            counts
                .iter()
                .any(|r| r.scope.subject.as_deref() == Some(""))
        );
        assert_eq!(
            f.store.partition_memories(&empty, 1, 0).unwrap()[0]
                .id
                .to_string(),
            "empty-subject-memory"
        );
        assert_eq!(
            f.store.partition_memories(&scope, 1, 0).unwrap()[0]
                .id
                .to_string(),
            "memory-2"
        );
        assert!(
            f.store
                .get_memory(&"empty-subject-memory".into(), &empty)
                .await
                .unwrap()
                .is_none()
        );
        assert!(f.store.partition_memories(&scope, 501, 0).is_err());
        println!(
            "TASK004 {mode:?}: full direct record/identity/vocabulary/inventory ports, directed traversal, scope and reopen assertions completed"
        );
    });
}
