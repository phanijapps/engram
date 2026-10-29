//! TASK-019 focused proof (SQLite recipe): both backend recipes expose the
//! required NEUTRAL binding/graph/source/reset handles with honest capability
//! states, bindings persist across recipe re-opens, and optional absent lanes
//! report unavailable (REQ-009 / REQ-013 / REQ-014; plan step 10 matrix).
//!
//! This target drives the REAL recipe entry `engram_backend_sqlite::open`
//! over a REAL file-backed store — no port doubles. The pgvector twin lives
//! in `backends/pgvector/tests/bound_recipe.rs` with the same case names.

use std::path::Path;

use engram_domain::types::ScopeMappingStrategy;
use engram_domain::{Actor, ActorKind, Id, RepoId, Scope};
use engram_integration::{
    CapabilityPolicy, EmbeddingProviderConfig, EngramConfig, EngramProvider, MigrationMode,
};
use engram_knowledge::CodeBindingRepository;
use engram_runtime::CoreError;
use futures::executor::block_on;

fn config(dir: &Path) -> EngramConfig {
    EngramConfig::new(
        dir.join("storage"),
        dir,
        ScopeMappingStrategy::Strict,
        EmbeddingProviderConfig {
            provider_type: "none".to_string(),
            model: "test_model".to_string(),
            dimensions: 384,
            prompt_profile: "query".to_string(),
            normalization: None,
            ..Default::default()
        },
        MigrationMode::DryRun,
        CapabilityPolicy::FailClosed,
    )
}

fn actor() -> Actor {
    Actor {
        id: Id::from("task-019-sqlite"),
        kind: ActorKind::Agent,
        display_name: None,
        metadata: None,
    }
}

fn scope() -> Scope {
    Scope {
        tenant: "task-019".to_string(),
        subject: None,
        workspace: Some("recipe".to_string()),
        session: None,
        environment: None,
    }
}

fn foreign_scope() -> Scope {
    Scope {
        tenant: "task-019".to_string(),
        subject: None,
        workspace: Some("foreign".to_string()),
        session: None,
        environment: None,
    }
}

fn repo() -> RepoId {
    RepoId("repo-recipe".to_string())
}

mod task_019 {
    use super::*;

    /// persisted_binding: a binding made through the OPENED recipe survives a
    /// recipe re-open (a new process-equivalent handle over the same
    /// storage), and ordinary scope isolation holds — the foreign workspace
    /// has no binding (BindingRequired), never a cross-scope read.
    #[test]
    fn persisted_binding() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(dir.path().join("storage")).expect("mkdir storage");
        {
            let provider = engram_backend_sqlite::open(&config(dir.path())).expect("open recipe");
            let coordinator = provider.code_bindings().expect("binding handle");
            let bound = block_on(coordinator.bind(&actor(), &scope(), &repo(), "refs/heads/main"))
                .expect("bind through recipe");
            assert_eq!(bound.branch_ref, "refs/heads/main");
            assert_eq!(bound.active_generation, None);
        }
        // A fresh recipe handle over the SAME storage (a re-opened process).
        let reopened = engram_backend_sqlite::open(&config(dir.path())).expect("reopen recipe");
        let coordinator = reopened
            .code_bindings()
            .expect("binding handle after reopen");
        let binding = block_on(coordinator.store().get_binding(&scope(), &repo()))
            .expect("durable read")
            .expect("binding persisted across recipe re-open");
        assert_eq!(binding.branch_ref, "refs/heads/main");
        assert_eq!(binding.version, 1);

        // Ordinary scope isolation: the foreign workspace never sees it.
        let foreign = block_on(coordinator.status(&actor(), &foreign_scope(), &repo(), None));
        assert!(
            matches!(foreign, Err(CoreError::InvalidRequest { ref reason }) if reason == "BindingRequired"),
            "foreign scope must deny with BindingRequired, got {foreign:?}"
        );
    }

    /// required_handles: the recipe exposes every REQUIRED neutral handle —
    /// binding coordinator, retained exact-source reader, graph, knowledge
    /// query — and the reset seam rides the SAME coordinator store (the
    /// cell's typed answer, never a provider-level "not configured"). All
    /// code capability states are Supported.
    #[test]
    fn required_handles() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(dir.path().join("storage")).expect("mkdir storage");
        let provider = engram_backend_sqlite::open(&config(dir.path())).expect("open recipe");

        // Required neutral handles.
        let coordinator = provider.code_bindings().expect("code binding handle");
        let _source = provider
            .retained_code_source()
            .expect("retained source handle");
        provider.require_graph().expect("graph handle");
        provider
            .require_knowledge_query()
            .expect("knowledge query handle");

        // Reset seam injection (TASK-023 pending): the reset PORT is reachable
        // through the coordinator's durable store — its answer is the cell's
        // typed outcome, never the provider-level "not configured" denial.
        let reset = block_on(coordinator.store().preview_reset(&scope(), &repo()));
        assert!(
            matches!(&reset, Err(CoreError::CapabilityUnsupported { capability, .. }) if capability == "code-reset"),
            "reset seam must be injected with the cell's typed outcome, got {reset:?}"
        );

        // Capability states: binding/source/reset supported on this recipe.
        let report = provider.capabilities();
        assert!(
            report.code_binding.is_supported(),
            "code_binding capability"
        );
        assert!(report.code_source.is_supported(), "code_source capability");
        assert!(report.code_reset.is_supported(), "code_reset capability");
    }

    /// unavailable_optional_capabilities: optional lanes the recipe did NOT
    /// configure report UNAVAILABLE — vectors disabled at runtime stays
    /// `Unsupported`/`FeatureDisabled`, and the hybrid-composition lane is
    /// honestly unavailable — never silently `Supported`.
    #[test]
    fn unavailable_optional_capabilities() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(dir.path().join("storage")).expect("mkdir storage");
        let mut cfg = config(dir.path());
        cfg.enable_vector = false;
        let provider = engram_backend_sqlite::open(&cfg).expect("open recipe");
        let report = provider.capabilities();
        assert!(
            !report.vectors.is_supported(),
            "disabled vector lane must report unavailable, got {:?}",
            report.vectors
        );
        assert!(
            matches!(
                report.vectors,
                engram_domain::CapabilityState::Unsupported {
                    reason: engram_domain::CapabilityReason::FeatureDisabled
                }
            ),
            "runtime-disabled vector lane reports FeatureDisabled, got {:?}",
            report.vectors
        );
        assert!(
            !report.hybrid_search.is_supported(),
            "absent optional hybrid lane must report unavailable, got {:?}",
            report.hybrid_search
        );
        // Required code capabilities are NOT optional: still supported.
        assert!(report.code_binding.is_supported());
        assert!(report.code_source.is_supported());
    }
}
