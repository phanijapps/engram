use engram_domain::*;
use engram_knowledge::{
    CodeSourceRepository, CodeSourceWrite, DocumentHeadRepository, KnowledgeRepository,
};
use engram_runtime::CoreError;
use engram_store_sqlite::SqlKnowledgeStore;
use futures::executor::block_on;
use serde_json::json;
use sha2::{Digest, Sha256};

fn manifest(path: &str, attempt: &str, text: &str) -> CodeSourceGeneration {
    serde_json::from_value(json!({
        "file":{"scope":{"tenant":"tenant","subject":"owner","workspace":"project"},"repository":"repo","path":path},
        "generation":digest(text),
        "attemptId":attempt,"sourceHash":digest(text),
        "fingerprint":"code-source-v1","state":"updating","byteLength":text.len(),
        "documentId":format!("doc-{path}"),"snapshotIds":[format!("snapshot-{path}")],
        "records":[{"kind":"source","id":format!("source-{path}")},{"kind":"chunk","id":format!("snapshot-{path}")},{"kind":"document","id":format!("doc-{path}")}],
        "policy":{"visibility":"private","retention":"durable","allowedUses":["retrieval"]},
        "provenance":{"source":"test","actor":{"id":"owner","kind":"agent"},"observedAt":"2026-10-14T00:00:00Z"}
    })).unwrap()
}

#[test]
fn generic_head_cannot_supersede_code_source_authority() {
    let store = SqlKnowledgeStore::open_in_memory().unwrap();
    let m = manifest("a.ts", "first", "hello");
    active(&store, &m, "hello");
    let key = DocumentHeadKey {
        scope: m.file.scope.clone(),
        repository: m.file.repository.clone(),
        source_key: m.file.path.clone(),
    };
    let mut h = block_on(store.get_document_head(&key)).unwrap().unwrap();
    h.state = CodeSourceState::Updating;
    h.attempt_id = "generic-overwrite".into();
    assert!(block_on(store.begin_document_head(h, Some("first"))).is_err());
    assert_eq!(
        block_on(store.get_document_head(&key))
            .unwrap()
            .unwrap()
            .attempt_id,
        "first"
    );
    assert!(block_on(store.read_code_source(read(&m), &m.file.scope)).is_ok());
}
fn chunk(m: &CodeSourceGeneration, text: &str) -> KnowledgeChunk {
    serde_json::from_value(json!({
        "id":m.snapshot_ids[0],"sourceId":format!("source-{}",m.file.path),"documentId":m.document_id,
        "kind":"code_block","text":text,"documentKind":"code","contentHash":m.source_hash,
        "policy":m.policy,"provenance":m.provenance,"createdAt":"2026-10-14T00:00:00Z",
        "code":{"role":"source_snapshot","file":m.file,"generation":m.generation,
          "sourceHash":m.source_hash,"fingerprint":m.fingerprint,"complete":true,
          "parseQuality":"clean","ranges":[{"start":0,"end":text.len()}]}
    }))
    .unwrap()
}
fn read(m: &CodeSourceGeneration) -> CodeSourceReadRequest {
    CodeSourceReadRequest {
        file: m.file.clone(),
        requester: serde_json::from_value(json!({"actor":{"id":"owner","kind":"agent"}})).unwrap(),
        generation: None,
        max_bytes: 1024,
    }
}
fn parents(m: &CodeSourceGeneration) -> (KnowledgeSource, SourceDocument) {
    let source=serde_json::from_value(json!({"id":format!("source-{}",m.file.path),"kind":"git_repository","scope":m.file.scope,"name":"test","metadata":{"stableSourceKey":m.file.repository},"policy":m.policy,"provenance":m.provenance,"createdAt":"2026-10-14T00:00:00Z"})).unwrap();
    let doc=serde_json::from_value(json!({"id":m.document_id,"sourceId":format!("source-{}",m.file.path),"kind":"code","path":m.file.path,"contentHash":m.source_hash,"policy":m.policy,"provenance":m.provenance,"createdAt":"2026-10-14T00:00:00Z"})).unwrap();
    (source, doc)
}
fn parent_records(store: &SqlKnowledgeStore, m: &CodeSourceGeneration) {
    let (source, doc) = parents(m);
    block_on(store.put_source(source)).unwrap();
    block_on(store.put_document(doc)).unwrap();
}
fn guarded_parents(store: &SqlKnowledgeStore, m: &CodeSourceGeneration) {
    let (source, doc) = parents(m);
    block_on(store.write_code_record(&m.file, &m.attempt_id, CodeSourceWrite::Source(source)))
        .unwrap();
    block_on(store.write_code_record(&m.file, &m.attempt_id, CodeSourceWrite::Document(doc)))
        .unwrap();
}
fn active(store: &SqlKnowledgeStore, m: &CodeSourceGeneration, text: &str) {
    block_on(store.begin_code_source(m.clone(), None)).unwrap();
    block_on(store.stage_code_records(&m.file, &m.attempt_id, &m.records)).unwrap();
    guarded_parents(store, m);
    block_on(store.write_code_record(
        &m.file,
        &m.attempt_id,
        CodeSourceWrite::Chunk(chunk(m, text)),
    ))
    .unwrap();
    let mut a = m.clone();
    a.state = CodeSourceState::Active;
    block_on(store.activate_code_source(a)).unwrap();
}

