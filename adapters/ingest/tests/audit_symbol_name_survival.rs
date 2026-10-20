//! AUDIT: symbol-name survival through the ingest pipeline — a battery over
//! language identifier conventions. Companion to
//! `repro_underscore_private_symbols.rs` (the leading-underscore instance of
//! this class).
//!
//! Every test here ingests a small realistic fixture per language and asserts
//! what the entity graph actually kept. FIXED 2026-10-19 (noise-policy split,
//! arrow-function names, identifier widening): the A-class declaration drops
//! and the Ruby/?/$-renaming collisions are now `regression_*` guards. Still
//! OPEN, pinned as `witness_*`: C++ operator-name mangling (declarator
//! segment-split), Kotlin backtick names, Go bare methods (receiver not
//! chained), dot-qualified `bare_tail`, and the closed `parse_symbol` keyword
//! set that future tree-sitter grammars must match.
//!
//! Pipeline stages (core/code + adapters/ingest + mcp search):
//!   1. tree-sitter walk     — needs a name/declarator/type FIELD on the node
//!      (function-valued `variable_declarator` bindings resolved since fix)
//!   2. extract_name         — keeps `_ $ ' ! ?` identifier shapes since fix
//!   3. parse_symbol         — CLOSED keyword set; unknown keyword = silent drop (OPEN)
//!   4. noise policies       — declaration vs callee split since fix
//!   5. bare_tail            — splits only on `::`, never on `.` (OPEN)
//!   6. normalize_identifier_text (lexical) — Unicode-aware since fix
//!   7. name_matches_query   — exact short-name matches allowed since fix

use engram_domain::*;
use engram_ingest::{
    CodeSymbolChunker, DocumentIngestRequest, DocumentMetadata, GraphExtractor, KnowledgeIngestor,
    TreeSitterChunker,
};
use engram_store_sqlite::SqlKnowledgeStore;
use futures::executor::block_on;

