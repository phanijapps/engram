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
use std::process::Command;
use std::sync::Arc;

use engram_domain::types::ScopeMappingStrategy;
use engram_domain::{
    Actor, ActorKind, AllowedUse, CodeResetJournal, Id, KnowledgeChunk, KnowledgeChunkKind,
    KnowledgeSource, Policy, Provenance, RepoId, Requester, Retention, RetrievalRequest, Scope,
    Sensitivity, SourceDocument, SourceDocumentKind, SourceKind, Visibility,
};
use engram_integration::{CapabilityPolicy, EmbeddingProviderConfig, EngramConfig, MigrationMode};
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

/// Builds one committed local repository and drives the opened Pg recipe through
/// register → bind → registered acquisition → staged rebuild/publish.
fn published_fixture(
    operation: &str,
    token: &str,
) -> (
    tempfile::TempDir,
    engram_integration::EngramProvider,
    Scope,
    RepoId,
) {
    let dir = tempfile::tempdir().expect("tempdir");
    let checkout = dir.path().join("checkout");
    std::fs::create_dir_all(&checkout).expect("mkdir checkout");
    std::fs::write(
        checkout.join("lib.rs"),
        format!("pub fn {token}() -> u32 {{ 1 }}\n"),
    )
    .expect("write fixture");
    for args in [
        ["init", "--initial-branch=main", "."].as_slice(),
        ["config", "user.email", "task019@engram.test"].as_slice(),
        ["config", "user.name", "task019"].as_slice(),
        ["add", "."].as_slice(),
        ["commit", "--no-gpg-sign", "-m", "fixture"].as_slice(),
    ] {
        assert!(
            Command::new("git")
                .args(args)
                .current_dir(&checkout)
                .status()
                .expect("git executable")
                .success(),
            "git fixture command failed"
        );
    }
    let suffix = unique_suffix();
    let scope = Scope {
        tenant: "task-019-pg-operation".to_owned(),
        subject: None,
        workspace: Some(format!("{operation}-{suffix}")),
        session: None,
        environment: None,
    };
    let repo = RepoId(format!("repo-{operation}-{suffix}"));
    let mut cfg = config(dir.path());
    cfg.migration_mode = MigrationMode::Apply;
    let provider = engram_backend_pgvector::open(&cfg)
        .expect("open real Pg recipe")
        .with_code_authorizer(Arc::new(
            engram_integration::code_binding::ScopeSetAuthorizer::new(vec![scope.clone()]),
        ))
        .expect("inject host policy");
    let coordinator = provider.code_bindings().expect("binding handle");
    block_on(coordinator.register(
        &actor(),
        &scope,
        engram_knowledge::RepositoryRegistration {
            repo_id: repo.clone(),
            locator_id: checkout.to_string_lossy().to_string(),
            acquisition: engram_knowledge::AcquisitionMode::LocalOnly,
        },
    ))
    .expect("register");
    block_on(coordinator.bind(&actor(), &scope, &repo, "refs/heads/main")).expect("bind");
    let ingested = block_on(coordinator.ingest_registered(
        &actor(),
        &scope,
        &repo,
        None,
        None,
        provider.code_ingestion().expect("ingestion lanes"),
    ))
    .expect("rebuild and publish");
    assert!(
        !ingested.idempotent && ingested.documents > 0 && ingested.entities > 0,
        "published fixture must contain staged code content"
    );
    (dir, provider, scope, repo)
}

fn requester() -> Requester {
    Requester {
        actor: actor(),
        roles: Vec::new(),
        permissions: Vec::new(),
        on_behalf_of: None,
    }
}

fn timestamp() -> engram_domain::Timestamp {
    "2026-10-30T00:00:00Z"
        .parse()
        .expect("RFC3339 fixture timestamp")
}

