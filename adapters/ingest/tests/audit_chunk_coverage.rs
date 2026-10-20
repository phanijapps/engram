//! AUDIT: chunk coverage for large files — regression guards for the
//! fetch-content single-chunk class (reproduced + fixed 2026-10-20).
//!
//! The defect: large files came back from `fetch_content` as ONE chunk
//! because the content was never chunked — (a) large TS interfaces are
//! containers whose members (`method_signature`) were not in the kind map,
//! so the whole interface was one leaf; (b) test files keep every
//! declaration inside `describe`/`test` call arguments, which the walk
//! could not name, so the file fell to the single `file` fallback chunk.
//!
//! The fix, guarded here at every layer:
//!   1. `method_signature` / `abstract_method_signature` map to `fn` —
//!      interface members become per-method chunks + entities.
//!   2. Innermost test-wrapper calls chunk as `test '<name>'` / \
//!      `describe '<name>'` (no entities — content coverage only).
//!   3. Oversized leaves (incl. the `file` fallback) split into
//!      span-aligned line windows (leaf_split).
//!   4. `ScanSummary.coverage_warnings` names large files whose chunked
//!      bytes cover under half the file.

use engram_domain::*;
use engram_ingest::{
    CodeSymbolChunker, DocumentIngestRequest, DocumentMetadata, GraphExtractor, KnowledgeIngestor,
    ScanOptions, TreeSitterChunker, scan_repository,
};
use engram_store_sqlite::SqlKnowledgeStore;
use futures::executor::block_on;

fn scope() -> Scope {
    Scope {
        tenant: "tenant-chunk-coverage".to_owned(),
        subject: None,
        workspace: Some("test".to_owned()),
        session: None,
        environment: Some("test".to_owned()),
    }
}

fn policy() -> Policy {
    Policy {
        visibility: Visibility::Workspace,
        retention: Retention::Durable,
        sensitivity: Some(Sensitivity::Medium),
        allowed_uses: vec![AllowedUse::Retrieval],
        expires_at: None,
        delete_mode: Some(DeleteMode::Tombstone),
    }
}

fn actor() -> Actor {
    Actor {
        id: Id::from("agent-1"),
        kind: ActorKind::Agent,
        display_name: None,
        metadata: None,
    }
}

/// The wild victim shape A: a large TS interface (~18 KB, 80 methods).
fn interface_fixture() -> String {
    let mut lines = vec!["interface Transport {".to_owned()];
    for i in 0..80 {
        lines.push(format!(
            "  /** Operation {i}: handles step {i} of the transport pipeline with retry semantics. */"
        ));
        lines.push(format!(
            "  sendStep{i}(payload: Step{i}Payload, options?: Step{i}Options): Promise<Step{i}Result>;"
        ));
        lines.push(format!(
            "  receiveStep{i}(token: string): Step{i}Receipt | undefined;"
        ));
        lines.push(String::new());
    }
    lines.push("}".to_owned());
    lines.join("\n")
}

/// The wild victim shape B: a large TS test file (all declarations inside
/// describe/test call arguments; ~8 KB, 60 tests + one nested helper).
fn test_file_fixture() -> String {
    let mut lines = vec![
        "import { renderHook } from '@testing-library/react';".to_owned(),
        String::new(),
        "describe('useQuickChat', () => {".to_owned(),
    ];
    for i in 0..60 {
        lines.push(format!(
            "  test('delivers message batch {i}', async () => {{"
        ));
        lines.push(format!("    const received{i} = await deliverBatch({i});"));
        lines.push(format!("    expect(received{i}.count).toBe({i});"));
        lines.push("  });".to_owned());
        lines.push(String::new());
    }
    lines.push("  function deliverBatch(n: number) {".to_owned());
    lines.push("    return Promise.resolve({ count: n });".to_owned());
    lines.push("  }".to_owned());
    lines.push("});".to_owned());
    lines.join("\n")
}

/// Chunks + entities for one source file through the production path.
fn ingest_file(path: &str, text: &str) -> (Vec<String>, Vec<String>, usize) {
    let store = SqlKnowledgeStore::open_in_memory().expect("open store");
    let ts = TreeSitterChunker::new().expect("tree-sitter grammars");
    let ext = path.rsplit('.').next().expect("extension");
    let candidates = ts.chunk_with_ext(text, ext).expect("chunk");
    let ingestor = KnowledgeIngestor::new(CodeSymbolChunker);
    let request = DocumentIngestRequest {
        source_kind: SourceKind::Filesystem,
        source_name: "demo".to_owned(),
        scope: scope(),
        document_kind: SourceDocumentKind::Code,
        document: DocumentMetadata {
            path: Some(path.to_owned()),
            ..Default::default()
        },
        text: text.to_owned(),
        policy: policy(),
        actor: actor(),
        stable_source_key: None,
        source_metadata: None,
    };
    let ingested =
        block_on(ingestor.ingest_with_candidates(&store, request, candidates)).expect("ingest");
    let extracted = block_on(GraphExtractor::new().extract_into(
        &store,
        &ingested.source,
        &ingested.document,
        &ingested.chunks,
        None,
    ))
    .expect("extract");
    let anchors = ingested
        .chunks
        .iter()
        .filter_map(|c| c.location.as_ref().and_then(|l| l.anchor.clone()))
        .collect();
    let names = extracted.entities.iter().map(|e| e.name.clone()).collect();
    (anchors, names, ingested.chunks.len())
}

