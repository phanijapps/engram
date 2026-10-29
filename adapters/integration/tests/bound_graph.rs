//! TASK-015 focused proof: graph seeds/traversal/analytics are constrained to
//! the published bound generations BEFORE budgets, while eligible
//! cross-repository edges survive and same names/paths from different bound
//! repositories stay distinct endpoints (REQ-005 / AC-007).
//!
//! Every case drives REAL owned behavior: a real file-backed
//! `SqlCodeBindingRepository` (two published repos + one staged-only repo), a
//! real `SqlKnowledgeStore` knowledge graph, and the real provider graph
//! composition (`code_graph_explore_bound` / `code_graph_file_dependencies_bound`).

use std::sync::Arc;

use async_trait::async_trait;
use engram_domain::*;
use engram_integration::EngramProvider;
use engram_integration::code_binding::{
    CodeAuthorityAction, CodeBindingAuthorizer, CodeBindingCoordinator,
};
use engram_knowledge::{CodeBindingRepository, KnowledgeRepository, StageReceipt};
use engram_runtime::{CoreError, CoreResult};
use engram_store_sqlite::knowledge::{SqlCodeBindingRepository, SqlKnowledgeStore};
use futures::executor::block_on;

// ---------- fixture ---------------------------------------------------------

fn scope() -> Scope {
    Scope {
        tenant: "task-015".to_string(),
        subject: None,
        workspace: Some("bound-graph".to_string()),
        session: None,
        environment: None,
    }
}

fn actor() -> Actor {
    Actor {
        id: Id::from("task-015-agent"),
        kind: ActorKind::Agent,
        display_name: None,
        metadata: None,
    }
}

struct Authorizer;
#[async_trait]
impl CodeBindingAuthorizer for Authorizer {
    async fn authorize(
        &self,
        actor: &Actor,
        requested: &Scope,
        _repo: &RepoId,
        _action: CodeAuthorityAction,
    ) -> CoreResult<()> {
        if actor.id == Id::from("task-015-agent") && requested == &scope() {
            Ok(())
        } else {
            Err(CoreError::PolicyDenied {
                reason: "fixture authorization denied".into(),
            })
        }
    }
}

fn provenance() -> Provenance {
    Provenance {
        source: "task-015-fixture".to_string(),
        actor: actor(),
        observed_at: chrono::Utc::now(),
        evidence: Vec::new(),
        derivations: Vec::new(),
        confidence: Some(1.0),
        method: Some("test".to_string()),
    }
}

fn policy() -> Policy {
    Policy {
        visibility: Visibility::Workspace,
        retention: Retention::Durable,
        sensitivity: None,
        allowed_uses: vec![AllowedUse::Retrieval],
        expires_at: None,
        delete_mode: None,
    }
}

fn generation(id: &str) -> CodeGeneration {
    CodeGeneration {
        id: id.to_string(),
        binding_version: 1,
        commit_oid: "a".repeat(40),
        snapshot_digest: "b".repeat(64),
        parser_fingerprint: "parser".to_string(),
        filter_fingerprint: "filter".to_string(),
    }
}

fn attempt(id: &str, expected_epoch: u64, expected_active: Option<&str>) -> CodeAttempt {
    CodeAttempt {
        id: id.to_string(),
        idempotency_key: format!("idempotent-{id}"),
        binding_version: 1,
        expected_epoch,
        expected_active_generation: expected_active.map(str::to_string),
        fencing_token: 0,
        state: CodeAttemptState::Created,
        receipt_kinds: vec![CodeReceiptKind::Source, CodeReceiptKind::Knowledge],
    }
}

fn receipt(attempt_id: &str, generation_id: &str, lane: CodeReceiptKind) -> StageReceipt {
    StageReceipt {
        attempt_id: attempt_id.to_string(),
        generation_id: generation_id.to_string(),
        lane: serde_json::to_value(&lane)
            .ok()
            .and_then(|value| value.as_str().map(str::to_owned))
            .unwrap_or_default(),
        digest: format!("digest-{attempt_id}-{lane:?}"),
    }
}

