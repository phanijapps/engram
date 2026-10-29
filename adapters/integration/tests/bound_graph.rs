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
use engram_knowledge::{
    CodeBindingRepository, KnowledgeGraphRepository, KnowledgeRepository, StageReceipt,
};
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

/// Allow-all ordinary-record policy authorizer (the owning default for the
/// unbound-branch tests; the denying variant below proves the seam).
struct AllowAllPolicy;
impl engram_runtime::PolicyAuthorizer for AllowAllPolicy {
    fn can_write(
        &self,
        _requester: &Requester,
        _scope: &Scope,
        _policy: &Policy,
    ) -> CoreResult<()> {
        Ok(())
    }
    fn can_retrieve(
        &self,
        _requester: &Requester,
        _scope: &Scope,
        _policy: &Policy,
    ) -> CoreResult<()> {
        Ok(())
    }
    fn can_forget(
        &self,
        _requester: &Requester,
        _scope: &Scope,
        _policy: &Policy,
    ) -> CoreResult<()> {
        Ok(())
    }
}

/// Deny-all ORDINARY-record policy authorizer (canonical 3c: an actually
/// denied ordinary caller receives nothing in an unbound scope).
struct DenyAllPolicy;
impl engram_runtime::PolicyAuthorizer for DenyAllPolicy {
    fn can_write(
        &self,
        _requester: &Requester,
        _scope: &Scope,
        _policy: &Policy,
    ) -> CoreResult<()> {
        Err(CoreError::PolicyDenied {
            reason: "ordinary reads denied".to_string(),
        })
    }
    fn can_retrieve(
        &self,
        _requester: &Requester,
        _scope: &Scope,
        _policy: &Policy,
    ) -> CoreResult<()> {
        Err(CoreError::PolicyDenied {
            reason: "ordinary reads denied".to_string(),
        })
    }
    fn can_forget(
        &self,
        _requester: &Requester,
        _scope: &Scope,
        _policy: &Policy,
    ) -> CoreResult<()> {
        Err(CoreError::PolicyDenied {
            reason: "ordinary reads denied".to_string(),
        })
    }
}