#[test]
fn exact_source_is_not_a_candidate_even_for_legacy_sql() {
    let store = SqlKnowledgeStore::open_in_memory().unwrap();
    let text = "\u{feff}const π = 'poison-snapshot';\r\n";
    let m = manifest("src/a.ts", "attempt-a", text);
    active(&store, &m, text);
    let result = block_on(store.read_code_source(read(&m), &m.file.scope)).unwrap();
    assert_eq!(result.text.as_bytes(), text.as_bytes());
    let conn = store.lock().unwrap();
    let candidates: i64 = conn
        .query_row("SELECT COUNT(*) FROM knowledge_chunks", [], |r| r.get(0))
        .unwrap();
    assert_eq!(candidates, 0);
    assert!(
        conn.execute("DELETE FROM knowledge_chunks", []).is_err(),
        "old writers must fail closed"
    );
}

#[test]
fn scopes_policy_limits_and_requested_generation_bind_before_read() {
    let store = SqlKnowledgeStore::open_in_memory().unwrap();
    let m = manifest("a.rs", "a", "fn a() {}\n");
    active(&store, &m, "fn a() {}\n");
    for dimension in 0..5 {
        let mut trusted = m.file.scope.clone();
        match dimension {
            0 => trusted.tenant = "other".into(),
            1 => trusted.subject = None,
            2 => trusted.workspace = None,
            3 => trusted.session = Some("x".into()),
            _ => trusted.environment = Some("x".into()),
        };
        assert!(matches!(
            block_on(store.read_code_source(read(&m), &trusted)),
            Err(CoreError::PolicyDenied { .. })
        ));
    }
    let mut request = read(&m);
    request.generation = Some("missing".into());
    assert!(matches!(
        block_on(store.read_code_source(request, &m.file.scope)),
        Err(CoreError::ProviderUnavailable { .. })
    ));
    let mut request = read(&m);
    request.max_bytes = 2;
    assert!(matches!(
        block_on(store.read_code_source(request, &m.file.scope)),
        Err(CoreError::InvalidRequest { .. })
    ));
    let mut request = read(&m);
    request.requester.actor.id = Id::from("stranger");
    request.requester.permissions = vec!["code:source:read".into()];
    assert!(matches!(
        block_on(store.read_code_source(request, &m.file.scope)),
        Err(CoreError::PolicyDenied { .. })
    ));
}

#[test]
fn aba_attempts_are_fenced_and_failed_inventory_stays_unavailable() {
    let store = SqlKnowledgeStore::open_in_memory().unwrap();
    let mut a = manifest("a.rs", "first", "hello");
    active(&store, &a, "hello");
    let mut b = a.clone();
    b.attempt_id = "second".into();
    b.state = CodeSourceState::Updating;
    block_on(store.begin_code_source(b.clone(), Some("first"))).unwrap();
    a.attempt_id = "third".into();
    a.state = CodeSourceState::Updating;
    block_on(store.begin_code_source(a.clone(), Some("second"))).unwrap();
    b.state = CodeSourceState::Active;
    assert!(matches!(
        block_on(store.activate_code_source(b)),
        Err(CoreError::Conflict { .. })
    ));
    assert!(block_on(store.read_code_source(read(&a), &a.file.scope)).is_err());
    a.state = CodeSourceState::Active;
    a.snapshot_ids.push(Id::from("missing"));
    assert!(block_on(store.activate_code_source(a.clone())).is_err());
    block_on(store.fail_code_source(&a.file, &a.attempt_id)).unwrap();
    assert_eq!(
        block_on(store.get_code_source(&a.file))
            .unwrap()
            .unwrap()
            .state,
        CodeSourceState::Incomplete
    );
    assert!(block_on(store.read_code_source(read(&a), &a.file.scope)).is_err());
}

#[test]
fn snapshot_corruption_and_missing_inventory_prevent_activation() {
    for text in ["wrong", ""] {
        let store = SqlKnowledgeStore::open_in_memory().unwrap();
        let mut m = manifest("a.ts", "a", "expected");
        block_on(store.begin_code_source(m.clone(), None)).unwrap();
        block_on(store.stage_code_records(&m.file, &m.attempt_id, &m.records)).unwrap();
        guarded_parents(&store, &m);
        block_on(store.write_code_record(
            &m.file,
            &m.attempt_id,
            CodeSourceWrite::Chunk(chunk(&m, text)),
        ))
        .unwrap();
        m.state = CodeSourceState::Active;
        assert!(block_on(store.activate_code_source(m)).is_err());
    }
}