/// Binds `repo` and publishes `generation` through the full durable state
/// machine (receipts → ready → publish). Synchronous: drives each port call
/// with `block_on` directly (no nested executors).
fn publish_generation(
    store: &SqlCodeBindingRepository,
    repo: &str,
    generation_id: &str,
    expected_epoch: u64,
    expected_active: Option<&str>,
) {
    let repo_id = RepoId(repo.to_string());
    let attempt_id = format!("attempt-{repo}-{generation_id}");
    block_on(store.begin_attempt(
        &scope(),
        &repo_id,
        generation(generation_id),
        attempt(&attempt_id, expected_epoch, expected_active),
    ))
    .expect("begin attempt");
    block_on(store.record_receipt(receipt(&attempt_id, generation_id, CodeReceiptKind::Source)))
        .expect("source receipt");
    block_on(store.record_receipt(receipt(
        &attempt_id,
        generation_id,
        CodeReceiptKind::Knowledge,
    )))
    .expect("knowledge receipt");
    let (_, stored) = block_on(store.get_attempt(&scope(), &repo_id, &attempt_id))
        .expect("get attempt")
        .expect("attempt exists");
    block_on(store.mark_ready(&scope(), &repo_id, &attempt_id, stored.fencing_token))
        .expect("mark ready");
    block_on(store.publish(&scope(), &repo_id, &attempt_id, expected_epoch)).expect("publish");
}

fn entity(
    id: &str,
    name: &str,
    kind: EntityKind,
    repo: Option<&str>,
    generation: Option<&str>,
) -> KnowledgeEntity {
    let mut metadata = Metadata::new();
    if let (Some(repo), Some(generation)) = (repo, generation) {
        metadata.insert(
            "codeRepoId".to_string(),
            serde_json::Value::String(repo.to_string()),
        );
        metadata.insert(
            "codeGenerationId".to_string(),
            serde_json::Value::String(generation.to_string()),
        );
    }
    KnowledgeEntity {
        id: Id::from(id),
        graph_id: None,
        kind,
        name: name.to_string(),
        aliases: Vec::new(),
        scope: scope(),
        source_refs: Vec::new(),
        concept_refs: Vec::new(),
        ontology_class_refs: Vec::new(),
        provenance: provenance(),
        created_at: chrono::Utc::now(),
        updated_at: None,
        valid_from: None,
        valid_until: None,
        metadata: Some(metadata),
        archived_at: None,
    }
}

fn ref_of(id: &str, name: &str) -> EntityRef {
    EntityRef {
        id: Some(Id::from(id)),
        kind: None,
        name: Some(name.to_string()),
        aliases: Vec::new(),
    }
}

fn relationship(
    id: &str,
    predicate: &str,
    subject: &str,
    subject_name: &str,
    object: &str,
    object_name: &str,
) -> KnowledgeRelationship {
    KnowledgeRelationship {
        id: Id::from(id),
        graph_id: None,
        predicate: predicate.to_string(),
        subject: ref_of(subject, subject_name),
        object: ref_of(object, object_name),
        scope: scope(),
        evidence: Vec::new(),
        confidence: None,
        provenance: provenance(),
        created_at: chrono::Utc::now(),
        updated_at: None,
        code_resolution: None,
        archived_at: None,
    }
}

/// The full fixture: repo-a + repo-b PUBLISHED, repo-c staged-only (durable
/// but never published). Same symbol names and same file paths appear in
/// repo-a and repo-b; a high-degree stale hub sits in repo-c.
struct GraphFixture {
    provider: EngramProvider,
}

