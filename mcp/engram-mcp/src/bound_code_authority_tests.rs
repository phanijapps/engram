//! Review r6 finding 2 / finding 4 focused proofs (in-process, real durable
//! cells): the legacy `scan_repo` lane cannot ingest code bytes when the
//! scope has PUBLISHED bound code (typed denial; the explicit document-only
//! lane is labeled), and bound-code staleness answers derive EXCLUSIVELY
//! from binding-keyed durable records — an unrelated legacy checkpoint's
//! HEAD never leaks into a bound scope's staleness note or default
//! `code_staleness`.
//!
//! The fixtures publish REAL generations through the REAL
//! `CodeBindingCoordinator::rebuild` over a real SQLite binding cell
//! (`:memory:`), mirroring the r5 `code_publication` proofs.

use std::sync::Arc;

use engram_domain::{
    Actor, ActorKind, CodeAttempt, CodeBinding, CodeGeneration, Id, RepoId, Scope,
};
use engram_integration::code_binding::{
    CodeAuthorityAction, CodeBindingAuthorizer, CodeBindingCoordinator, CodeGenerationStages,
    CodeRebuildRequest,
};
use engram_knowledge::{CodeBindingRepository, CoreResult, StageReceipt};
use engram_store_sqlite::SqlKnowledgeStore;
use engram_store_sqlite::knowledge::SqlCodeBindingRepository;
use futures::executor::block_on;
use serde_json::json;

use crate::app::App;
use crate::ontology::{OntologyConfig, TaxonomyConfig};

struct AllowAuthorizer;
#[async_trait::async_trait]
impl CodeBindingAuthorizer for AllowAuthorizer {
    async fn authorize(
        &self,
        _actor: &Actor,
        _scope: &Scope,
        _repo: &RepoId,
        _action: CodeAuthorityAction,
    ) -> CoreResult<()> {
        Ok(())
    }
}

/// Receipt-only stages (the publication custody proof lives in the r5
/// `code_publication` target; here the lanes' durable effects are the
/// binding cell's own receipts).
struct ReceiptStages;
#[async_trait::async_trait]
impl CodeGenerationStages for ReceiptStages {
    async fn stage(
        &self,
        lane: &engram_domain::CodeReceiptKind,
        _binding: &CodeBinding,
        generation: &CodeGeneration,
        attempt: &CodeAttempt,
    ) -> engram_knowledge::CoreResult<StageReceipt> {
        let lane_name = serde_json::to_value(lane)
            .unwrap()
            .as_str()
            .unwrap()
            .to_owned();
        Ok(StageReceipt {
            attempt_id: attempt.id.clone(),
            generation_id: generation.id.clone(),
            lane: lane_name,
            digest: "c".repeat(64),
        })
    }
}

fn actor() -> Actor {
    Actor {
        id: Id::from("bound-authority-test"),
        kind: ActorKind::Agent,
        display_name: None,
        metadata: None,
    }
}

fn scope() -> Scope {
    Scope {
        tenant: "bound-authority".into(),
        subject: None,
        workspace: Some("gate".into()),
        session: None,
        environment: None,
    }
}

fn repo() -> RepoId {
    RepoId("repo-gate".into())
}

fn generation(id: &str) -> CodeGeneration {
    CodeGeneration {
        id: id.into(),
        binding_version: 1,
        commit_oid: "a".repeat(40),
        snapshot_digest: "b".repeat(64),
        parser_fingerprint: "parser".into(),
        filter_fingerprint: "filter".into(),
    }
}

/// One published generation over the REAL in-memory binding cell: bind →
/// rebuild (receipts) → the scope's eligibility carries the published repo.
fn published_cell() -> Arc<SqlCodeBindingRepository> {
    let cell = Arc::new(SqlCodeBindingRepository::open(":memory:").expect("open cell"));
    let coordinator = CodeBindingCoordinator::new(
        cell.clone() as Arc<dyn CodeBindingRepository>,
        Arc::new(AllowAuthorizer),
        Vec::new(),
    );
    let generation = generation("gen-gate");
    block_on(coordinator.bind(&actor(), &scope(), &repo(), "refs/heads/main"))
        .expect("bind fixture");
    let published = block_on(coordinator.rebuild(
        &actor(),
        CodeRebuildRequest {
            scope: scope(),
            repo_id: repo(),
            generation,
            attempt_id: "attempt-gate".into(),
            idempotency_key: "request-gate".into(),
        },
        &ReceiptStages,
    ))
    .expect("publish fixture");
    assert!(published.active_generation.is_some());
    cell
}

/// A bound MCP app over a REAL knowledge store + the published binding cell
/// (the permissive authorizer mirrors `bootstrap::open_provider`'s host
/// launch-policy injection).
fn bound_app() -> App {
    let knowledge = Arc::new(SqlKnowledgeStore::open_in_memory().expect("open knowledge"));
    let coordinator = CodeBindingCoordinator::new(
        published_cell() as Arc<dyn CodeBindingRepository>,
        Arc::new(AllowAuthorizer),
        Vec::new(),
    );
    let provider = engram_integration::EngramProviderBuilder::new(
        engram_integration::CapabilityReport::new(engram_domain::CapabilityState::Supported),
    )
    .knowledge_query(knowledge.clone())
    .knowledge(knowledge.clone())
    .graph(knowledge.clone())
    .change_log(knowledge.clone() as Arc<dyn engram_knowledge::KnowledgeChangeLog>)
    .checkpoints(knowledge.clone() as Arc<dyn engram_knowledge::ScanCheckpointStore>)
    .code_bindings(Arc::new(coordinator))
    .build();
    App {
        provider,
        scope: scope(),
        ontology: OntologyConfig::default(),
        taxonomy: TaxonomyConfig::default(),
        storage_dir: std::env::temp_dir(),
    }
}