fn digest(text: &str) -> String {
    let hex: String = Sha256::digest(text.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    format!("sha256:{hex}")
}

#[test]
fn missing_parent_document_cannot_activate() {
    let store = SqlKnowledgeStore::open_in_memory().unwrap();
    let mut m = manifest("a.rs", "a", "hello");
    block_on(store.begin_code_source(m.clone(), None)).unwrap();
    block_on(store.stage_code_records(&m.file, &m.attempt_id, &m.records)).unwrap();
    block_on(store.write_code_record(
        &m.file,
        &m.attempt_id,
        CodeSourceWrite::Chunk(chunk(&m, "hello")),
    ))
    .unwrap();
    m.state = CodeSourceState::Active;
    assert!(block_on(store.activate_code_source(m)).is_err());
}

#[test]
fn updating_one_file_keeps_siblings_and_independent_shared_owners_visible() {
    let store = SqlKnowledgeStore::open_in_memory().unwrap();
    let mut a = manifest("a.ts", "a", "a");
    let mut b = manifest("b.ts", "b", "b");
    let shared = CodeSourceRecord {
        kind: CodeSourceRecordKind::Entity,
        id: "shared".into(),
    };
    let own = CodeSourceRecord {
        kind: CodeSourceRecordKind::Entity,
        id: "only-a".into(),
    };
    a.records.extend([shared.clone(), own]);
    b.records.push(shared);
    for id in ["shared", "only-a"] {
        block_on(store.put_entity(serde_json::from_value(json!({"id":id,"kind":"function","name":id,"scope":a.file.scope,"provenance":a.provenance,"createdAt":"2026-10-14T00:00:00Z"})).unwrap())).unwrap();
    }
    active(&store, &a, "a");
    active(&store, &b, "b");
    let mut next = a.clone();
    next.attempt_id = "next-a".into();
    block_on(store.begin_code_source(next.clone(), Some("a"))).unwrap();
    assert!(block_on(store.read_code_source(read(&a), &a.file.scope)).is_err());
    assert_eq!(
        block_on(store.read_code_source(read(&b), &b.file.scope))
            .unwrap()
            .text,
        "b"
    );
    assert!(
        block_on(store.get_entity(&Id::from("only-a"), &a.file.scope))
            .unwrap()
            .is_none()
    );
    assert!(
        block_on(store.get_entity(&Id::from("shared"), &a.file.scope))
            .unwrap()
            .is_some()
    );
    block_on(store.fail_code_source(&next.file, &next.attempt_id)).unwrap();
    assert!(
        block_on(store.get_entity(&Id::from("shared"), &a.file.scope))
            .unwrap()
            .is_some()
    );
}

#[test]
fn candidate_sql_modules_cannot_bypass_eligibility_views() {
    for module in [
        include_str!("../src/knowledge/retrieval.rs"),
        include_str!("../src/knowledge/service.rs"),
        include_str!("../src/knowledge/ontology.rs"),
    ] {
        assert!(!module.contains("knowledge_chunks_records"));
        assert!(!module.contains("knowledge_entities_records"));
        assert!(!module.contains("knowledge_relationships_records"));
        assert!(!module.contains("knowledge_graphs_records"));
    }
}

#[test]
fn cleanup_is_fenced_and_preserves_an_independent_active_owner() {
    let store = SqlKnowledgeStore::open_in_memory().unwrap();
    let mut a = manifest("a.ts", "a", "a");
    let mut b = manifest("b.ts", "b", "b");
    let shared = CodeSourceRecord {
        kind: CodeSourceRecordKind::Entity,
        id: "shared".into(),
    };
    a.records.push(shared.clone());
    b.records.push(shared.clone());
    block_on(store.put_entity(serde_json::from_value(json!({"id":"shared","kind":"function","name":"shared","scope":a.file.scope,"provenance":a.provenance,"createdAt":"2026-10-14T00:00:00Z"})).unwrap())).unwrap();
    active(&store, &a, "a");
    active(&store, &b, "b");
    let mut next = a.clone();
    next.attempt_id = "next".into();
    block_on(store.begin_code_source(next.clone(), Some("a"))).unwrap();
    assert!(matches!(
        block_on(store.remove_code_record(&a.file, "a", "a", &shared)),
        Err(CoreError::Conflict { .. })
    ));
    assert!(!block_on(store.remove_code_record(&a.file, "next", "a", &shared)).unwrap());
    assert!(
        block_on(store.get_entity(&Id::from("shared"), &a.file.scope))
            .unwrap()
            .is_some()
    );
    let foreign = CodeSourceRecord {
        kind: CodeSourceRecordKind::Entity,
        id: "not-owned".into(),
    };
    assert!(matches!(
        block_on(store.remove_code_record(&a.file, "next", "a", &foreign)),
        Err(CoreError::PolicyDenied { .. })
    ));
}

#[test]
fn old_graph_cache_cannot_resurrect_an_updating_file() {
    use engram_retrieval::{GraphCache, GraphSnapshot, InMemoryGraphCache, RetrievalIndex};
    use engram_store_sqlite::GraphRetrievalIndex;
    use std::sync::Arc;
    let store = SqlKnowledgeStore::open_in_memory().unwrap();
    let mut m = manifest("a.ts", "a", "a");
    m.records.push(CodeSourceRecord {
        kind: CodeSourceRecordKind::Entity,
        id: "cache-poison".into(),
    });
    let entity:KnowledgeEntity=serde_json::from_value(json!({"id":"cache-poison","kind":"function","name":"cache-poison","scope":m.file.scope,"provenance":m.provenance,"createdAt":"2026-10-14T00:00:00Z"})).unwrap();
    block_on(store.put_entity(entity.clone())).unwrap();
    active(&store, &m, "a");
    let cache = Arc::new(InMemoryGraphCache::new());
    let revision = store.code_source_revision().unwrap();
    block_on(cache.put(
        &m.file.scope,
        Arc::new(GraphSnapshot {
            source_revision: revision,
            entities: vec![entity],
            relationships: vec![],
            community_labels: None,
        }),
    ));
    let mut next = m.clone();
    next.attempt_id = "new".into();
    block_on(store.begin_code_source(next, Some("a"))).unwrap();
    let index = GraphRetrievalIndex::with_cache(Arc::new(store), 20, cache);
    let request: RetrievalRequest = serde_json::from_value(
        json!({"query":"cache-poison","scope":m.file.scope,"requester":read(&m).requester}),
    )
    .unwrap();
    assert!(
        block_on(index.retrieve_candidates(&request))
            .unwrap()
            .is_empty()
    );
}

#[test]
fn attempt_identity_cannot_be_reused_after_supersession() {
    let store = SqlKnowledgeStore::open_in_memory().unwrap();
    let a = manifest("a.rs", "A", "same");
    block_on(store.begin_code_source(a.clone(), None)).unwrap();
    let mut b = a.clone();
    b.attempt_id = "B".into();
    block_on(store.begin_code_source(b, Some("A"))).unwrap();
    assert!(matches!(
        block_on(store.begin_code_source(a, Some("B"))),
        Err(CoreError::Conflict { .. })
    ));
}

#[test]
fn overlapping_cleanup_preserves_new_attempt_registration() {
    let store = SqlKnowledgeStore::open_in_memory().unwrap();
    let mut a = manifest("a.rs", "old", "a");
    let record = CodeSourceRecord {
        kind: CodeSourceRecordKind::Entity,
        id: "reused".into(),
    };
    a.records.push(record.clone());
    block_on(store.put_entity(serde_json::from_value(json!({"id":"reused","kind":"function","name":"reused","scope":a.file.scope,"provenance":a.provenance,"createdAt":"2026-10-14T00:00:00Z"})).unwrap())).unwrap();
    active(&store, &a, "a");
    let mut next = a.clone();
    next.attempt_id = "new".into();
    block_on(store.begin_code_source(next, Some("old"))).unwrap();
    block_on(store.stage_code_records(&a.file, "new", std::slice::from_ref(&record))).unwrap();
    assert!(!block_on(store.remove_code_record(&a.file, "new", "old", &record)).unwrap());
    let conn = store.lock().unwrap();
    assert_eq!(conn.query_row("SELECT COUNT(*) FROM code_source_contributions WHERE attempt_id='new' AND record_id='reused'",[],|r|r.get::<_,i64>(0)).unwrap(),1);
    assert_eq!(
        conn.query_row(
            "SELECT COUNT(*) FROM knowledge_entities_records WHERE id='reused'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        1
    );
}

#[test]
fn migrated_file_excludes_attributed_legacy_rows_and_retains_legacy_sibling() {
    use engram_knowledge::KnowledgeGraphRepository;
    let store = SqlKnowledgeStore::open_in_memory().unwrap();
    let a = manifest("a.ts", "fresh-a", "new a");
    let b = manifest("b.ts", "unused", "b");
    for m in [&a, &b] {
        parent_records(&store, m);
        let mut legacy = chunk(m, "legacy poison");
        legacy.code = None;
        legacy.document_kind = None;
        legacy.id = Id::from(format!("legacy-{}", m.file.path));
        block_on(store.put_chunk(legacy)).unwrap();
        block_on(store.put_graph(serde_json::from_value(json!({"id":format!("graph-{}",m.file.path),"name":"legacy","scope":m.file.scope,"policy":m.policy,"provenance":m.provenance,"createdAt":"2026-10-14T00:00:00Z","metadata":{"stableSourceKey":"repo","path":m.file.path,"documentId":m.document_id}})).unwrap())).unwrap();
    }
    assert!(
        block_on(store.get_chunk(&Id::from("legacy-a.ts"), &a.file.scope))
            .unwrap()
            .is_some()
    );
    block_on(store.begin_code_source(a.clone(), None)).unwrap();
    assert!(
        block_on(store.get_chunk(&Id::from("legacy-a.ts"), &a.file.scope))
            .unwrap()
            .is_none()
    );
    assert!(
        block_on(store.get_chunk(&Id::from("legacy-b.ts"), &b.file.scope))
            .unwrap()
            .is_some()
    );
    assert!(
        block_on(store.get_graph(&Id::from("graph-a.ts"), &a.file.scope))
            .unwrap()
            .is_none()
    );
    assert!(
        block_on(store.get_graph(&Id::from("graph-b.ts"), &b.file.scope))
            .unwrap()
            .is_some()
    );
    let prior = block_on(store.prior_code_records(&a.file, &a.attempt_id)).unwrap();
    assert!(prior.iter().any(|(_, r)| r.id == "legacy-a.ts"));
    for (owner, record) in prior {
        block_on(store.remove_code_record(&a.file, &a.attempt_id, &owner, &record)).unwrap();
    }
    block_on(store.stage_code_records(&a.file, &a.attempt_id, &a.records)).unwrap();
    guarded_parents(&store, &a);
    block_on(store.write_code_record(
        &a.file,
        &a.attempt_id,
        CodeSourceWrite::Chunk(chunk(&a, "new a")),
    ))
    .unwrap();
    let mut active = a.clone();
    active.state = CodeSourceState::Active;
    block_on(store.activate_code_source(active)).unwrap();
    assert!(
        block_on(store.get_chunk(&Id::from("legacy-a.ts"), &a.file.scope))
            .unwrap()
            .is_none()
    );
    assert_eq!(
        block_on(store.read_code_source(read(&a), &a.file.scope))
            .unwrap()
            .text,
        "new a"
    );
    assert!(
        block_on(store.get_chunk(&Id::from("legacy-b.ts"), &b.file.scope))
            .unwrap()
            .is_some()
    );
}

#[test]
fn legacy_shared_owner_is_registered_even_when_another_file_already_owns_it() {
    use engram_knowledge::KnowledgeGraphRepository;
    let store = SqlKnowledgeStore::open_in_memory().unwrap();
    let mut a = manifest("a.ts", "a", "a");
    let mut b = manifest("b.ts", "b", "b");
    let shared = CodeSourceRecord {
        kind: CodeSourceRecordKind::Entity,
        id: "shared".into(),
    };
    a.records.push(shared.clone());
    b.records.push(shared.clone());
    block_on(store.put_entity(serde_json::from_value(json!({"id":"shared","graphId":"legacy-b","kind":"function","name":"shared","scope":a.file.scope,"provenance":a.provenance,"createdAt":"2026-10-14T00:00:00Z"})).unwrap())).unwrap();
    active(&store, &a, "a");
    parent_records(&store, &b);
    block_on(store.put_graph(serde_json::from_value(json!({"id":"legacy-b","name":"legacy","scope":b.file.scope,"policy":b.policy,"provenance":b.provenance,"createdAt":"2026-10-14T00:00:00Z","metadata":{"stableSourceKey":"repo","path":"b.ts","documentId":b.document_id}})).unwrap())).unwrap();
    block_on(store.begin_code_source(b.clone(), None)).unwrap();
    let prior = block_on(store.prior_code_records(&b.file, "b")).unwrap();
    assert!(prior.iter().any(|(_, record)| record == &shared));
    for (owner, record) in prior {
        block_on(store.remove_code_record(&b.file, "b", &owner, &record)).unwrap();
    }
    block_on(store.stage_code_records(&b.file, "b", &b.records)).unwrap();
    guarded_parents(&store, &b);
    block_on(store.write_code_record(
        &b.file,
        &b.attempt_id,
        CodeSourceWrite::Chunk(chunk(&b, "b")),
    ))
    .unwrap();
    let mut active_b = b.clone();
    active_b.state = CodeSourceState::Active;
    block_on(store.activate_code_source(active_b)).unwrap();
    let mut next = a.clone();
    next.attempt_id = "next-a".into();
    block_on(store.begin_code_source(next, Some("a"))).unwrap();
    assert!(!block_on(store.remove_code_record(&a.file, "next-a", "a", &shared)).unwrap());
    assert!(
        block_on(store.get_entity(&Id::from("shared"), &b.file.scope))
            .unwrap()
            .is_some()
    );
}

#[test]
fn source_writes_are_attempt_fenced_and_generic_put_cannot_mutate_active_code() {
    use engram_knowledge::CodeSourceWrite;
    let store = SqlKnowledgeStore::open_in_memory().unwrap();
    let a = manifest("a.ts", "old", "old");
    active(&store, &a, "old");
    assert!(
        block_on(store.put_chunk(chunk(&a, "old"))).is_err(),
        "ordinary CRUD must not bypass the active source lease"
    );
    let mut next = a.clone();
    next.attempt_id = "new".into();
    block_on(store.begin_code_source(next.clone(), Some("old"))).unwrap();
    block_on(store.stage_code_records(&next.file, &next.attempt_id, &next.records)).unwrap();
    assert!(matches!(
        block_on(store.write_code_record(&a.file, "old", CodeSourceWrite::Chunk(chunk(&a, "old")))),
        Err(CoreError::Conflict { .. })
    ));
    let mut wrong = chunk(&next, "old");
    wrong.code.as_mut().unwrap().generation = "foreign".into();
    assert!(
        block_on(store.write_code_record(&next.file, "new", CodeSourceWrite::Chunk(wrong)))
            .is_err()
    );
}

#[test]
fn semantic_coverage_must_match_utf8_source_slices() {
    for (range, rendered) in [
        (CodeByteRange { start: 1, end: 2 }, "π"),
        (CodeByteRange { start: 0, end: 2 }, "unrelated"),
    ] {
        let store = SqlKnowledgeStore::open_in_memory().unwrap();
        let mut m = manifest("unicode.ts", "a", "π\n");
        m.records.push(CodeSourceRecord {
            kind: CodeSourceRecordKind::Chunk,
            id: "semantic".into(),
        });
        block_on(store.begin_code_source(m.clone(), None)).unwrap();
        block_on(store.stage_code_records(&m.file, &m.attempt_id, &m.records)).unwrap();
        guarded_parents(&store, &m);
        block_on(store.write_code_record(
            &m.file,
            &m.attempt_id,
            CodeSourceWrite::Chunk(chunk(&m, "π\n")),
        ))
        .unwrap();
        let mut semantic = chunk(&m, "π\n");
        semantic.id = Id::from("semantic");
        semantic.text = rendered.into();
        semantic.content_hash = digest(rendered);
        let code = semantic.code.as_mut().unwrap();
        code.role = CodeChunkRole::Symbol;
        code.ranges = vec![range];
        block_on(store.write_code_record(&m.file, &m.attempt_id, CodeSourceWrite::Chunk(semantic)))
            .unwrap();
        m.state = CodeSourceState::Active;
        assert!(block_on(store.activate_code_source(m)).is_err());
    }
}

#[test]
fn unfenced_deletion_invalidates_whole_source_view() {
    let store = SqlKnowledgeStore::open_in_memory().unwrap();
    let m = manifest("a.ts", "a", "hello");
    active(&store, &m, "hello");
    block_on(store.delete_chunk(&m.snapshot_ids[0], &m.file.scope)).unwrap();
    assert_eq!(
        block_on(store.get_code_source(&m.file))
            .unwrap()
            .unwrap()
            .state,
        CodeSourceState::Incomplete
    );
    assert!(block_on(store.read_code_source(read(&m), &m.file.scope)).is_err());
}

#[test]
fn source_minimum_reader_is_checked_before_any_migration() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("knowledge.db");
    let store = SqlKnowledgeStore::open_file(&path).unwrap();
    let m = manifest("a.ts", "a", "hello");
    active(&store, &m, "hello");
    store
        .lock()
        .unwrap()
        .execute("UPDATE code_source_reader SET minimum_version=2", [])
        .unwrap();
    drop(store);
    assert!(matches!(
        SqlKnowledgeStore::open_file(&path),
        Err(CoreError::CapabilityUnsupported { .. })
    ));
    let conn = rusqlite::Connection::open(&path).unwrap();
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM knowledge_chunks_records", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(count, 1);
}

#[test]
fn concurrent_source_schema_openers_all_acquire_durable_handles() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("knowledge.db");
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(8));
    let workers: Vec<_> = (0..8)
        .map(|_| {
            let path = path.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                SqlKnowledgeStore::open_file(path).map(|s| s.code_source_revision().unwrap())
            })
        })
        .collect();
    for worker in workers {
        let result = worker.join().unwrap();
        assert!(
            result.is_ok(),
            "concurrent source-schema opener failed: {result:?}"
        );
    }
}

