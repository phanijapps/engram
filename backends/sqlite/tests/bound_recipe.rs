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

/// Host test policy (review r6 finding 1): the recipes' own explicit host
/// policy type over the fixture scopes — the same seam
/// (`CodeBindingCoordinator::with_authorizer` + `ScopeSetAuthorizer`) a real
/// host uses to replace the recipes' deny-by-default `RecipeDefaultAuthorizer`.
fn host_policy() -> std::sync::Arc<engram_integration::code_binding::ScopeSetAuthorizer> {
    std::sync::Arc::new(engram_integration::code_binding::ScopeSetAuthorizer::new(
        vec![scope(), foreign_scope()],
    ))
}

mod task_019 {
    use super::*;

    /// persisted_binding: a binding made through the OPENED recipe (with the
    /// host's EXPLICIT authorizer injected — the recipes deny by default,
    /// review r6 finding 1) survives a recipe re-open (a new
    /// process-equivalent handle over the same storage), and ordinary scope
    /// isolation holds — the foreign workspace has no binding
    /// (BindingRequired), never a cross-scope read.
    #[test]
    fn persisted_binding() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(dir.path().join("storage")).expect("mkdir storage");
        {
            let provider = engram_backend_sqlite::open(&config(dir.path())).expect("open recipe");
            let coordinator = provider.code_bindings().expect("binding handle");
            // Deny-by-default FIRST (review r6 finding 1): the recipe-built
            // coordinator denies even the fixture scope until policy is
            // injected.
            let denied = block_on(coordinator.bind(&actor(), &scope(), &repo(), "refs/heads/main"));
            assert!(
                matches!(&denied, Err(CoreError::PolicyDenied { reason }) if reason.contains("CodeAuthorizationRequired")),
                "recipe default must deny bind with the typed envelope, got {denied:?}"
            );
            let authorized = coordinator.with_authorizer(host_policy());
            let bound = block_on(authorized.bind(&actor(), &scope(), &repo(), "refs/heads/main"))
                .expect("bind through injected host policy");
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

        // Ordinary scope isolation: the foreign workspace never sees it
        // (read through the injected-policy coordinator so the denial under
        // test is the BINDING, not the policy).
        let authorized = coordinator.with_authorizer(host_policy());
        let foreign = block_on(authorized.status(&actor(), &foreign_scope(), &repo(), None));
        assert!(
            matches!(foreign, Err(CoreError::InvalidRequest { ref reason }) if reason == "BindingRequired"),
            "foreign scope must deny with BindingRequired, got {foreign:?}"
        );
    }

