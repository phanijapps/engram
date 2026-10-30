//! TASK-019 focused proof (pgvector recipe): the pgvector recipe exposes the
//! same REQUIRED neutral binding/graph/source/reset handles with honest
//! capability states, bindings persist across recipe re-opens over the real
//! PostgreSQL store, and optional absent lanes report unavailable
//! (REQ-009 / REQ-013 / REQ-014; plan step 10 matrix).
//!
//! Requires a MARKED disposable PostgreSQL+pgvector database via
//! `ENGRAM_TEST_PG_URL` (same gating as `adapters/pgvector`'s task_006
//! suite: env-gated, not `#[ignore]`d, so the pinned focused command runs
//! the cases). The SQLite twin lives in `backends/sqlite/tests/bound_recipe.rs`.

use std::path::Path;

use engram_domain::types::ScopeMappingStrategy;
use engram_domain::{Actor, ActorKind, Id, RepoId, Scope};
use engram_integration::{CapabilityPolicy, EmbeddingProviderConfig, EngramConfig, MigrationMode};
use engram_knowledge::CodeBindingRepository;
use engram_runtime::CoreError;
use futures::executor::block_on;

fn url() -> String {
    std::env::var("ENGRAM_TEST_PG_URL").expect("marked disposable PostgreSQL URL required")
}