#[test]
fn legacy_unverified_ownership_is_denied_without_breaking_sibling_access() {
    let store = SqlKnowledgeStore::open_in_memory().unwrap();
    let m = manifest("a.ts", "a", "hello");
    active(&store, &m, "hello");
    store.lock().unwrap().execute("INSERT INTO code_source_contributions VALUES('orphan-legacy','legacy:unverified','entity','old-symbol')", []).unwrap();
    let request: RetrievalRequest = serde_json::from_value(json!({"scope":m.file.scope,"query":"hello","requester":{"actor":{"id":"owner","kind":"agent"}}})).unwrap();
    let access = store.source_record_access(&request).unwrap();
    assert!(!access.allows(RetrievalTargetType::Entity, "old-symbol"));
    assert!(access.allows(RetrievalTargetType::Entity, "ordinary-sibling"));
    store.lock().unwrap().execute("INSERT INTO code_source_contributions VALUES('missing-real','real-attempt','entity','bad-symbol')", []).unwrap();
    assert!(store.source_record_access(&request).is_err());
}

#[test]
fn malformed_parent_unit_prevents_activation() {
    let store = SqlKnowledgeStore::open_in_memory().unwrap();
    let mut m = manifest("parent.ts", "parent", "hello");
    let mut unit = chunk(&m, "hello");
    unit.id = Id::from("parent-unit");
    let descriptor = unit.code.as_mut().unwrap();
    descriptor.role = CodeChunkRole::Symbol;
    descriptor.unit = Some("child".into());
    descriptor.parent_unit = Some("missing-parent".into());
    descriptor.declaration_range = Some(CodeByteRange { start: 0, end: 5 });
    m.records.push(CodeSourceRecord {
        kind: CodeSourceRecordKind::Chunk,
        id: unit.id.to_string(),
    });
    block_on(store.begin_code_source(m.clone(), None)).unwrap();
    block_on(store.stage_code_records(&m.file, &m.attempt_id, &m.records)).unwrap();
    guarded_parents(&store, &m);
    for c in [chunk(&m, "hello"), unit] {
        block_on(store.write_code_record(&m.file, &m.attempt_id, CodeSourceWrite::Chunk(c)))
            .unwrap();
    }
    m.state = CodeSourceState::Active;
    assert!(matches!(
        block_on(store.activate_code_source(m)),
        Err(CoreError::ValidationFailed { .. })
    ));
}