    /// deny_by_default_authorization (review r6 finding 1): BOTH authority
    /// actions through the recipe-constructed coordinator — bind, register,
    /// and inspect — deny with the SAME typed `CodeAuthorizationRequired`
    /// envelope until a host injects an explicit authorizer; the denial is
    /// not a binding/conflict artifact (the scope is fully unbound).
    #[test]
    fn deny_by_default_authorization() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(dir.path().join("storage")).expect("mkdir storage");
        let provider = engram_backend_sqlite::open(&config(dir.path())).expect("open recipe");
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
        // Nothing leaked through the denials: the durable cell stays empty.
        assert!(
            block_on(coordinator.store().get_binding(&scope(), &repo()))
                .expect("durable read")
                .is_none(),
            "a denied bind must not persist a binding"
        );
    }

    /// authorized_registration (review r6 finding 1): with an EXPLICIT host
    /// authorizer injected, registration succeeds through the SAME shared
    /// coordinator seam (authorize-before-store) and the locator is durable;
    /// the same request through the DEFAULT (uninjected) coordinator stays
    /// denied — an unauthorized direct register never reaches storage.
    #[test]
    fn authorized_registration() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(dir.path().join("storage")).expect("mkdir storage");
        let provider = engram_backend_sqlite::open(&config(dir.path())).expect("open recipe");
        let coordinator = provider.code_bindings().expect("binding handle");
        let registration = engram_knowledge::RepositoryRegistration {
            repo_id: repo(),
            locator_id: dir.path().to_string_lossy().to_string(),
            acquisition: engram_knowledge::AcquisitionMode::LocalOnly,
        };
        // Unauthorized (default policy): denied, nothing stored.
        assert!(matches!(
            block_on(coordinator.register(&actor(), &scope(), registration.clone())),
            Err(CoreError::PolicyDenied { .. })
        ));
        assert!(
            block_on(coordinator.store().get_registration(&repo()))
                .expect("registration read")
                .is_none(),
            "a denied register must not persist a locator"
        );
        // Authorized (injected policy): the shared seam stores it durably.
        let authorized = coordinator.with_authorizer(host_policy());
        let stored = block_on(authorized.register(&actor(), &scope(), registration))
            .expect("authorized registration");
        assert_eq!(stored.repo_id, repo());
        let durable = block_on(coordinator.store().get_registration(&repo()))
            .expect("registration read")
            .expect("locator persisted through the authorized seam");
        assert_eq!(durable.locator_id, dir.path().to_string_lossy().to_string());
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

    /// retained_snapshot_reuse: the SQLite recipe creates distinct
    /// generations for a changed filter and another scope at the same commit,
    /// while revalidating retained bytes before every reuse and rejecting
    /// corruption instead of trusting a collision.
    #[test]
    fn retained_snapshot_reuse() {
        let dir = tempfile::tempdir().expect("tempdir");
        let repository = dir.path().join("repository");
        std::fs::create_dir_all(&repository).expect("mkdir repository");
        let git = |args: &[&str]| {
            let status = std::process::Command::new("git")
                .args(args)
                .current_dir(&repository)
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .env("GIT_CONFIG_SYSTEM", "/dev/null")
                .status()
                .expect("run git");
            assert!(status.success(), "git {args:?}");
        };
        git(&["init", "--initial-branch=main", "."]);
        git(&["config", "user.email", "recipe@example.test"]);
        git(&["config", "user.name", "recipe"]);
        std::fs::write(repository.join("lib.rs"), "pub fn retained_snapshot() {}\n")
            .expect("write fixture");
        git(&["add", "."]);
        git(&["commit", "--no-gpg-sign", "-m", "fixture"]);

        std::fs::create_dir_all(dir.path().join("storage")).expect("mkdir storage");
        let provider = engram_backend_sqlite::open(&config(dir.path())).expect("open recipe");
        let coordinator = provider
            .code_bindings()
            .expect("binding handle")
            .with_authorizer(host_policy());
        let lanes = provider.code_ingestion().expect("ingestion lanes");
        let registration = engram_knowledge::RepositoryRegistration {
            repo_id: repo(),
            locator_id: repository.to_string_lossy().to_string(),
            acquisition: engram_knowledge::AcquisitionMode::LocalOnly,
        };
        block_on(coordinator.register(&actor(), &scope(), registration)).expect("register");
        block_on(coordinator.bind(&actor(), &scope(), &repo(), "refs/heads/main"))
            .expect("bind primary scope");
        let first = block_on(coordinator.ingest_registered(
            &actor(),
            &scope(),
            &repo(),
            None,
            None,
            &lanes,
        ))
        .expect("first ingestion");

        let changed_filter = engram_ingest::ScanFilterConfig::default();
        let changed = block_on(coordinator.ingest_registered(
            &actor(),
            &scope(),
            &repo(),
            None,
            Some(&changed_filter),
            &lanes,
        ))
        .expect("changed filter reuses retained snapshot");
        assert_ne!(changed.generation_id, first.generation_id);
        assert_eq!(changed.snapshot_digest, first.snapshot_digest);

        block_on(coordinator.bind(&actor(), &foreign_scope(), &repo(), "refs/heads/main"))
            .expect("bind foreign scope");
        let cross_scope = block_on(coordinator.ingest_registered(
            &actor(),
            &foreign_scope(),
            &repo(),
            None,
            None,
            &lanes,
        ))
        .expect("cross-scope ingestion reuses retained snapshot");
        assert_ne!(cross_scope.generation_id, first.generation_id);
        assert_eq!(cross_scope.snapshot_digest, first.snapshot_digest);

        let retained_file = lanes.retained_root.join(&first.commit_oid).join("lib.rs");
        let mut permissions = std::fs::metadata(&retained_file)
            .expect("retained metadata")
            .permissions();
        permissions.set_readonly(false);
        std::fs::set_permissions(&retained_file, permissions).expect("make retained file writable");
        std::fs::write(&retained_file, "pub fn corrupted_snapshot() {}\n")
            .expect("corrupt retained snapshot");
        let another_filter = engram_ingest::ScanFilterConfig {
            concepts: engram_ingest::ConceptFilterConfig {
                min_name_length: Some(9),
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(
            block_on(coordinator.ingest_registered(
                &actor(),
                &scope(),
                &repo(),
                None,
                Some(&another_filter),
                &lanes,
            ))
            .is_err()
        );
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