fn seed_ordinary_record(
    lanes: &engram_integration::code_binding::CodeIngestionLanes,
    scope: &Scope,
    id: &str,
    token: &str,
) {
    let provenance = Provenance {
        source: "bound-recipe-serving".into(),
        actor: actor(),
        observed_at: timestamp(),
        evidence: Vec::new(),
        derivations: Vec::new(),
        confidence: Some(1.0),
        method: None,
    };
    let policy = Policy {
        visibility: Visibility::Workspace,
        retention: Retention::Durable,
        sensitivity: Some(Sensitivity::Medium),
        allowed_uses: vec![AllowedUse::Retrieval],
        expires_at: None,
        delete_mode: None,
    };
    let source = KnowledgeSource {
        id: Id::from(format!("recipe-serving-ordinary-source-{id}")),
        kind: SourceKind::Filesystem,
        scope: scope.clone(),
        name: "recipe-serving-ordinary".into(),
        uri: None,
        version: None,
        repository_key: None,
        git_ref: None,
        policy: policy.clone(),
        provenance: provenance.clone(),
        created_at: timestamp(),
        updated_at: None,
        metadata: None,
    };
    let document = SourceDocument {
        id: Id::from(format!("recipe-serving-ordinary-document-{id}")),
        source_id: source.id.clone(),
        kind: SourceDocumentKind::Markdown,
        uri: None,
        path: Some("ordinary.md".into()),
        title: None,
        mime_type: None,
        language: None,
        version: None,
        content_hash: format!("recipe-serving-ordinary-content-{id}"),
        provenance: provenance.clone(),
        policy: policy.clone(),
        created_at: timestamp(),
        updated_at: None,
        metadata: None,
    };
    let chunk = KnowledgeChunk {
        id: Id::from(format!("recipe-serving-ordinary-chunk-{id}")),
        document_id: document.id.clone(),
        source_id: source.id.clone(),
        kind: KnowledgeChunkKind::Paragraph,
        text: token.into(),
        summary: None,
        location: None,
        entities: Vec::new(),
        concepts: Vec::new(),
        embedding_refs: Vec::new(),
        content_hash: format!("recipe-serving-ordinary-chunk-content-{id}"),
        provenance,
        policy,
        created_at: timestamp(),
        updated_at: None,
        metadata: None,
    };
    block_on(lanes.knowledge.put_source(source)).expect("seed ordinary source through recipe lane");
    block_on(lanes.knowledge.put_document(document))
        .expect("seed ordinary document through recipe lane");
    block_on(lanes.knowledge.put_chunk(chunk)).expect("seed ordinary chunk through recipe lane");
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

    /// real_pg_scan_publish_content: the real Pg recipe registers, binds, and
    /// publishes a committed fixture, then proves scan/rebuild output carries
    /// expected code symbols while a separate run-unique repository cannot
    /// appear in this scope.
    #[test]
    #[ignore = "requires sourced ENGRAM_TEST_PG_URL"]
    fn real_pg_scan_publish_content() {
        let (_dir, provider, scope, repo) = published_fixture("scan", "pg_scan_expected_symbol");
        let chunks = block_on(
            provider
                .require_knowledge_query()
                .expect("query")
                .list_chunks(&scope),
        )
        .expect("list published chunks");
        let text = format!("{chunks:?}");
        assert!(
            text.contains("pg_scan_expected_symbol"),
            "published Pg scan must retain expected symbol: {text}"
        );
        let foreign = Scope {
            workspace: Some(format!("foreign-{}", unique_suffix())),
            ..scope.clone()
        };
        let foreign_chunks = block_on(
            provider
                .require_knowledge_query()
                .expect("query")
                .list_chunks(&foreign),
        )
        .expect("list foreign scope");
        assert!(
            foreign_chunks.is_empty(),
            "foreign scope must exclude this published content"
        );
        println!("real_pg_scan_publish_content {}", repo.0);
    }

    /// real_pg_retrieval_eligible_content: recall over the real Pg recipe
    /// returns the published bound token and excludes a token that exists only
    /// in another run-unique bound scope.
    #[test]
    #[ignore = "requires sourced ENGRAM_TEST_PG_URL"]
    fn real_pg_retrieval_eligible_content() {
        let (_dir, provider, scope, _repo) =
            published_fixture("retrieval", "pg_retrieval_expected_token");
        let (_foreign_dir, _foreign_provider, _foreign_scope, _foreign_repo) =
            published_fixture("retrieval-control", "pg_retrieval_ineligible_token");
        let payload = block_on(provider.require_recall().expect("recall").recall(
            RetrievalRequest {
                query: "pg_retrieval_expected_token".to_owned(),
                scope,
                requester: requester(),
                modes: vec![engram_domain::RetrievalMode::Graph],
                filters: None,
                cues: Vec::new(),
                limit: None,
                budget: None,
                include_explanations: Some(true),
            },
        ))
        .expect("recall bound Pg content");
        let text = format!("{payload:?}");
        assert!(
            text.contains("pg_retrieval_expected_token"),
            "recall must serve eligible bound content: {text}"
        );
        assert!(
            !text.contains("pg_retrieval_ineligible_token"),
            "recall must exclude foreign bound control: {text}"
        );
        println!("real_pg_retrieval_eligible_content {}", payload.items.len());
    }

    /// real_pg_graph_bound_answer: traversal through the real Pg recipe finds
    /// the published symbol context and excludes the foreign control symbol.
    #[test]
    #[ignore = "requires sourced ENGRAM_TEST_PG_URL"]
    fn real_pg_graph_bound_answer() {
        let (_dir, provider, scope, _repo) = published_fixture("graph", "pg_graph_expected_symbol");
        let (_foreign_dir, _foreign_provider, _foreign_scope, _foreign_repo) =
            published_fixture("graph-control", "pg_graph_ineligible_symbol");
        let nodes = block_on(provider.code_graph_explore_bound(
            &actor(),
            &scope,
            "pg_graph_expected_symbol",
            Some(2),
            Some(24),
            Some(64),
            true,
        ))
        .expect("traverse bound Pg graph");
        let text = format!("{nodes:?}");
        assert!(
            text.contains("pg_graph_expected_symbol"),
            "graph must answer with bound symbol context: {text}"
        );
        assert!(
            !text.contains("pg_graph_ineligible_symbol"),
            "graph must exclude foreign symbol: {text}"
        );
        println!("real_pg_graph_bound_answer {}", nodes.len());
    }

    /// real_pg_source_exact_bytes_and_span: exact source fetch through the
    /// real Pg recipe returns the retained bytes and correct subspan, while a
    /// foreign scope cannot use this bound repository.
    #[test]
    #[ignore = "requires sourced ENGRAM_TEST_PG_URL"]
    fn real_pg_source_exact_bytes_and_span() {
        let (_dir, provider, scope, repo) =
            published_fixture("source", "pg_source_expected_symbol");
        let whole = block_on(provider.code_fetch_exact(&actor(), &scope, &repo.0, "lib.rs", None))
            .expect("fetch retained bytes");
        let expected = b"pub fn pg_source_expected_symbol() -> u32 { 1 }\n";
        assert_eq!(
            whole.bytes, expected,
            "exact fetch must return retained bytes"
        );
        let span = 7..32;
        let sliced = block_on(provider.code_fetch_exact(
            &actor(),
            &scope,
            &repo.0,
            "lib.rs",
            Some(span.clone()),
        ))
        .expect("fetch retained span");
        assert_eq!(
            sliced.bytes,
            expected[span].to_vec(),
            "span fetch must return exact retained range"
        );
        let foreign = Scope {
            workspace: Some(format!("foreign-{}", unique_suffix())),
            ..scope.clone()
        };
        assert!(
            block_on(provider.code_fetch_exact(&actor(), &foreign, &repo.0, "lib.rs", None))
                .is_err(),
            "foreign scope must not fetch bound bytes"
        );
        println!(
            "real_pg_source_exact_bytes_and_span {}",
            whole.served_digest
        );
    }

    /// recipe_serving_excludes_retired_code_and_preserves_live_records:
    /// warm the opened Pg recipe's recall and bound graph paths, reset one
    /// bound generation, and use those same paths to prove the live durable
    /// eligibility resolver removes retired code without hiding ordinary or
    /// preserved records.
    #[test]
    #[ignore = "requires sourced ENGRAM_TEST_PG_URL"]
    fn recipe_serving_excludes_retired_code_and_preserves_live_records() {
        let dir = tempfile::tempdir().expect("tempdir");
        let id = unique_suffix().replace('-', "_");
        let make_repository = |name: &str, symbol: &str| {
            let repository = dir.path().join(name);
            std::fs::create_dir_all(&repository).expect("mkdir repository");
            let git = |args: &[&str]| {
                let status = Command::new("git")
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
            std::fs::write(
                repository.join("lib.rs"),
                format!("pub fn {symbol}() -> u32 {{ 1 }}\\n"),
            )
            .expect("write fixture");
            git(&["add", "."]);
            git(&["commit", "--no-gpg-sign", "-m", "fixture"]);
            repository
        };
        let retired_token = format!("pg_retired_serving_{id}");
        let preserved_token = format!("pg_preserved_serving_{id}");
        let ordinary_token = format!("pg_ordinary_serving_{id}");
        let retired_repository = make_repository("retired", &retired_token);
        let preserved_repository = make_repository("preserved", &preserved_token);
        std::fs::create_dir_all(dir.path().join("storage")).expect("mkdir storage");
        let mut cfg = config(dir.path());
        cfg.migration_mode = MigrationMode::Apply;
        let serving_scope = Scope {
            tenant: format!("recipe-serving-pg-{id}"),
            subject: None,
            workspace: Some("recipe-serving".into()),
            session: None,
            environment: None,
        };
        let provider = engram_backend_pgvector::open(&cfg)
            .expect("open recipe")
            .with_code_authorizer(Arc::new(
                engram_integration::code_binding::ScopeSetAuthorizer::new(vec![
                    serving_scope.clone(),
                ]),
            ))
            .expect("inject serving host policy");
        let coordinator = provider.code_bindings().expect("binding handle");
        let lanes = provider.code_ingestion().expect("recipe ingestion lanes");
        let retired_repo = RepoId(format!("recipe-serving-retired-{id}"));
        let preserved_repo = RepoId(format!("recipe-serving-preserved-{id}"));
        for (repo, repository) in [
            (&retired_repo, &retired_repository),
            (&preserved_repo, &preserved_repository),
        ] {
            block_on(coordinator.register(
                &actor(),
                &serving_scope,
                engram_knowledge::RepositoryRegistration {
                    repo_id: repo.clone(),
                    locator_id: repository.to_string_lossy().to_string(),
                    acquisition: engram_knowledge::AcquisitionMode::LocalOnly,
                },
            ))
            .expect("register bound fixture");
            block_on(coordinator.bind(&actor(), &serving_scope, repo, "refs/heads/main"))
                .expect("bind fixture");
            block_on(coordinator.ingest_registered(
                &actor(),
                &serving_scope,
                repo,
                None,
                None,
                lanes,
            ))
            .expect("publish through recipe ingestion");
        }
        seed_ordinary_record(lanes, &serving_scope, &id, &ordinary_token);
        let recall = |query: &str| {
            block_on(
                provider
                    .require_recall()
                    .expect("recall")
                    .recall(RetrievalRequest {
                        query: query.into(),
                        scope: serving_scope.clone(),
                        requester: requester(),
                        modes: vec![engram_domain::RetrievalMode::Graph],
                        filters: None,
                        cues: Vec::new(),
                        limit: None,
                        budget: None,
                        include_explanations: Some(true),
                    }),
            )
            .expect("serve recall")
        };
        let explore = |symbol: &str| {
            block_on(provider.code_graph_explore_bound(
                &actor(),
                &serving_scope,
                symbol,
                Some(2),
                Some(24),
                Some(64),
                true,
            ))
            .expect("serve graph")
        };
        let warm_recall = recall(&retired_token);
        let warm_graph = explore(&retired_token);
        assert!(format!("{warm_recall:?}").contains(&retired_token));
        assert!(format!("{warm_graph:?}").contains(&retired_token));
        let preview = block_on(coordinator.preview_reset(&actor(), &serving_scope, &retired_repo))
            .expect("authorized reset preview");
        let journal = CodeResetJournal {
            reset_id: format!("recipe-serving-reset-{id}"),
            preview_hash: preview.preview_hash.clone(),
            target_fingerprints: preview
                .targets
                .iter()
                .map(|target| (target.record_id.clone(), target.fingerprint.clone()))
                .collect(),
            authorizing_decisions: vec!["recipe-serving-approved".into()],
            checkpoints: vec!["authorized".into()],
            preservation_digests: preview.preservation_digests.clone(),
            preview,
        };
        let approved = block_on(coordinator.journal_reset(
            &actor(),
            journal,
            "recipe-serving-approved".into(),
        ))
        .expect("journal reset");
        block_on(coordinator.apply_reset(&actor(), approved)).expect("apply reset");
        let after_recall = recall(&retired_token);
        let after_graph = explore(&retired_token);
        assert!(
            !format!("{after_recall:?}").contains(&retired_token),
            "retired code must not remain in recall: {after_recall:?}"
        );
        assert!(
            !format!("{after_graph:?}").contains(&retired_token),
            "retired code must not remain in graph answers: {after_graph:?}"
        );
        assert_ne!(
            format!("{warm_recall:?}"),
            format!("{after_recall:?}"),
            "post-reset recall must differ from warm serving output"
        );
        assert!(
            format!("{:?}", recall(&ordinary_token)).contains(&ordinary_token),
            "ordinary recipe record must remain served"
        );
        let preserved_recall = recall(&preserved_token);
        let preserved_graph = explore(&preserved_token);
        assert!(format!("{preserved_recall:?}").contains(&preserved_token));
        assert!(format!("{preserved_graph:?}").contains(&preserved_token));
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