#[test]
fn target_access_matches_full_access_for_every_owner_without_visiting_unrelated_rows() {
    let store = SqlKnowledgeStore::open_in_memory().unwrap();
    let first = manifest("first.ts", "one", "hello");
    active(&store, &first, "hello");
    let mut second = manifest("second.ts", "two", "world");
    second.provenance.actor.id = Id::from("other");
    active(&store, &second, "world");
    {
        let conn = store.lock().unwrap();
        for (m, ids) in [
            (&first, vec!["shared", "owner-only"]),
            (&second, vec!["shared", "other-only"]),
        ] {
            for id in ids {
                conn.execute(
                    "INSERT INTO code_source_contributions VALUES(?1,?2,'entity',?3)",
                    rusqlite::params![serde_json::to_string(&m.file).unwrap(), m.attempt_id, id],
                )
                .unwrap();
            }
        }
        conn.execute("INSERT INTO code_source_contributions VALUES('unverified','legacy:unverified','entity','legacy-only')",[]).unwrap();
    }
    // A real generic document head owns the same ID as a code entity, with
    // a different private actor. Kind-specific ownership must stay separate.
    let mut doc_head = block_on(store.get_document_head(&DocumentHeadKey {
        scope: first.file.scope.clone(),
        repository: "repo".into(),
        source_key: "first.ts".into(),
    }))
    .unwrap()
    .unwrap();
    doc_head.key.source_key = "README.md".into();
    doc_head.attempt_id = "doc-attempt".into();
    doc_head.state = CodeSourceState::Updating;
    doc_head.document_id = Some(Id::from("generic-doc"));
    doc_head.chunk_ids = vec![Id::from("owner-only")];
    doc_head.provenance.actor.id = Id::from("other");
    block_on(store.begin_document_head(doc_head.clone(), None)).unwrap();
    let (mut source, mut doc) = parents(&first);
    source.id = Id::from("generic-source");
    source.policy = doc_head.policy.clone();
    source.provenance = doc_head.provenance.clone();
    doc.id = Id::from("generic-doc");
    doc.source_id = source.id.clone();
    doc.kind = SourceDocumentKind::Markdown;
    doc.path = Some("README.md".into());
    doc.policy = doc_head.policy.clone();
    doc.provenance = doc_head.provenance.clone();
    let paragraph:KnowledgeChunk=serde_json::from_value(json!({"id":"owner-only","documentId":doc.id,"sourceId":source.id,"kind":"paragraph","documentKind":"markdown","text":"owned documentation","contentHash":"doc-hash","policy":doc_head.policy,"provenance":doc_head.provenance,"createdAt":"2026-10-14T00:00:00Z"})).unwrap();
    for payload in [
        engram_knowledge::DocumentHeadWrite::Source(source),
        engram_knowledge::DocumentHeadWrite::Document(doc),
        engram_knowledge::DocumentHeadWrite::Chunk(Box::new(paragraph)),
    ] {
        block_on(store.write_document_record(&doc_head.key, &doc_head.attempt_id, payload))
            .unwrap();
    }
    doc_head.state = CodeSourceState::Active;
    block_on(store.activate_document_head(doc_head.clone())).unwrap();
    {
        let conn = store.lock().unwrap();
        conn.execute(
            "INSERT INTO document_head_records VALUES(?1,?2,'relationship','doc-edge')",
            rusqlite::params![
                serde_json::to_string(&doc_head.key).unwrap(),
                doc_head.attempt_id
            ],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO code_source_contributions VALUES(?1,?2,'relationship','code-edge')",
            rusqlite::params![
                serde_json::to_string(&first.file).unwrap(),
                first.attempt_id
            ],
        )
        .unwrap();
    }
    for actor in ["owner", "other", "stranger"] {
        for scope in [
            first.file.scope.clone(),
            serde_json::from_value(json!({"tenant":"wrong"})).unwrap(),
        ] {
            let request:RetrievalRequest=serde_json::from_value(json!({"scope":scope,"query":"q","requester":{"actor":{"id":actor,"kind":"agent"}}})).unwrap();
            let full = store.source_record_access(&request).unwrap();
            if scope == first.file.scope && actor == "owner" {
                assert!(full.allows(RetrievalTargetType::Entity, "owner-only"));
                assert!(!full.allows(RetrievalTargetType::Chunk, "owner-only"));
                assert!(
                    full.policy(RetrievalTargetType::Relationship, "code-edge")
                        .is_some()
                );
                assert!(!full.allows(RetrievalTargetType::Relationship, "doc-edge"));
            }
            if scope == first.file.scope && actor == "other" {
                assert!(
                    full.policy(RetrievalTargetType::Chunk, "owner-only")
                        .is_some()
                );
                assert!(
                    full.policy(RetrievalTargetType::Relationship, "doc-edge")
                        .is_some()
                );
            }
            for id in [
                "shared",
                "owner-only",
                "other-only",
                "legacy-only",
                "unmanaged",
                "doc-edge",
                "code-edge",
                first.snapshot_ids[0].as_str(),
            ] {
                let target = store.source_record_access_for_target(&request, id).unwrap();
                for kind in [
                    RetrievalTargetType::Entity,
                    RetrievalTargetType::Chunk,
                    RetrievalTargetType::Relationship,
                ] {
                    assert_eq!(
                        target.allows(kind.clone()),
                        full.allows(kind.clone(), id),
                        "{actor} {id}"
                    );
                    assert_eq!(
                        target.policy(kind.clone()),
                        full.policy(kind, id),
                        "{actor} {id}"
                    );
                }
            }
        }
    }
    // An unrelated broken ownership row cannot widen the target or make a
    // bounded target read consult another record's unavailable manifest.
    store
        .lock()
        .unwrap()
        .execute(
            "INSERT INTO code_source_contributions VALUES('broken','real','entity','unrelated')",
            [],
        )
        .unwrap();
    let request:RetrievalRequest=serde_json::from_value(json!({"scope":first.file.scope,"query":"q","requester":{"actor":{"id":"owner","kind":"agent"}}})).unwrap();
    assert!(
        store
            .source_record_access_for_target(&request, "shared")
            .unwrap()
            .allows(RetrievalTargetType::Entity)
    );
    assert!(
        store
            .source_record_access_for_target(&request, "unrelated")
            .is_err()
    );
}

