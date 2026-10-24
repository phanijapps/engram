//! REPRODUCTION (claim verification): "search can't find a symbol by its
//! exact name when the name starts with an underscore, even though the symbol
//! is indexed" (engram 0.2.15-alpha, search limit 5; e.g. `_tokens` /
//! `_check_review` not found while `run_gate` / `check_review` are).
//!
//! Verdict of this investigation: the claim is TRUE, with one correction —
//! the symbols are never *resolved* because they are never *indexed as
//! entities* in the first place. The code TEXT chunks containing them are
//! indexed (which is why the symbol feels present), but the entity emission
//! drops the symbol outright:
//!
//!   adapters/ingest/src/extractor.rs (entity emission):
//!       let bare = engram_code::bare_tail(&name).unwrap_or(&name);
//!       if bare.is_empty() || engram_code::is_noise_symbol(&bare) {
//!           continue; // symbol never becomes an entity
//!       }
//!
//!   core/code/src/noise.rs (`is_noise_symbol`):
//!       if !lower.starts_with(|c: char| c.is_alphabetic()) {
//!           return true; // "_anything" → noise
//!       }
//!
//! The rule targets punctuation/code-block noise ("...", "###", "123abc") but
//! also rejects every leading-underscore identifier — Python / Rust
//! private-by-convention helpers (`_tokens`, `_check_review`, `__init__`),
//! which are often real logic. Downstream, `search` cannot find them by exact
//! name because both the BM25 symbol feed and the exact-match injection
//! (`inject_exact_matches` in mcp/engram-mcp/src/codegraph.rs) operate on the
//! entity set. The user's "likely cause: engram treats private python helpers
//! as skippable" is essentially correct — it is a noise-filter overreach, not
//! a deliberate private-symbol policy.
//!
//! Witness tests (`defect_witness_*`) pin today's behavior; contract tests
//! (`contract_*`) encode the desired behavior and are RED until the filter is
//! fixed.

use engram_domain::*;
use engram_ingest::{
    CodeSymbolChunker, DocumentIngestRequest, DocumentMetadata, GraphExtractor, KnowledgeIngestor,
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

/// Python module mixing public helpers with leading-underscore (private by
/// convention) helpers — the shape the claim was made about.
const PY_MODULE: &str = "\
def run_gate():
    return _tokens()

def check_review():
    return _check_review()

def _tokens():
    return []

def _check_review():
    return True
";

/// Ingests `PY_MODULE` and returns (entity names, calls edges, chunk texts).
fn ingest_and_extract() -> (Vec<String>, Vec<(String, String)>, Vec<String>) {
    let store = SqlKnowledgeStore::open_in_memory().expect("open store");
    let ingestor = KnowledgeIngestor::new(CodeSymbolChunker);
    let request = DocumentIngestRequest {
        source_kind: SourceKind::Filesystem,
        source_name: "demo".to_owned(),
        scope: scope(),
        document_kind: SourceDocumentKind::Code,
        document: DocumentMetadata {
            path: Some("validators.py".to_owned()),
            ..Default::default()
        },
        text: PY_MODULE.to_owned(),
        policy: policy(),
        actor: Actor {
            id: Id::from("agent-1"),
            kind: ActorKind::Agent,
            display_name: None,
            metadata: None,
        },
        stable_source_key: None,
        repository_key: None,
        git_ref: None,
        source_metadata: None,
    };

    let ingested = block_on(ingestor.ingest(&store, request)).expect("ingest");
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
    let texts: Vec<String> = ingested.chunks.iter().map(|c| c.text.clone()).collect();
    (names, calls, texts)
}

// ---------------------------------------------------------------------------
// Regression guards — FIXED 2026-10-19 (noise policy split:
// `is_noise_declaration` at emission, leading `_`/`$` allowed).
// ---------------------------------------------------------------------------

/// The unit rule after the fix: declaration policy keeps leading-underscore
/// and `$` names; the callee policy (`is_noise_symbol`) also keeps them (so
/// unknown `_helper` callees reach the resolution ledger) while bare
/// generics stay callee-noise.
#[test]
fn regression_noise_policies_keep_private_convention_names() {
    assert!(!engram_code::is_noise_declaration("_tokens"));
    assert!(!engram_code::is_noise_declaration("$apply"));
    assert!(!engram_code::is_noise_declaration("new"));
    assert!(!engram_code::is_noise_symbol("_tokens"));
    assert!(!engram_code::is_noise_symbol("$apply"));
    // Bare generics remain callee-side noise only.
    assert!(engram_code::is_noise_symbol("new"));
    assert!(!engram_code::is_noise_symbol("run_gate"));
}

// ---------------------------------------------------------------------------
// Contract — desired behavior. RED until the noise filter is fixed.
// ---------------------------------------------------------------------------

/// Leading-underscore helpers are real logic (Python/Rust private-by-
/// convention); they must become entities so exact-name search can find them.
#[test]
fn contract_underscore_prefixed_python_helpers_become_entities() {
    let (names, _calls, _texts) = ingest_and_extract();

    for expected in ["run_gate", "check_review", "_tokens", "_check_review"] {
        assert!(
            names.contains(&expected.to_owned()),
            "{expected} must be a searchable entity, got {names:?}"
        );
    }
}

/// Call edges to underscore helpers must resolve too — dropping the helper
/// entity also erases the caller→helper edge from the graph (blast radius
/// beyond search: dependency paths / blast-radius queries go blind on
/// private helpers).
#[test]
fn contract_calls_to_underscore_helpers_resolve() {
    let (_names, calls, _texts) = ingest_and_extract();

    assert!(
        calls.iter().any(|(s, o)| s == "run_gate" && o == "_tokens"),
        "run_gate -> _tokens call edge must exist, got {calls:?}"
    );
    assert!(
        calls
            .iter()
            .any(|(s, o)| s == "check_review" && o == "_check_review"),
        "check_review -> _check_review call edge must exist, got {calls:?}"
    );
}
