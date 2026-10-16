use engram_domain::*;
use engram_knowledge::{
    CodeSourceRepository, DocumentHeadRepository, DocumentHeadWrite, KnowledgeRepository,
};
use engram_store_sqlite::SqlKnowledgeStore;
use futures::executor::block_on;
use serde_json::json;

fn head(attempt: &str) -> DocumentHead {
    serde_json::from_value(json!({"key":{"scope":{"tenant":"t","workspace":"w"},"repository":"repo","sourceKey":"README.md"},
        "generation":format!("generation-{attempt}"),"attemptId":attempt,"state":"updating","sourceHash":format!("hash-{attempt}"),
        "documentId":format!("doc-{attempt}"),"chunkIds":[format!("chunk-{attempt}")],
        "policy":{"visibility":"workspace","retention":"durable","allowedUses":["retrieval"]},
        "provenance":{"source":"test","actor":{"id":"owner","kind":"agent"},"observedAt":"2026-10-14T00:00:00Z"}})).unwrap()
}

fn write(store: &SqlKnowledgeStore, h: &DocumentHead) {
    let source=serde_json::from_value(json!({"id":format!("source-{}",h.attempt_id),"kind":"git_repository","scope":h.key.scope,"name":"repo","metadata":{"stableSourceKey":h.key.repository},"policy":h.policy,"provenance":h.provenance,"createdAt":"2026-10-14T00:00:00Z"})).unwrap();
    let doc=serde_json::from_value(json!({"id":h.document_id,"sourceId":format!("source-{}",h.attempt_id),"kind":"markdown","path":"README.md","contentHash":h.source_hash,"policy":h.policy,"provenance":h.provenance,"createdAt":"2026-10-14T00:00:00Z"})).unwrap();
    let chunk=serde_json::from_value(json!({"id":h.chunk_ids[0],"documentId":h.document_id,"sourceId":format!("source-{}",h.attempt_id),"kind":"paragraph","documentKind":"markdown","text":format!("Same heading, body {}",h.attempt_id),"contentHash":format!("chunk-hash-{}",h.attempt_id),"policy":h.policy,"provenance":h.provenance,"createdAt":"2026-10-14T00:00:00Z"})).unwrap();
    for payload in [
        DocumentHeadWrite::Source(source),
        DocumentHeadWrite::Document(doc),
        DocumentHeadWrite::Chunk(chunk),
    ] {
        block_on(store.write_document_record(&h.key, &h.attempt_id, payload)).unwrap();
    }
}