#[test]
fn indexed_revision_is_authorized_current_metadata_with_document_source_fallbacks() {
    let store = SqlKnowledgeStore::open_in_memory().unwrap();
    for (path, document_version, source_version, revision, expected) in [
        (
            "doc.ts",
            Some("doc-rev"),
            Some("source-rev"),
            Some("git-rev"),
            Some("doc-rev"),
        ),
        (
            "source.ts",
            None,
            Some("source-rev"),
            Some("git-rev"),
            Some("source-rev"),
        ),
        ("git.ts", None, None, Some("git-rev"), Some("git-rev")),
        ("unknown.ts", None, None, None, None),
    ] {
        let mut m = manifest(path, "first", "hello");
        block_on(store.begin_code_source(m.clone(), None)).unwrap();
        block_on(store.stage_code_records(&m.file, &m.attempt_id, &m.records)).unwrap();
        let (mut source, mut document) = parents(&m);
        source.version = source_version.map(str::to_owned);
        document.version = document_version.map(str::to_owned);
        if let Some(revision) = revision {
            source
                .metadata
                .as_mut()
                .unwrap()
                .insert("revision".into(), json!(revision));
        }
        for record in [
            CodeSourceWrite::Source(source),
            CodeSourceWrite::Document(document),
            CodeSourceWrite::Chunk(chunk(&m, "hello")),
        ] {
            block_on(store.write_code_record(&m.file, &m.attempt_id, record)).unwrap();
        }
        m.state = CodeSourceState::Active;
        block_on(store.activate_code_source(m.clone())).unwrap();
        let requester = read(&m).requester;
        assert_eq!(
            block_on(store.indexed_code_revision(&m.file, &m.file.scope, &requester))
                .unwrap()
                .as_deref(),
            expected
        );
        let stranger: Requester =
            serde_json::from_value(json!({"actor":{"id":"stranger","kind":"agent"}})).unwrap();
        assert!(
            block_on(store.indexed_code_revision(&m.file, &m.file.scope, &stranger))
                .unwrap()
                .is_none()
        );
        let wrong: Scope = serde_json::from_value(json!({"tenant":"other"})).unwrap();
        assert!(
            block_on(store.indexed_code_revision(&m.file, &wrong, &requester))
                .unwrap()
                .is_none()
        );
        let prior = m.attempt_id.clone();
        m.attempt_id = "next".into();
        m.state = CodeSourceState::Updating;
        block_on(store.begin_code_source(m.clone(), Some(&prior))).unwrap();
        assert!(
            block_on(store.indexed_code_revision(&m.file, &m.file.scope, &requester))
                .unwrap()
                .is_none()
        );
    }
}