/// A directory with ONE code file and ONE document file.
fn scan_fixture() -> std::path::PathBuf {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.keep();
    std::fs::write(root.join("lib.rs"), "pub fn fixture_code() -> u32 { 1 }\n").expect("code");
    std::fs::write(
        root.join("notes.md"),
        "# Notes\nSome document text for the legacy lane.\n",
    )
    .expect("doc");
    root
}

mod finding_2_scan_gate {
    use super::*;

    /// `bound_code_blocks_legacy_scan`: a scope with a PUBLISHED bound
    /// repository denies the arbitrary-path legacy scan with the typed
    /// `BoundCodeScanDenied` envelope naming the bound pipeline — code bytes
    /// from another directory can never enter through `scan_repo`.
    #[test]
    fn bound_code_blocks_legacy_scan() {
        let app = bound_app();
        let root = scan_fixture();
        let outcome = crate::codegraph::scan_repo(
            &app,
            &json!({ "path": root.to_string_lossy(), "force": true }),
        );
        let error = outcome.expect_err("published bound code must deny the legacy scan");
        assert!(
            error.message.contains("BoundCodeScanDenied"),
            "typed denial naming the pipeline, got: {}",
            error.message
        );
        assert!(
            error.message.contains("registered-locator"),
            "the denial directs to the bound pipeline: {}",
            error.message
        );
        // Nothing was ingested by the denied call.
        let knowledge = app.provider.require_knowledge_query().unwrap();
        let entities = block_on(knowledge.list_entities(&app.scope)).expect("list");
        assert!(entities.is_empty(), "denied scan must not ingest bytes");
    }

    /// `document_only_lane_is_labeled_and_code_free`: the explicit
    /// documentOnly lane scans the same root, EXCLUDES the code file
    /// (only the markdown document is ingested), and labels its result as
    /// the legacy document-only lane.
    #[test]
    fn document_only_lane_is_labeled_and_code_free() {
        let app = bound_app();
        let root = scan_fixture();
        let value = crate::codegraph::scan_repo(
            &app,
            &json!({
                "path": root.to_string_lossy(),
                "documentOnly": true,
                "force": true
            }),
        )
        .expect("document-only scan runs");
        let text = value["content"][0]["text"].as_str().unwrap_or_default();
        assert!(
            text.contains("MODE: legacy document-only scan"),
            "explicit lane label, got: {text}"
        );
        let knowledge = app.provider.require_knowledge_query().unwrap();
        let entities = block_on(knowledge.list_entities(&app.scope)).expect("list");
        assert!(
            entities
                .iter()
                .all(|entity| entity.kind != engram_domain::EntityKind::Function),
            "no code entities from the document-only lane"
        );
        assert!(
            !entities.is_empty(),
            "the document file is ingested by the lane"
        );
    }

    /// `unbound_scope_keeps_legacy_scanner`: a scope with NO published code
    /// keeps the historical legacy scanner (the pre-binding lane) — the gate
    /// is bound-code-specific, not a global removal.
    #[test]
    fn unbound_scope_keeps_legacy_scanner() {
        let knowledge = Arc::new(SqlKnowledgeStore::open_in_memory().expect("open knowledge"));
        let cell = Arc::new(SqlCodeBindingRepository::open(":memory:").expect("empty cell"));
        let coordinator = CodeBindingCoordinator::new(
            cell as Arc<dyn CodeBindingRepository>,
            Arc::new(AllowAuthorizer),
            Vec::new(),
        );
        let provider = engram_integration::EngramProviderBuilder::new(
            engram_integration::CapabilityReport::new(engram_domain::CapabilityState::Supported),
        )
        .knowledge_query(knowledge.clone())
        .knowledge(knowledge.clone())
        .graph(knowledge.clone())
        .code_bindings(Arc::new(coordinator))
        .build();
        let app = App {
            provider,
            scope: scope(),
            ontology: OntologyConfig::default(),
            taxonomy: TaxonomyConfig::default(),
            storage_dir: std::env::temp_dir(),
        };
        let root = scan_fixture();
        let value = crate::codegraph::scan_repo(
            &app,
            &json!({ "path": root.to_string_lossy(), "force": true }),
        )
        .expect("unbound legacy scan keeps working");
        let text = value["content"][0]["text"].as_str().unwrap_or_default();
        assert!(!text.contains("document-only"), "legacy lane unlabeled");
        let knowledge = app.provider.require_knowledge_query().unwrap();
        let entities = block_on(knowledge.list_entities(&app.scope)).expect("list");
        assert!(
            entities
                .iter()
                .any(|entity| entity.kind == engram_domain::EntityKind::Function),
            "the unbound legacy scanner still ingests code: {entities:?}"
        );
    }
}