fn fixture(dir: &std::path::Path) -> GraphFixture {
    // Durable bindings: repo-a/gen-a and repo-b/gen-b published; repo-c
    // staged only.
    let bindings =
        SqlCodeBindingRepository::open(dir.join("bindings.sqlite")).expect("open binding store");
    for repo in ["repo-a", "repo-b", "repo-c"] {
        block_on(bindings.bind(CodeBinding {
            scope: scope(),
            repo_id: RepoId(repo.to_string()),
            branch_ref: "refs/heads/main".to_string(),
            version: 1,
            active_generation: None,
            epoch: 0,
        }))
        .expect("bind");
    }
    let bindings = Arc::new(bindings);
    publish_generation(&bindings, "repo-a", "gen-a", 0, None);
    publish_generation(&bindings, "repo-b", "gen-b", 0, None);
    {
        // repo-c: durable staged attempt, never published.
        let repo_id = RepoId("repo-c".to_string());
        let attempt_id = "attempt-repo-c-gen-c";
        block_on(bindings.begin_attempt(
            &scope(),
            &repo_id,
            generation("gen-c"),
            attempt(attempt_id, 0, None),
        ))
        .expect("begin repo-c attempt");
        block_on(bindings.record_receipt(receipt(attempt_id, "gen-c", CodeReceiptKind::Source)))
            .expect("repo-c source receipt");
    }

    let coordinator = CodeBindingCoordinator::new(
        bindings.clone() as Arc<dyn CodeBindingRepository>,
        Arc::new(Authorizer),
        Vec::new(),
    );

    // Knowledge graph: same names in repo-a/repo-b, cross-repo call edge,
    // cross-repo import edge, and a stale high-degree hub in repo-c.
    let knowledge = Arc::new(SqlKnowledgeStore::open_in_memory().expect("knowledge open"));
    let entities = vec![
        entity(
            "a-render",
            "render",
            EntityKind::Function,
            Some("repo-a"),
            Some("gen-a"),
        ),
        entity(
            "b-render",
            "render",
            EntityKind::Function,
            Some("repo-b"),
            Some("gen-b"),
        ),
        entity(
            "b-util",
            "util",
            EntityKind::Function,
            Some("repo-b"),
            Some("gen-b"),
        ),
        // Same names again, one per repo, at FILE kind.
        entity(
            "a-app-file",
            "src/app.rs",
            EntityKind::File,
            Some("repo-a"),
            Some("gen-a"),
        ),
        entity(
            "b-util-file",
            "src/util.rs",
            EntityKind::File,
            Some("repo-b"),
            Some("gen-b"),
        ),
        // Stale staged repo-c: a hub with the SAME names plus satellites.
        entity(
            "c-render",
            "render",
            EntityKind::Function,
            Some("repo-c"),
            Some("gen-c"),
        ),
        entity(
            "c-sat-1",
            "renderSat1",
            EntityKind::Function,
            Some("repo-c"),
            Some("gen-c"),
        ),
        entity(
            "c-sat-2",
            "renderSat2",
            EntityKind::Function,
            Some("repo-c"),
            Some("gen-c"),
        ),
        entity(
            "c-sat-3",
            "renderSat3",
            EntityKind::Function,
            Some("repo-c"),
            Some("gen-c"),
        ),
    ];
    let relationships = vec![
        // Authorized cross-repository edge between two BOUND repositories.
        relationship("r-1", "calls", "a-render", "render", "b-util", "util"),
        // Same-name edge inside repo-b.
        relationship("r-2", "calls", "b-render", "render", "b-util", "util"),
        // Cross-repo import edge (file → module path).
        relationship(
            "r-3",
            "imports",
            "a-app-file",
            "src/app.rs",
            "b-util-file",
            "./util",
        ),
        // Stale edges: repo-c hub + satellites (would dominate unconstrained).
        relationship(
            "r-4",
            "calls",
            "c-render",
            "render",
            "c-sat-1",
            "renderSat1",
        ),
        relationship(
            "r-5",
            "calls",
            "c-render",
            "render",
            "c-sat-2",
            "renderSat2",
        ),
        relationship(
            "r-6",
            "calls",
            "c-render",
            "render",
            "c-sat-3",
            "renderSat3",
        ),
        relationship("r-7", "calls", "a-render", "render", "c-render", "render"),
    ];
    for entity in entities {
        block_on(knowledge.put_entity(entity)).expect("put entity");
    }
    for relationship in relationships {
        block_on(knowledge.put_relationship(relationship)).expect("put relationship");
    }

    let report = engram_integration::CapabilityReport::new(CapabilityState::Supported);
    let provider = engram_integration::EngramProviderBuilder::new(report)
        .knowledge_query(knowledge.clone())
        .code_bindings(Arc::new(coordinator))
        .build();
    GraphFixture { provider }
}