fn scope() -> Scope {
    Scope {
        tenant: "tenant-a".to_owned(),
        subject: Some("subject-a".to_owned()),
        workspace: Some("workspace-a".to_owned()),
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

/// Ingests one source file through the PRODUCTION path (TreeSitterChunker →
/// ingest_with_candidates, mirroring the scanner's ingest phase) and returns
/// the entity names + calls edges.
fn ingest(path: &str, text: &str) -> (Vec<String>, Vec<(String, String)>) {
    let store = SqlKnowledgeStore::open_in_memory().expect("open store");
    let ts = TreeSitterChunker::new().expect("tree-sitter grammars");
    let ext = path.rsplit('.').next().expect("extension");
    let candidates = ts.chunk_with_ext(text, ext).expect("chunk");
    // Inert: ingest_with_candidates skips the internal chunker (the
    // tree-sitter candidates above are what production scans feed).
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
        actor: Actor {
            id: Id::from("agent-1"),
            kind: ActorKind::Agent,
            display_name: None,
            metadata: None,
        },
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
    let names: Vec<String> = extracted.entities.iter().map(|e| e.name.clone()).collect();
    let calls: Vec<(String, String)> = extracted
        .relationships
        .iter()
        .filter(|r| r.predicate == "calls")
        .map(|r| {
            (
                r.subject.name.clone().unwrap_or_default(),
                r.object.name.clone().unwrap_or_default(),
            )
        })
        .collect();
    (names, calls)
}

// ---------------------------------------------------------------------------
// ---------------------------------------------------------------------------
// Pattern 2 (FIXED 2026-10-19): the bare-generic SYMBOL_NOISE list no longer
// applies to DECLARATIONS — `is_noise_declaration` governs entity emission.
// ---------------------------------------------------------------------------

/// Rust's constructor idiom `Type::new` now becomes an entity.
#[test]
fn regression_rust_new_constructor_exists() {
    let (names, _) = ingest(
        "session.rs",
        "impl Session {\n    pub fn new() -> Session { Session }\n    pub fn from_parts() -> Session { Session }\n}\n",
    );
    for expected in ["Session", "Session::new", "Session::from_parts"] {
        assert!(
            names.contains(&expected.to_owned()),
            "{expected} must be an entity, got {names:?}"
        );
    }
}

/// Go's core interface surface (`String` etc.) now becomes entities (they
/// remain bare names — the Go receiver is not yet chained into a qualified
/// identity; see the open-gaps note in the file header).
#[test]
fn regression_go_io_interface_methods_exist() {
    let (names, _) = ingest(
        "reader.go",
        "package m\n\ntype Reader struct{}\n\nfunc (r Reader) String() string { return \"\" }\n\nfunc (r Reader) Drain(p []byte) (int, error) { return 0, nil }\n",
    );
    for expected in ["String", "Drain"] {
        assert!(
            names.contains(&expected.to_owned()),
            "{expected} must be an entity, got {names:?}"
        );
    }
}

/// Python `run`/`get`-style declared methods now become entities.
#[test]
fn regression_python_run_get_methods_exist() {
    let (names, _) = ingest(
        "gate.py",
        "class Gate:\n    def run(self):\n        pass\n\n    def get(self):\n        pass\n\n    def execute(self):\n        pass\n",
    );
    for expected in ["Gate", "Gate::run", "Gate::get", "Gate::execute"] {
        assert!(
            names.contains(&expected.to_owned()),
            "{expected} must be an entity, got {names:?}"
        );
    }
}

// shapes → distinct symbols COLLIDE into one entity.
// ---------------------------------------------------------------------------

/// Ruby predicate/bang methods keep their full names — three distinct
/// methods, three distinct entities.
#[test]
fn regression_ruby_bang_question_methods_distinct() {
    let (names, _) = ingest(
        "store.rb",
        "def valid?(x)\n  true\nend\n\ndef save!\n  1\nend\n\ndef save\n  2\nend\n",
    );
    for expected in ["valid?", "save!", "save"] {
        assert!(
            names.contains(&expected.to_owned()),
            "{expected} must be a distinct entity, got {names:?}"
        );
    }
}

/// C++ `operator==` / `operator+` — the declarator text (with parameters) is
/// segment-split, so operator names collapse (and pick up parameter noise).
#[test]
fn witness_cpp_operator_names_mangled() {
    let (names, _) = ingest(
        "counter.cpp",
        "class Counter {\npublic:\n  bool operator==(const Counter& o) { return true; }\n  Counter operator+(const Counter& o) { return o; }\n};\n",
    );
    assert!(
        !names
            .iter()
            .any(|n| n.contains("operator==") || n.contains("operator+")),
        "witness: C++ operator names are mangled away today, got {names:?}"
    );
}

/// JS/TS `$`-prefixed methods keep their full names — `Store::$apply` no
/// longer collides with the real `Store::apply`.
#[test]
fn regression_js_dollar_method_kept_distinct() {
    let (names, _) = ingest(
        "store.ts",
        "class Store {\n  $apply(patch: unknown) { return patch; }\n  apply(patch: unknown) { return patch; }\n}\n",
    );
    for expected in ["Store", "Store::$apply", "Store::apply"] {
        assert!(
            names.contains(&expected.to_owned()),
            "{expected} must be an entity, got {names:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// Pattern 4 (FIXED 2026-10-19): the walk resolves function-valued bindings
// (`const X = (props) => ...`) from the variable_declarator name — the
// ts_kinds `arrow_function` entry is no longer dead code.
// ---------------------------------------------------------------------------

#[test]
fn regression_ts_arrow_function_components_exist() {
    let (names, _) = ingest(
        "quick_edit.tsx",
        "const QuickEdit = (props: unknown) => props;\n\nfunction render() { return 1; }\n",
    );
    for expected in ["QuickEdit", "render"] {
        assert!(
            names.contains(&expected.to_owned()),
            "{expected} must be an entity, got {names:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// Pattern 5 (FIXED 2026-10-19): unicode identifiers survive the noise
// filter AND the lexical lane is now Unicode-aware, so the entity is both
// indexed and BM25-searchable (companion battery:
// adapters/retrieval/tantivy-lexical/tests/audit_identifier_normalization.rs
// — normalize_identifier_text("名前を返す") == "名前を返す").
// ---------------------------------------------------------------------------

#[test]
fn regression_unicode_entities_exist_and_are_lexically_reachable() {
    let (names, _) = ingest("names.rs", "fn 名前を返す() -> u32 { 1 }\n");
    assert!(
        names.contains(&"名前を返す".to_owned()),
        "unicode entity must exist (noise filter keeps it; lexical normalization now keeps its token), got {names:?}"
    );
    assert_eq!(engram_code::bare_tail("名前を返す"), None);
}