#[test]
fn callable_comment_prefix_can_activate_without_widening_declaration_span() {
    for (role, comment_bytes, suffix, expected) in [
        (CodeChunkRole::Symbol, 20, false, true),
        (CodeChunkRole::ContainerContext, 20, false, false),
        (CodeChunkRole::Symbol, 8193, false, false),
        (CodeChunkRole::Symbol, 20, true, false),
    ] {
        let store = SqlKnowledgeStore::open_in_memory().unwrap();
        let text = format!("//{}\nfunction run() {{}}", "x".repeat(comment_bytes));
        let mut m = manifest("callable.ts", "comment", &text);
        let mut unit = chunk(&m, &text);
        unit.id = Id::from("callable-with-comment");
        let code = unit.code.as_mut().unwrap();
        code.role = role;
        code.unit = Some("run".into());
        code.declaration_range = Some(CodeByteRange {
            start: text.find("function run").unwrap() as u64,
            end: text.len() as u64 - u64::from(suffix),
        });
        let declaration = code.declaration_range;
        m.records.push(CodeSourceRecord {
            kind: CodeSourceRecordKind::Chunk,
            id: unit.id.to_string(),
        });
        block_on(store.begin_code_source(m.clone(), None)).unwrap();
        block_on(store.stage_code_records(&m.file, &m.attempt_id, &m.records)).unwrap();
        guarded_parents(&store, &m);
        for c in [chunk(&m, &text), unit] {
            block_on(store.write_code_record(&m.file, &m.attempt_id, CodeSourceWrite::Chunk(c)))
                .unwrap();
        }
        m.state = CodeSourceState::Active;
        let result = block_on(store.activate_code_source(m.clone()));
        assert_eq!(
            result.is_ok(),
            expected,
            "role={role:?} prefix={comment_bytes} suffix={suffix}: {result:?}"
        );
        if expected {
            assert_eq!(
                block_on(store.read_code_source(read(&m), &m.file.scope))
                    .unwrap()
                    .text,
                text
            );
            let loaded =
                block_on(store.get_chunk(&Id::from("callable-with-comment"), &m.file.scope))
                    .unwrap()
                    .unwrap();
            assert_eq!(loaded.code.unwrap().declaration_range, declaration);
        }
    }
}