#[test]
fn large_interface_chunks_per_member_and_entities_follow() {
    let text = interface_fixture();
    let (anchors, names, chunk_count) = ingest_file("interface.ts", &text);

    // THE DEFECT, inverted: not one chunk — the container plus per-member
    // chunks (fetch_content now stitches many span-aligned chunks).
    assert!(
        chunk_count > 10,
        "large interface must not be one chunk, got {chunk_count}: {anchors:?}"
    );
    // Members are declarations with the receiver chain.
    assert!(
        anchors.contains(&"fn Transport::sendStep0".to_owned()),
        "member chunks must carry receiver-qualified anchors, got {anchors:?}"
    );
    assert!(
        names.contains(&"Transport::sendStep0".to_owned()),
        "interface methods must become entities (searchable by exact name), got {names:?}"
    );
}

#[test]
fn test_file_bodies_are_chunked_without_becoming_entities() {
    let text = test_file_fixture();
    let (anchors, names, chunk_count) = ingest_file("useQuickChat.test.ts", &text);

    // THE DEFECT, inverted: the test bodies are content chunks now.
    let test_chunks = anchors
        .iter()
        .filter(|a| a.starts_with("test '") || a.starts_with("describe '"))
        .count();
    assert!(
        test_chunks >= 60,
        "every innermost test call must be a chunk, got {test_chunks} of 60: {anchors:?}"
    );
    // Byte coverage: chunk text must cover (well past half of) the file.
    let ts = TreeSitterChunker::new().unwrap();
    let candidates = ts.chunk_with_ext(&text, "ts").unwrap();
    let chunked: usize = candidates.iter().map(|c| c.text.len()).sum();
    assert!(
        chunked as f64 / text.len() as f64 > 0.9,
        "test file coverage must exceed 90%, got {} of {}",
        chunked,
        text.len()
    );
    // Wrapper chunks are content, not symbol entities: no test-title
    // entities, and the nested helper directly under `describe` still
    // resolves as one.
    assert!(
        !names.iter().any(|n| n.contains("delivers message batch")),
        "test wrapper names must not become entities, got {names:?}"
    );
    assert!(
        names.contains(&"deliverBatch".to_owned()),
        "nested named helper under describe stays an entity, got {names:?}"
    );
    let _ = chunk_count;
}

#[test]
fn declaration_free_large_file_splits_into_line_windows() {
    // A ~12 KB code file with no declarations at all: previously the single
    // `file` fallback chunk; now span-aligned windows.
    let text: String = (0..400)
        .map(|i| format!("// commentary line {i} with enough padding to look like source code\n"))
        .collect();
    let ts = TreeSitterChunker::new().expect("tree-sitter");
    let chunks = ts.chunk_with_ext(&text, "rs").expect("chunk");
    assert!(
        chunks.len() > 1,
        "declaration-free large file must split into windows, got {}",
        chunks.len()
    );
    // Windows are contiguous, share the `file` anchor, and reassemble the file.
    let mut expected_start = 1u32;
    for c in &chunks {
        let loc = c.location.as_ref().unwrap();
        assert_eq!(loc.start_line, Some(expected_start), "contiguous windows");
        assert_eq!(
            loc.anchor.as_deref(),
            Some("file"),
            "windows keep the fallback anchor"
        );
        expected_start = loc.end_line.unwrap() + 1;
    }
    assert_eq!(expected_start, 401, "windows cover every line");
    let stitched = chunks
        .iter()
        .map(|c| c.text.clone())
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(stitched, text.trim_end());
}

#[test]
fn scan_summary_warns_on_low_chunk_coverage() {
    let store = SqlKnowledgeStore::open_in_memory().expect("open store");
    let dir = std::env::temp_dir().join(format!("engram-cov-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");

    // Pathological: one tiny declaration + ~10 KB of unchunkable commentary.
    let mut sparse = String::from("fn tiny() {}\n");
    for i in 0..300 {
        sparse.push_str(&format!(
            "// trailing commentary {i} that no declaration owns\n"
        ));
    }
    std::fs::write(dir.join("sparse.rs"), &sparse).expect("write sparse");

    // Healthy: many real declarations.
    let mut dense = String::new();
    for i in 0..100 {
        dense.push_str(&format!("fn dense_{i}() -> u32 {{ {i} }}\n"));
    }
    std::fs::write(dir.join("dense.rs"), &dense).expect("write dense");

    let (summary, _manifest) = scan_repository(
        &dir,
        &ScanOptions {
            scope: scope(),
            policy: policy(),
            actor: actor(),
            source_name: "cov-test".to_owned(),
            max_bytes: 0,
            manifest: Default::default(),
            scan_filter: engram_ingest::ScanFilter::default(),
        },
        &store,
        |_| {},
    )
    .expect("scan");

    assert_eq!(summary.errors, 0);
    assert!(
        summary
            .coverage_warnings
            .iter()
            .any(|w| w.contains("sparse.rs") && w.contains("chunk coverage")),
        "the sparse large file must be flagged, got {:?}",
        summary.coverage_warnings
    );
    assert!(
        !summary
            .coverage_warnings
            .iter()
            .any(|w| w.contains("dense.rs")),
        "the healthy file must not be flagged, got {:?}",
        summary.coverage_warnings
    );
    let _ = std::fs::remove_dir_all(&dir);
}