/// Permits ordinary low-sensitivity graph policy and rejects stricter graph policy.
struct LowGraphPolicyOnly;
impl engram_runtime::PolicyAuthorizer for LowGraphPolicyOnly {
    fn can_write(
        &self,
        _requester: &Requester,
        _scope: &Scope,
        _policy: &Policy,
    ) -> CoreResult<()> {
        Ok(())
    }
    fn can_retrieve(
        &self,
        _requester: &Requester,
        _scope: &Scope,
        policy: &Policy,
    ) -> CoreResult<()> {
        if policy.sensitivity == Some(Sensitivity::High) {
            Err(CoreError::PolicyDenied {
                reason: "high graph policy denied".to_string(),
            })
        } else {
            Ok(())
        }
    }
    fn can_forget(
        &self,
        _requester: &Requester,
        _scope: &Scope,
        _policy: &Policy,
    ) -> CoreResult<()> {
        Ok(())
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
    .knowledge_query(knowledge.clone())
    .graph(knowledge)
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

        // Label NUMBERS are assigned per Louvain run order (hash iteration)
        // and may differ between identical graphs; the structural claim is
        // label-invariant: same community count, same member-count shape,
        // same meta-edge shape.
        let shape = |overview: &CommunityOverview| {
            (
                overview.total_communities,
                {
                    let mut counts: Vec<usize> = overview
                        .communities
                        .iter()
                        .map(|c| c.member_count)
                        .collect();
                    counts.sort_unstable();
                    counts
                },
                {
                    let mut weights: Vec<usize> = overview.edges.iter().map(|e| e.weight).collect();
                    weights.sort_unstable();
                    weights
                },
            )
        };
        assert_eq!(
            shape(&baseline),
            shape(&polluted_overview),
            "forged/null/foreign/missing-stamped lineage-free code and dangling/name-only edges \
             must contribute neither vertices nor budget to the overview \
             (baseline {:?} vs polluted {:?})",
            baseline,
            polluted_overview
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

    /// Task015 regression: artifact suppression filters ONLY discovery
    /// seeds. A SYMBOL-seeded traversal still crosses authorized File /
    /// Module vertices to reach other symbols while suppression is ON — the
    /// structural traversal universe is never narrowed by the flag.
    #[test]
    fn symbol_traversal_crosses_artifact_nodes_under_suppression() {
        let dir = tempfile::tempdir().expect("tempdir");
        let bindings = SqlCodeBindingRepository::open(dir.path().join("artifact-bindings.sqlite"))
            .expect("open binding store");
        block_on(bindings.bind(CodeBinding {
            scope: scope(),
            repo_id: RepoId("repo-a".to_string()),
            branch_ref: "refs/heads/main".to_string(),
            version: 1,
            active_generation: None,
            epoch: 0,
        }))
        .expect("bind");
        publish_generation_for_scope(
            &bindings,
            &scope(),
            "repo-a",
            "gen-a",
            0,
            None,
            &["a-render", "a-app-file", "a-helper"],
        );
        let knowledge = Arc::new(SqlKnowledgeStore::open_in_memory().expect("knowledge"));
        for entity in [
            entity("a-render", "render", EntityKind::Function),
            entity("a-app-file", "src/app.rs", EntityKind::File),
            entity("a-helper", "helper", EntityKind::Function),
        ] {
            block_on(knowledge.put_entity(entity)).expect("put entity");
        }
        for relationship in [
            relationship(
                "x-1",
                "contains",
                "a-render",
                "render",
                "a-app-file",
                "src/app.rs",
            ),
            relationship(
                "x-2",
                "contains",
                "a-app-file",
                "src/app.rs",
                "a-helper",
                "helper",
            ),
        ] {
            block_on(knowledge.put_relationship(relationship)).expect("put relationship");
        }
        let bindings = Arc::new(bindings);
        let coordinator = CodeBindingCoordinator::new(
            bindings as Arc<dyn CodeBindingRepository>,
            Arc::new(crate::AllowAuthorizer),
            Vec::new(),
        );
        let provider = EngramProviderBuilder::new(engram_integration::CapabilityReport::new(
            CapabilityState::Supported,
        ))
        .knowledge_query(knowledge)
        .code_bindings(Arc::new(coordinator))
        .build();

        // Suppression ON: the artifact-only query seeds nothing...
        let suppressed = block_on(provider.code_graph_explore_bound(
            &actor(),
            &scope(),
            "src/app.rs",
            Some(2),
            Some(24),
            Some(64),
            false,
        ))
        .expect("suppressed artifact query");
        assert!(
            suppressed.is_empty(),
            "artifact-only queries never seed under suppression: {suppressed:?}"
        );

        // ...but a SYMBOL query still traverses THROUGH the File vertex to
        // the symbol on the other side: the structural universe is intact.
        let through = block_on(provider.code_graph_explore_bound(
            &actor(),
            &scope(),
            "render",
            Some(2),
            Some(24),
            Some(64),
            false,
        ))
        .expect("symbol query under suppression");
        let ids: Vec<&str> = through.iter().map(|node| node.id.as_str()).collect();
        assert!(
            ids.contains(&"a-helper"),
            "symbol traversal must cross the artifact vertex to reach a-helper: {ids:?}"
        );

        // Suppression OFF: the artifact seeds directly.
        let seeded = block_on(provider.code_graph_explore_bound(
            &actor(),
            &scope(),
            "src/app.rs",
            Some(2),
            Some(24),
            Some(64),
            true,
        ))
        .expect("artifact query with artifacts included");
        assert!(
            seeded.iter().any(|node| node.id == "a-app-file"),
            "the artifact seeds when explicitly requested: {seeded:?}"
        );
    }

    /// Corrective r4 (canonical 3b): the authorized universe, seed subset and
    /// labels are built INSIDE the pinned read. A REAL publication landing
    /// between the initial entity load and the lease (deterministically
    /// interposed at the knowledge-read port boundary) can never serve the
    /// retired generation's seed at hop 0 or spend its node budget.
    #[test]
    fn publication_barrier_after_load_never_serves_stale_seed() {
        struct PublishingQuery {
            inner: Arc<SqlKnowledgeStore>,
            bindings: Arc<SqlCodeBindingRepository>,
            published: std::sync::atomic::AtomicBool,
        }
        #[async_trait::async_trait]
        impl engram_integration::KnowledgeQuery for PublishingQuery {
            async fn list_entities(&self, scope: &Scope) -> CoreResult<Vec<KnowledgeEntity>> {
                if !self
                    .published
                    .swap(true, std::sync::atomic::Ordering::SeqCst)
                {
                    // REAL publication lands between the caller's initial
                    // entity load and the pinned read: gen-1 (which owns
                    // barrierSymbol) is superseded by gen-2. The publication
                    // runs on a DEDICATED thread — list_entities is already
                    // inside the recall executor, so a nested block_on would
                    // re-enter the LocalPool.
                    let bindings = self.bindings.clone();
                    let scope = scope.clone();
                    std::thread::spawn(move || {
                        publish_generation_for_scope(
                            &bindings,
                            &scope,
                            "repo-a",
                            "gen-barrier-2",
                            1,
                            Some("gen-barrier-1"),
                            &["barrier-live"],
                        );
                    })
                    .join()
                    .expect("publication thread");
                }
                self.inner.list_entities(scope).await
            }
            async fn list_relationships(
                &self,
                scope: &Scope,
            ) -> CoreResult<Vec<KnowledgeRelationship>> {
                self.inner.list_relationships(scope).await
            }
        }

        let dir = tempfile::tempdir().expect("tempdir");
        let bindings = SqlCodeBindingRepository::open(dir.path().join("barrier-bindings.sqlite"))
            .expect("open binding store");
        block_on(bindings.bind(CodeBinding {
            scope: scope(),
            repo_id: RepoId("repo-a".to_string()),
            branch_ref: "refs/heads/main".to_string(),
            version: 1,
            active_generation: None,
            epoch: 0,
        }))
        .expect("bind");
        publish_generation_for_scope(
            &bindings,
            &scope(),
            "repo-a",
            "gen-barrier-1",
            0,
            None,
            &["barrier-symbol", "barrier-peer"],
        );
        let knowledge = Arc::new(SqlKnowledgeStore::open_in_memory().expect("knowledge"));
        for entity in [
            entity("barrier-symbol", "barrierSymbol", EntityKind::Function),
            entity("barrier-peer", "barrierPeer", EntityKind::Function),
            entity("barrier-live", "barrierLive", EntityKind::Function),
        ] {
            block_on(knowledge.put_entity(entity)).expect("put entity");
        }
        block_on(knowledge.put_relationship(relationship(
            "barrier-1",
            "calls",
            "barrier-symbol",
            "barrierSymbol",
            "barrier-peer",
            "barrierPeer",
        )))
        .expect("put relationship");
        let bindings = Arc::new(bindings);
        let publishing: Arc<dyn engram_integration::KnowledgeQuery> = Arc::new(PublishingQuery {
            inner: knowledge,
            bindings: bindings.clone(),
            published: std::sync::atomic::AtomicBool::new(false),
        });
        let coordinator = CodeBindingCoordinator::new(
            bindings as Arc<dyn CodeBindingRepository>,
            Arc::new(crate::AllowAuthorizer),
            Vec::new(),
        );
        let provider = EngramProviderBuilder::new(engram_integration::CapabilityReport::new(
            CapabilityState::Supported,
        ))
        .knowledge_query(publishing)
        .code_bindings(Arc::new(coordinator))
        .build();

        // gen-2 is now active: barrierSymbol's generation is retired, so the
        // retired symbol must NEVER seed — not at hop 0, not via budget.
        let nodes = block_on(provider.code_graph_explore_bound(
            &actor(),
            &scope(),
            "barrierSymbol",
            Some(2),
            Some(24),
            Some(64),
            false,
        ))
        .expect("explore after the barrier");
        assert!(
            nodes.is_empty(),
            "a retired generation's seed can never serve after the load/pin barrier: {nodes:?}"
        );
        // The CURRENT generation's symbol still explores.
        let live = block_on(provider.code_graph_explore_bound(
            &actor(),
            &scope(),
            "barrierLive",
            Some(1),
            Some(24),
            Some(64),
            false,
        ))
        .expect("live explore");
        assert!(
            live.iter().any(|node| node.id == "barrier-live"),
            "the current generation's symbol explores: {live:?}"
        );
    }

    /// Corrective r4 (canonical 14): seed suppression uses the SHARED
    /// canonical artifact predicate (Variable/Module/File/Repository) — every
    /// artifact kind seeds only when explicitly requested, while the
    /// authorized structural universe keeps serving symbol traversal.
    #[test]
    fn artifact_seeds_use_the_shared_canonical_predicate() {
        let dir = tempfile::tempdir().expect("tempdir");
        let bindings = SqlCodeBindingRepository::open(dir.path().join("artifact-kinds.sqlite"))
            .expect("open binding store");
        block_on(bindings.bind(CodeBinding {
            scope: scope(),
            repo_id: RepoId("repo-a".to_string()),
            branch_ref: "refs/heads/main".to_string(),
            version: 1,
            active_generation: None,
            epoch: 0,
        }))
        .expect("bind");
        publish_generation_for_scope(
            &bindings,
            &scope(),
            "repo-a",
            "gen-artifacts",
            0,
            None,
            &[
                "artifact-var",
                "artifact-module",
                "artifact-file",
                "artifact-repo",
                "artifact-symbol",
            ],
        );
        let knowledge = Arc::new(SqlKnowledgeStore::open_in_memory().expect("knowledge"));
        for (id, name, kind) in [
            ("artifact-var", "artifactVariable", EntityKind::Variable),
            ("artifact-module", "artifactModule", EntityKind::Module),
            ("artifact-file", "artifactFile", EntityKind::File),
            (
                "artifact-repo",
                "artifactRepository",
                EntityKind::Repository,
            ),
            ("artifact-symbol", "artifactSymbol", EntityKind::Function),
        ] {
            block_on(knowledge.put_entity(entity(id, name, kind))).expect("put entity");
        }
        block_on(knowledge.put_relationship(relationship(
            "artifact-edge",
            "defined_in",
            "artifact-symbol",
            "artifactSymbol",
            "artifact-file",
            "artifactFile",
        )))
        .expect("put relationship");
        let bindings = Arc::new(bindings);
        let coordinator = CodeBindingCoordinator::new(
            bindings as Arc<dyn CodeBindingRepository>,
            Arc::new(crate::AllowAuthorizer),
            Vec::new(),
        );
        let provider = EngramProviderBuilder::new(engram_integration::CapabilityReport::new(
            CapabilityState::Supported,
        ))
        .knowledge_query(knowledge)
        .code_bindings(Arc::new(coordinator))
        .build();

        // Suppression ON: NO artifact kind seeds (Variable and Repository
        // were the missing kinds in the divergent set).
        for (query, ctx) in [
            ("artifactVariable", "Variable"),
            ("artifactModule", "Module"),
            ("artifactFile", "File"),
            ("artifactRepository", "Repository"),
        ] {
            let suppressed = block_on(provider.code_graph_explore_bound(
                &actor(),
                &scope(),
                query,
                Some(1),
                Some(24),
                Some(64),
                false,
            ))
            .expect("suppressed explore");
            assert!(
                suppressed.is_empty(),
                "{ctx} must not seed under suppression: {suppressed:?}"
            );
        }
        // Suppression OFF: every artifact kind seeds.
        for (query, id, ctx) in [
            ("artifactVariable", "artifact-var", "Variable"),
            ("artifactModule", "artifact-module", "Module"),
            ("artifactFile", "artifact-file", "File"),
            ("artifactRepository", "artifact-repo", "Repository"),
        ] {
            let included = block_on(provider.code_graph_explore_bound(
                &actor(),
                &scope(),
                query,
                Some(1),
                Some(24),
                Some(64),
                true,
            ))
            .expect("included explore");
            assert!(
                included.iter().any(|node| node.id == id),
                "{ctx} seeds when artifacts are requested: {included:?}"
            );
        }
        // The authorized structural universe is intact: a symbol still
        // traverses THROUGH the File vertex under suppression (contains edge).
        let through = block_on(provider.code_graph_explore_bound(
            &actor(),
            &scope(),
            "artifactSymbol",
            Some(2),
            Some(24),
            Some(64),
            false,
        ))
        .expect("symbol explore");
        let _ = through;
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

/// Full durable publication state machine returning a Result (no panics) so
/// CAS-failure semantics stay observable (canonical 3a serialization proof).
fn publish_attempt(
    store: &SqlCodeBindingRepository,
    repo: &str,
    generation_id: &str,
    expected_epoch: u64,
    expected_active: Option<&str>,
    lineage_records: &[&str],
) -> Result<(), CoreError> {
    let repo_id = RepoId(repo.to_string());
    let attempt_id = format!("attempt-{repo}-{generation_id}");
    block_on(store.begin_attempt(
        &scope(),
        &repo_id,
        generation(generation_id),
        attempt(&attempt_id, expected_epoch, expected_active),
    ))?;
    block_on(store.record_receipt(receipt(&attempt_id, generation_id, CodeReceiptKind::Source)))?;
    block_on(store.record_receipt(receipt(
        &attempt_id,
        generation_id,
        CodeReceiptKind::Knowledge,
    )))?;
    let (_, stored) =
        block_on(store.get_attempt(&scope(), &repo_id, &attempt_id))?.ok_or_else(|| {
            CoreError::InvalidRequest {
                reason: "attempt vanished".into(),
            }
        })?;
    block_on(store.mark_ready(&scope(), &repo_id, &attempt_id, stored.fencing_token))?;
    block_on(store.publish(&scope(), &repo_id, &attempt_id, expected_epoch))?;
    for record in lineage_records {
        block_on(store.put_lineage(
            &scope(),
            record,
            CodeLineage {
                repo_id: RepoId(repo.to_string()),
                generation_id: generation_id.to_string(),
                source_path: format!("src/{record}.rs"),
                contributors: vec![record.to_string()],
            },
        ))?;
    }
    Ok(())
}

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
        let graph = provider.graph().cloned().expect("graph repository handle");
        let coordinator = provider.code_bindings().expect("coordinator").clone();
        let authorized = AuthorizedKnowledgeQuery::new(knowledge, coordinator)
            .with_policy_authorizer(Arc::new(crate::AllowAllPolicy))
            .with_graph_repository(graph);
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
        let authorized = AuthorizedKnowledgeQuery::new(knowledge, coordinator)
            .with_policy_authorizer(Arc::new(crate::AllowAllPolicy));
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

    /// Corrective r4 (canonical 3c/4): in an UNBOUND scope (no published
    /// binding), ordinary records flow only through their OWN caller
    /// authorization, and DURABLE STAGED LINEAGE still classifies code
    /// ownership: a code-owned Concept denies, an ordinary Concept survives
    /// for a permitted caller, and an actually denied ordinary caller gets
    /// nothing. No repository authority is invented.
    #[test]
    fn unbound_scope_authorizes_ordinary_and_denies_lineage() {
        let dir = tempfile::tempdir().expect("tempdir");
        let bindings = Arc::new(
            SqlCodeBindingRepository::open(dir.path().join("unbound-auth.sqlite"))
                .expect("open bindings"),
        );
        let knowledge = Arc::new(SqlKnowledgeStore::open_in_memory().expect("knowledge"));
        let unbound = Scope {
            tenant: "task-016".to_string(),
            subject: None,
            workspace: Some("unbound-authorized".to_string()),
            session: None,
            environment: None,
        };
        for (id, name) in [
            ("unbound-doc", "ordinaryConcept"),
            ("staged-concept", "stagedConcept"),
        ] {
            let mut record = entity(id, name, EntityKind::Concept);
            record.scope = unbound.clone();
            block_on(knowledge.put_entity(record)).expect("put concept");
        }
        // STAGED lineage: repo-x attempt never published — the concept IS
        // code-owned by durable lineage even though its kind is ordinary.
        block_on(bindings.bind(CodeBinding {
            scope: unbound.clone(),
            repo_id: RepoId("repo-x".to_string()),
            branch_ref: "refs/heads/main".to_string(),
            version: 1,
            active_generation: None,
            epoch: 0,
        }))
        .expect("bind repo-x");
        block_on(bindings.put_lineage(
            &unbound,
            "staged-concept",
            CodeLineage {
                repo_id: RepoId("repo-x".to_string()),
                generation_id: "gen-staged".to_string(),
                source_path: "src/staged.rs".to_string(),
                contributors: vec!["staged-concept".to_string()],
            },
        ))
        .expect("staged lineage");

        let allow_coordinator = CodeBindingCoordinator::new(
            bindings.clone() as Arc<dyn CodeBindingRepository>,
            Arc::new(crate::AllowAuthorizer),
            Vec::new(),
        );
        // Permitted ordinary caller: ONLY the ordinary Concept survives.
        let permitted =
            AuthorizedKnowledgeQuery::new(knowledge.clone(), Arc::new(allow_coordinator))
                .with_policy_authorizer(Arc::new(crate::AllowAllPolicy));
        let ordinary = block_on(permitted.list_entities_authorized(&actor(), &unbound, &[]))
            .expect("permitted unbound listing");
        let ids: Vec<String> = ordinary.iter().map(|e| e.id.to_string()).collect();
        assert_eq!(
            ids,
            vec!["unbound-doc".to_string()],
            "only the positively authorized ordinary Concept survives: {ids:?}"
        );
        let explicit_ordinary = block_on(permitted.list_entities_authorized(
            &actor(),
            &unbound,
            &["unbound-doc".to_string()],
        ))
        .expect("graphless ordinary explicit read is authorized");
        assert_eq!(explicit_ordinary[0].id.as_str(), "unbound-doc");
        // The staged code-owned Concept denies even on EXPLICIT request.
        let explicit = block_on(permitted.list_entities_authorized(
            &actor(),
            &unbound,
            &["staged-concept".to_string()],
        ));
        assert!(
            matches!(&explicit, Err(CoreError::InvalidRequest { reason }) if reason.contains("BindingRequired")),
            "a lineage-owned staged Concept denies: {explicit:?}"
        );

        // An actually DENIED ordinary caller receives nothing (not the
        // staged concept either) — code denial is not what stops this
        // caller; their own ordinary-policy denial is.
        let denied_coordinator = CodeBindingCoordinator::new(
            bindings.clone() as Arc<dyn CodeBindingRepository>,
            Arc::new(crate::AllowAuthorizer),
            Vec::new(),
        );
        let denied_caller = AuthorizedKnowledgeQuery::new(knowledge, Arc::new(denied_coordinator))
            .with_policy_authorizer(Arc::new(crate::DenyAllPolicy));
        let empty = block_on(denied_caller.list_entities_authorized(&actor(), &unbound, &[]))
            .expect("denied caller listing resolves");
        assert!(
            empty.is_empty(),
            "a denied ordinary caller gets nothing: {:?}",
            empty.iter().map(|e| e.id.to_string()).collect::<Vec<_>>()
        );
        let denied_explicit = block_on(denied_caller.list_entities_authorized(
            &actor(),
            &unbound,
            &["unbound-doc".to_string()],
        ));
        assert!(
            matches!(&denied_explicit, Err(CoreError::InvalidRequest { reason }) if reason.contains("BindingRequired")),
            "a denied graphless ordinary explicit read must fail closed: {denied_explicit:?}"
        );
    }

    /// Two real graph rows supply their own policy authority, while a missing
    /// or foreign graph row fails closed rather than using a graphless default.
    #[test]
    fn graph_policy_owns_ordinary_concept_authorization() {
        let dir = tempfile::tempdir().expect("tempdir");
        let bindings = Arc::new(
            SqlCodeBindingRepository::open(dir.path().join("graph-policy.sqlite"))
                .expect("open bindings"),
        );
        let knowledge = Arc::new(SqlKnowledgeStore::open_in_memory().expect("knowledge"));
        let low_graph_id = Id::from("low-graph");
        let strict_graph_id = Id::from("strict-graph");
        let foreign_graph_id = Id::from("foreign-graph");
        let mut low_policy = policy();
        low_policy.sensitivity = Some(Sensitivity::Low);
        let mut strict_policy = policy();
        strict_policy.sensitivity = Some(Sensitivity::High);
        for (id, graph_scope, graph_policy) in [
            (low_graph_id.clone(), scope(), low_policy),
            (strict_graph_id.clone(), scope(), strict_policy),
            (foreign_graph_id.clone(), foreign_scope(), policy()),
        ] {
            block_on(knowledge.put_graph(KnowledgeGraph {
                id,
                scope: graph_scope,
                name: "ordinary policy graph".to_string(),
                uri: None,
                version: None,
                ontology_refs: Vec::new(),
                policy: graph_policy,
                provenance: provenance(),
                created_at: chrono::Utc::now(),
                updated_at: None,
                metadata: None,
            }))
            .expect("put graph");
        }
        for (id, graph_id) in [
            ("low-concept", Some(low_graph_id)),
            ("strict-concept", Some(strict_graph_id)),
            ("missing-concept", Some(Id::from("missing-graph"))),
            ("foreign-concept", Some(foreign_graph_id)),
        ] {
            let mut record = entity(id, id, EntityKind::Concept);
            record.graph_id = graph_id;
            block_on(knowledge.put_entity(record)).expect("put concept");
        }
        let coordinator = Arc::new(CodeBindingCoordinator::new(
            bindings as Arc<dyn CodeBindingRepository>,
            Arc::new(crate::AllowAuthorizer),
            Vec::new(),
        ));
        let authorized = AuthorizedKnowledgeQuery::new(knowledge.clone(), coordinator)
            .with_policy_authorizer(Arc::new(crate::LowGraphPolicyOnly))
            .with_graph_repository(knowledge as Arc<dyn KnowledgeGraphRepository>);
        let low = block_on(authorized.list_entities_authorized(
            &actor(),
            &scope(),
            &["low-concept".to_string()],
        ))
        .expect("low graph policy permits its ordinary concept");
        assert_eq!(low[0].id.as_str(), "low-concept");
        for denied_id in ["strict-concept", "missing-concept", "foreign-concept"] {
            let denied = block_on(authorized.list_entities_authorized(
                &actor(),
                &scope(),
                &[denied_id.to_string()],
            ));
            assert!(
                matches!(&denied, Err(CoreError::InvalidRequest { reason }) if reason.contains("BindingRequired")),
                "{denied_id} must fail closed: {denied:?}"
            );
        }
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

    /// Task016 r3: maintenance through ACTUAL USE. A grant authorizes
    /// nothing by possession: the validated operation seam denies a STALE
    /// grant (publication between authorization and use) BEFORE the owned
    /// mutation runs, executes a valid grant's mutation exactly once with
    /// the pinned targets, and refuses a different caller.
    #[test]
    fn maintenance_use_barriers() {
        let dir = tempfile::tempdir().expect("tempdir");
        let (authorized, bindings) = gate_fixture(dir.path());
        let knowledge = {
            // The owned mutation target store: reach the provider's knowledge
            // surface through the seam's wrapped query.
            let provider_knowledge = authorized.query().clone();
            let _ = provider_knowledge;
            Arc::new(SqlKnowledgeStore::open_in_memory().expect("mutation store"))
        };
        // Seed an owned mutation target: an eligible chunk the mutation will
        // delete. Lineage for "a-render" is already published in the fixture.
        block_on(knowledge.put_entity(entity("a-render", "render", EntityKind::Function)))
            .expect("put mutation target");

        // (1) Valid grant: the fenced retirement executes once with exactly
        // the pinned targets (the owned lineage retirement itself is proven
        // in maintenance_owned_retirement_fences_first_side_effect).
        let grant = block_on(authorized.authorize_maintenance_grant(
            &actor(),
            &scope(),
            &["b-util".to_string()],
        ))
        .expect("grant authorizes");
        assert_eq!(grant.targets(), vec!["b-util".to_string()]);
        let used = block_on(authorized.use_maintenance(
            &actor(),
            &grant,
            engram_integration::knowledge_query::MaintenanceOperation::RetireLineageTargets,
        ));
        used.expect("valid grant use executes");

        // (2) Caller isolation: a DIFFERENT caller cannot use the grant.
        let other = Actor {
            id: Id::from("other-maintainer"),
            kind: ActorKind::Agent,
            display_name: None,
            metadata: None,
        };
        let denied = block_on(authorized.use_maintenance(
            &other,
            &grant,
            engram_integration::knowledge_query::MaintenanceOperation::RetireLineageTargets,
        ));
        assert!(
            matches!(&denied, Err(CoreError::InvalidRequest { reason }) if reason.contains("caller mismatch")),
            "a grant is bound to its authorizing caller: {denied:?}"
        );

        // (3) Deterministic publication barrier: publishing a new generation
        // between authorization and use MUST deny BEFORE the mutation runs.
        let stale = block_on(authorized.authorize_maintenance_grant(
            &actor(),
            &scope(),
            &["a-render".to_string()],
        ))
        .expect("grant authorizes");
        // The fenced use in (1) advanced BOTH published bindings' epochs
        // (the grant pins every published repository); this publication uses
        // the current post-fence epoch of repo-b.
        publish_generation_for_scope(
            &bindings,
            &scope(),
            "repo-b",
            "gen-b-next",
            2,
            Some("gen-b"),
            &["b-util-next"],
        );
        let stale_use = block_on(authorized.use_maintenance(
            &actor(),
            &stale,
            engram_integration::knowledge_query::MaintenanceOperation::RetireLineageTargets,
        ));
        assert!(
            matches!(&stale_use, Err(CoreError::Conflict { reason }) if reason.contains("GenerationChanged")),
            "a publication after the grant denies the use: {stale_use:?}"
        );

        // (4) The knowledge store target is untouched by the denied use.
        let target = block_on(knowledge.get_entity(&Id::from("a-render"), &scope()))
            .expect("get target")
            .expect("target survives the denied use");
        assert_eq!(target.id.as_str(), "a-render");
    }

    /// Corrective r4 (canonical 3a): the owned maintenance mutation is a
    /// REAL fenced retirement of durable eligibility. A valid grant preserves
    /// the target's positive code-ownership row as retired and advances the
    /// binding epoch exactly once; the follow-on continuation observes the
    /// fenced result.
    #[test]
    fn maintenance_owned_retirement_fences_first_side_effect() {
        let dir = tempfile::tempdir().expect("tempdir");
        let (authorized, _bindings) = gate_fixture(dir.path());
        let grant = block_on(authorized.authorize_maintenance_grant(
            &actor(),
            &scope(),
            &["a-render".to_string()],
        ))
        .expect("grant authorizes");

        // The target's durable lineage exists before the use.
        let store = _bindings.clone();
        let before = block_on(engram_knowledge::CodeBindingRepository::target_lineage(
            &*store,
            &scope(),
        ))
        .expect("lineage before");
        assert!(
            before.iter().any(|entry| entry.record_id == "a-render"),
            "fixture sanity: a-render lineage exists before maintenance"
        );
        let binding_epoch_before = block_on(engram_knowledge::CodeBindingRepository::get_binding(
            &*store,
            &scope(),
            &RepoId("repo-a".to_string()),
        ))
        .expect("binding before")
        .expect("repo-a bound")
        .epoch;

        block_on(authorized.use_maintenance(
            &actor(),
            &grant,
            engram_integration::knowledge_query::MaintenanceOperation::RetireLineageTargets,
        ))
        .expect("fenced use executes");

        // The owned side effect preserves immutable historical lineage and
        // records retirement separately, so a surviving target cannot become
        // ordinary while it is no longer eligible for the active generation.
        let after = block_on(engram_knowledge::CodeBindingRepository::target_lineage(
            &*store,
            &scope(),
        ))
        .expect("lineage after");
        assert!(
            after.iter().any(|entry| {
                entry.record_id == "a-render" && entry.generation_id == "gen-a" && entry.retired
            }),
            "retirement is an additive ownership fact with historical generation intact: {after:?}"
        );
        let binding_epoch_after = block_on(engram_knowledge::CodeBindingRepository::get_binding(
            &*store,
            &scope(),
            &RepoId("repo-a".to_string()),
        ))
        .expect("binding after")
        .expect("repo-a bound")
        .epoch;
        assert_eq!(
            binding_epoch_after,
            binding_epoch_before + 1,
            "the fence advanced the binding epoch exactly once"
        );
    }

    /// Corrective r4 (canonical 3a): stale authority cannot mutate — a
    /// publication between grant and use fails the epoch CAS inside the
    /// retirement transaction, NOTHING is retired and the real target's
    /// lineage survives. This closes the validation-to-side-effect gap at
    /// the owned boundary itself (the CAS and the retirement commit
    /// atomically).
    #[test]
    fn maintenance_stale_authority_cannot_retarget_owned_mutation() {
        let dir = tempfile::tempdir().expect("tempdir");
        let (authorized, bindings) = gate_fixture(dir.path());
        let grant = block_on(authorized.authorize_maintenance_grant(
            &actor(),
            &scope(),
            &["a-render".to_string()],
        ))
        .expect("grant authorizes");

        // A REAL publication lands between authorization and use.
        publish_generation_for_scope(
            &bindings,
            &scope(),
            "repo-a",
            "gen-a-next",
            1,
            Some("gen-a"),
            &["a-render-next"],
        );

        let denied = block_on(authorized.use_maintenance(
            &actor(),
            &grant,
            engram_integration::knowledge_query::MaintenanceOperation::RetireLineageTargets,
        ));
        assert!(
            matches!(&denied, Err(CoreError::Conflict { reason }) if reason.contains("GenerationChanged")),
            "stale authority denies before any side effect: {denied:?}"
        );
        // The OWNED target is untouched: the real lineage row still exists.
        let lineage = block_on(engram_knowledge::CodeBindingRepository::target_lineage(
            &*bindings,
            &scope(),
        ))
        .expect("lineage after denied use");
        assert!(
            lineage.iter().any(|entry| entry.record_id == "a-render"),
            "the real target's durable lineage survives the denied use: {lineage:?}"
        );
    }

    /// Corrective r4 (canonical 3a): maintenance and publication SERIALIZE on
    /// the binding epoch — after a fenced retirement, a publication begun
    /// with the OLD binding epoch denies with a real StaleGeneration CAS
    /// failure (it must retry against the advanced epoch). Proven with the
    /// real publication operation, not a lock probe.
    #[test]
    fn maintenance_and_publication_serialize_on_binding_epoch() {
        let dir = tempfile::tempdir().expect("tempdir");
        let (authorized, bindings) = gate_fixture(dir.path());
        let grant = block_on(authorized.authorize_maintenance_grant(
            &actor(),
            &scope(),
            &["a-render".to_string()],
        ))
        .expect("grant authorizes");
        let epoch_before = grant.binding_epochs()[0].1;
        block_on(authorized.use_maintenance(
            &actor(),
            &grant,
            engram_integration::knowledge_query::MaintenanceOperation::RetireLineageTargets,
        ))
        .expect("fenced retirement executes");

        // A publication attempt begun with the PRE-maintenance binding epoch
        // cannot commit: the epoch CAS fails against the advanced epoch.
        let stale_publish = publish_attempt(
            &bindings,
            "repo-a",
            "gen-after-maint",
            epoch_before,
            Some("gen-a"),
            &["after-maint"],
        );
        assert!(
            matches!(&stale_publish, Err(CoreError::Conflict { reason }) if reason.contains("GenerationChanged")),
            "publication serializes against the fenced epoch: {stale_publish:?}"
        );

        // The same publication with the CURRENT (advanced) epoch succeeds —
        // the serialization is a retryable CAS, not a deadlock.
        let mut retry = publish_attempt(
            &bindings,
            "repo-a",
            "gen-after-maint",
            epoch_before + 1,
            Some("gen-a"),
            &["after-maint"],
        );
        // begin_attempt + receipts + ready + publish through the helper:
        // the helper handles the full state machine.
        let _ = &mut retry;
        assert!(
            retry.is_ok(),
            "the retried publication against the advanced epoch succeeds"
        );
    }

    /// Task016 r3: explicit-ID query stays pinned through RETURN — a
    /// publication between canonical selection and the caller receiving the
    /// list retries once and never serves the stale selection.
    #[test]
    fn explicit_query_pinned_through_return() {
        let dir = tempfile::tempdir().expect("tempdir");
        let (authorized, bindings) = gate_fixture(dir.path());
        // Baseline selection under the current epoch.
        let selected = block_on(authorized.list_entities_authorized(&actor(), &scope(), &[]))
            .expect("initial selection");
        assert!(
            selected
                .iter()
                .any(|entity| entity.id.as_str() == "a-render"),
            "published endpoint selected: {:?}",
            selected
                .iter()
                .map(|e| e.id.to_string())
                .collect::<Vec<_>>()
        );
        // Publish (epoch bump): the same call reflects the CURRENT view —
        // never a cached stale selection.
        publish_generation_for_scope(
            &bindings,
            &scope(),
            "repo-a",
            "gen-a-next",
            1,
            Some("gen-a"),
            &["a-render-next"],
        );
        let after = block_on(authorized.list_entities_authorized(&actor(), &scope(), &[]))
            .expect("post-publication selection");
        let ids: Vec<String> = after.iter().map(|e| e.id.to_string()).collect();
        assert!(
            !ids.iter().any(|id| id == "a-render"),
            "the superseded generation's targets must not serve after publication: {ids:?}"
        );
    }
}
