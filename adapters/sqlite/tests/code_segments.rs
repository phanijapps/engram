use engram_domain::*;
use engram_knowledge::{
    CodeSegmentRepository, CodeSourceRepository, CodeSourceWrite, code_segment_text,
};
use engram_store_sqlite::SqlKnowledgeStore;
use futures::executor::block_on;
use serde_json::json;
use sha2::{Digest, Sha256};

fn fixture(
    private: bool,
) -> (
    SqlKnowledgeStore,
    CodeSourceGeneration,
    CodeSegmentInventory,
    CodeEmbeddingSegment,
    Requester,
) {
    let store = SqlKnowledgeStore::open_in_memory().unwrap();
    let hash: String = Sha256::digest(b"hello")
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let hash = format!("sha256:{hash}");
    let policy = json!({"visibility": if private {"private"} else {"workspace"}, "retention":"durable","allowedUses":["retrieval"]});
    let provenance = json!({"source":"fixture","actor":{"id":"owner","kind":"agent"},"observedAt":"2026-10-14T00:00:00Z"});
    let file =
        json!({"scope":{"tenant":"t","workspace":"w"},"repository":"repo","path":"src/a.ts"});
    let mut m: CodeSourceGeneration = serde_json::from_value(json!({"file":file,"generation":"generation","attemptId":"first","sourceHash":hash,"fingerprint":"fixture","state":"updating","byteLength":5,"documentId":"doc","snapshotIds":["snapshot"],"records":[{"kind":"source","id":"source"},{"kind":"document","id":"doc"},{"kind":"chunk","id":"snapshot"},{"kind":"chunk","id":"parent"}],"policy":policy,"provenance":provenance})).unwrap();
    block_on(store.begin_code_source(m.clone(), None)).unwrap();
    block_on(store.stage_code_records(&m.file, &m.attempt_id, &m.records)).unwrap();
    let source = serde_json::from_value(json!({"id":"source","kind":"filesystem","scope":m.file.scope,"name":"fixture","metadata":{"stableSourceKey":"repo"},"policy":policy,"provenance":provenance,"createdAt":"2026-10-14T00:00:00Z"})).unwrap();
    let doc = serde_json::from_value(json!({"id":"doc","sourceId":"source","kind":"code","path":"src/a.ts","contentHash":hash,"policy":policy,"provenance":provenance,"createdAt":"2026-10-14T00:00:00Z"})).unwrap();
    for value in [
        CodeSourceWrite::Source(source),
        CodeSourceWrite::Document(doc),
    ] {
        block_on(store.write_code_record(&m.file, "first", value)).unwrap();
    }
    for (id, role, unit) in [
        ("snapshot", "source_snapshot", None),
        ("parent", "symbol", Some("logical-parent")),
    ] {
        let chunk = serde_json::from_value(json!({"id":id,"sourceId":"source","documentId":"doc","kind":"code_block","documentKind":"code","text":"hello","contentHash":hash,"policy":policy,"provenance":provenance,"createdAt":"2026-10-14T00:00:00Z","code":{"role":role,"file":file,"generation":m.generation,"sourceHash":hash,"fingerprint":"fixture","ranges":[{"start":0,"end":5}],"unit":unit,"complete":true,"parseQuality":"clean"}})).unwrap();
        block_on(store.write_code_record(&m.file, "first", CodeSourceWrite::Chunk(chunk))).unwrap();
    }
    m.state = CodeSourceState::Active;
    block_on(store.activate_code_source(m.clone())).unwrap();
    let inventory = CodeSegmentInventory {
        file: m.file.clone(),
        generation: m.generation.clone(),
        attempt_id: m.attempt_id.clone(),
        representation_id: "representation-v1".into(),
        segment_ids: vec!["segment".into()],
        state: CodeSourceState::Updating,
    };
    let segment = CodeEmbeddingSegment {
        id: "segment".into(),
        parent_chunk_id: Id::from("parent"),
        parent_unit_id: "logical-parent".into(),
        file: m.file.clone(),
        generation: m.generation.clone(),
        attempt_id: m.attempt_id.clone(),
        source_hash: m.source_hash.clone(),
        representation_id: inventory.representation_id.clone(),
        ranges: vec![CodeByteRange { start: 0, end: 5 }],
        continuation: false,
        source_category: CodeSourceCategory::Implementation,
        context: vec![],
        embedding_input: CodeTokenCoverage {
            code_tokens: 5,
            context_tokens: 0,
            query_tokens: 0,
            special_tokens: 2,
            total_tokens: 7,
            truncated: false,
        },
    };
    let requester = serde_json::from_value(json!({"actor":{"id":"owner","kind":"agent"}})).unwrap();
    (store, m, inventory, segment, requester)
}

