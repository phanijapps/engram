//! TASK-015 + TASK-016 focused proofs (corrective r2): graph
//! seeds/traversal/analytics constrained to the published bound generations
//! BEFORE budgets (REQ-005 / AC-007), with stable entity-id traversal
//! (finding 10), both-endpoint authorization (finding 8), shared caller
//! authority (finding 3), and the shared scoped authorization seam for
//! explicit ids / wildcards / maintenance with durable lineage authority
//! (finding 4).
//!
//! Every case drives REAL owned behavior: real file-backed
//! `SqlCodeBindingRepository` (published + staged repos, DURABLE staged
//! lineage), real `SqlKnowledgeStore` graph, real provider graph composition
//! (`code_graph_explore_bound` / `code_graph_file_dependencies_bound`), and
//! the real `AuthorizedKnowledgeQuery` seam over the same coordinator.

use std::sync::Arc;

use async_trait::async_trait;
use engram_domain::*;
use engram_integration::code_binding::{
    CodeAuthorityAction, CodeBindingAuthorizer, CodeBindingCoordinator,
};
use engram_integration::{EngramProvider, EngramProviderBuilder};
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

fn foreign_scope() -> Scope {
    Scope {
        tenant: "task-015".to_string(),
        subject: None,
        workspace: Some("other-workspace".to_string()),
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

/// Deny-everything authorizer (finding 3): even fully populated scopes deny.
struct DenyAllAuthorizer;
#[async_trait]
impl CodeBindingAuthorizer for DenyAllAuthorizer {
    async fn authorize(
        &self,
        _actor: &Actor,
        _requested: &Scope,
        _repo: &RepoId,
        _action: CodeAuthorityAction,
    ) -> CoreResult<()> {
        Err(CoreError::PolicyDenied {
            reason: "deny-all fixture".into(),
        })
    }
}

struct AllowAuthorizer;
#[async_trait]
impl CodeBindingAuthorizer for AllowAuthorizer {
    async fn authorize(
        &self,
        _actor: &Actor,
        _requested: &Scope,
        _repo: &RepoId,
        _action: CodeAuthorityAction,
    ) -> CoreResult<()> {
        Ok(())
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

fn publish_generation_for_scope(
    store: &SqlCodeBindingRepository,
    target_scope: &Scope,
    repo: &str,
    generation_id: &str,
    expected_epoch: u64,
    expected_active: Option<&str>,
    lineage_records: &[&str],
) {
    let repo_id = RepoId(repo.to_string());
    let attempt_id = format!("attempt-{repo}-{generation_id}");
    block_on(store.begin_attempt(
        target_scope,
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
    let (_, stored) = block_on(store.get_attempt(target_scope, &repo_id, &attempt_id))
        .expect("get attempt")
        .expect("attempt exists");
    block_on(store.mark_ready(target_scope, &repo_id, &attempt_id, stored.fencing_token))
        .expect("mark ready");
    block_on(store.publish(target_scope, &repo_id, &attempt_id, expected_epoch)).expect("publish");
    for record in lineage_records {
        block_on(store.put_lineage(
            target_scope,
            record,
            CodeLineage {
                repo_id: RepoId(repo.to_string()),
                generation_id: generation_id.to_string(),
                source_path: format!("src/{record}.rs"),
                contributors: vec![record.to_string()],
            },
        ))
        .expect("put lineage");
    }
}

fn publish_generation(
    store: &SqlCodeBindingRepository,
    repo: &str,
    generation_id: &str,
    expected_epoch: u64,
    expected_active: Option<&str>,
    lineage_records: &[&str],
) {
    publish_generation_for_scope(
        store,
        &scope(),
        repo,
        generation_id,
        expected_epoch,
        expected_active,
        lineage_records,
    );
}

fn entity(id: &str, name: &str, kind: EntityKind) -> KnowledgeEntity {
    // Corrective r2 (finding 4): metadata is NOT authority — the fixture's
    // durable lineage names the code records.
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
        metadata: None,
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

fn ref_without_id(name: &str) -> EntityRef {
    EntityRef {
        id: None,
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

fn name_only_edge(
    id: &str,
    subject: &str,
    subject_name: &str,
    object_name: &str,
) -> KnowledgeRelationship {
    KnowledgeRelationship {
        id: Id::from(id),
        graph_id: None,
        predicate: "calls".to_string(),
        subject: ref_of(subject, subject_name),
        object: ref_without_id(object_name),
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

/// Durable lineage: repo-a published at gen-a (all its entities), repo-b
/// published at gen-b, repo-c staged only.
fn seed_graph(bindings: &SqlCodeBindingRepository, knowledge: &SqlKnowledgeStore) {
    publish_generation(
        bindings,
        "repo-a",
        "gen-a",
        0,
        None,
        &["a-render", "a-render-2", "a-app-file", "a-util-file"],
    );
    publish_generation(
        bindings,
        "repo-b",
        "gen-b",
        0,
        None,
        &["b-render", "b-util", "b-util-file"],
    );
    // repo-c: durable staged attempt + lineage, never published.
    let repo_id = RepoId("repo-c".to_string());
    block_on(bindings.begin_attempt(
        &scope(),
        &repo_id,
        generation("gen-c"),
        attempt("attempt-repo-c-gen-c", 0, None),
    ))
    .expect("begin repo-c attempt");
    block_on(bindings.put_lineage(
        &scope(),
        "c-render",
        CodeLineage {
            repo_id: RepoId("repo-c".to_string()),
            generation_id: "gen-c".to_string(),
            source_path: "src/c-render.rs".to_string(),
            contributors: vec!["c-render".to_string()],
        },
    ))
    .expect("repo-c lineage");

    let entities = vec![
        // SAME NAME in repo-a and repo-b (and staged repo-c).
        entity("a-render", "render", EntityKind::Function),
        entity("b-render", "render", EntityKind::Function),
        // SAME NAME, DIFFERENT FILE within ONE repository (finding 10).
        entity("a-render-2", "render", EntityKind::Function),
        entity("b-util", "util", EntityKind::Function),
        // IDENTICAL FILE PATHS in repo-a and repo-b (finding 10).
        entity("a-app-file", "src/app.rs", EntityKind::File),
        entity("a-util-file", "src/util.rs", EntityKind::File),
        entity("b-util-file", "src/util.rs", EntityKind::File),
        // Staged repo-c hub + satellites (unpublished).
        entity("c-render", "render", EntityKind::Function),
        entity("c-sat-1", "renderSat1", EntityKind::Function),
        entity("c-sat-2", "renderSat2", EntityKind::Function),
        // An UNBOUND name-only endpoint (dangling — finding 8) and an
        // ordinary non-code entity (authorized ordinary reads — finding 4).
        entity("plain-doc", "ordinaryConcept", EntityKind::Concept),
    ];
    let relationships = vec![
        // Authorized cross-repository edge between two BOUND repos.
        relationship("r-1", "calls", "a-render", "render", "b-util", "util"),
        // Same-name edge inside repo-b.
        relationship("r-2", "calls", "b-render", "render", "b-util", "util"),
        // Second same-name symbol inside repo-a (distinct id — finding 10).
        relationship("r-2b", "calls", "a-render-2", "render", "b-util", "util"),
        // Cross-repo import edge: repo-a's app.rs imports "./util" with the
        // object endpoint RESOLVED to repo-b's identical-path util.rs.
        relationship(
            "r-3",
            "imports",
            "a-app-file",
            "src/app.rs",
            "b-util-file",
            "./util",
        ),
        // Staged repo-c edges (would dominate unconstrained).
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
        // Dangling / name-only edges ahead of valid edges (finding 8).
        name_only_edge("r-6", "a-render", "render", "ghostSymbol"),
        relationship("r-7", "calls", "a-render", "render", "c-render", "render"),
    ];
    for entity in entities {
        block_on(knowledge.put_entity(entity)).expect("put entity");
    }
    for relationship in relationships {
        block_on(knowledge.put_relationship(relationship)).expect("put relationship");
    }
}

fn seed_foreign_scope(bindings: &SqlCodeBindingRepository, knowledge: &SqlKnowledgeStore) {
    let foreign = foreign_scope();
    let repo = RepoId("repo-foreign".to_string());
    block_on(bindings.bind(CodeBinding {
        scope: foreign.clone(),
        repo_id: repo,
        branch_ref: "refs/heads/foreign".to_string(),
        version: 1,
        active_generation: None,
        epoch: 0,
    }))
    .expect("bind populated foreign scope");
    publish_generation_for_scope(
        bindings,
        &foreign,
        "repo-foreign",
        "gen-foreign",
        0,
        None,
        &["foreign-render"],
    );
    let mut foreign_entity = entity("foreign-render", "foreignRender", EntityKind::Function);
    foreign_entity.scope = foreign;
    block_on(knowledge.put_entity(foreign_entity)).expect("put populated foreign entity");
}

fn provider_with(
    dir: &std::path::Path,
    authorizer: Arc<dyn CodeBindingAuthorizer>,
) -> (EngramProvider, Arc<SqlCodeBindingRepository>) {
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
    let knowledge = Arc::new(SqlKnowledgeStore::open_in_memory().expect("knowledge open"));
    seed_graph(&bindings, &knowledge);
    seed_foreign_scope(&bindings, &knowledge);
    let bindings = Arc::new(bindings);
    let coordinator = CodeBindingCoordinator::new(
        bindings.clone() as Arc<dyn CodeBindingRepository>,
        authorizer,
        Vec::new(),
    );
    let provider = EngramProviderBuilder::new(engram_integration::CapabilityReport::new(
        CapabilityState::Supported,
    ))
    .knowledge_query(knowledge)
    .code_bindings(Arc::new(coordinator))
    .build();
    (provider, bindings)
}

fn fixture(dir: &std::path::Path) -> (EngramProvider, Arc<SqlCodeBindingRepository>) {
    provider_with(dir, Arc::new(AllowAuthorizer))
}

// ---------- task_015 focused cases -----------------------------------------

/// TASK-014 focused proof: the eligibility-constrained community OVERVIEW
/// authorizes BOTH endpoints of every edge into the canonical eligible
/// vertex set BEFORE Louvain, community detection and limits (finding 8):
/// dangling/name-only edges and forged/null/foreign/missing-stamped
/// lineage-free code contribute neither vertices nor budget under TIGHT
/// limits, while ordinary non-code records survive.
mod task_014 {
    use super::*;

    fn stamped_entity(
        id: &str,
        name: &str,
        kind: EntityKind,
        generation_stamp: Option<serde_json::Value>,
        repo_stamp: Option<serde_json::Value>,
    ) -> KnowledgeEntity {
        let mut record = entity(id, name, kind);
        let mut metadata = Metadata::new();
        if let Some(generation) = generation_stamp {
            metadata.insert("codeGenerationId".to_owned(), generation);
        }
        if let Some(repo) = repo_stamp {
            metadata.insert("codeRepoId".to_owned(), repo);
        }
        record.metadata = Some(metadata);
        record
    }

    /// overview_endpoints_authorized_before_budgets: the polluted universe
    /// (name-only/dangling edges + stamped lineage-free code ahead of every
    /// valid edge) produces EXACTLY the authorized-only overview under a
    /// tight limit — the forgeries contribute neither vertices nor budget,
    /// and the ordinary concept counterpart survives.
    #[test]
    fn overview_endpoints_authorized_before_budgets() {
        let dir = tempfile::tempdir().expect("tempdir");
        let bindings = SqlCodeBindingRepository::open(dir.path().join("overview-bindings.sqlite"))
            .expect("open binding store");
        for repo in ["repo-a", "repo-b"] {
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
        publish_generation_for_scope(
            &bindings,
            &scope(),
            "repo-a",
            "gen-a",
            0,
            None,
            &["a-render", "a-hub"],
        );
        publish_generation_for_scope(&bindings, &scope(), "repo-b", "gen-b", 0, None, &["b-util"]);

        // Authorized baseline universe: published repo-a clique + repo-b
        // endpoint + one ordinary concept.
        let authorized_entities = vec![
            entity("a-render", "render", EntityKind::Function),
            entity("a-hub", "hub", EntityKind::Struct),
            entity("b-util", "util", EntityKind::Function),
            entity("plain-doc", "ordinaryConcept", EntityKind::Concept),
        ];
        let authorized_relationships = vec![
            relationship("o-1", "calls", "a-render", "render", "a-hub", "hub"),
            relationship("o-2", "calls", "a-render", "render", "b-util", "util"),
            relationship(
                "o-3",
                "mentions",
                "a-hub",
                "hub",
                "plain-doc",
                "ordinaryConcept",
            ),
        ];

        // Polluted universe: the authorized graph PLUS forged/null/foreign/
        // missing-stamped lineage-free code entities, each wired with TWO
        // edges (one to an authorized endpoint, one name-only/dangling), all
        // ahead of the authorized edges so they would dominate budgets.
        let mut entities = authorized_entities.clone();
        let mut relationships = authorized_relationships.clone();
        let forgeries = [
            ("forged-fn", "forgedParser"),
            ("null-stamp-fn", "nullParser"),
            ("foreign-stamp-fn", "foreignParser"),
            ("missing-stamp-fn", "missingParser"),
        ];
        entities.push(stamped_entity(
            "forged-fn",
            "forgedParser",
            EntityKind::Function,
            Some(serde_json::Value::String("gen-a".to_owned())),
            Some(serde_json::Value::String("repo-a".to_owned())),
        ));
        entities.push(stamped_entity(
            "null-stamp-fn",
            "nullParser",
            EntityKind::Function,
            Some(serde_json::Value::Null),
            Some(serde_json::Value::Null),
        ));
        entities.push(stamped_entity(
            "foreign-stamp-fn",
            "foreignParser",
            EntityKind::Function,
            Some(serde_json::Value::String("gen-foreign".to_owned())),
            Some(serde_json::Value::String("repo-foreign".to_owned())),
        ));
        entities.push(stamped_entity(
            "missing-stamp-fn",
            "missingParser",
            EntityKind::Function,
            None,
            None,
        ));
        for (id, name) in forgeries {
            relationships.push(relationship(
                &format!("poll-{id}"),
                "calls",
                id,
                name,
                "a-render",
                "render",
            ));
            relationships.push(name_only_edge(
                &format!("poll-dangle-{id}"),
                id,
                name,
                "ghostSymbol",
            ));
        }

        let view = |_knowledge: &Arc<SqlKnowledgeStore>, _polluted: bool| {
            let resolver =
                engram_integration::code_binding::RepositoryEligibilitySource::new(Arc::new(
                    SqlCodeBindingRepository::open(dir.path().join("overview-bindings.sqlite"))
                        .expect("reopen bindings"),
                ));
            use engram_retrieval::CodeEligibilityResolver as _;
            let eligibility = block_on(resolver.resolve(&scope())).expect("resolve eligibility");
            engram_retrieval::BoundCodeView::from_eligibility(&eligibility)
        };

        // Baseline overview: authorized universe only, TIGHT limit 1.
        let baseline_store = Arc::new(SqlKnowledgeStore::open_in_memory().expect("store"));
        for entity in authorized_entities {
            block_on(baseline_store.put_entity(entity)).expect("put entity");
        }
        for relationship in authorized_relationships {
            block_on(baseline_store.put_relationship(relationship)).expect("put relationship");
        }
        let baseline_view = view(&baseline_store, false);
        let baseline = block_on(engram_integration::sqlite::overview_eligible(
            &baseline_store,
            &scope(),
            1,
            &baseline_view,
        ))
        .expect("baseline overview");

        // Polluted overview: same authorized universe plus the forgeries,
        // under the SAME tight limit.
        let polluted_store = Arc::new(SqlKnowledgeStore::open_in_memory().expect("store"));
        for entity in entities {
            block_on(polluted_store.put_entity(entity)).expect("put entity");
        }
        for relationship in relationships {
            block_on(polluted_store.put_relationship(relationship)).expect("put relationship");
        }
        let polluted_view = view(&polluted_store, true);
        let polluted_overview = block_on(engram_integration::sqlite::overview_eligible(
            &polluted_store,
            &scope(),
            1,
            &polluted_view,
        ))
        .expect("polluted overview");

        assert_eq!(
            baseline, polluted_overview,
            "forged/null/foreign/missing-stamped lineage-free code and dangling/name-only edges \
             must contribute neither vertices nor budget to the overview"
        );
        // The authorized universe is genuinely present (not an empty equal
        // comparison): members and communities exist and the ordinary
        // concept participates through its authorized edge.
        assert!(
            !polluted_overview.communities.is_empty(),
            "the authorized universe must still produce communities"
        );
        let unconstrained_total = polluted_overview.total_communities;
        assert!(
            unconstrained_total >= 1,
            "authorized communities exist: {polluted_overview:?}"
        );
    }
}

mod task_015 {
    use super::*;

    /// same_names_cross_repo: same symbol name across two PUBLISHED repos and
    /// a staged-only repo; ALSO the same name twice in DIFFERENT files of
    /// ONE repository (finding 10). All three published endpoints stay
    /// DISTINCT (stable ids, repository-qualified labels); the staged one
    /// never appears.
    #[test]
    fn same_names_cross_repo() {
        let dir = tempfile::tempdir().expect("tempdir");
        let (provider, _) = fixture(dir.path());
        let nodes = block_on(provider.code_graph_explore_bound(
            &actor(),
            &scope(),
            "render",
            Some(1),
            Some(24),
            Some(64),
            false,
        ))
        .expect("bound explore");
        let by_id: Vec<(&str, &str)> = nodes
            .iter()
            .map(|n| (n.id.as_str(), n.label.as_str()))
            .collect();
        assert!(
            by_id.iter().any(|(id, _)| *id == "a-render"),
            "repo-a's published endpoint must appear: {by_id:?}"
        );
        assert!(
            by_id.iter().any(|(id, _)| *id == "b-render"),
            "repo-b's published endpoint must appear distinctly: {by_id:?}"
        );
        assert!(
            by_id.iter().any(|(id, _)| *id == "a-render-2"),
            "the SAME NAME in a different file of repo-a must stay a DISTINCT node: {by_id:?}"
        );
        assert!(
            by_id.iter().all(|(_, label)| !label.starts_with("repo-c")),
            "the staged-only repository must not appear: {by_id:?}"
        );
        // Labels are repository-qualified for code records (display only).
        assert!(
            by_id.iter().any(|(_, label)| *label == "repo-a::render"),
            "code display labels carry the repository: {by_id:?}"
        );
    }

    /// artifact_suppression_filters_only_discovery_seeds: a File-only query
    /// does not seed when artifacts are suppressed, but it does when callers
    /// request artifacts.  The existing cross-repository symbol traversal
    /// below proves those structural vertices remain in the traversal graph.
    #[test]
    fn artifact_suppression_filters_only_discovery_seeds() {
        let dir = tempfile::tempdir().expect("tempdir");
        let (provider, _) = fixture(dir.path());
        let suppressed = block_on(provider.code_graph_explore_bound(
            &actor(),
            &scope(),
            "app",
            Some(1),
            Some(24),
            Some(64),
            false,
        ))
        .expect("suppressed artifact query");
        assert!(
            suppressed.is_empty(),
            "an artifact-only match must not seed when suppression is false: {suppressed:?}"
        );
        let included = block_on(provider.code_graph_explore_bound(
            &actor(),
            &scope(),
            "app",
            Some(1),
            Some(24),
            Some(64),
            true,
        ))
        .expect("included artifact query");
        assert!(
            included.iter().any(|node| node.id == "a-app-file"),
            "the authorized file seeds only when requested: {included:?}"
        );
    }

    /// bound_cross_repo_edges: an authorized edge BETWEEN two bound
    /// published repositories stays traversable; an import that explicitly
    /// targets repo-b's IDENTICAL PATH resolves there (finding 10) even
    /// though repo-a has the same `src/util.rs`; dangling/name-only edges
    /// never traverse (finding 8).
    #[test]
    fn bound_cross_repo_edges() {
        let dir = tempfile::tempdir().expect("tempdir");
        let (provider, _) = fixture(dir.path());

        let nodes = block_on(provider.code_graph_explore_bound(
            &actor(),
            &scope(),
            "render",
            Some(2),
            Some(24),
            Some(64),
            false,
        ))
        .expect("bound explore");
        let ids: Vec<&str> = nodes.iter().map(|node| node.id.as_str()).collect();
        assert!(
            ids.contains(&"b-util"),
            "the cross-repository call edge (a-render → b-util) must stay traversable: {ids:?}"
        );
        assert!(
            !ids.iter()
                .any(|id| id.starts_with("c-") || id.starts_with("repo-c")),
            "staged-repository and dangling endpoints must never be reachable: {ids:?}"
        );

        let dependencies =
            block_on(provider.code_graph_file_dependencies_bound(&actor(), &scope()))
                .expect("bound file dependencies");
        let cross_repo = dependencies
            .iter()
            .find(|dependency| dependency.from_path == "repo-a::src/app.rs");
        let Some(cross_repo) = cross_repo else {
            panic!("repo-a's file must keep its import edge: {dependencies:?}");
        };
        assert_eq!(
            cross_repo.resolved_to.as_deref(),
            Some("repo-b::src/util.rs"),
            "the scanner-resolved endpoint wins over identical-path ambiguity: {cross_repo:?}"
        );
    }

    /// eligible_graph_budgets: node/edge budgets apply AFTER eligibility —
    /// with a two-node budget the page fills with eligible endpoints even
    /// though the staged repo-c hub, its satellites, the dangling edges and
    /// the unauthorized cross edges would otherwise dominate the traversal.
    #[test]
    fn eligible_graph_budgets() {
        let dir = tempfile::tempdir().expect("tempdir");
        let (provider, _) = fixture(dir.path());
        let nodes = block_on(provider.code_graph_explore_bound(
            &actor(),
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
            nodes.iter().map(|n| n.id.clone()).collect::<Vec<_>>()
        );
        assert!(
            nodes.iter().all(|node| !node.id.starts_with("c-")),
            "no budget slot may be consumed by ineligible or dangling endpoints: {:?}",
            nodes.iter().map(|n| n.id.clone()).collect::<Vec<_>>()
        );
        assert!(
            nodes.iter().any(|node| node.id.contains("render")),
            "eligible seeds must fill the budget: {:?}",
            nodes.iter().map(|n| n.id.clone()).collect::<Vec<_>>()
        );
    }

    /// Corrective r2 (finding 3): a DENYING authorizer denies the bound
    /// graph reads even though the scope is fully populated — the shared
    /// seam authenticates the caller before any lineage consult.
    #[test]
    fn denying_authorizer_denies_populated_scope() {
        let dir = tempfile::tempdir().expect("tempdir");
        let (provider, _) = provider_with(dir.path(), Arc::new(DenyAllAuthorizer));
        let denied = block_on(provider.code_graph_explore_bound(
            &actor(),
            &scope(),
            "render",
            Some(1),
            Some(24),
            Some(64),
            false,
        ));
        assert!(
            matches!(&denied, Err(CoreError::PolicyDenied { reason }) if reason.contains("deny-all")),
            "a deny-all authorizer must deny even for a populated scope: {denied:?}"
        );
        let deps_denied = block_on(provider.code_graph_file_dependencies_bound(&actor(), &scope()));
        assert!(deps_denied.is_err());
    }
}

// ---------- task_016 focused cases -----------------------------------------

mod task_016 {
    use super::*;
    use engram_integration::knowledge_query::AuthorizedKnowledgeQuery;

    fn gate_fixture(
        dir: &std::path::Path,
    ) -> (AuthorizedKnowledgeQuery, Arc<SqlCodeBindingRepository>) {
        // One provider per tempdir (the durable store must not be re-seeded
        // through a second handle — the fencing state machine would see a
        // concurrent writer).
        let (provider, bindings) = provider_with(dir, Arc::new(AllowAuthorizer));
        let knowledge = provider
            .knowledge_query()
            .cloned()
            .expect("knowledge query handle");
        let coordinator = provider.code_bindings().expect("coordinator").clone();
        let authorized = AuthorizedKnowledgeQuery::new(knowledge, coordinator);
        (authorized, bindings)
    }

    /// foreign_ids_denied: explicit ids from a staged-only generation (or
    /// unknown) deny the whole read; published ids authorize; cross-scope
    /// denies; and a DENYING authorizer denies for a POPULATED foreign scope
    /// (finding 3).
    #[test]
    fn foreign_ids_denied() {
        let dir = tempfile::tempdir().expect("tempdir");
        let (authorized, bindings) = gate_fixture(dir.path());

        let allowed = block_on(authorized.authorize_read(
            &actor(),
            &scope(),
            &["a-render".to_string(), "b-util".to_string()],
            10,
        ))
        .expect("published ids authorize");
        assert_eq!(allowed, vec!["a-render".to_string(), "b-util".to_string()]);

        let mixed = block_on(authorized.authorize_read(
            &actor(),
            &scope(),
            &["a-render".to_string(), "c-render".to_string()],
            10,
        ));
        assert!(
            matches!(&mixed, Err(CoreError::InvalidRequest { reason }) if reason.contains("foreign or unpublished")),
            "one staged/foreign id denies the entire explicit read: {mixed:?}"
        );

        let unknown = block_on(authorized.authorize_read(
            &actor(),
            &scope(),
            &["no-such-target".to_string()],
            10,
        ));
        assert!(unknown.is_err(), "unknown ids never authorize");

        // A DIFFERENT (populated) scope never authorizes this scope's ids.
        let foreign = block_on(authorized.authorize_read(
            &actor(),
            &foreign_scope(),
            &["a-render".to_string()],
            10,
        ));
        assert!(
            matches!(&foreign, Err(CoreError::InvalidRequest { reason }) if reason.contains("foreign or unpublished")),
            "cross-scope explicit reads deny: {foreign:?}"
        );

        // Finding 3: a deny-all authorizer denies for a POPULATED scope —
        // the seam consults authority BEFORE lineage existence. (Fresh
        // tempdir: the durable store is seeded once per handle.)
        let deny_dir = tempfile::tempdir().expect("deny tempdir");
        let (deny_provider, _) = provider_with(deny_dir.path(), Arc::new(DenyAllAuthorizer));
        let deny_authorized = AuthorizedKnowledgeQuery::new(
            deny_provider.knowledge_query().cloned().expect("handle"),
            deny_provider.code_bindings().expect("coordinator").clone(),
        );
        let denied = block_on(deny_authorized.authorize_read(
            &actor(),
            &scope(),
            &["a-render".to_string()],
            10,
        ));
        assert!(
            matches!(&denied, Err(CoreError::PolicyDenied { .. })),
            "a denying caller is denied even though the lineage exists: {denied:?}"
        );
        let _ = bindings;
    }

    /// wildcard_cannot_widen: an empty (wildcard) request resolves to exactly
    /// the PUBLISHED target set — staged repo-c records and unknown targets
    /// never appear; unstamped legacy CODE kinds never appear in wildcard
    /// listings (finding 4); ordinary non-code entities DO appear.
    #[test]
    fn wildcard_cannot_widen() {
        let dir = tempfile::tempdir().expect("tempdir");
        let (authorized, _bindings) = gate_fixture(dir.path());

        let wildcard =
            block_on(authorized.authorize_read(&actor(), &scope(), &[], 100)).expect("wildcard");
        assert!(
            wildcard.contains(&"a-render".to_string()) && wildcard.contains(&"b-util".to_string()),
            "the wildcard resolves the published set: {wildcard:?}"
        );
        assert!(
            !wildcard.contains(&"c-render".to_string()),
            "the wildcard can never widen to a staged generation: {wildcard:?}"
        );

        let entities = block_on(authorized.list_entities_authorized(&actor(), &scope(), &[]))
            .expect("authorized listing");
        let ids: Vec<String> = entities.iter().map(|e| e.id.to_string()).collect();
        assert!(
            ids.contains(&"a-render".to_string()),
            "published code entity listed: {ids:?}"
        );
        assert!(
            ids.contains(&"plain-doc".to_string()),
            "non-code entity preserved: {ids:?}"
        );
        assert!(
            !ids.contains(&"c-render".to_string()),
            "staged code entity never listed: {ids:?}"
        );
    }

    /// Unbound scopes retain positively established ordinary knowledge while
    /// rejecting code IDs without inventing a repository anchor.
    #[test]
    fn unbound_scope_denies_code_and_preserves_ordinary_records() {
        let dir = tempfile::tempdir().expect("tempdir");
        let bindings = Arc::new(
            SqlCodeBindingRepository::open(dir.path().join("unbound-bindings.sqlite"))
                .expect("open bindings"),
        );
        let knowledge = Arc::new(SqlKnowledgeStore::open_in_memory().expect("knowledge"));
        let unbound = Scope {
            tenant: "task-015".to_string(),
            subject: None,
            workspace: Some("unbound-ordinary".to_string()),
            session: None,
            environment: None,
        };
        let mut ordinary = entity("unbound-doc", "ordinary", EntityKind::Concept);
        ordinary.scope = unbound.clone();
        block_on(knowledge.put_entity(ordinary)).expect("put ordinary");
        let mut code = entity("unbound-code", "render", EntityKind::Function);
        code.scope = unbound.clone();
        block_on(knowledge.put_entity(code)).expect("put code");
        let coordinator = Arc::new(CodeBindingCoordinator::new(
            bindings as Arc<dyn CodeBindingRepository>,
            Arc::new(AllowAuthorizer),
            Vec::new(),
        ));
        let authorized = AuthorizedKnowledgeQuery::new(knowledge, coordinator);
        let ordinary = block_on(authorized.list_entities_authorized(&actor(), &unbound, &[]))
            .expect("ordinary records survive without a code binding");
        assert_eq!(ordinary.len(), 1);
        assert_eq!(ordinary[0].id.as_str(), "unbound-doc");
        let denied = block_on(authorized.list_entities_authorized(
            &actor(),
            &unbound,
            &["unbound-code".to_string()],
        ));
        assert!(
            matches!(&denied, Err(CoreError::InvalidRequest { reason }) if reason.contains("BindingRequired")),
            "unbound code IDs must deny: {denied:?}"
        );
    }

    /// maintenance_scope_denied (accepted name): maintenance requires
    /// explicit ELIGIBLE CODE ids — ordinary ids, staged ids and empty
    /// requests all deny; published code authorizes. The finding-4 companion
    /// assertion (explicit ORDINARY reads authorize) is included: the same
    /// seam keeps authorized non-code reads working.
    #[test]
    fn maintenance_scope_denied() {
        let dir = tempfile::tempdir().expect("tempdir");
        let (authorized, _bindings) = gate_fixture(dir.path());

        // Ordinary explicit read: authorized non-code.
        let ordinary =
            block_on(authorized.authorize_read(&actor(), &scope(), &["plain-doc".to_string()], 10))
                .expect("an authorized ordinary entity needs no code lineage");
        assert_eq!(ordinary, vec!["plain-doc".to_string()]);

        // Maintenance requires ELIGIBLE CODE ids: ordinary ids deny.
        let ordinary_maintenance = block_on(authorized.authorize_maintenance(
            &actor(),
            &scope(),
            &["plain-doc".to_string()],
        ));
        assert!(
            matches!(&ordinary_maintenance, Err(CoreError::InvalidRequest { reason }) if reason.contains("foreign or unpublished")),
            "maintenance is code-only: {ordinary_maintenance:?}"
        );

        // Staged target denies maintenance; published code authorizes.
        let staged = block_on(authorized.authorize_maintenance(
            &actor(),
            &scope(),
            &["c-render".to_string()],
        ));
        assert!(staged.is_err(), "staged targets deny maintenance");
        let maintained = block_on(authorized.authorize_maintenance(
            &actor(),
            &scope(),
            &["a-render".to_string()],
        ))
        .expect("published maintenance authorizes");
        assert_eq!(maintained, vec!["a-render".to_string()]);

        // The scope-wide shortcut (no explicit ids) denies.
        let empty = block_on(authorized.authorize_maintenance(&actor(), &scope(), &[]));
        assert!(
            matches!(&empty, Err(CoreError::InvalidRequest { reason }) if reason.contains("explicit target ids")),
            "no scope-wide maintenance shortcut exists: {empty:?}"
        );
    }
}