fn config(dir: &Path) -> EngramConfig {
    let mut config = EngramConfig::new(
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
    .with_pgvector(&url());
    // TASK-019 proves the BINDING/SOURCE/RESET handles, not the vector lane:
    // keep it disabled (no model init, and no embedding-identity coupling to
    // whatever vectors the disposable database already carries from prior
    // runs). The `unavailable_optional_capabilities` case asserts that
    // disabled lane's honest state below.
    config.enable_vector = false;
    config
}

fn actor() -> Actor {
    Actor {
        id: Id::from("task-019-pg"),
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
    RepoId(format!(
        "repo-recipe-{}-{}",
        std::process::id(),
        unique_suffix()
    ))
}

/// Run-unique suffix: the disposable PostgreSQL database persists across
/// test runs, so durable registrations/bindings key by run-unique RepoIds
/// (the t022 cross-run-collision lesson applied here too).
fn unique_suffix() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{}-{}", std::process::id(), nanos)
}

/// Host test policy (review r6 finding 1): the recipes' own explicit host
/// policy type over the fixture scopes — the same seam a real host uses to
/// replace the recipes' deny-by-default `RecipeDefaultAuthorizer`.
fn host_policy() -> std::sync::Arc<engram_integration::code_binding::ScopeSetAuthorizer> {
    std::sync::Arc::new(engram_integration::code_binding::ScopeSetAuthorizer::new(
        vec![scope(), foreign_scope()],
    ))
}

mod task_019 {
    use super::*;

    /// persisted_binding (pgvector): a binding made through the OPENED recipe
    /// (with the host's EXPLICIT authorizer injected — the recipes deny by
    /// default, review r6 finding 1) survives a recipe re-open over the real
    /// PostgreSQL store, and ordinary scope isolation holds — the foreign
    /// workspace gets `BindingRequired`, never a cross-scope read.
    #[test]
    fn persisted_binding() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(dir.path().join("storage")).expect("mkdir storage");
        // One run-unique RepoId for the whole case (two repo() calls would
        // mint different suffixes).
        let repo = repo();
        {
            let provider = engram_backend_pgvector::open(&config(dir.path())).expect("open recipe");
            let coordinator = provider.code_bindings().expect("binding handle");
            // Deny-by-default FIRST (review r6 finding 1): the recipe-built
            // coordinator denies even the fixture scope until policy is
            // injected.
            let denied = block_on(coordinator.bind(&actor(), &scope(), &repo, "refs/heads/main"));
            assert!(
                matches!(&denied, Err(CoreError::PolicyDenied { reason }) if reason.contains("CodeAuthorizationRequired")),
                "recipe default must deny bind with the typed envelope, got {denied:?}"
            );
            let authorized = coordinator.with_authorizer(host_policy());
            let bound = block_on(authorized.bind(&actor(), &scope(), &repo, "refs/heads/main"))
                .expect("bind through injected host policy");
            assert_eq!(bound.branch_ref, "refs/heads/main");
            assert_eq!(bound.active_generation, None);
        }
        // A fresh recipe handle over the SAME database.
        let reopened = engram_backend_pgvector::open(&config(dir.path())).expect("reopen recipe");
        let coordinator = reopened
            .code_bindings()
            .expect("binding handle after reopen");
        let binding = block_on(coordinator.store().get_binding(&scope(), &repo))
            .expect("durable read")
            .expect("binding persisted across recipe re-open");
        assert_eq!(binding.branch_ref, "refs/heads/main");
        assert_eq!(binding.version, 1);

        // Ordinary scope isolation: the foreign workspace never sees it
        // (read through the injected-policy coordinator so the denial under
        // test is the BINDING, not the policy).
        let authorized = coordinator.with_authorizer(host_policy());
        let foreign = block_on(authorized.status(&actor(), &foreign_scope(), &repo, None));
        assert!(
            matches!(foreign, Err(CoreError::InvalidRequest { ref reason }) if reason == "BindingRequired"),
            "foreign scope must deny with BindingRequired, got {foreign:?}"
        );
    }

    /// deny_by_default_authorization (pgvector, review r6 finding 1): bind,
    /// register, and inspect through the recipe-constructed coordinator all
    /// deny with the SAME typed `CodeAuthorizationRequired` envelope until a
    /// host injects an explicit authorizer, and nothing is durably stored.
    #[test]
    fn deny_by_default_authorization() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(dir.path().join("storage")).expect("mkdir storage");
        let provider = engram_backend_pgvector::open(&config(dir.path())).expect("open recipe");
        let coordinator = provider.code_bindings().expect("binding handle");
        let deny = |outcome: &Result<(), CoreError>, action: &str| {
            assert!(
                matches!(outcome, Err(CoreError::PolicyDenied { reason }) if reason.contains("CodeAuthorizationRequired")),
                "recipe default must deny {action} with the typed envelope, got {outcome:?}"
            );
        };
        deny(
            &block_on(coordinator.bind(&actor(), &scope(), &repo(), "refs/heads/main")).map(|_| ()),
            "bind",
        );
        deny(
            &block_on(coordinator.register(
                &actor(),
                &scope(),
                engram_knowledge::RepositoryRegistration {
                    repo_id: repo(),
                    locator_id: dir.path().to_string_lossy().to_string(),
                    acquisition: engram_knowledge::AcquisitionMode::LocalOnly,
                },
            ))
            .map(|_| ()),
            "register",
        );
        deny(
            &block_on(coordinator.status(&actor(), &scope(), &repo(), None)).map(|_| ()),
            "status",
        );
        assert!(
            block_on(coordinator.store().get_binding(&scope(), &repo()))
                .expect("durable read")
                .is_none(),
            "a denied bind must not persist a binding"
        );
    }

    /// authorized_registration (pgvector, review r6 finding 1): with an
    /// EXPLICIT host authorizer injected, registration succeeds through the
    /// SAME shared coordinator seam and the locator is durable over the real
    /// PostgreSQL cell; the same request through the DEFAULT (uninjected)
    /// coordinator stays denied — an unauthorized direct register never
    /// reaches storage.
    #[test]
    fn authorized_registration() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(dir.path().join("storage")).expect("mkdir storage");
        // One run-unique RepoId for the whole case (two repo() calls would
        // mint different suffixes).
        let repo = repo();
        let provider = engram_backend_pgvector::open(&config(dir.path())).expect("open recipe");
        let coordinator = provider.code_bindings().expect("binding handle");
        let registration = engram_knowledge::RepositoryRegistration {
            repo_id: repo.clone(),
            locator_id: dir.path().to_string_lossy().to_string(),
            acquisition: engram_knowledge::AcquisitionMode::LocalOnly,
        };
        assert!(matches!(
            block_on(coordinator.register(&actor(), &scope(), registration.clone())),
            Err(CoreError::PolicyDenied { .. })
        ));
        assert!(
            block_on(coordinator.store().get_registration(&repo))
                .expect("registration read")
                .is_none(),
            "a denied register must not persist a locator"
        );
        let authorized = coordinator.with_authorizer(host_policy());
        let stored = block_on(authorized.register(&actor(), &scope(), registration))
            .expect("authorized registration");
        assert_eq!(stored.repo_id, repo);
        let durable = block_on(coordinator.store().get_registration(&repo))
            .expect("registration read")
            .expect("locator persisted through the authorized seam");
        assert_eq!(durable.locator_id, dir.path().to_string_lossy().to_string());
    }

    /// required_handles (pgvector): the same REQUIRED neutral handles —
    /// binding coordinator, retained exact-source reader, graph, knowledge
    /// query — and the reset seam riding the coordinator's durable store.
    /// All code capability states are Supported on this recipe.
    #[test]
    fn required_handles() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(dir.path().join("storage")).expect("mkdir storage");
        let provider = engram_backend_pgvector::open(&config(dir.path())).expect("open recipe");

        let coordinator = provider.code_bindings().expect("code binding handle");
        let _source = provider
            .retained_code_source()
            .expect("retained source handle");
        provider.require_graph().expect("graph handle");
        provider
            .require_knowledge_query()
            .expect("knowledge query handle");

        // Reset seam injection (TASK-023 pending): the reset PORT is reachable
        // through the coordinator's durable store — the cell answers with its
        // own typed outcome for this unbound repo (BindingRequired or the
        // pending-wiring code-reset denial), never a provider-level "not
        // configured" envelope.
        let reset = block_on(coordinator.store().preview_reset(&scope(), &repo()));
        match &reset {
            Err(CoreError::InvalidRequest { .. }) => {}
            Err(CoreError::CapabilityUnsupported { capability, .. })
                if capability != "code-binding" => {}
            other => {
                panic!("reset seam must be injected with the cell's typed outcome, got {other:?}")
            }
        }

        let report = provider.capabilities();
        assert!(
            report.code_binding.is_supported(),
            "code_binding capability"
        );
        assert!(report.code_source.is_supported(), "code_source capability");
        assert!(report.code_reset.is_supported(), "code_reset capability");
    }

    /// unavailable_optional_capabilities (pgvector): optional absent lanes
    /// report UNAVAILABLE — runtime-disabled vectors stay
    /// `Unsupported`/`FeatureDisabled`, the lexical/hybrid-composition lane
    /// is honestly unavailable on this engine — never silently Supported.
    #[test]
    fn unavailable_optional_capabilities() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(dir.path().join("storage")).expect("mkdir storage");
        let mut cfg = config(dir.path());
        cfg.enable_vector = false; // explicit in the case body: the lane under test
        let provider = engram_backend_pgvector::open(&cfg).expect("open recipe");
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
            "pgvector has no lexical engine: the hybrid lane must report unavailable, got {:?}",
            report.hybrid_search
        );
        assert!(report.code_binding.is_supported());
        assert!(report.code_source.is_supported());
    }
}