fn activate(
    store: &SqlKnowledgeStore,
    inventory: &CodeSegmentInventory,
    segments: Vec<CodeEmbeddingSegment>,
) {
    let lease = block_on(store.begin_code_segments(inventory.clone(), None)).unwrap();
    block_on(store.write_code_segments(&lease, segments)).unwrap();
    block_on(store.activate_code_segments(&lease)).unwrap();
}

#[test]
fn active_segments_resolve_to_canonical_parent_and_reject_wrong_rep_scope_or_actor() {
    let (store, _, inventory, segment, requester) = fixture(true);
    activate(&store, &inventory, vec![segment.clone()]);
    let resolved = block_on(store.resolve_code_segment(
        &segment.id,
        &inventory.file.scope,
        &requester,
        &inventory.representation_id,
    ))
    .unwrap()
    .unwrap();
    assert_eq!(resolved.parent.id, segment.parent_chunk_id);
    assert_eq!(
        code_segment_text(&resolved.parent, &segment.ranges).unwrap(),
        "hello"
    );
    assert!(
        block_on(store.resolve_code_segment(
            "segment",
            &inventory.file.scope,
            &requester,
            "incompatible-representation"
        ))
        .unwrap()
        .is_none()
    );
    let mut outsider = requester;
    outsider.actor.id = Id::from("outsider");
    assert!(
        block_on(store.resolve_code_segment(
            "segment",
            &inventory.file.scope,
            &outsider,
            &inventory.representation_id
        ))
        .unwrap()
        .is_none()
    );
    let mut other = inventory.file.scope.clone();
    other.session = Some("other".into());
    assert!(
        block_on(store.resolve_code_segment(
            "segment",
            &other,
            &outsider,
            &inventory.representation_id
        ))
        .unwrap()
        .is_none()
    );
}

#[test]
fn gaps_snapshot_donors_and_stale_attempts_cannot_activate_or_resolve() {
    let (store, mut m, inventory, mut segment, requester) = fixture(false);
    let lease = block_on(store.begin_code_segments(inventory.clone(), None)).unwrap();
    let mut snapshot = segment.clone();
    snapshot.parent_chunk_id = Id::from("snapshot");
    assert!(block_on(store.write_code_segments(&lease, vec![snapshot])).is_err());
    segment.ranges[0].end = 3;
    segment.embedding_input.code_tokens = 3;
    segment.embedding_input.total_tokens = 5;
    block_on(store.write_code_segments(&lease, vec![segment])).unwrap();
    assert!(
        block_on(store.activate_code_segments(&lease)).is_err(),
        "partial method cover is not a ready representation"
    );
    assert!(
        block_on(store.resolve_code_segment(
            "segment",
            &m.file.scope,
            &requester,
            &inventory.representation_id
        ))
        .unwrap()
        .is_none()
    );
    m.state = CodeSourceState::Updating;
    m.attempt_id = "second".into();
    block_on(store.begin_code_source(m.clone(), Some("first"))).unwrap();
    assert!(
        block_on(store.load_current_code_segments(
            &m.file,
            &inventory.representation_id,
            &requester
        ))
        .unwrap()
        .is_none()
    );
    assert!(block_on(store.write_code_segments(&lease, vec![])).is_err());
}