// ---------- task_015 focused cases -----------------------------------------

mod task_015 {
    use super::*;

    /// same_names_cross_repo: the same symbol name (and the same file path)
    /// exists in two BOUND repositories and in a staged-only repo. The bound
    /// graph keeps BOTH published endpoints distinct (repository-qualified)
    /// and drops the staged one.
    #[test]
    fn same_names_cross_repo() {
        let dir = tempfile::tempdir().expect("tempdir");
        let fixture = fixture(dir.path());
        let nodes = block_on(fixture.provider.code_graph_explore_bound(
            &scope(),
            "render",
            Some(1),
            Some(24),
            Some(64),
            false,
        ))
        .expect("bound explore");
        let names: Vec<&str> = nodes.iter().map(|node| node.name.as_str()).collect();
        assert!(
            names.contains(&"repo-a::render"),
            "repo-a's published endpoint must appear: {names:?}"
        );
        assert!(
            names.contains(&"repo-b::render"),
            "repo-b's published endpoint must appear distinctly: {names:?}"
        );
        assert!(
            !names.iter().any(|name| name.starts_with("repo-c")),
            "the staged-only repository must not appear: {names:?}"
        );
    }

    /// bound_cross_repo_edges: an authorized edge BETWEEN two bound published
    /// repositories stays traversable (and importable) after constraint — the
    /// stale repo-c edges are gone while the repo-a → repo-b edge serves.
    #[test]
    fn bound_cross_repo_edges() {
        let dir = tempfile::tempdir().expect("tempdir");
        let fixture = fixture(dir.path());

        let nodes = block_on(fixture.provider.code_graph_explore_bound(
            &scope(),
            "render",
            Some(2),
            Some(24),
            Some(64),
            false,
        ))
        .expect("bound explore");
        let names: Vec<&str> = nodes.iter().map(|node| node.name.as_str()).collect();
        assert!(
            names.contains(&"repo-b::util"),
            "the cross-repository call edge (repo-a::render → repo-b::util) must stay traversable: {names:?}"
        );
        assert!(
            !names.iter().any(|name| name.starts_with("repo-c")),
            "staged-repository endpoints must never be reachable: {names:?}"
        );

        let dependencies = block_on(
            fixture
                .provider
                .code_graph_file_dependencies_bound(&scope()),
        )
        .expect("bound file dependencies");
        let cross_repo = dependencies
            .iter()
            .find(|dependency| dependency.from_path == "repo-a::src/app.rs");
        let Some(cross_repo) = cross_repo else {
            panic!(
                "repo-a's file must keep its import edge: {:?}",
                dependencies
            )
        };
        assert_eq!(
            cross_repo.resolved_to.as_deref(),
            Some("repo-b::src/util.rs"),
            "the import resolves to the BOUND repo-b file, crossing repositories honestly: {cross_repo:?}"
        );
    }

    /// eligible_graph_budgets: node/edge budgets apply AFTER eligibility —
    /// with a two-node budget the page fills with eligible endpoints even
    /// though the staged repo-c hub plus its satellites would otherwise
    /// dominate the traversal.
    #[test]
    fn eligible_graph_budgets() {
        let dir = tempfile::tempdir().expect("tempdir");
        let fixture = fixture(dir.path());
        let nodes = block_on(fixture.provider.code_graph_explore_bound(
            &scope(),
            "render",
            Some(2),
            Some(2),
            Some(2),
            false,
        ))
        .expect("budgeted bound explore");
        assert!(
            nodes.len() <= 2,
            "the node budget is respected: {:?}",
            nodes.iter().map(|n| n.name.clone()).collect::<Vec<_>>()
        );
        assert!(
            nodes.iter().all(|node| !node.name.starts_with("repo-c")),
            "no budget slot may be consumed by ineligible nodes: {:?}",
            nodes.iter().map(|n| n.name.clone()).collect::<Vec<_>>()
        );
        // And the budgeted page still carries eligible seeds.
        assert!(
            nodes.iter().any(|node| node.name.ends_with("::render")),
            "eligible seeds must fill the budget: {:?}",
            nodes.iter().map(|n| n.name.clone()).collect::<Vec<_>>()
        );
    }
}