#[test]
fn four_versions_keep_history_but_only_verified_head_is_current() {
    let store = SqlKnowledgeStore::open_in_memory().unwrap();
    let mut expected = None;
    for attempt in ["one", "two", "three", "four"] {
        let mut h = head(attempt);
        block_on(store.begin_document_head(h.clone(), expected.as_deref())).unwrap();
        assert!(
            block_on(store.current_document_for_chunk(&h.chunk_ids[0], &h.key.scope))
                .unwrap()
                .is_none()
        );
        write(&store, &h);
        h.state = CodeSourceState::Active;
        block_on(store.activate_document_head(h.clone())).unwrap();
        assert_eq!(
            block_on(store.current_document_for_chunk(&h.chunk_ids[0], &h.key.scope)).unwrap(),
            Some(h.clone())
        );
        expected = Some(attempt.to_string());
    }
    let first = head("one");
    assert!(
        block_on(store.current_document_for_chunk(&first.chunk_ids[0], &first.key.scope))
            .unwrap()
            .is_none()
    );
    let conn = store.lock().unwrap();
    let retained: i64 = conn
        .query_row("SELECT count(*) FROM knowledge_chunks_records", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(retained, 4);
}

#[test]
fn missing_inventory_or_superseded_attempt_cannot_activate() {
    let store = SqlKnowledgeStore::open_in_memory().unwrap();
    let a = head("a");
    block_on(store.begin_document_head(a.clone(), None)).unwrap();
    let mut active = a.clone();
    active.state = CodeSourceState::Active;
    assert!(block_on(store.activate_document_head(active)).is_err());
    let b = head("b");
    block_on(store.begin_document_head(b.clone(), Some("a"))).unwrap();
    assert!(block_on(store.fail_document_head(&a.key, "a")).is_err());
    assert!(
        block_on(store.begin_document_head(a, Some("b"))).is_err(),
        "attempt ids cannot be reused"
    );
    write(&store, &b);
    let mut active = b.clone();
    active.state = CodeSourceState::Active;
    block_on(store.activate_document_head(active.clone())).unwrap();
    let mut other = active.key.scope.clone();
    other.workspace = Some("other".into());
    assert!(
        block_on(store.current_document_for_chunk(&active.chunk_ids[0], &other))
            .unwrap()
            .is_none()
    );
    assert!(store.document_heads().is_some());
}

#[test]
fn unfenced_writes_and_direct_canonical_changes_cannot_keep_head_active() {
    let store = SqlKnowledgeStore::open_in_memory().unwrap();
    let mut h = head("a");
    block_on(store.begin_document_head(h.clone(), None)).unwrap();
    write(&store, &h);
    h.state = CodeSourceState::Active;
    block_on(store.activate_document_head(h.clone())).unwrap();
    let conn = store.lock().unwrap();
    let raw: String = conn
        .query_row(
            "SELECT record_json FROM knowledge_chunks_records WHERE id=?1",
            [h.chunk_ids[0].as_str()],
            |r| r.get(0),
        )
        .unwrap();
    drop(conn);
    let mut c: KnowledgeChunk = serde_json::from_str(&raw).unwrap();
    c.text = "forged".into();
    assert!(block_on(store.put_chunk(c)).is_err());
    store.lock().unwrap().execute("UPDATE knowledge_chunks_records SET record_json=json_set(record_json,'$.text','changed') WHERE id=?1",[h.chunk_ids[0].as_str()]).unwrap();
    assert!(
        block_on(store.current_document_for_chunk(&h.chunk_ids[0], &h.key.scope))
            .unwrap()
            .is_none()
    );
    let retained:String=store.lock().unwrap().query_row("SELECT record_json FROM canonical_document_history WHERE kind='chunk' AND record_id=?1",[h.chunk_ids[0].as_str()],|r|r.get(0)).unwrap();
    assert_eq!(retained, raw);
}

#[test]
fn a_managed_chunk_id_cannot_be_rebound_to_different_vector_content() {
    let store = SqlKnowledgeStore::open_in_memory().unwrap();
    let mut a = head("a");
    block_on(store.begin_document_head(a.clone(), None)).unwrap();
    write(&store, &a);
    a.state = CodeSourceState::Active;
    block_on(store.activate_document_head(a.clone())).unwrap();
    let conn = store.lock().unwrap();
    let source: KnowledgeSource = serde_json::from_str(
        &conn
            .query_row("SELECT record_json FROM knowledge_sources", [], |r| {
                r.get::<_, String>(0)
            })
            .unwrap(),
    )
    .unwrap();
    let doc: SourceDocument = serde_json::from_str(
        &conn
            .query_row("SELECT record_json FROM knowledge_documents", [], |r| {
                r.get::<_, String>(0)
            })
            .unwrap(),
    )
    .unwrap();
    let mut chunk: KnowledgeChunk = serde_json::from_str(
        &conn
            .query_row(
                "SELECT record_json FROM knowledge_chunks_records",
                [],
                |r| r.get::<_, String>(0),
            )
            .unwrap(),
    )
    .unwrap();
    drop(conn);
    let mut b = a.clone();
    b.attempt_id = "b".into();
    b.state = CodeSourceState::Updating;
    block_on(store.begin_document_head(b.clone(), Some("a"))).unwrap();
    for payload in [
        DocumentHeadWrite::Source(source),
        DocumentHeadWrite::Document(doc),
    ] {
        block_on(store.write_document_record(&b.key, "b", payload)).unwrap();
    }
    chunk.text = "different body under an already embedded ID".into();
    let error = block_on(store.write_document_record(&b.key, "b", DocumentHeadWrite::Chunk(chunk)))
        .unwrap_err();
    assert!(error.to_string().contains("immutable per ID"), "{error}");
    b.state = CodeSourceState::Active;
    assert!(
        block_on(store.activate_document_head(b.clone())).is_err(),
        "failed content cannot be published using a cached vector ID"
    );
    assert!(
        block_on(store.current_document_for_chunk(&a.chunk_ids[0], &a.key.scope))
            .unwrap()
            .is_none()
    );
}

#[test]
fn updating_generic_lease_blocks_code_takeover_and_wildcard_mutation() {
    let store = SqlKnowledgeStore::open_in_memory().unwrap();
    let mut h = head("a");
    block_on(store.begin_document_head(h.clone(), None)).unwrap();
    let code: CodeSourceGeneration = serde_json::from_value(json!({"file":{"scope":h.key.scope,"repository":h.key.repository,"path":h.key.source_key},"generation":"code-generation","attemptId":"code-attempt","sourceHash":"code-hash","fingerprint":"code-test","state":"updating","byteLength":5,"snapshotIds":[],"records":[],"policy":h.policy,"provenance":h.provenance})).unwrap();
    assert!(block_on(store.begin_code_source(code.clone(), None)).is_err());
    assert!(
        block_on(store.get_code_source(&code.file))
            .unwrap()
            .is_none()
    );
    write(&store, &h);
    let mut doc: SourceDocument = serde_json::from_str(
        &store
            .lock()
            .unwrap()
            .query_row("SELECT record_json FROM knowledge_documents", [], |r| {
                r.get::<_, String>(0)
            })
            .unwrap(),
    )
    .unwrap();
    doc.path = Some("another-file.md".into());
    assert!(
        block_on(store.write_document_record(&h.key, "a", DocumentHeadWrite::Document(doc)))
            .is_err()
    );
    h.state = CodeSourceState::Active;
    block_on(store.activate_document_head(h.clone())).unwrap();
    let mut broad = h.key.scope.clone();
    broad.workspace = None;
    assert!(block_on(store.delete_chunk(&h.chunk_ids[0], &broad)).is_err());
    assert_eq!(
        block_on(store.current_document_for_chunk(&h.chunk_ids[0], &h.key.scope)).unwrap(),
        Some(h)
    );
}
