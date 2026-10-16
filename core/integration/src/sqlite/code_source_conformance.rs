//! Small runtime source-cell conformance, isolated from the user's database.
use engram_domain::*;
use engram_knowledge::{CodeSourceRepository, CodeSourceWrite};
use engram_runtime::CoreResult;
use engram_store_sqlite::SqlKnowledgeStore;
use futures::executor::block_on;
use serde_json::json;
pub(super) fn passes() -> bool {
    block_on(check()).is_ok()
}
async fn check() -> CoreResult<()> {
    let store = SqlKnowledgeStore::open_in_memory()?;
    let hash = format!("sha256:{}", crate::record_key_hash("π\r\n"));
    let m:CodeSourceGeneration=serde_json::from_value(json!({
 "file":{"scope":{"tenant":"source-conformance"},"repository":"repo","path":"a.ts"},"generation":"g1","attemptId":"a1","sourceHash":hash,"fingerprint":"source-v1","state":"updating","byteLength":4,"documentId":"d1","snapshotIds":["c1"],"records":[{"kind":"source","id":"s1"},{"kind":"document","id":"d1"},{"kind":"chunk","id":"c1"}],"policy":{"visibility":"private","retention":"durable","allowedUses":["retrieval"]},"provenance":{"source":"conformance","actor":{"id":"source-owner","kind":"agent"},"observedAt":"2026-10-14T00:00:00Z"}
 })).map_err(|e|engram_runtime::CoreError::ValidationFailed{reason:e.to_string()})?;
    let source:KnowledgeSource=serde_json::from_value(json!({"id":"s1","scope":m.file.scope,"kind":"filesystem","name":"conformance","policy":m.policy,"provenance":m.provenance,"createdAt":"2026-10-14T00:00:00Z","metadata":{"stableSourceKey":"repo"}})).unwrap();
    let doc:SourceDocument=serde_json::from_value(json!({"id":"d1","sourceId":"s1","kind":"code","path":"a.ts","contentHash":hash,"policy":m.policy,"provenance":m.provenance,"createdAt":"2026-10-14T00:00:00Z"})).unwrap();
    let chunk:KnowledgeChunk=serde_json::from_value(json!({"id":"c1","sourceId":"s1","documentId":"d1","documentKind":"code","kind":"code_block","text":"π\r\n","contentHash":hash,"policy":m.policy,"provenance":m.provenance,"createdAt":"2026-10-14T00:00:00Z","code":{"role":"source_snapshot","file":m.file,"generation":"g1","sourceHash":hash,"fingerprint":"source-v1","ranges":[{"start":0,"end":4}],"complete":true,"parseQuality":"clean"}})).unwrap();
    store.begin_code_source(m.clone(), None).await?;
    store
        .stage_code_records(&m.file, &m.attempt_id, &m.records)
        .await?;
    for payload in [
        CodeSourceWrite::Source(source),
        CodeSourceWrite::Document(doc),
        CodeSourceWrite::Chunk(chunk),
    ] {
        store
            .write_code_record(&m.file, &m.attempt_id, payload)
            .await?;
    }
    let mut active = m.clone();
    active.state = CodeSourceState::Active;
    store.activate_code_source(active).await?;
    let requester: Requester =
        serde_json::from_value(json!({"actor":{"id":"source-owner","kind":"agent"}})).unwrap();
    let result = store
        .read_code_source(
            CodeSourceReadRequest {
                file: m.file.clone(),
                requester,
                generation: Some(m.generation),
                max_bytes: 100,
            },
            &m.file.scope,
        )
        .await?;
    if result.text != "π\r\n" || !store.list_chunks(&m.file.scope).await?.is_empty() {
        return Err(engram_runtime::CoreError::ValidationFailed {
            reason: "source conformance failed".into(),
        });
    }
    Ok(())
}