#[test]
fn inventory_compare_and_set_and_monotonic_revision_preserve_ownership() {
    let (store, _, inventory, segment, requester) = fixture(false);
    let before = block_on(store.code_index_revision()).unwrap();
    activate(&store, &inventory, vec![segment.clone()]);
    assert!(block_on(store.code_index_revision()).unwrap() > before);
    assert!(block_on(store.begin_code_segments(inventory.clone(), None)).is_err());
    let active =
        block_on(store.code_segment_inventory(&inventory.file, &inventory.representation_id))
            .unwrap()
            .unwrap();
    let mut different = inventory.clone();
    different.representation_id = "other-representation".into();
    let foreign_lease = block_on(store.begin_code_segments(different.clone(), None)).unwrap();
    let mut foreign = segment;
    foreign.representation_id = different.representation_id.clone();
    assert!(
        block_on(store.write_code_segments(&foreign_lease, vec![foreign])).is_err(),
        "a representation cannot overwrite another target ID"
    );
    assert!(
        block_on(store.resolve_code_segment(
            "segment",
            &inventory.file.scope,
            &requester,
            &active.inventory.representation_id
        ))
        .unwrap()
        .is_some()
    );
}

#[test]
fn missing_begin_records_and_failed_attempts_are_recoverable_through_cas() {
    let (store, _, inventory, segment, requester) = fixture(false);
    let lease = block_on(store.begin_code_segments(inventory.clone(), None)).unwrap();
    assert!(
        block_on(store.load_current_code_segments(
            &inventory.file,
            &inventory.representation_id,
            &requester
        ))
        .unwrap()
        .is_none()
    );
    block_on(store.fail_code_segments(&lease)).unwrap();
    assert!(
        block_on(store.load_current_code_segments(
            &inventory.file,
            &inventory.representation_id,
            &requester
        ))
        .unwrap()
        .is_none()
    );
    let failed =
        block_on(store.code_segment_inventory(&inventory.file, &inventory.representation_id))
            .unwrap()
            .unwrap();
    let restarted = block_on(store.begin_code_segments(inventory.clone(), Some(&failed))).unwrap();
    assert!(restarted.revision > lease.revision);
    assert!(block_on(store.write_code_segments(&lease, vec![])).is_err());
    assert!(block_on(store.activate_code_segments(&lease)).is_err());
    assert!(block_on(store.fail_code_segments(&lease)).is_err());
    let lease = restarted;
    block_on(store.write_code_segments(&lease, vec![segment])).unwrap();
    block_on(store.activate_code_segments(&lease)).unwrap();
    assert!(
        block_on(store.load_current_code_segments(
            &inventory.file,
            &inventory.representation_id,
            &requester
        ))
        .unwrap()
        .is_some()
    );
}

#[test]
fn direct_segment_mutation_revokes_active_inventory_and_advances_revision() {
    for column in [
        "record_json=json_set(record_json,'$.continuation',json('true'))",
        "file_key='other-file'",
        "representation_id='other-representation'",
        "id='other-id'",
    ] {
        let (store, _, inventory, segment, requester) = fixture(false);
        activate(&store, &inventory, vec![segment]);
        let before = block_on(store.code_index_revision()).unwrap();
        store
            .shared_connection()
            .lock()
            .unwrap()
            .execute(
                &format!("UPDATE code_embedding_segments SET {column} WHERE id='segment'"),
                [],
            )
            .unwrap();
        assert!(block_on(store.code_index_revision()).unwrap() > before);
        assert_eq!(
            block_on(store.code_segment_inventory(&inventory.file, &inventory.representation_id))
                .unwrap()
                .unwrap()
                .inventory
                .state,
            CodeSourceState::Incomplete
        );
        assert!(
            block_on(store.resolve_code_segment(
                "segment",
                &inventory.file.scope,
                &requester,
                &inventory.representation_id
            ))
            .unwrap()
            .is_none()
        );
    }
}
