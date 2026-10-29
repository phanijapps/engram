//! TASK-013 focused proof (corrective r2): retrieval-lane eligibility applies
//! BEFORE ranking, limits and budgets (REQ-005 / AC-006), with DURABLE
//! TARGET LINEAGE as the classification authority (finding 4) and the REAL
//! owned candidate paths constrained (finding 5).
//!
//! Every case drives REAL owned behavior:
//! - the real `SqlKnowledgeStore` + real file-backed `SqlCodeBindingRepository`
//!   (published vs staged generations, DURABLE staged lineage records),
//! - the real persistent Tantivy `LexicalIndex` (contributor-keyed documents),
//! - the real bound lexical lane (`lexical_recall_lane_bound`) and the real
//!   `SqlUnifiedRecall` with `with_code_eligibility` (facts filter + post-lane
//!   guard + rerank/compose inputs),
//! - the REAL sqlite-vec vector lane (`vector_recall_lane_bound`) with the
//!   query-embedding port stubbed only at the model boundary,
//! - the real `SqlMemoryService` facts lane and `SqlBeliefStore` beliefs lane.
//!
//! Decisive corrective cases: the ineligible candidate always OUTSCORES the
//! eligible one unconstrained; top-K = 1 on the real vector lane; repo-B
//! crowd-out vs a repo-A narrowing on the lexical collector; authority
//! failure fails closed (finding 6); forged metadata stamps do nothing
//! (finding 4).

use std::sync::{Arc, Mutex};

use engram_conformance::SqlUnifiedRecall;
use engram_domain::*;
use engram_integration::UnifiedRecall;
use engram_integration::code_binding::RepositoryEligibilitySource;
use engram_knowledge::{
    CodeBindingRepository, KnowledgeGraphRepository, KnowledgeRepository, StageReceipt,
};
use engram_memory::MemoryService;
use engram_retrieval::{RetrievalIndex, RetrievalReranker};
use engram_runtime::{CoreError, CoreResult};
use engram_store_lexical::LexicalIndex;
use engram_store_sqlite::SqlBeliefStore;
use engram_store_sqlite::SqlMemoryService;
use engram_store_sqlite::knowledge::{SqlCodeBindingRepository, SqlKnowledgeStore};
use futures::executor::block_on;

// ---------- shared fixture -------------------------------------------------

fn scope() -> Scope {
    Scope {
        tenant: "task-013".to_string(),
        subject: None,
        workspace: Some("bound-retrieval".to_string()),
        session: None,
        environment: None,
    }
}

fn actor() -> Actor {
    Actor {
        id: Id::from("task-013-agent"),
        kind: ActorKind::Agent,
        display_name: None,
        metadata: None,
    }
}

fn requester() -> Requester {
    Requester {
        actor: actor(),
        roles: Vec::new(),
        permissions: vec!["memory.write".to_string(), "memory.retrieve".to_string()],
        on_behalf_of: None,
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

fn provenance() -> Provenance {
    Provenance {
        source: "task-013-fixture".to_string(),
        actor: actor(),
        observed_at: chrono::Utc::now(),
        evidence: Vec::new(),
        derivations: Vec::new(),
        confidence: Some(1.0),
        method: Some("test".to_string()),
    }
}

fn request(query: &str, limit: u32) -> RetrievalRequest {
    RetrievalRequest {
        query: query.to_string(),
        scope: scope(),
        requester: requester(),
        modes: Vec::new(),
        filters: None,
        cues: Vec::new(),
        limit: Some(limit),
        budget: None,
        include_explanations: None,
    }
}

fn empty_filter_with_keys(keys: Option<Vec<String>>) -> QueryFilter {
    QueryFilter {
        memory_kinds: Vec::new(),
        source_kinds: Vec::new(),
        chunk_kinds: Vec::new(),
        concept_ids: Vec::new(),
        entity_ids: Vec::new(),
        since: None,
        until: None,
        code_index_keys: keys,
        min_confidence: None,
        include_archived: None,
        as_of: None,
        include_code_artifacts: None,
    }
}

fn chunk(id: &str, text: &str, kind: KnowledgeChunkKind) -> KnowledgeChunk {
    KnowledgeChunk {
        id: Id::from(id),
        document_id: Id::from("document-task-013"),
        // Review r5 (finding 7): the ordinary-chunk fixtures live on an
        // UNBOUND source; the repository-bound `source-task-013` (no settled
        // lineage) is deliberately the fail-closed negative.
        source_id: Id::from("source-ordinary-task-013"),
        kind,
        text: text.to_string(),
        summary: None,
        location: None,
        entities: Vec::new(),
        concepts: Vec::new(),
        embedding_refs: Vec::new(),
        content_hash: format!("sha256:{id}"),
        provenance: provenance(),
        policy: policy(),
        created_at: chrono::Utc::now(),
        updated_at: None,
        // Corrective r2 (finding 4): metadata stamps are NOT authority.
        metadata: None,
    }
}

fn symbol_entity(id: &str, name: &str) -> KnowledgeEntity {
    KnowledgeEntity {
        id: Id::from(id),
        graph_id: Some(Id::from("graph-task-013")),
        kind: EntityKind::Function,
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
/// machine, then records the DURABLE LINEAGE for `record_ids`.
fn publish_with_lineage(
    store: &SqlCodeBindingRepository,
    repo: &str,
    generation_id: &str,
    expected_epoch: u64,
    expected_active: Option<&str>,
    record_ids: &[&str],
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
    for record in record_ids {
        block_on(store.put_lineage(
            &scope(),
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

/// Real seeding: published repo-a (gen-a) + staged repo-b (gen-b), eligible
/// + ineligible code chunks/entities, ordinary documents, a legacy unbound
/// symbol, and a REAL Tantivy index fed with the same contributor keys the
/// bound feed port uses.
struct Fixture {
    memory: Arc<SqlMemoryService>,
    beliefs: Arc<SqlBeliefStore>,
    knowledge: Arc<SqlKnowledgeStore>,
    lexical: Arc<LexicalIndex>,
    bindings: Arc<SqlCodeBindingRepository>,
    memory_id: Id,
}

fn fixture(dir: &std::path::Path) -> Fixture {
    let memory = Arc::new(SqlMemoryService::open_in_memory().expect("memory open"));
    let beliefs = Arc::new(SqlBeliefStore::open_in_memory().expect("beliefs open"));
    let knowledge = Arc::new(SqlKnowledgeStore::open_in_memory().expect("knowledge open"));
    block_on(knowledge.put_graph(KnowledgeGraph {
        id: Id::from("graph-task-013"),
        scope: scope(),
        name: "task-013 ordinary graph".to_string(),
        uri: None,
        version: None,
        ontology_refs: Vec::new(),
        policy: policy(),
        provenance: provenance(),
        created_at: chrono::Utc::now(),
        updated_at: None,
        metadata: None,
    }))
    .expect("put canonical graph");

    // Durable binding store: repo-a PUBLISHED at gen-a; repo-b staged only.
    let bindings =
        SqlCodeBindingRepository::open(dir.join("bindings.sqlite")).expect("open binding store");
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
    // Two published generations of repo-a: gen-a-1 first (owns the
    // SUPERSEDED chunk's lineage), then gen-a-2 (the active generation).
    // After gen-a-2 publishes, gen-a-1's records are stale-but-durable.
    publish_with_lineage(&bindings, "repo-a", "gen-a-1", 0, None, &["chunk-stale"]);
    publish_with_lineage(
        &bindings,
        "repo-a",
        "gen-a-2",
        1,
        Some("gen-a-1"),
        &["chunk-eligible", "entity-published"],
    );
    // repo-b: staged attempt + lineage, NEVER published.
    {
        let repo_id = RepoId("repo-b".to_string());
        block_on(bindings.begin_attempt(
            &scope(),
            &repo_id,
            generation("gen-b"),
            attempt("attempt-repo-b-gen-b", 0, None),
        ))
        .expect("begin repo-b attempt");
        block_on(bindings.put_lineage(
            &scope(),
            "chunk-repo-b",
            CodeLineage {
                repo_id: RepoId("repo-b".to_string()),
                generation_id: "gen-b".to_string(),
                source_path: "src/crowd.rs".to_string(),
                contributors: vec!["chunk-repo-b".to_string()],
            },
        ))
        .expect("repo-b lineage");
    }
    let bindings = Arc::new(bindings);

    // Source + document rows so chunk visibility resolves (scope join).
    block_on(knowledge.put_source(KnowledgeSource {
        id: Id::from("source-task-013"),
        kind: SourceKind::GitRepository,
        scope: scope(),
        name: "task-013-fixture-source".to_string(),
        uri: None,
        version: None,
        repository_key: Some("repo-a".to_string()),
        git_ref: Some("refs/heads/main".to_string()),
        policy: policy(),
        provenance: provenance(),
        created_at: chrono::Utc::now(),
        updated_at: None,
        metadata: None,
    }))
    .expect("put source");
    // Review r5 (finding 7): the ordinary document source — unbound, so a
    // paragraph chunk of it settles positively ordinary through the source's
    // own classification (repository-bound origins fail closed without
    // settled ownership and have their own decisive cases).
    block_on(knowledge.put_source(KnowledgeSource {
        id: Id::from("source-ordinary-task-013"),
        kind: SourceKind::Upload,
        scope: scope(),
        name: "task-013-ordinary-source".to_string(),
        uri: None,
        version: None,
        repository_key: None,
        git_ref: None,
        policy: policy(),
        provenance: provenance(),
        created_at: chrono::Utc::now(),
        updated_at: None,
        metadata: None,
    }))
    .expect("put ordinary source");
    block_on(knowledge.put_document(SourceDocument {
        id: Id::from("document-task-013"),
        source_id: Id::from("source-task-013"),
        kind: SourceDocumentKind::Code,
        uri: None,
        path: Some("fixture.rs".to_string()),
        title: None,
        mime_type: None,
        language: Some("rust".to_string()),
        version: None,
        content_hash: "sha256:document-task-013".to_string(),
        provenance: provenance(),
        policy: policy(),
        created_at: chrono::Utc::now(),
        updated_at: None,
        metadata: None,
    }))
    .expect("put document");

    // Chunks: the stale/staged/foreign records OUTSCORE the eligible ones.
    block_on(knowledge.put_chunk(chunk(
        "chunk-eligible",
        "parse parse eligible published",
        KnowledgeChunkKind::CodeSymbol,
    )))
    .expect("put eligible chunk");
    block_on(knowledge.put_chunk(chunk(
        "chunk-stale",
        "parse parse parse parse parse staletoken",
        KnowledgeChunkKind::CodeSymbol,
    )))
    .expect("put stale chunk (same published repo, superseded lineage absent)");
    block_on(knowledge.put_chunk(chunk(
        "chunk-repo-b",
        "parse crowd crowd crowd crowd crowd crowd crowd crowd crowd crowd",
        KnowledgeChunkKind::CodeSymbol,
    )))
    .expect("put staged repo-b crowd chunk");
    // Forged metadata (finding 4): stamped with the published generation but
    // NOT in the eligible lineage — wait, it IS listed in gen-a lineage above
    // so `chunk-forged` is eligible; the FORGED case is a chunk WITH stamps
    // but NO lineage at all:
    let mut forged_meta = Metadata::new();
    forged_meta.insert(
        "codeGenerationId".to_string(),
        serde_json::Value::String("gen-a".to_string()),
    );
    forged_meta.insert(
        "codeRepoId".to_string(),
        serde_json::Value::String("repo-a".to_string()),
    );
    let mut forged = chunk(
        "chunk-forged-no-lineage",
        "parse forged forged",
        KnowledgeChunkKind::CodeSymbol,
    );
    forged.metadata = Some(forged_meta);
    block_on(knowledge.put_chunk(forged)).expect("put forged chunk");
    // Unstamped code chunk (legacy unbound code — finding 4).
    block_on(knowledge.put_chunk(chunk(
        "chunk-legacy-code",
        "parse legacy unbound",
        KnowledgeChunkKind::CodeSymbol,
    )))
    .expect("put legacy code chunk");
    // Ordinary document chunk.
    block_on(knowledge.put_chunk(chunk(
        "chunk-doc",
        "parse ordinary note",
        KnowledgeChunkKind::Paragraph,
    )))
    .expect("put doc chunk");

    // Entities: a published symbol + a legacy unbound symbol.
    block_on(knowledge.put_entity(symbol_entity("entity-published", "parsePublished")))
        .expect("put published entity");
    block_on(knowledge.put_entity(symbol_entity("entity-legacy", "parseLegacy")))
        .expect("put legacy entity");

    // Real Tantivy index fed with the same contributor keys the bound feed
    // port stamps (`code:<generation>` / ordinary `doc`).
    let lexical = Arc::new(LexicalIndex::new().expect("lexical index"));
    lexical
        .upsert_batch_stamped(&[
            (
                "chunk-eligible".to_string(),
                "parse parse eligible published".to_string(),
                engram_store_lexical::code_contributor_key("gen-a-2"),
            ),
            (
                "chunk-stale".to_string(),
                "parse parse parse parse parse staletoken".to_string(),
                engram_store_lexical::code_contributor_key("gen-a-1"),
            ),
            (
                "chunk-repo-b".to_string(),
                "parse crowd crowd crowd crowd crowd crowd crowd crowd crowd crowd".to_string(),
                engram_store_lexical::code_contributor_key("gen-b"),
            ),
            (
                "chunk-forged-no-lineage".to_string(),
                "parse forged forged".to_string(),
                engram_store_lexical::code_contributor_key("gen-a-2"),
            ),
            (
                "chunk-legacy-code".to_string(),
                "parse legacy unbound".to_string(),
                "doc".to_string(),
            ),
            (
                "chunk-doc".to_string(),
                "parse ordinary note".to_string(),
                "doc".to_string(),
            ),
            (
                "entity-published".to_string(),
                "parsePublished Function".to_string(),
                engram_store_lexical::code_contributor_key("gen-a-2"),
            ),
            (
                "entity-legacy".to_string(),
                "parseLegacy Function".to_string(),
                "doc".to_string(),
            ),
        ])
        .expect("feed lexical index");

    let written = block_on(memory.write_memory(WriteMemoryRequest {
        kind: MemoryKind::Observation,
        content: MemoryContent {
            text: "parse ordinary memory note".to_string(),
            summary: None,
            entities: Vec::new(),
            language: None,
            format: None,
            structured: None,
            hash: None,
        },
        scope: scope(),
        requester: requester(),
        provenance: provenance(),
        policy: policy(),
        links: Vec::new(),
        verdict: None,
        idempotency_key: None,
    }))
    .expect("write memory");

    Fixture {
        memory,
        beliefs,
        knowledge,
        lexical,
        bindings,
        memory_id: written.record.id.clone(),
    }
}

impl Fixture {
    fn eligibility(&self) -> Arc<RepositoryEligibilitySource> {
        Arc::new(RepositoryEligibilitySource::new(
            self.bindings.clone() as Arc<dyn CodeBindingRepository>
        ))
    }

    /// The REAL bound recall: bound lexical lane + facts/beliefs lanes, with
    /// the resolver wired into the recall composition.
    fn bound_recall(&self) -> SqlUnifiedRecall {
        let lane = engram_integration::sqlite::lexical_recall_lane_bound(
            self.knowledge.clone(),
            self.lexical.clone(),
            self.eligibility() as Arc<dyn engram_retrieval::CodeEligibilityResolver>,
        );
        SqlUnifiedRecall::new(
            self.memory.clone(),
            vec![lane as Arc<dyn RetrievalIndex>],
            self.beliefs.clone(),
        )
        .with_code_eligibility(
            self.eligibility() as Arc<dyn engram_retrieval::CodeEligibilityResolver>
        )
    }

    /// The REAL canonical evidence resolver over the owning repositories,
    /// sharing the memory service's configured OWNING authorizer and the
    /// REAL durable ownership catalog (the fixture's binding store) — no
    /// resolver-invented permissive bypass, no ad-hoc second catalog.
    fn candidate_evidence(
        &self,
    ) -> Arc<engram_integration::candidate_evidence::RepositoryCandidateEvidence> {
        Arc::new(
            engram_integration::candidate_evidence::RepositoryCandidateEvidence::new(
                self.memory.clone() as Arc<dyn engram_memory::MemoryRepository>,
                self.beliefs.clone() as Arc<dyn engram_belief::BeliefRepository>,
                self.knowledge.clone() as Arc<dyn engram_knowledge::KnowledgeRepository>,
                self.memory.retrieval_authorizer(),
                Some(self.bindings.clone() as Arc<dyn engram_knowledge::CodeBindingRepository>),
                Some(self.knowledge.clone() as Arc<dyn engram_knowledge::KnowledgeGraphRepository>),
            ),
        )
    }
}

fn target_ids(payload: &ContextPayload) -> Vec<String> {
    payload.items.iter().map(|i| i.target_id.clone()).collect()
}

/// Deterministic query-embedding provider at the model boundary (the vector
/// CELL and lane are real; only the embedding model is a port double).
struct FixedEmbedder;

impl engram_store_sqlite::VectorQueryProvider for FixedEmbedder {
    fn query_vector(&self, request: &RetrievalRequest) -> CoreResult<Vec<f32>> {
        // Encode the query deterministically: dimension 2, x = "stale" bias.
        let stale = request.query.contains("stale") || request.query.contains("nearest");
        Ok(vec![if stale { 1.0 } else { 0.0 }, 1.0])
    }
}

// ---------- task_013 focused cases -----------------------------------------

mod task_013 {
    use super::*;

    /// ineligible_outscores_eligible: stale/foreign/forged/legacy records
    /// that BEAT every eligible candidate unconstrained must never appear in
    /// the fused page, and the eligible candidates must still fill the
    /// request budget. The unconstrained lane is asserted first to prove the
    /// ineligible candidates really outrank the eligible ones.
    #[test]
    fn ineligible_outscores_eligible() {
        let dir = tempfile::tempdir().expect("tempdir");
        let fixture = fixture(dir.path());

        // Contrast: the historical unconstrained lexical lane ranks the
        // ineligible crowd/stale documents FIRST on the shared token.
        let unconstrained_lane = engram_integration::sqlite::lexical_recall_lane(
            fixture.knowledge.clone(),
            fixture.lexical.clone(),
        );
        let unconstrained = block_on(unconstrained_lane.retrieve_candidates(&request("parse", 10)))
            .expect("unconstrained lane");
        assert!(
            !unconstrained
                .first()
                .is_some_and(|hit| hit.target_id == "chunk-eligible"),
            "fixture must actually outrank eligible candidates unconstrained: {:?}",
            unconstrained
                .iter()
                .map(|h| h.target_id.clone())
                .collect::<Vec<_>>()
        );

        let recall = fixture
            .bound_recall()
            .with_candidate_evidence(fixture.candidate_evidence());
        let payload = block_on(recall.recall(request("parse", 2))).expect("bound recall");
        let ids = target_ids(&payload);
        assert!(
            !ids.iter().any(|id| id.contains("stale")
                || id.contains("repo-b")
                || id.contains("forged")
                || id.contains("legacy")),
            "ineligible code must never reach the fused page: {ids:?}"
        );
        assert!(
            ids.contains(&"chunk-eligible".to_string()),
            "published code must be retrievable: {ids:?}"
        );
        assert_eq!(
            payload.items.len(),
            2,
            "the full budget stays available to eligible candidates: {ids:?}"
        );
    }

    /// Corrective r2 (finding 4): a FORGED metadata stamp never makes a
    /// record eligible — the record above carries `codeGenerationId=gen-a`
    /// stamps but NO durable lineage, so it is legacy unbound code and drops.
    #[test]
    fn forged_metadata_stamp_is_not_authority() {
        let dir = tempfile::tempdir().expect("tempdir");
        let fixture = fixture(dir.path());
        let recall = fixture
            .bound_recall()
            .with_candidate_evidence(fixture.candidate_evidence());
        let payload = block_on(recall.recall(request("forged", 10))).expect("recall");
        let ids = target_ids(&payload);
        assert!(
            !ids.contains(&"chunk-forged-no-lineage".to_string()),
            "a forged generation stamp without durable lineage must never serve: {ids:?}"
        );
        // The unstamped legacy code chunk is equally denied.
        let legacy = block_on(recall.recall(request("legacy unbound", 10))).expect("recall");
        let legacy_ids = target_ids(&legacy);
        assert!(
            !legacy_ids.contains(&"chunk-legacy-code".to_string())
                && !legacy_ids.contains(&"entity-legacy".to_string()),
            "unstamped code-kind records are legacy unbound code, never eligible: {legacy_ids:?}"
        );
    }

    /// Corrective r2 (finding 5): the REAL sqlite-vec vector lane constrains
    /// its candidate set BEFORE the top-K — with top-K = 1 and the ineligible
    /// vector NEAREST to the query, the lane still returns the eligible
    /// vector (a post-filter implementation would return nothing).
    #[test]
    fn real_vector_lane_constrains_before_top_k() {
        use engram_store_sqlite::SqliteVectorIndex;
        let dir = tempfile::tempdir().expect("tempdir");
        let fixture = fixture(dir.path());

        // Real sqlite-vec index, dimension 2. The ineligible vectors sit
        // EXACTLY on the query direction; the eligible one is off-axis.
        let index = SqliteVectorIndex::open_in_memory(2).expect("vector index");
        let insert = |id: &str, vector: [f32; 2]| engram_store_sqlite::VectorEntry {
            id: format!("v-{id}"),
            target_type: EmbeddingTargetType::Chunk,
            target_id: id.to_string(),
            model: "test".to_string(),
            dimensions: 2,
            content_hash: String::new(),
            embedding: vector.to_vec(),
        };
        index
            .insert(insert("chunk-stale", [1.0, 0.0]))
            .expect("insert stale");
        index
            .insert(insert("chunk-repo-b", [0.99, 0.01]))
            .expect("insert repo-b");
        index
            .insert(insert("chunk-eligible", [0.0, 1.0]))
            .expect("insert eligible");
        index
            .insert(insert("chunk-doc", [0.0, 0.98]))
            .expect("insert doc");

        // The REAL bound vector lane over the real cell + real store.
        let lane = engram_integration::sqlite::vector_recall_lane_bound(
            fixture.knowledge.clone(),
            index,
            Arc::new(FixedEmbedder),
            fixture.eligibility() as Arc<dyn engram_retrieval::CodeEligibilityResolver>,
        );
        // Query embedding [1,1] (the FixedEmbedder's stale-biased vector):
        // the ineligible stale/repo-b vectors are nearest. Top-K = 1 must
        // still fill from the eligible set — the pre-filter made the
        // ineligible rows invisible to the KNN scan.
        let mut nearest = request("nearest stale", 1);
        nearest.limit = Some(1);
        let results = block_on(lane.retrieve_candidates(&nearest)).expect("vector lane");
        assert_eq!(
            results.len(),
            1,
            "the constrained page must fill from the eligible set: {:?}",
            results
                .iter()
                .map(|r| r.target_id.clone())
                .collect::<Vec<_>>()
        );
        assert_eq!(
            results[0].target_id, "chunk-eligible",
            "the nearest ELIGIBLE vector wins even though ineligible vectors are closer"
        );

        // Direct unconstrained cell search proves the stale vector is
        // genuinely nearest for this embedding.
        let direct = {
            // Direct unconstrained cell search proves the stale vector is
            // genuinely nearest for this embedding.
            let index2 = SqliteVectorIndex::open_in_memory(2).expect("index2");
            let insert = |id: &str, vector: [f32; 2]| engram_store_sqlite::VectorEntry {
                id: format!("v2-{id}"),
                target_type: EmbeddingTargetType::Chunk,
                target_id: id.to_string(),
                model: "test".to_string(),
                dimensions: 2,
                content_hash: String::new(),
                embedding: vector.to_vec(),
            };
            index2
                .insert(insert("chunk-stale", [1.0, 0.0]))
                .expect("insert");
            index2
                .insert(insert("chunk-eligible", [0.0, 1.0]))
                .expect("insert");
            index2.search(&[1.0, 0.0], 1).expect("search")
        };
        assert_eq!(direct[0].target_id, "chunk-stale");
    }

    /// Corrective r2 (finding 5): the request's repository narrowing applies
    /// in the LEXICAL COLLECTOR before the limit — repo-a selected while
    /// repo-b's higher-ranked crowd documents exist: repo-a's targets serve;
    /// and with repo-b explicitly requested, repo-a's targets are excluded
    /// even though they rank higher on the token.
    #[test]
    fn lexical_narrowing_applies_in_the_collector() {
        let dir = tempfile::tempdir().expect("tempdir");
        let fixture = fixture(dir.path());
        let lane = engram_integration::sqlite::lexical_recall_lane_bound(
            fixture.knowledge.clone(),
            fixture.lexical.clone(),
            fixture.eligibility() as Arc<dyn engram_retrieval::CodeEligibilityResolver>,
        );

        // Narrow to repo-a (bare key): repo-b's crowd cannot crowd out
        // repo-a's eligible chunk — the collector never sees gen-b keys.
        let mut narrowed = request("parse crowd", 5);
        narrowed.filters = Some(empty_filter_with_keys(Some(vec!["repo-a".to_string()])));
        let results = block_on(lane.retrieve_candidates(&narrowed)).expect("narrowed lane");
        let ids: Vec<String> = results.iter().map(|r| r.target_id.clone()).collect();
        assert!(
            !ids.iter().any(|id| id.contains("repo-b")),
            "repo-b documents must not enter the collector under a repo-a narrowing: {ids:?}"
        );
        assert!(
            ids.contains(&"chunk-eligible".to_string()),
            "repo-a's eligible target must serve: {ids:?}"
        );
    }

    /// fallback_injection_denied: when only an ineligible target matches the
    /// query, the lane returns nothing and the recall composition injects NO
    /// scope-wide fallback record — and legacy unbound code never leaks
    /// through a documents fallback.
    #[test]
    fn fallback_injection_denied() {
        let dir = tempfile::tempdir().expect("tempdir");
        let fixture = fixture(dir.path());

        let recall = fixture.bound_recall();
        let payload = block_on(recall.recall(request("staletoken", 10))).expect("recall");
        let ids = target_ids(&payload);
        assert!(
            !ids.iter()
                .any(|id| id.contains("stale") || id.contains("legacy") || id.contains("repo-b")),
            "no ineligible code may be injected as a fallback: {ids:?}"
        );
        assert!(
            payload
                .source_failures
                .iter()
                .all(|failure| !failure.source.contains("lexical")),
            "a clean empty lane must not be recorded as a failure: {:?}",
            payload.source_failures
        );

        let legacy = block_on(recall.recall(request("parseLegacy", 10))).expect("recall");
        let legacy_ids = target_ids(&legacy);
        assert!(
            !legacy_ids.contains(&"entity-legacy".to_string()),
            "legacy unbound code must never leak through a documents fallback: {legacy_ids:?}"
        );
    }

    /// non_code_preserved: ordinary documents and memories keep flowing
    /// alongside constrained code. Since A3 the post-lane guard requires the
    /// canonical evidence resolver for any acceptance (a missing resolver
    /// fails closed), so the positive variant attaches it; the missing-
    /// resolver fail-closed behavior is proven by
    /// `missing_evidence_resolver_fails_closed_with_successful_view`.
    #[test]
    fn non_code_preserved() {
        let dir = tempfile::tempdir().expect("tempdir");
        let fixture = fixture(dir.path());

        let recall = fixture
            .bound_recall()
            .with_candidate_evidence(fixture.candidate_evidence());
        let payload = block_on(recall.recall(request("ordinary", 10))).expect("recall");
        let ids = target_ids(&payload);
        assert!(
            ids.contains(&"chunk-doc".to_string()),
            "the ordinary document chunk must stay retrievable: {ids:?}"
        );
        assert!(
            ids.iter().any(|id| id == &fixture.memory_id.to_string()),
            "the ordinary facts-lane memory must stay retrievable: {ids:?}"
        );
    }

    /// Corrective r2 (finding 6) + r3: an eligibility AUTHORITY FAILURE fails
    /// closed through REAL composition — a bound vector lane whose authority
    /// read errors degrades as a lane failure, the historical graph lane
    /// still contributes chunk/entity candidates, and the post-lane evidence
    /// guard (canonical repository evidence + owning authorizer + real
    /// ownership catalog) preserves POSITIVELY AUTHORIZED ordinary records
    /// (document chunk, concept entity, facts-lane memory) while denying
    /// code-dependent/unknown counterparts (stale/staged/forged/legacy code
    /// chunks, symbol entities), with the failures surfaced as source
    /// failures instead of aborting the recall or emptying the page.
    #[test]
    fn authority_failure_fails_closed_preserving_non_code() {
        struct ErroringResolver;
        #[async_trait::async_trait]
        impl engram_retrieval::CodeEligibilityResolver for ErroringResolver {
            async fn resolve(
                &self,
                _scope: &Scope,
            ) -> CoreResult<engram_retrieval::BoundCodeEligibility> {
                Err(CoreError::Adapter {
                    adapter: "test-authority".to_string(),
                    message: "authority unavailable".to_string(),
                })
            }
        }

        let dir = tempfile::tempdir().expect("tempdir");
        let fixture = fixture(dir.path());
        let erroring: Arc<dyn engram_retrieval::CodeEligibilityResolver> =
            Arc::new(ErroringResolver);
        // A positively authorized ORDINARY concept entity: no code kind, no
        // durable lineage, ordinary provenance — the ordinary counterpart
        // that must survive the authority failure.
        let mut concept = symbol_entity("entity-concept", "parseConcept");
        concept.kind = EntityKind::Concept;
        block_on(fixture.knowledge.put_entity(concept)).expect("put concept entity");
        // Corrective r4 (four kinds through the composed recall): an
        // ORDINARY belief (the belief lane is live-subject 0-or-1 per query),
        // a code-DERIVED memory and an UNKNOWN-evidence memory as the
        // counterparts.
        {
            let subject = engram_domain::BeliefSubject {
                key: "parse".to_string(),
                entity_ref: None,
                concept_ref: None,
                aliases: Vec::new(),
            };
            let ordinary_belief = engram_domain::Belief {
                id: engram_domain::BeliefId::from("belief-ordinary-recall"),
                scope: scope(),
                subject,
                content: "parse ordinary belief".to_string(),
                status: engram_domain::BeliefStatus::Active,
                confidence: 0.9,
                sources: Vec::new(),
                valid_from: None,
                valid_until: None,
                superseded_by: None,
                stale: None,
                synthesizer: None,
                reasoning: None,
                embedding_refs: Vec::new(),
                policy: policy(),
                provenance: provenance(),
                created_at: chrono::Utc::now(),
                updated_at: None,
                metadata: None,
            };
            use engram_belief::BeliefRepository as _;
            block_on(fixture.beliefs.put_belief(ordinary_belief)).expect("put ordinary belief");
        }
        let stale_derived = {
            let mut deps = provenance();
            deps.evidence = vec![engram_domain::EvidenceRef {
                target_type: engram_domain::EvidenceTargetType::Chunk,
                target_id: Some("chunk-stale".to_string()),
                uri: None,
                quote: None,
                location: None,
            }];
            block_on(
                fixture
                    .memory
                    .write_memory(engram_domain::WriteMemoryRequest {
                        kind: MemoryKind::Observation,
                        content: engram_domain::MemoryContent {
                            text: "parse derived from stale code".to_string(),
                            summary: None,
                            entities: Vec::new(),
                            language: None,
                            format: None,
                            structured: None,
                            hash: None,
                        },
                        scope: scope(),
                        requester: requester(),
                        provenance: deps,
                        policy: policy(),
                        links: Vec::new(),
                        verdict: None,
                        idempotency_key: None,
                    }),
            )
            .expect("write stale-derived memory")
            .record
        };
        let unknown_evidence = {
            let mut deps = provenance();
            deps.evidence = vec![engram_domain::EvidenceRef {
                target_type: engram_domain::EvidenceTargetType::Memory,
                target_id: Some("memory-never-written".to_string()),
                uri: None,
                quote: None,
                location: None,
            }];
            block_on(
                fixture
                    .memory
                    .write_memory(engram_domain::WriteMemoryRequest {
                        kind: MemoryKind::Observation,
                        content: engram_domain::MemoryContent {
                            text: "parse unknown evidence".to_string(),
                            summary: None,
                            entities: Vec::new(),
                            language: None,
                            format: None,
                            structured: None,
                            hash: None,
                        },
                        scope: scope(),
                        requester: requester(),
                        provenance: deps,
                        policy: policy(),
                        links: Vec::new(),
                        verdict: None,
                        idempotency_key: None,
                    }),
            )
            .expect("write unknown-evidence memory")
            .record
        };

        // REAL bound vector lane whose authority read fails: the lane itself
        // degrades as a source failure (it never widens to an unconstrained
        // KNN page).
        let vector_index =
            engram_store_sqlite::SqliteVectorIndex::open_in_memory(2).expect("vector index");
        let vector_lane = engram_integration::sqlite::vector_recall_lane_bound(
            fixture.knowledge.clone(),
            vector_index,
            Arc::new(FixedEmbedder),
            erroring.clone(),
        );
        // The historical graph lane (deployments without per-lane bindings):
        // contributes chunk + entity candidates the evidence guard classifies.
        let graph_lane: Arc<dyn RetrievalIndex> = Arc::new(
            engram_store_sqlite::knowledge::GraphRetrievalIndex::new(fixture.knowledge.clone()),
        );
        let recall = SqlUnifiedRecall::new(
            fixture.memory.clone(),
            vec![graph_lane, vector_lane as Arc<dyn RetrievalIndex>],
            fixture.beliefs.clone(),
        )
        .with_code_eligibility(erroring)
        .with_candidate_evidence(fixture.candidate_evidence());

        let payload = block_on(recall.recall(request("parse", 10))).expect("recall");
        let ids = target_ids(&payload);
        assert!(
            ids.iter().any(|id| id == "chunk-doc"),
            "the positively authorized ORDINARY document chunk must survive the authority failure: {ids:?}"
        );
        assert!(
            ids.iter().any(|id| id == "belief-ordinary-recall"),
            "the positively authorized ORDINARY belief must survive through the belief lane: {ids:?}"
        );
        assert!(
            ids.iter().any(|id| id == "entity-concept"),
            "the positively authorized ORDINARY concept entity must survive: {ids:?}"
        );
        assert!(
            ids.iter().any(|id| id == &fixture.memory_id.to_string()),
            "the independently authorized non-code memory must survive the authority failure: {ids:?}"
        );
        assert!(
            !ids.iter().any(|id| id.contains("stale")
                || id.contains("repo-b")
                || id.contains("forged")
                || id.contains("legacy")
                || id == "chunk-eligible"
                || id == "entity-published"),
            "code candidates are denied while the authority is unavailable (fail closed): {ids:?}"
        );
        assert!(
            !ids.iter().any(|id| id == stale_derived.id.as_str()),
            "the stale-code-DERIVED memory denies through canonical evidence: {ids:?}"
        );
        assert!(
            !ids.iter().any(|id| id == unknown_evidence.id.as_str()),
            "an UNKNOWN-evidence memory denies (never ordinary): {ids:?}"
        );
        assert!(
            payload
                .source_failures
                .iter()
                .any(|failure| failure.source == "code_eligibility"),
            "the authority failure must be surfaced as a source failure: {:?}",
            payload.source_failures
        );
        assert!(
            payload
                .source_failures
                .iter()
                .any(|failure| failure.source.starts_with("retrieval_lane_")),
            "the erroring bound vector lane must degrade as a lane failure: {:?}",
            payload.source_failures
        );
    }

    /// Canonical candidate-evidence classification (r3): decisive NEW cases
    /// over the REAL owning repositories + owning authorizer + real durable
    /// ownership catalog — bounded cycle-safe dependency evaluation, positive
    /// ordinary origin, payload/revision binding, mismatch/foreign/missing
    /// degradation, and the shared owning authorization decision.
    mod evidence {
        use super::*;
        use engram_belief::BeliefRepository;
        use engram_domain::{
            Belief, BeliefId, BeliefStatus, BeliefSubject, DerivationKind, DerivationRef,
            EntityKind, EvidenceRef, EvidenceTargetType, MemoryContent, MemoryKind,
            WriteMemoryRequest,
        };
        use engram_retrieval::CandidateEvidenceResolver;
        use engram_runtime::PolicyAuthorizer;

        fn evidence_ref(target_type: EvidenceTargetType, id: &str) -> EvidenceRef {
            EvidenceRef {
                target_type,
                target_id: Some(id.to_string()),
                uri: None,
                quote: None,
                location: None,
            }
        }

        fn provenance_with(refs: Vec<EvidenceRef>) -> Provenance {
            let mut provenance = provenance();
            provenance.evidence = refs;
            provenance
        }

        fn derivation_with(inputs: Vec<EvidenceRef>) -> Provenance {
            let mut provenance = provenance();
            provenance.derivations = vec![DerivationRef {
                kind: DerivationKind::Summarization,
                model: None,
                prompt_hash: None,
                input_refs: inputs,
                created_at: chrono::Utc::now(),
            }];
            provenance
        }

        /// Writes an ordinary memory whose provenance is exactly `provenance`
        /// and returns the STORED record (the candidate is then built from the
        /// canonical record, guaranteeing payload binding).
        fn write_memory(
            memory: &Arc<SqlMemoryService>,
            text: &str,
            provenance: Provenance,
        ) -> engram_domain::MemoryRecord {
            block_on(memory.write_memory(WriteMemoryRequest {
                kind: MemoryKind::Observation,
                content: MemoryContent {
                    text: text.to_string(),
                    summary: None,
                    entities: Vec::new(),
                    language: None,
                    format: None,
                    structured: None,
                    hash: None,
                },
                scope: scope(),
                requester: requester(),
                provenance,
                policy: policy(),
                links: Vec::new(),
                verdict: None,
                idempotency_key: None,
            }))
            .expect("write memory")
            .record
        }

        fn put_belief(
            beliefs: &Arc<SqlBeliefStore>,
            id: &str,
            content: &str,
            provenance: Provenance,
        ) {
            block_on(beliefs.put_belief(Belief {
                id: BeliefId::from(id),
                scope: scope(),
                subject: BeliefSubject {
                    key: "parse".to_string(),
                    entity_ref: None,
                    concept_ref: None,
                    aliases: Vec::new(),
                },
                content: content.to_string(),
                status: BeliefStatus::Active,
                confidence: 0.9,
                sources: Vec::new(),
                valid_from: None,
                valid_until: None,
                superseded_by: None,
                stale: None,
                synthesizer: None,
                reasoning: None,
                embedding_refs: Vec::new(),
                policy: policy(),
                provenance,
                created_at: chrono::Utc::now(),
                updated_at: None,
                metadata: None,
            }))
            .expect("put belief");
        }

        fn candidate(
            target_type: RetrievalTargetType,
            id: &str,
            content: &str,
            provenance: Provenance,
            policy: Policy,
        ) -> RetrievalResult {
            RetrievalResult {
                id: format!("candidate-{id}"),
                target_type,
                target_id: id.to_string(),
                content: content.to_string(),
                score: engram_domain::RetrievalScore {
                    total: 1.0,
                    relevance: Some(1.0),
                    recency: None,
                    confidence: None,
                    cue_match: None,
                    hierarchical_fit: None,
                    policy_fit: Some(1.0),
                },
                provenance,
                policy,
                explanation: None,
                fusion_trace: None,
                metadata: None,
            }
        }

        fn memory_candidate(record: &engram_domain::MemoryRecord) -> RetrievalResult {
            candidate(
                RetrievalTargetType::Memory,
                record.id.as_str(),
                &record.content.text.clone(),
                record.provenance.clone(),
                record.policy.clone(),
            )
        }

        fn chunk_candidate(id: &str, text: &str) -> RetrievalResult {
            candidate(RetrievalTargetType::Chunk, id, text, provenance(), policy())
        }

        /// A chunk candidate whose payload/provenance/policy bind the STORED
        /// canonical record (the honest positively-authorized candidate).
        fn bound_chunk_candidate(fixture: &Fixture, id: &str) -> RetrievalResult {
            let record = block_on(fixture.knowledge.get_chunk(&Id::from(id), &scope()))
                .expect("get chunk")
                .expect("chunk exists");
            candidate(
                RetrievalTargetType::Chunk,
                id,
                &record.text.clone(),
                record.provenance.clone(),
                record.policy.clone(),
            )
        }

        fn entity_candidate(id: &str, name: &str) -> RetrievalResult {
            candidate(
                RetrievalTargetType::Entity,
                id,
                name,
                provenance(),
                policy(),
            )
        }

        /// An entity candidate whose provenance binds the STORED record
        /// (cycle/chain fixtures carry reference-bearing provenance).
        fn bound_entity_candidate(fixture: &Fixture, id: &str) -> RetrievalResult {
            let record = block_on(fixture.knowledge.get_entity(&Id::from(id), &scope()))
                .expect("get entity")
                .expect("entity exists");
            candidate(
                RetrievalTargetType::Entity,
                id,
                &record.name.clone(),
                record.provenance.clone(),
                policy(),
            )
        }

        fn belief_candidate(id: &str, content: &str, provenance: Provenance) -> RetrievalResult {
            candidate(
                RetrievalTargetType::Belief,
                id,
                content,
                provenance,
                policy(),
            )
        }

        /// Seeds a belief with TYPED sources and an optional synthesizer-bearing
        /// provenance (canonical 6b fixtures). The stored record's provenance
        /// stays EMPTY so only the typed fields carry the dependency.
        fn put_belief_with_sources(
            beliefs: &Arc<SqlBeliefStore>,
            id: &str,
            content: &str,
            sources: Vec<engram_domain::BeliefSource>,
            provenance: Option<Provenance>,
        ) {
            let mut provenance = provenance.unwrap_or_else(super::provenance);
            let synthesizer = provenance.derivations.first().cloned();
            provenance.derivations.clear();
            provenance.evidence.clear();
            block_on(beliefs.put_belief(Belief {
                id: BeliefId::from(id),
                scope: scope(),
                subject: BeliefSubject {
                    key: "parse".to_string(),
                    entity_ref: None,
                    concept_ref: None,
                    aliases: Vec::new(),
                },
                content: content.to_string(),
                status: BeliefStatus::Active,
                confidence: 0.9,
                sources,
                valid_from: None,
                valid_until: None,
                superseded_by: None,
                stale: None,
                synthesizer,
                reasoning: None,
                embedding_refs: Vec::new(),
                policy: policy(),
                provenance,
                created_at: chrono::Utc::now(),
                updated_at: None,
                metadata: None,
            }))
            .expect("put typed belief");
        }

        /// A belief candidate whose payload/provenance/policy bind the STORED
        /// record (typed-dependency fixtures carry empty provenance).
        fn bound_belief_candidate(fixture: &Fixture, id: &str, content: &str) -> RetrievalResult {
            let record = block_on(engram_belief::BeliefRepository::get_belief_by_id(
                &*fixture.beliefs,
                &BeliefId::from(id),
                &scope(),
            ))
            .expect("get belief")
            .expect("belief exists");
            assert_eq!(record.content, content, "fixture content sanity");
            candidate(
                RetrievalTargetType::Belief,
                id,
                content,
                record.provenance.clone(),
                record.policy.clone(),
            )
        }

        use engram_retrieval::CandidateEvidence;

        fn evidence(fixture: &Fixture, candidate: &RetrievalResult) -> CandidateEvidence {
            block_on(
                fixture
                    .candidate_evidence()
                    .evidence(candidate, &request("parse", 10)),
            )
            .expect("evidence resolution")
        }

        /// An ordinary memory with no dependency references is POSITIVELY
        /// ordinary (positive ordinary origin, not merely not-code).
        #[test]
        fn ordinary_memory_is_positive_ordinary() {
            let dir = tempfile::tempdir().expect("tempdir");
            let fixture = fixture(dir.path());
            let record = write_memory(&fixture.memory, "ordinary standalone", provenance());
            assert_eq!(
                evidence(&fixture, &memory_candidate(&record)),
                CandidateEvidence::Ordinary
            );
        }

        /// A memory whose DERIVATION INPUT references a code chunk is
        /// code-dependent — discovered through the recursive walk (finding 6:
        /// code-derived memories cannot pass while the authority is down).
        #[test]
        fn code_derived_memory_denied_recursively() {
            let dir = tempfile::tempdir().expect("tempdir");
            let fixture = fixture(dir.path());
            let inner = write_memory(
                &fixture.memory,
                "intermediate derivation",
                derivation_with(vec![evidence_ref(
                    EvidenceTargetType::Chunk,
                    "chunk-eligible",
                )]),
            );
            let outer = write_memory(
                &fixture.memory,
                "outer derivation",
                derivation_with(vec![evidence_ref(
                    EvidenceTargetType::Memory,
                    inner.id.as_str(),
                )]),
            );
            assert_eq!(
                evidence(&fixture, &memory_candidate(&outer)),
                CandidateEvidence::CodeDependent
            );
        }

        /// Cycles in the dependency graph terminate: a pure ordinary cycle
        /// stays ordinary, and code anywhere in a cycle still surfaces.
        /// Entities carry explicit ids (put_entity), so the cycle is real.
        #[test]
        fn dependency_cycles_are_cycle_safe() {
            let dir = tempfile::tempdir().expect("tempdir");
            let fixture = fixture(dir.path());
            let put_cycle_entity = |id: &str, name: &str, refs: Vec<EvidenceRef>| {
                let mut entity = symbol_entity(id, name);
                entity.kind = EntityKind::Concept;
                entity.provenance = provenance_with(refs);
                block_on(fixture.knowledge.put_entity(entity)).expect("put cycle entity");
            };
            // Ordinary cycle: a <-> b, nothing else.
            put_cycle_entity(
                "cycle-a",
                "cycleA",
                vec![evidence_ref(EvidenceTargetType::Entity, "cycle-b")],
            );
            put_cycle_entity(
                "cycle-b",
                "cycleB",
                vec![evidence_ref(EvidenceTargetType::Entity, "cycle-a")],
            );
            assert_eq!(
                evidence(&fixture, &bound_entity_candidate(&fixture, "cycle-a")),
                CandidateEvidence::Ordinary,
                "an ordinary cycle must terminate and stay ordinary"
            );

            // Code cycle: c <-> d, d also references stale code.
            put_cycle_entity(
                "cycle-c",
                "cycleC",
                vec![evidence_ref(EvidenceTargetType::Entity, "cycle-d")],
            );
            put_cycle_entity(
                "cycle-d",
                "cycleD",
                vec![
                    evidence_ref(EvidenceTargetType::Entity, "cycle-c"),
                    evidence_ref(EvidenceTargetType::Chunk, "chunk-stale"),
                ],
            );
            assert_eq!(
                evidence(&fixture, &bound_entity_candidate(&fixture, "cycle-c")),
                CandidateEvidence::CodeDependent,
                "code anywhere in the cycle must surface"
            );
        }

        /// A dependency chain deeper than the evaluation bound degrades to
        /// Unknown — never Ordinary (bounded evaluation is not a bypass).
        /// Entities carry explicit ids, so the chain is real and terminal
        /// code sits beyond the depth bound.
        #[test]
        fn depth_bounded_chain_is_unknown_not_ordinary() {
            let dir = tempfile::tempdir().expect("tempdir");
            let fixture = fixture(dir.path());
            const CHAIN: usize = 12;
            let put_chain_entity = |id: &str, next: Option<&str>| {
                let mut entity = symbol_entity(id, &format!("chain{id}"));
                entity.kind = EntityKind::Concept;
                let refs = match next {
                    Some(next) => vec![evidence_ref(EvidenceTargetType::Entity, next)],
                    None => vec![evidence_ref(EvidenceTargetType::Chunk, "chunk-stale")],
                };
                entity.provenance = provenance_with(refs);
                block_on(fixture.knowledge.put_entity(entity)).expect("put chain entity");
            };
            for index in 0..CHAIN {
                let next_id = format!("chain-{}", index + 1);
                let next = if index + 1 < CHAIN {
                    Some(next_id.as_str())
                } else {
                    None
                };
                let id = format!("chain-{index}");
                put_chain_entity(&id, next);
            }
            assert_eq!(
                evidence(&fixture, &bound_entity_candidate(&fixture, "chain-0")),
                CandidateEvidence::Unknown,
                "a chain deeper than the evaluation bound is Unknown, never ordinary"
            );
        }

        /// Payload/revision binding: a candidate whose payload does not match
        /// the canonical record is Unknown, never ordinary.
        #[test]
        fn payload_mismatch_is_unknown() {
            let dir = tempfile::tempdir().expect("tempdir");
            let fixture = fixture(dir.path());
            let mut mismatched = chunk_candidate("chunk-doc", "tampered replacement text");
            mismatched.provenance = provenance();
            assert_eq!(
                evidence(&fixture, &mismatched),
                CandidateEvidence::Unknown,
                "a tampered payload must never classify as ordinary"
            );
            // The genuine payload is ordinary.
            assert_eq!(
                evidence(&fixture, &bound_chunk_candidate(&fixture, "chunk-doc")),
                CandidateEvidence::Ordinary
            );
        }

        /// Foreign/missing canonical records are Unknown.
        #[test]
        fn foreign_or_missing_records_are_unknown() {
            let dir = tempfile::tempdir().expect("tempdir");
            let fixture = fixture(dir.path());
            assert_eq!(
                evidence(
                    &fixture,
                    &chunk_candidate("chunk-never-written", "any text")
                ),
                CandidateEvidence::Unknown
            );
            assert_eq!(
                evidence(
                    &fixture,
                    &entity_candidate("entity-never-written", "any name")
                ),
                CandidateEvidence::Unknown
            );
        }

        /// A DENYING owning authorizer degrades evidence to Unknown: the
        /// resolver shares the memory service's configured authorizer, and
        /// policy equality alone is never authorization.
        struct DenyingAuthorizer;
        impl PolicyAuthorizer for DenyingAuthorizer {
            fn can_write(
                &self,
                _requester: &engram_domain::Requester,
                _scope: &engram_domain::Scope,
                _policy: &Policy,
            ) -> CoreResult<()> {
                Ok(())
            }
            fn can_retrieve(
                &self,
                _requester: &engram_domain::Requester,
                _scope: &engram_domain::Scope,
                _policy: &Policy,
            ) -> CoreResult<()> {
                Err(CoreError::PolicyDenied {
                    reason: "denied by owning authorizer".to_string(),
                })
            }
            fn can_forget(
                &self,
                _requester: &engram_domain::Requester,
                _scope: &engram_domain::Scope,
                _policy: &Policy,
            ) -> CoreResult<()> {
                Err(CoreError::PolicyDenied {
                    reason: "denied by owning authorizer".to_string(),
                })
            }
        }

        #[test]
        fn denying_owning_authorizer_degrades_to_unknown() {
            let dir = tempfile::tempdir().expect("tempdir");
            // The REAL owning construction path: with_dependencies carries a
            // record-sensitive denying authorizer; the evidence path must
            // receive the SAME decisions through the accessor.
            let memory = Arc::new(
                SqlMemoryService::with_dependencies(
                    engram_store_sqlite::memory::SqlMemoryStore::open_in_memory()
                        .expect("memory store"),
                    Arc::new(DenyingAuthorizer),
                    Arc::new(engram_store_sqlite::memory::SystemClock),
                    Arc::new(engram_store_sqlite::memory::SequentialIdGenerator::new()),
                )
                .expect("owning service"),
            );
            let record = write_memory(&memory, "ordinary but denied", provenance());
            let knowledge = Arc::new(SqlKnowledgeStore::open_in_memory().expect("knowledge"));
            let beliefs = Arc::new(SqlBeliefStore::open_in_memory().expect("beliefs"));
            let resolver = engram_integration::candidate_evidence::RepositoryCandidateEvidence::new(
                memory.clone() as Arc<dyn engram_memory::MemoryRepository>,
                beliefs.clone() as Arc<dyn engram_belief::BeliefRepository>,
                knowledge.clone() as Arc<dyn engram_knowledge::KnowledgeRepository>,
                memory.retrieval_authorizer(),
                None,
                Some(knowledge.clone() as Arc<dyn engram_knowledge::KnowledgeGraphRepository>),
            );
            let classification =
                block_on(resolver.evidence(&memory_candidate(&record), &request("parse", 10)))
                    .expect("evidence resolution");
            assert_eq!(
                classification,
                CandidateEvidence::Unknown,
                "an unauthorized canonical read is Unknown, never ordinary"
            );
        }

        /// Positive ordinary entity classification needs the REAL ownership
        /// catalog: with the reader present a non-code entity outside the
        /// lineage is positively Ordinary; without the reader the same
        /// entity stays Unknown (missing evidence is not ordinary proof).
        #[test]
        fn positive_ordinary_entity_requires_real_ownership_reader() {
            let dir = tempfile::tempdir().expect("tempdir");
            let fixture = fixture(dir.path());
            let mut concept = symbol_entity("entity-concept-e", "parseConcept");
            concept.kind = EntityKind::Concept;
            block_on(fixture.knowledge.put_entity(concept)).expect("put concept");

            // Real ownership catalog wired: positively ordinary.
            assert_eq!(
                evidence(
                    &fixture,
                    &bound_entity_candidate(&fixture, "entity-concept-e")
                ),
                CandidateEvidence::Ordinary
            );

            // Same canonical records, NO ownership reader: Unknown.
            let resolver = engram_integration::candidate_evidence::RepositoryCandidateEvidence::new(
                fixture.memory.clone() as Arc<dyn engram_memory::MemoryRepository>,
                fixture.beliefs.clone() as Arc<dyn engram_belief::BeliefRepository>,
                fixture.knowledge.clone() as Arc<dyn engram_knowledge::KnowledgeRepository>,
                fixture.memory.retrieval_authorizer(),
                None,
                Some(fixture.knowledge.clone()
                    as Arc<dyn engram_knowledge::KnowledgeGraphRepository>),
            );
            let classification = block_on(resolver.evidence(
                &bound_entity_candidate(&fixture, "entity-concept-e"),
                &request("parse", 10),
            ))
            .expect("evidence resolution");
            assert_eq!(
                classification,
                CandidateEvidence::Unknown,
                "without the ownership reader absence of lineage is not positive proof"
            );
        }

        /// Durable lineage dominates entity kind: a CONCEPT entity that
        /// carries durable code lineage is code-dependent, and a code-KIND
        /// entity without lineage (legacy unbound code) is denied too.
        #[test]
        fn lineage_and_kind_dominate_entity_classification() {
            let dir = tempfile::tempdir().expect("tempdir");
            let fixture = fixture(dir.path());
            // "entity-published" is a Function in gen-a-2 lineage; "entity-legacy"
            // is a Function with no lineage.
            assert_eq!(
                evidence(
                    &fixture,
                    &bound_entity_candidate(&fixture, "entity-published")
                ),
                CandidateEvidence::CodeDependent,
                "lineage-bound symbol entities are code-dependent"
            );
            assert_eq!(
                evidence(&fixture, &bound_entity_candidate(&fixture, "entity-legacy")),
                CandidateEvidence::CodeDependent,
                "code-kind entities without lineage are legacy unbound code"
            );
        }

        /// A repository-bound source reference is ambiguous typed origin
        /// while the binding authority is unavailable: Unknown, not ordinary.
        #[test]
        fn repository_source_reference_is_unknown() {
            let dir = tempfile::tempdir().expect("tempdir");
            let fixture = fixture(dir.path());
            let record = write_memory(
                &fixture.memory,
                "derived from repository source",
                provenance_with(vec![evidence_ref(
                    EvidenceTargetType::Source,
                    "source-task-013",
                )]),
            );
            assert_eq!(
                evidence(&fixture, &memory_candidate(&record)),
                CandidateEvidence::Unknown
            );
        }

        /// Corrective r4 (canonical 6a): a chunk reference whose SOURCE policy
        /// is denied is Unknown even when the chunk's own policy is allowed —
        /// both canonical authorizations are required on dependency paths.
        struct DeniesHighSensitivity;
        impl engram_runtime::PolicyAuthorizer for DeniesHighSensitivity {
            fn can_write(
                &self,
                _requester: &engram_domain::Requester,
                _scope: &engram_domain::Scope,
                _policy: &Policy,
            ) -> CoreResult<()> {
                Ok(())
            }
            fn can_retrieve(
                &self,
                _requester: &engram_domain::Requester,
                _scope: &engram_domain::Scope,
                policy: &Policy,
            ) -> CoreResult<()> {
                if policy.sensitivity == Some(engram_domain::Sensitivity::High) {
                    Err(CoreError::PolicyDenied {
                        reason: "high sensitivity denied".to_string(),
                    })
                } else {
                    Ok(())
                }
            }
            fn can_forget(
                &self,
                _requester: &engram_domain::Requester,
                _scope: &engram_domain::Scope,
                _policy: &Policy,
            ) -> CoreResult<()> {
                Ok(())
            }
        }

        #[test]
        fn denied_source_policy_with_allowed_chunk_is_unknown() {
            let dir = tempfile::tempdir().expect("tempdir");
            let fixture = fixture(dir.path());
            // A denied-SOURCE variant: same chunk bytes, source carrying a
            // High-sensitivity policy the owning authorizer denies.
            let mut denied_source: engram_domain::KnowledgeSource = {
                let mut source = engram_domain::KnowledgeSource {
                    id: Id::from("source-denied"),
                    kind: engram_domain::SourceKind::Upload,
                    scope: scope(),
                    name: "denied source".to_string(),
                    uri: None,
                    version: None,
                    repository_key: None,
                    git_ref: None,
                    policy: policy(),
                    provenance: provenance(),
                    created_at: chrono::Utc::now(),
                    updated_at: None,
                    metadata: None,
                };
                source.policy.sensitivity = Some(engram_domain::Sensitivity::High);
                source
            };
            let mut denied_chunk = chunk(
                "chunk-denied-source",
                "parse denied source",
                KnowledgeChunkKind::Paragraph,
            );
            denied_chunk.source_id = denied_source.id.clone();
            use engram_knowledge::KnowledgeRepository as _;
            block_on(fixture.knowledge.put_source(denied_source.clone()))
                .expect("put denied source");
            block_on(fixture.knowledge.put_chunk(denied_chunk.clone())).expect("put chunk");
            let record = block_on(
                fixture
                    .knowledge
                    .get_chunk(&Id::from("chunk-denied-source"), &scope()),
            )
            .expect("get chunk")
            .expect("chunk exists");

            let resolver = engram_integration::candidate_evidence::RepositoryCandidateEvidence::new(
                fixture.memory.clone() as Arc<dyn engram_memory::MemoryRepository>,
                fixture.beliefs.clone() as Arc<dyn engram_belief::BeliefRepository>,
                fixture.knowledge.clone() as Arc<dyn engram_knowledge::KnowledgeRepository>,
                Arc::new(DeniesHighSensitivity),
                Some(fixture.bindings.clone() as Arc<dyn engram_knowledge::CodeBindingRepository>),
                Some(fixture.knowledge.clone()
                    as Arc<dyn engram_knowledge::KnowledgeGraphRepository>),
            );
            let result = candidate(
                RetrievalTargetType::Chunk,
                "chunk-denied-source",
                &record.text.clone(),
                record.provenance.clone(),
                record.policy.clone(),
            );
            let classification = block_on(resolver.evidence(&result, &request("parse", 10)))
                .expect("evidence resolution");
            assert_eq!(
                classification,
                CandidateEvidence::Unknown,
                "a denied source policy must degrade the chunk to Unknown even with an allowed chunk policy"
            );
        }

        /// Corrective r4 (canonical 6a): ID-less derived references and
        /// canonically unresolvable categories (Document/Concept) are
        /// Unknown, never Ordinary.
        #[test]
        fn idless_and_unresolvable_references_are_unknown() {
            let dir = tempfile::tempdir().expect("tempdir");
            let fixture = fixture(dir.path());
            let idless = write_memory(
                &fixture.memory,
                "idless derived",
                provenance_with(vec![EvidenceRef {
                    target_type: EvidenceTargetType::Memory,
                    target_id: None,
                    uri: Some("urn:engram:memory:somewhere".to_string()),
                    quote: None,
                    location: None,
                }]),
            );
            assert_eq!(
                evidence(&fixture, &memory_candidate(&idless)),
                CandidateEvidence::Unknown,
                "an ID-less derived reference is unresolved evidence"
            );
            let document_ref = write_memory(
                &fixture.memory,
                "document derived",
                provenance_with(vec![evidence_ref(
                    EvidenceTargetType::Document,
                    "document-task-013",
                )]),
            );
            assert_eq!(
                evidence(&fixture, &memory_candidate(&document_ref)),
                CandidateEvidence::Unknown,
                "documents have no scoped canonical read: Unknown"
            );
            let concept_ref = write_memory(
                &fixture.memory,
                "concept derived",
                provenance_with(vec![evidence_ref(
                    EvidenceTargetType::Concept,
                    "any-concept",
                )]),
            );
            assert_eq!(
                evidence(&fixture, &memory_candidate(&concept_ref)),
                CandidateEvidence::Unknown,
                "concept references carry no canonical code association: Unknown"
            );
        }

        /// Corrective r4 (canonical 6a) + review r5 (finding 4, REQUIRED):
        /// relationship references resolve canonically through the graph
        /// port and classify through the relationship's own provenance AND
        /// its subject/object ENDPOINTS — ID-bearing endpoints resolve
        /// through the bounded canonical Entity path; a MISSING endpoint is
        /// unresolvable identity (Unknown, never ordinary); an eligible-code
        /// endpoint contributes a code dependency; an ID-less endpoint is
        /// unresolvable. Ordinary endpoints keep an ordinary chain ordinary.
        #[test]
        fn relationship_references_resolve_canonically() {
            use engram_knowledge::KnowledgeRepository as _;
            let dir = tempfile::tempdir().expect("tempdir");
            let fixture = fixture(dir.path());
            // Ordinary endpoint entities (Concept kind, canonical graph).
            for id in ["entity-concept-a", "entity-concept-b"] {
                let mut concept = symbol_entity(id, id);
                concept.kind = EntityKind::Concept;
                block_on(fixture.knowledge.put_entity(concept)).expect("put endpoint");
            }
            let relationship_of =
                |id: &str, subject: (&str, &str), object: (&str, &str), refs: Vec<EvidenceRef>| {
                    engram_domain::KnowledgeRelationship {
                        id: engram_domain::RelationshipId::from(id),
                        graph_id: Some(Id::from("graph-task-013")),
                        predicate: "mentions".to_string(),
                        subject: engram_domain::EntityRef {
                            id: Some(Id::from(subject.0)),
                            kind: None,
                            name: Some(subject.1.to_string()),
                            aliases: Vec::new(),
                        },
                        object: engram_domain::EntityRef {
                            id: Some(Id::from(object.0)),
                            kind: None,
                            name: Some(object.1.to_string()),
                            aliases: Vec::new(),
                        },
                        scope: scope(),
                        evidence: Vec::new(),
                        confidence: None,
                        provenance: provenance_with(refs),
                        created_at: chrono::Utc::now(),
                        updated_at: None,
                        code_resolution: None,
                        archived_at: None,
                    }
                };
            // Ordinary endpoints: the chain stays ordinary.
            block_on(fixture.knowledge.put_relationship(relationship_of(
                "rel-ordinary",
                ("entity-concept-a", "conceptA"),
                ("entity-concept-b", "conceptB"),
                vec![evidence_ref(
                    EvidenceTargetType::Memory,
                    &format!("{}", fixture.memory_id),
                )],
            )))
            .expect("put ordinary relationship");
            // The code result is attributable to PROVENANCE (stale chunk),
            // with ordinary endpoints.
            block_on(fixture.knowledge.put_relationship(relationship_of(
                "rel-code",
                ("entity-concept-a", "conceptA"),
                ("entity-concept-b", "conceptB"),
                vec![evidence_ref(EvidenceTargetType::Chunk, "chunk-stale")],
            )))
            .expect("put code-derived relationship");
            // MISSING endpoint: unresolvable identity.
            block_on(fixture.knowledge.put_relationship(relationship_of(
                "rel-endpoint-missing",
                ("entity-concept-a", "conceptA"),
                ("entity-never-written", "ghost"),
                Vec::new(),
            )))
            .expect("put missing-endpoint relationship");
            // ELIGIBLE-CODE endpoint: a code dependency.
            block_on(fixture.knowledge.put_relationship(relationship_of(
                "rel-endpoint-eligible",
                ("entity-concept-a", "conceptA"),
                ("entity-published", "parsePublished"),
                Vec::new(),
            )))
            .expect("put eligible-endpoint relationship");
            // ID-LESS endpoint: unresolvable identity.
            let mut idless = relationship_of(
                "rel-endpoint-idless",
                ("entity-concept-a", "conceptA"),
                ("entity-concept-b", "conceptB"),
                Vec::new(),
            );
            idless.object.id = None;
            block_on(fixture.knowledge.put_relationship(idless)).expect("put idless-endpoint rel");

            let chain = |fixture: &Fixture, rel: &str| {
                write_memory(
                    &fixture.memory,
                    &format!("relationship chain {rel}"),
                    provenance_with(vec![evidence_ref(EvidenceTargetType::Relationship, rel)]),
                )
            };
            assert_eq!(
                evidence(
                    &fixture,
                    &memory_candidate(&chain(&fixture, "rel-ordinary"))
                ),
                CandidateEvidence::Ordinary,
                "ordinary endpoints + ordinary provenance stay ordinary"
            );
            assert_eq!(
                evidence(&fixture, &memory_candidate(&chain(&fixture, "rel-code"))),
                CandidateEvidence::CodeDependent,
                "a relationship whose provenance derives from code is code-dependent"
            );
            assert_eq!(
                evidence(
                    &fixture,
                    &memory_candidate(&chain(&fixture, "rel-endpoint-missing"))
                ),
                CandidateEvidence::Unknown,
                "an ID-bearing endpoint that cannot resolve fails closed (finding 4)"
            );
            assert_eq!(
                evidence(
                    &fixture,
                    &memory_candidate(&chain(&fixture, "rel-endpoint-eligible"))
                ),
                CandidateEvidence::CodeDependent,
                "an eligible-code endpoint contributes a code dependency (finding 4)"
            );
            assert_eq!(
                evidence(
                    &fixture,
                    &memory_candidate(&chain(&fixture, "rel-endpoint-idless"))
                ),
                CandidateEvidence::Unknown,
                "an ID-less endpoint is unresolvable identity, never ordinary (finding 4)"
            );
        }

        /// Corrective r4 (canonical 6a): an ordinary chunk from a
        /// REPOSITORY-BOUND source with no ownership reader is Unknown on the
        /// dependency path — missing ownership evidence is not a negative
        /// lineage lookup.
        #[test]
        fn repository_source_without_reader_is_unknown_on_reference_path() {
            let dir = tempfile::tempdir().expect("tempdir");
            let fixture = fixture(dir.path());
            // A paragraph chunk of the REPOSITORY-BOUND source
            // (source-task-013, no settled lineage).
            use engram_knowledge::KnowledgeRepository as _;
            let mut repo_bound = chunk(
                "chunk-repo-bound-origin",
                "repositorybound originchunk",
                KnowledgeChunkKind::Paragraph,
            );
            repo_bound.source_id = Id::from("source-task-013");
            block_on(fixture.knowledge.put_chunk(repo_bound)).expect("put repo-bound chunk");
            let referencing = write_memory(
                &fixture.memory,
                "references ordinary repo chunk",
                provenance_with(vec![evidence_ref(
                    EvidenceTargetType::Chunk,
                    "chunk-repo-bound-origin",
                )]),
            );
            let resolver = engram_integration::candidate_evidence::RepositoryCandidateEvidence::new(
                fixture.memory.clone() as Arc<dyn engram_memory::MemoryRepository>,
                fixture.beliefs.clone() as Arc<dyn engram_belief::BeliefRepository>,
                fixture.knowledge.clone() as Arc<dyn engram_knowledge::KnowledgeRepository>,
                fixture.memory.retrieval_authorizer(),
                None,
                Some(fixture.knowledge.clone()
                    as Arc<dyn engram_knowledge::KnowledgeGraphRepository>),
            );
            let classification =
                block_on(resolver.evidence(&memory_candidate(&referencing), &request("parse", 10)))
                    .expect("evidence resolution");
            assert_eq!(
                classification,
                CandidateEvidence::Unknown,
                "repository-bound origin without an ownership reader is ambiguous, not ordinary"
            );
        }

        /// Corrective r4 (canonical 6b): a belief's TYPED sources and
        /// synthesizer inputs are canonical dependencies: stale-code sources
        /// or code synthesizer inputs deny even with EMPTY provenance, while
        /// ordinary typed sources stay positively ordinary.
        #[test]
        fn typed_belief_sources_and_synthesizer_are_dependencies() {
            let dir = tempfile::tempdir().expect("tempdir");
            let fixture = fixture(dir.path());
            let source_of = |target_type: engram_domain::BeliefSourceTargetType, id: &str| {
                engram_domain::BeliefSource {
                    target_type,
                    target_id: id.to_string(),
                    authority_level: None,
                    weight: None,
                    confidence: None,
                    valid_from: None,
                    valid_until: None,
                }
            };
            // Stale-code typed source, EMPTY provenance.
            put_belief_with_sources(
                &fixture.beliefs,
                "belief-typed-code",
                "parse typed code belief",
                vec![source_of(
                    engram_domain::BeliefSourceTargetType::Chunk,
                    "chunk-stale",
                )],
                None,
            );
            // Ordinary typed memory source.
            put_belief_with_sources(
                &fixture.beliefs,
                "belief-typed-ordinary",
                "parse typed ordinary belief",
                vec![source_of(
                    engram_domain::BeliefSourceTargetType::Memory,
                    &format!("{}", fixture.memory_id),
                )],
                None,
            );
            // Synthesizer derivation referencing code.
            let mut synthesizer_provenance = provenance();
            synthesizer_provenance.derivations = vec![DerivationRef {
                kind: DerivationKind::Summarization,
                model: None,
                prompt_hash: None,
                input_refs: vec![evidence_ref(EvidenceTargetType::Chunk, "chunk-eligible")],
                created_at: chrono::Utc::now(),
            }];
            put_belief_with_sources(
                &fixture.beliefs,
                "belief-synthesized-code",
                "parse synthesized belief",
                Vec::new(),
                Some(synthesizer_provenance),
            );

            assert_eq!(
                evidence(
                    &fixture,
                    &bound_belief_candidate(
                        &fixture,
                        "belief-typed-code",
                        "parse typed code belief"
                    )
                ),
                CandidateEvidence::CodeDependent,
                "typed stale-code belief sources deny regardless of provenance"
            );
            assert_eq!(
                evidence(
                    &fixture,
                    &bound_belief_candidate(
                        &fixture,
                        "belief-typed-ordinary",
                        "parse typed ordinary belief"
                    )
                ),
                CandidateEvidence::Ordinary,
                "ordinary typed belief sources stay positively ordinary"
            );
            assert_eq!(
                evidence(
                    &fixture,
                    &bound_belief_candidate(
                        &fixture,
                        "belief-synthesized-code",
                        "parse synthesized belief"
                    )
                ),
                CandidateEvidence::CodeDependent,
                "a synthesizer referencing code denies"
            );
        }

        /// Corrective r4 (canonical 6d): the TOTAL reference budget stops the
        /// walk — a wide dependency array beyond the bound is Unknown, and a
        /// within-bound array of the same shape stays ordinary. Processing
        /// stays prompt (bounded work, not input-proportional).
        #[test]
        fn wide_dependency_array_hits_total_work_bound() {
            let dir = tempfile::tempdir().expect("tempdir");
            let fixture = fixture(dir.path());
            let ordinary_target = write_memory(&fixture.memory, "wide target", provenance());
            let target_id = ordinary_target.id.to_string();

            let wide = write_memory(
                &fixture.memory,
                "wide dependency array",
                provenance_with(
                    std::iter::repeat_n(evidence_ref(EvidenceTargetType::Memory, &target_id), 2000)
                        .collect(),
                ),
            );
            let started = std::time::Instant::now();
            assert_eq!(
                evidence(&fixture, &memory_candidate(&wide)),
                CandidateEvidence::Unknown,
                "a dependency array beyond the total work budget is Unknown, never ordinary"
            );
            let wide_elapsed = started.elapsed();

            let narrow = write_memory(
                &fixture.memory,
                "narrow dependency array",
                provenance_with(
                    std::iter::repeat_n(evidence_ref(EvidenceTargetType::Memory, &target_id), 1000)
                        .collect(),
                ),
            );
            assert_eq!(
                evidence(&fixture, &memory_candidate(&narrow)),
                CandidateEvidence::Ordinary,
                "a within-budget array of the same shape stays ordinary"
            );
            assert!(
                wide_elapsed < std::time::Duration::from_secs(10),
                "the bounded walk must stay prompt: {wide_elapsed:?}"
            );
        }

        /// Corrective r4 (canonical 6c): with a SUCCESSFUL view, a derived
        /// memory whose code dependency is INELIGIBLE denies on the normal
        /// path, while positively authorized ordinary records and
        /// eligible-code-derived records keep flowing.
        #[test]
        fn successful_view_denies_stale_derived_memory_and_preserves_ordinary() {
            let dir = tempfile::tempdir().expect("tempdir");
            let fixture = fixture(dir.path());
            let stale_derived = write_memory(
                &fixture.memory,
                "parse stale derived",
                provenance_with(vec![evidence_ref(EvidenceTargetType::Chunk, "chunk-stale")]),
            );
            let eligible_derived = write_memory(
                &fixture.memory,
                "parse eligible derived",
                provenance_with(vec![evidence_ref(
                    EvidenceTargetType::Chunk,
                    "chunk-eligible",
                )]),
            );
            // The historical graph lane contributes the ordinary doc chunk;
            // the facts lane contributes the three memories.
            let graph_lane: Arc<dyn RetrievalIndex> = Arc::new(
                engram_store_sqlite::knowledge::GraphRetrievalIndex::new(fixture.knowledge.clone()),
            );
            let recall = SqlUnifiedRecall::new(
                fixture.memory.clone(),
                vec![graph_lane],
                fixture.beliefs.clone(),
            )
            .with_code_eligibility(
                fixture.eligibility() as Arc<dyn engram_retrieval::CodeEligibilityResolver>
            )
            .with_candidate_evidence(fixture.candidate_evidence());

            let payload = block_on(recall.recall(request("parse", 10))).expect("recall");
            let ids = target_ids(&payload);
            assert!(
                ids.iter().any(|id| id == &format!("{}", fixture.memory_id)),
                "the independent ordinary memory survives: {ids:?}"
            );
            assert!(
                ids.contains(&"chunk-doc".to_string()),
                "the positively authorized ordinary chunk survives: {ids:?}"
            );
            assert!(
                ids.iter().any(|id| id == eligible_derived.id.as_str()),
                "a memory derived from ELIGIBLE code keeps flowing: {ids:?}"
            );
            assert!(
                !ids.iter().any(|id| id == stale_derived.id.as_str()),
                "a memory derived from stale code denies on the successful path: {ids:?}"
            );
            assert!(
                !ids.contains(&"chunk-stale".to_string()),
                "the stale chunk itself denies: {ids:?}"
            );
        }

        /// Beliefs classify through the same canonical evidence path.
        #[test]
        fn code_derived_belief_denied_and_ordinary_belief_preserved() {
            let dir = tempfile::tempdir().expect("tempdir");
            let fixture = fixture(dir.path());
            let ordinary_provenance = provenance();
            put_belief(
                &fixture.beliefs,
                "belief-ordinary",
                "ordinary belief content",
                ordinary_provenance.clone(),
            );
            let code_provenance = provenance_with(vec![evidence_ref(
                EvidenceTargetType::Chunk,
                "chunk-eligible",
            )]);
            put_belief(
                &fixture.beliefs,
                "belief-code",
                "code derived belief",
                code_provenance.clone(),
            );
            assert_eq!(
                evidence(
                    &fixture,
                    &belief_candidate(
                        "belief-ordinary",
                        "ordinary belief content",
                        ordinary_provenance
                    )
                ),
                CandidateEvidence::Ordinary
            );
            assert_eq!(
                evidence(
                    &fixture,
                    &belief_candidate("belief-code", "code derived belief", code_provenance)
                ),
                CandidateEvidence::CodeDependent
            );
        }
    }

    /// Work package A3 (corrective r5) decisive matrix: every case drives
    /// the REAL composed recall — real facts/belief/graph/lexical lanes, the
    /// real eligibility view, the real repository evidence resolver — never
    /// resolver-only candidates.
    mod a3_matrix {
        use super::*;
        use engram_belief::BeliefRepository;
        use engram_domain::{
            Belief, BeliefId, BeliefSource, BeliefSourceTargetType, BeliefStatus, BeliefSubject,
            DerivationKind, DerivationRef, EntityKind, EvidenceRef, EvidenceTargetType,
            MemoryContent, MemoryKind, WriteMemoryRequest,
        };
        use engram_retrieval::CandidateEvidence;
        use engram_retrieval::CandidateEvidenceResolver;
        use engram_runtime::PolicyAuthorizer;

        fn evidence_ref(target_type: EvidenceTargetType, id: &str) -> EvidenceRef {
            EvidenceRef {
                target_type,
                target_id: Some(id.to_string()),
                uri: None,
                quote: None,
                location: None,
            }
        }

        fn provenance_with(refs: Vec<EvidenceRef>) -> Provenance {
            let mut provenance = provenance();
            provenance.evidence = refs;
            provenance
        }

        fn write_memory_with_refs(
            fixture: &Fixture,
            text: &str,
            refs: Vec<EvidenceRef>,
        ) -> engram_domain::MemoryRecord {
            block_on(fixture.memory.write_memory(WriteMemoryRequest {
                kind: MemoryKind::Observation,
                content: MemoryContent {
                    text: text.to_string(),
                    summary: None,
                    entities: Vec::new(),
                    language: None,
                    format: None,
                    structured: None,
                    hash: None,
                },
                scope: scope(),
                requester: requester(),
                provenance: provenance_with(refs),
                policy: policy(),
                links: Vec::new(),
                verdict: None,
                idempotency_key: None,
            }))
            .expect("write memory")
            .record
        }

        /// Writes a belief with a custom SUBJECT key (the composed belief lane
        /// is live-subject 0-or-1 per query, so each case needs its own
        /// subject) plus optional typed sources and synthesizer inputs.
        fn put_belief_matrix(
            fixture: &Fixture,
            id: &str,
            subject: &str,
            content: &str,
            sources: Vec<BeliefSource>,
            synthesizer_inputs: Vec<EvidenceRef>,
            provenance_refs: Vec<EvidenceRef>,
        ) {
            let mut provenance = provenance();
            provenance.evidence = provenance_refs;
            let synthesizer = (!synthesizer_inputs.is_empty()).then(|| DerivationRef {
                kind: DerivationKind::Summarization,
                model: None,
                prompt_hash: None,
                input_refs: synthesizer_inputs,
                created_at: chrono::Utc::now(),
            });
            block_on(fixture.beliefs.put_belief(Belief {
                id: BeliefId::from(id),
                scope: scope(),
                subject: BeliefSubject {
                    key: subject.to_string(),
                    entity_ref: None,
                    concept_ref: None,
                    aliases: Vec::new(),
                },
                content: content.to_string(),
                status: BeliefStatus::Active,
                confidence: 0.9,
                sources,
                valid_from: None,
                valid_until: None,
                superseded_by: None,
                stale: None,
                synthesizer,
                reasoning: None,
                embedding_refs: Vec::new(),
                policy: policy(),
                provenance,
                created_at: chrono::Utc::now(),
                updated_at: None,
                metadata: None,
            }))
            .expect("put matrix belief");
        }

        fn belief_source_of(target_type: BeliefSourceTargetType, id: &str) -> BeliefSource {
            BeliefSource {
                target_type,
                target_id: id.to_string(),
                authority_level: None,
                weight: None,
                confidence: None,
                valid_from: None,
                valid_until: None,
            }
        }

        /// Composed recall over the REAL graph lane with the eligibility
        /// view AND the repository evidence resolver wired (the successful
        /// authority + configured resolver configuration).
        fn composed_recall(fixture: &Fixture) -> SqlUnifiedRecall {
            composed_recall_with_evidence(fixture, fixture.candidate_evidence())
        }

        fn composed_recall_with_evidence(
            fixture: &Fixture,
            evidence: Arc<engram_integration::candidate_evidence::RepositoryCandidateEvidence>,
        ) -> SqlUnifiedRecall {
            let graph_lane: Arc<dyn RetrievalIndex> = Arc::new(
                engram_store_sqlite::knowledge::GraphRetrievalIndex::new(fixture.knowledge.clone()),
            );
            SqlUnifiedRecall::new(
                fixture.memory.clone(),
                vec![graph_lane],
                fixture.beliefs.clone(),
            )
            .with_code_eligibility(
                fixture.eligibility() as Arc<dyn engram_retrieval::CodeEligibilityResolver>
            )
            .with_candidate_evidence(evidence)
        }

        /// An eligibility resolver whose authority read always errors.
        struct ErroringAuthority;
        #[async_trait::async_trait]
        impl engram_retrieval::CodeEligibilityResolver for ErroringAuthority {
            async fn resolve(
                &self,
                _scope: &Scope,
            ) -> CoreResult<engram_retrieval::BoundCodeEligibility> {
                Err(CoreError::Adapter {
                    adapter: "test-authority".to_string(),
                    message: "authority unavailable".to_string(),
                })
            }
        }

        fn erroring_recall(fixture: &Fixture) -> SqlUnifiedRecall {
            let graph_lane: Arc<dyn RetrievalIndex> = Arc::new(
                engram_store_sqlite::knowledge::GraphRetrievalIndex::new(fixture.knowledge.clone()),
            );
            SqlUnifiedRecall::new(
                fixture.memory.clone(),
                vec![graph_lane],
                fixture.beliefs.clone(),
            )
            .with_code_eligibility(Arc::new(ErroringAuthority))
            .with_candidate_evidence(fixture.candidate_evidence())
        }

        /// An owning authorizer that denies High-sensitivity policies (the
        /// composed denial row: authorization failure inside evidence).
        struct DeniesHighSensitivity;
        impl PolicyAuthorizer for DeniesHighSensitivity {
            fn can_write(
                &self,
                _requester: &engram_domain::Requester,
                _scope: &Scope,
                _policy: &Policy,
            ) -> CoreResult<()> {
                Ok(())
            }
            fn can_retrieve(
                &self,
                _requester: &engram_domain::Requester,
                _scope: &Scope,
                policy: &Policy,
            ) -> CoreResult<()> {
                if policy.sensitivity == Some(engram_domain::Sensitivity::High) {
                    Err(CoreError::PolicyDenied {
                        reason: "high sensitivity denied".to_string(),
                    })
                } else {
                    Ok(())
                }
            }
            fn can_forget(
                &self,
                _requester: &engram_domain::Requester,
                _scope: &Scope,
                _policy: &Policy,
            ) -> CoreResult<()> {
                Ok(())
            }
        }

        fn denying_evidence(
            fixture: &Fixture,
        ) -> Arc<engram_integration::candidate_evidence::RepositoryCandidateEvidence> {
            Arc::new(
                engram_integration::candidate_evidence::RepositoryCandidateEvidence::new(
                    fixture.memory.clone() as Arc<dyn engram_memory::MemoryRepository>,
                    fixture.beliefs.clone() as Arc<dyn engram_belief::BeliefRepository>,
                    fixture.knowledge.clone() as Arc<dyn engram_knowledge::KnowledgeRepository>,
                    Arc::new(DeniesHighSensitivity),
                    Some(fixture.bindings.clone()
                        as Arc<dyn engram_knowledge::CodeBindingRepository>),
                    Some(fixture.knowledge.clone()
                        as Arc<dyn engram_knowledge::KnowledgeGraphRepository>),
                ),
            )
        }

        /// Seeds a denied-policy source + a paragraph chunk of it.
        fn put_denied_source_chunk(fixture: &Fixture, chunk_id: &str, text: &str) {
            use engram_knowledge::KnowledgeRepository as _;
            let mut denied_policy = policy();
            denied_policy.sensitivity = Some(engram_domain::Sensitivity::High);
            let mut source = engram_domain::KnowledgeSource {
                id: Id::from("source-denied-matrix"),
                kind: engram_domain::SourceKind::Upload,
                scope: scope(),
                name: "denied matrix source".to_string(),
                uri: None,
                version: None,
                repository_key: None,
                git_ref: None,
                policy: denied_policy,
                provenance: provenance(),
                created_at: chrono::Utc::now(),
                updated_at: None,
                metadata: None,
            };
            source.policy.sensitivity = Some(engram_domain::Sensitivity::High);
            block_on(fixture.knowledge.put_source(source)).expect("put denied source");
            let mut denied_chunk = chunk(chunk_id, text, KnowledgeChunkKind::Paragraph);
            denied_chunk.source_id = Id::from("source-denied-matrix");
            block_on(fixture.knowledge.put_chunk(denied_chunk)).expect("put denied chunk");
        }

        /// Puts a memory with an EXPLICIT id (cycle/counter fixtures need
        /// stable ids); returns the stored record.
        fn put_memory_with_id(
            fixture: &Fixture,
            id: &str,
            text: &str,
            refs: Vec<EvidenceRef>,
        ) -> engram_domain::MemoryRecord {
            use engram_memory::MemoryRepository;
            let record = engram_domain::MemoryRecord {
                id: Id::from(id),
                kind: MemoryKind::Observation,
                content: MemoryContent {
                    text: text.to_string(),
                    summary: None,
                    entities: Vec::new(),
                    language: None,
                    format: None,
                    structured: None,
                    hash: None,
                },
                scope: scope(),
                provenance: provenance_with(refs),
                policy: policy(),
                status: engram_domain::MemoryStatus::Active,
                links: Vec::new(),
                assertions: Vec::new(),
                verdict: None,
                created_at: chrono::Utc::now(),
                updated_at: None,
                metadata: None,
            };
            block_on(fixture.memory.put_memory(record)).expect("put memory with id")
        }

        /// CHUNK row of the decisive matrix through composed recall: ordinary
        /// positive, own-code eligible, own-code stale, own-code unbound
        /// (legacy), forged stamp — each accepted or denied exactly as the
        /// view decides.
        #[test]
        fn chunk_matrix_composed() {
            let dir = tempfile::tempdir().expect("tempdir");
            let fixture = fixture(dir.path());
            let payload =
                block_on(composed_recall(&fixture).recall(request("parse", 20))).expect("recall");
            let ids = target_ids(&payload);
            assert!(
                ids.contains(&"chunk-doc".to_string()),
                "ordinary chunk: {ids:?}"
            );
            assert!(
                ids.contains(&"chunk-eligible".to_string()),
                "own-code eligible chunk: {ids:?}"
            );
            assert!(
                !ids.contains(&"chunk-stale".to_string()),
                "own-code stale chunk denies: {ids:?}"
            );
            assert!(
                !ids.contains(&"chunk-legacy-code".to_string()),
                "own-code unbound (legacy) chunk denies: {ids:?}"
            );
            assert!(
                !ids.contains(&"chunk-forged-no-lineage".to_string()),
                "forged-stamp chunk denies: {ids:?}"
            );
            assert!(
                !ids.contains(&"chunk-repo-b".to_string()),
                "staged (unpublished) own-code chunk denies: {ids:?}"
            );
        }

        /// ENTITY row: ordinary positive, own-code eligible, own-code
        /// unbound — through composed recall.
        #[test]
        fn entity_matrix_composed() {
            let dir = tempfile::tempdir().expect("tempdir");
            let fixture = fixture(dir.path());
            let mut concept = symbol_entity("entity-concept-matrix", "parseConceptMatrix");
            concept.kind = EntityKind::Concept;
            block_on(fixture.knowledge.put_entity(concept)).expect("put concept entity");
            let payload =
                block_on(composed_recall(&fixture).recall(request("parse", 20))).expect("recall");
            let ids = target_ids(&payload);
            assert!(
                ids.contains(&"entity-concept-matrix".to_string()),
                "ordinary entity: {ids:?}"
            );
            assert!(
                ids.contains(&"entity-published".to_string()),
                "own-code eligible entity: {ids:?}"
            );
            assert!(
                !ids.contains(&"entity-legacy".to_string()),
                "own-code unbound (legacy) entity denies: {ids:?}"
            );
        }

        /// MEMORY row: ordinary, code-derived eligible, code-derived stale,
        /// unknown/missing-derived, foreign-derived — through the facts lane.
        #[test]
        fn memory_matrix_composed() {
            let dir = tempfile::tempdir().expect("tempdir");
            let fixture = fixture(dir.path());
            // A foreign-scope chunk: chunks inherit the source's scope, so
            // the canonical read under the request scope misses.
            {
                use engram_knowledge::KnowledgeRepository as _;
                let mut foreign_scope = scope();
                foreign_scope.workspace = Some("foreign-workspace".to_string());
                let foreign_source = engram_domain::KnowledgeSource {
                    id: Id::from("source-foreign"),
                    kind: engram_domain::SourceKind::Upload,
                    scope: foreign_scope,
                    name: "foreign source".to_string(),
                    uri: None,
                    version: None,
                    repository_key: None,
                    git_ref: None,
                    policy: policy(),
                    provenance: provenance(),
                    created_at: chrono::Utc::now(),
                    updated_at: None,
                    metadata: None,
                };
                block_on(fixture.knowledge.put_source(foreign_source)).expect("put foreign source");
                let mut foreign = chunk(
                    "chunk-foreign",
                    "parse foreign",
                    KnowledgeChunkKind::Paragraph,
                );
                foreign.source_id = Id::from("source-foreign");
                block_on(fixture.knowledge.put_chunk(foreign)).expect("put foreign chunk");
            }

            let eligible = write_memory_with_refs(
                &fixture,
                "parse eligible derived",
                vec![evidence_ref(EvidenceTargetType::Chunk, "chunk-eligible")],
            );
            let stale = write_memory_with_refs(
                &fixture,
                "parse stale derived",
                vec![evidence_ref(EvidenceTargetType::Chunk, "chunk-stale")],
            );
            let missing = write_memory_with_refs(
                &fixture,
                "parse missing derived",
                vec![evidence_ref(
                    EvidenceTargetType::Chunk,
                    "chunk-never-written",
                )],
            );
            let foreign_derived = write_memory_with_refs(
                &fixture,
                "parse foreign derived",
                vec![evidence_ref(EvidenceTargetType::Chunk, "chunk-foreign")],
            );

            let payload =
                block_on(composed_recall(&fixture).recall(request("parse", 20))).expect("recall");
            let ids = target_ids(&payload);
            assert!(
                ids.iter().any(|id| id == fixture.memory_id.as_str()),
                "ordinary memory: {ids:?}"
            );
            assert!(
                ids.iter().any(|id| id == eligible.id.as_str()),
                "eligible-code-derived memory: {ids:?}"
            );
            assert!(
                !ids.iter().any(|id| id == stale.id.as_str()),
                "stale-code-derived memory denies: {ids:?}"
            );
            assert!(
                !ids.iter().any(|id| id == missing.id.as_str()),
                "missing-derived memory denies: {ids:?}"
            );
            assert!(
                !ids.iter().any(|id| id == foreign_derived.id.as_str()),
                "foreign-derived memory denies: {ids:?}"
            );
        }

        /// BELIEF row (one live subject per recall): ordinary, typed
        /// code-source, typed ordinary-source, synthesizer code input,
        /// provenance code ref.
        #[test]
        fn belief_matrix_composed() {
            let dir = tempfile::tempdir().expect("tempdir");
            let fixture = fixture(dir.path());
            put_belief_matrix(
                &fixture,
                "belief-m-ordinary",
                "subject-ordinary",
                "parse ordinary matrix belief",
                Vec::new(),
                Vec::new(),
                Vec::new(),
            );
            put_belief_matrix(
                &fixture,
                "belief-m-code-source",
                "subject-code-source",
                "parse code source belief",
                vec![belief_source_of(
                    BeliefSourceTargetType::Chunk,
                    "chunk-stale",
                )],
                Vec::new(),
                Vec::new(),
            );
            put_belief_matrix(
                &fixture,
                "belief-m-ordinary-source",
                "subject-ordinary-source",
                "parse ordinary source belief",
                vec![belief_source_of(
                    BeliefSourceTargetType::Memory,
                    fixture.memory_id.as_str(),
                )],
                Vec::new(),
                Vec::new(),
            );
            put_belief_matrix(
                &fixture,
                "belief-m-synth-code",
                "subject-synth-code",
                "parse synth code belief",
                Vec::new(),
                vec![evidence_ref(EvidenceTargetType::Chunk, "chunk-eligible")],
                Vec::new(),
            );
            put_belief_matrix(
                &fixture,
                "belief-m-provenance-code",
                "subject-provenance-code",
                "parse provenance code belief",
                Vec::new(),
                Vec::new(),
                vec![evidence_ref(EvidenceTargetType::Chunk, "chunk-stale")],
            );

            let recall = composed_recall(&fixture);
            let present = |subject: &str| {
                target_ids(&block_on(recall.recall(request(subject, 20))).expect("recall"))
                    .contains(&format!("belief-m-{subject}").replace("subject-", ""))
            };
            assert!(
                present("subject-ordinary"),
                "ordinary belief survives through the belief lane"
            );
            assert!(
                !present("subject-code-source"),
                "typed stale-code-source belief denies"
            );
            assert!(
                present("subject-ordinary-source"),
                "typed ordinary-source belief survives"
            );
            assert!(
                present("subject-synth-code"),
                "eligible-code synthesizer input belief survives"
            );
            assert!(
                !present("subject-provenance-code"),
                "stale-code provenance belief denies"
            );
        }

        /// Missing evidence resolver ≠ derived-record independence: with a
        /// SUCCESSFUL view and no evidence resolver every candidate fails
        /// closed and the denial is surfaced — no own-id exemption.
        #[test]
        fn missing_evidence_resolver_fails_closed_with_successful_view() {
            let dir = tempfile::tempdir().expect("tempdir");
            let fixture = fixture(dir.path());
            let recall = fixture.bound_recall(); // eligibility, NO evidence
            let payload = block_on(recall.recall(request("parse", 20))).expect("recall");
            assert!(
                payload.items.is_empty(),
                "no own-id exemption: even ordinary candidates deny while the evidence resolver is missing: {:?}",
                target_ids(&payload)
            );
            assert!(
                payload
                    .source_failures
                    .iter()
                    .any(|failure| failure.source == "candidate_evidence"),
                "the fail-closed denial is surfaced: {:?}",
                payload.source_failures
            );
        }

        /// Erroring authority row: positively authorized ordinary records of
        /// every kind survive; own-code (lineage-owned) and derived records
        /// deny; the failure is surfaced.
        #[test]
        fn authority_error_matrix_composed() {
            let dir = tempfile::tempdir().expect("tempdir");
            let fixture = fixture(dir.path());
            // A lineage-owned (own-code) memory under a PUBLISHED generation:
            // ordinary kind, empty provenance — own lineage alone is code.
            let own_code_memory =
                write_memory_with_refs(&fixture, "parse own code memory", Vec::new());
            publish_with_lineage(
                &fixture.bindings,
                "repo-a",
                "gen-a-3",
                2,
                Some("gen-a-2"),
                &[own_code_memory.id.as_str()],
            );
            let derived = write_memory_with_refs(
                &fixture,
                "parse derived from stale",
                vec![evidence_ref(EvidenceTargetType::Chunk, "chunk-stale")],
            );
            put_belief_matrix(
                &fixture,
                "belief-m-authority-ordinary",
                "parse",
                "parse authority ordinary belief",
                Vec::new(),
                Vec::new(),
                Vec::new(),
            );

            let payload =
                block_on(erroring_recall(&fixture).recall(request("parse", 20))).expect("recall");
            let ids = target_ids(&payload);
            assert!(
                ids.iter().any(|id| id == fixture.memory_id.as_str()),
                "ordinary memory survives the authority error: {ids:?}"
            );
            assert!(
                ids.contains(&"chunk-doc".to_string()),
                "ordinary chunk survives: {ids:?}"
            );
            assert!(
                ids.contains(&"belief-m-authority-ordinary".to_string()),
                "ordinary belief survives: {ids:?}"
            );
            assert!(
                !ids.iter().any(|id| id == own_code_memory.id.as_str()),
                "own-code (lineage-owned) memory denies: {ids:?}"
            );
            assert!(
                !ids.contains(&"chunk-eligible".to_string()),
                "eligible own-code chunk denies while the authority is unavailable: {ids:?}"
            );
            assert!(
                !ids.iter().any(|id| id == derived.id.as_str()),
                "code-derived memory denies: {ids:?}"
            );
            assert!(
                payload
                    .source_failures
                    .iter()
                    .any(|failure| failure.source == "code_eligibility"),
                "the authority error is surfaced: {:?}",
                payload.source_failures
            );
        }

        /// Review r5 (finding 2): a retired published code-owned CONCEPT
        /// (ordinary kind, retained knowledge row) denies through DEGRADED
        /// recall (authority failure) while an unrelated ordinary Concept
        /// survives the same degraded page; the successful-view page denies
        /// it just the same. Retirement stays POSITIVE code ownership in the
        /// evidence resolver's keyed lineage read.
        #[test]
        fn retired_code_owned_concept_denied_degraded() {
            let dir = tempfile::tempdir().expect("tempdir");
            let fixture = fixture(dir.path());
            let concept = |id: &str, name: &str| {
                let mut record = symbol_entity(id, name);
                record.kind = EntityKind::Concept;
                record
            };
            block_on(
                fixture
                    .knowledge
                    .put_entity(concept("concept-retired", "retiredConcept")),
            )
            .expect("put retired concept");
            block_on(
                fixture
                    .knowledge
                    .put_entity(concept("concept-ordinary", "ordinaryConcept")),
            )
            .expect("put ordinary concept");
            publish_with_lineage(
                &fixture.bindings,
                "repo-a",
                "gen-a-3",
                2,
                Some("gen-a-2"),
                &["concept-retired"],
            );
            // Retire through the typed store fence (binding epoch is 3 after
            // the fixture's two publications plus gen-a-3).
            block_on(
                engram_knowledge::CodeBindingRepository::retire_lineage_targets(
                    &*fixture.bindings,
                    &scope(),
                    &[(RepoId("repo-a".to_string()), 3)],
                    &[engram_knowledge::RetirementTarget {
                        repo_id: "repo-a".to_string(),
                        generation_id: "gen-a-3".to_string(),
                        record_id: "concept-retired".to_string(),
                    }],
                ),
            )
            .expect("retire the concept");

            let degraded = block_on(erroring_recall(&fixture).recall(request("concept", 20)))
                .expect("degraded recall");
            let ids = target_ids(&degraded);
            assert!(
                ids.contains(&"concept-ordinary".to_string()),
                "the unrelated ordinary Concept survives the degraded page: {ids:?}"
            );
            assert!(
                !ids.contains(&"concept-retired".to_string()),
                "the retired code-owned Concept denies through the degraded page (retained lineage stays positive code ownership): {ids:?}"
            );

            let successful = block_on(composed_recall(&fixture).recall(request("concept", 20)))
                .expect("composed recall");
            let ids = target_ids(&successful);
            assert!(
                !ids.contains(&"concept-retired".to_string()),
                "the retired code-owned Concept denies through the bound view as well: {ids:?}"
            );
            assert!(
                ids.contains(&"concept-ordinary".to_string()),
                "the ordinary Concept keeps flowing through the bound view: {ids:?}"
            );
        }

        /// Sibling orders are order-independent: [current, stale] /
        /// [stale, current], [current, unknown] / [unknown, current], and
        /// denied siblings in both orders all deny; the all-eligible pair
        /// survives in both orders.
        #[test]
        fn sibling_orders_composed() {
            let dir = tempfile::tempdir().expect("tempdir");
            let fixture = fixture(dir.path());
            let eligible = evidence_ref(EvidenceTargetType::Chunk, "chunk-eligible");
            let stale = evidence_ref(EvidenceTargetType::Chunk, "chunk-stale");
            let unknown = evidence_ref(EvidenceTargetType::Chunk, "chunk-never-written");
            let doc = evidence_ref(EvidenceTargetType::Chunk, "chunk-doc");
            let current_stale = write_memory_with_refs(
                &fixture,
                "parse current stale",
                vec![eligible.clone(), stale.clone()],
            );
            let stale_current = write_memory_with_refs(
                &fixture,
                "parse stale current",
                vec![stale, eligible.clone()],
            );
            let current_unknown = write_memory_with_refs(
                &fixture,
                "parse current unknown",
                vec![eligible.clone(), unknown.clone()],
            );
            let unknown_current = write_memory_with_refs(
                &fixture,
                "parse unknown current",
                vec![unknown, eligible.clone()],
            );
            let all_eligible = write_memory_with_refs(
                &fixture,
                "parse all eligible",
                vec![eligible.clone(), doc.clone()],
            );
            let all_eligible_reversed = write_memory_with_refs(
                &fixture,
                "parse all eligible reversed",
                vec![doc, eligible],
            );

            let payload =
                block_on(composed_recall(&fixture).recall(request("parse", 20))).expect("recall");
            let ids = target_ids(&payload);
            for denied in [
                current_stale.id.as_str(),
                stale_current.id.as_str(),
                current_unknown.id.as_str(),
                unknown_current.id.as_str(),
            ] {
                assert!(
                    !ids.iter().any(|id| id == denied),
                    "mixed sibling order must deny regardless of order: {ids:?}"
                );
            }
            assert!(
                ids.iter().any(|id| id == all_eligible.id.as_str())
                    && ids.iter().any(|id| id == all_eligible_reversed.id.as_str()),
                "all-eligible siblings survive in both orders: {ids:?}"
            );
        }

        /// Denied siblings in both orders through a DENYING owning
        /// authorizer (authorization failure inside evidence, not missing
        /// records).
        #[test]
        fn denied_siblings_both_orders_composed() {
            let dir = tempfile::tempdir().expect("tempdir");
            let fixture = fixture(dir.path());
            put_denied_source_chunk(&fixture, "chunk-denied-matrix", "parse denied sibling");
            let denied = evidence_ref(EvidenceTargetType::Chunk, "chunk-denied-matrix");
            let eligible = evidence_ref(EvidenceTargetType::Chunk, "chunk-eligible");
            let first = write_memory_with_refs(
                &fixture,
                "parse eligible denied",
                vec![eligible.clone(), denied.clone()],
            );
            let second =
                write_memory_with_refs(&fixture, "parse denied eligible", vec![denied, eligible]);
            let recall = composed_recall_with_evidence(&fixture, denying_evidence(&fixture));
            let payload = block_on(recall.recall(request("parse", 20))).expect("recall");
            let ids = target_ids(&payload);
            assert!(
                !ids.iter()
                    .any(|id| id == first.id.as_str() || id == second.id.as_str()),
                "a denied sibling denies in both orders: {ids:?}"
            );
            assert!(
                ids.contains(&"chunk-doc".to_string()),
                "positively authorized ordinary chunk survives under the denying authorizer: {ids:?}"
            );
        }

        /// Nested chains and cycles: a chain into eligible code survives, a
        /// chain into stale code denies; an ordinary cycle survives; cycles
        /// containing code or invalid dependencies deny.
        #[test]
        fn nested_chains_and_cycles_composed() {
            let dir = tempfile::tempdir().expect("tempdir");
            let fixture = fixture(dir.path());
            // Nested chain into eligible code.
            let mid_eligible = write_memory_with_refs(
                &fixture,
                "parse mid eligible",
                vec![evidence_ref(EvidenceTargetType::Chunk, "chunk-eligible")],
            );
            let chain_eligible = write_memory_with_refs(
                &fixture,
                "parse chain eligible",
                vec![evidence_ref(
                    EvidenceTargetType::Memory,
                    mid_eligible.id.as_str(),
                )],
            );
            // Nested chain into stale code.
            let mid_stale = write_memory_with_refs(
                &fixture,
                "parse mid stale",
                vec![evidence_ref(EvidenceTargetType::Chunk, "chunk-stale")],
            );
            let chain_stale = write_memory_with_refs(
                &fixture,
                "parse chain stale",
                vec![evidence_ref(
                    EvidenceTargetType::Memory,
                    mid_stale.id.as_str(),
                )],
            );
            // Ordinary cycle: A ↔ B.
            put_memory_with_id(
                &fixture,
                "memory-cycle-a",
                "parse cycle alpha",
                vec![evidence_ref(EvidenceTargetType::Memory, "memory-cycle-b")],
            );
            put_memory_with_id(
                &fixture,
                "memory-cycle-b",
                "parse cycle beta",
                vec![evidence_ref(EvidenceTargetType::Memory, "memory-cycle-a")],
            );
            // Cycle containing code.
            put_memory_with_id(
                &fixture,
                "memory-code-cycle-a",
                "parse code cycle alpha",
                vec![evidence_ref(
                    EvidenceTargetType::Memory,
                    "memory-code-cycle-b",
                )],
            );
            put_memory_with_id(
                &fixture,
                "memory-code-cycle-b",
                "parse code cycle beta",
                vec![
                    evidence_ref(EvidenceTargetType::Memory, "memory-code-cycle-a"),
                    evidence_ref(EvidenceTargetType::Chunk, "chunk-stale"),
                ],
            );
            // Cycle containing an invalid dependency.
            put_memory_with_id(
                &fixture,
                "memory-invalid-cycle-a",
                "parse invalid cycle alpha",
                vec![evidence_ref(
                    EvidenceTargetType::Memory,
                    "memory-invalid-cycle-b",
                )],
            );
            put_memory_with_id(
                &fixture,
                "memory-invalid-cycle-b",
                "parse invalid cycle beta",
                vec![
                    evidence_ref(EvidenceTargetType::Memory, "memory-invalid-cycle-a"),
                    evidence_ref(EvidenceTargetType::Chunk, "chunk-never-written"),
                ],
            );

            let payload =
                block_on(composed_recall(&fixture).recall(request("parse", 30))).expect("recall");
            let ids = target_ids(&payload);
            assert!(
                ids.iter().any(|id| id == chain_eligible.id.as_str()),
                "nested chain into eligible code survives: {ids:?}"
            );
            assert!(
                !ids.iter().any(|id| id == chain_stale.id.as_str()),
                "nested chain into stale code denies: {ids:?}"
            );
            assert!(
                ids.contains(&"memory-cycle-a".to_string())
                    && ids.contains(&"memory-cycle-b".to_string()),
                "ordinary cycle survives: {ids:?}"
            );
            assert!(
                !ids.contains(&"memory-code-cycle-a".to_string())
                    && !ids.contains(&"memory-code-cycle-b".to_string()),
                "cycle containing stale code denies: {ids:?}"
            );
            assert!(
                !ids.contains(&"memory-invalid-cycle-b".to_string()),
                "cycle containing an invalid dependency denies: {ids:?}"
            );
        }

        /// Allowed chunk + denied source: the chunk's own policy is fine but
        /// its SOURCE policy denies — the chunk denies through composed
        /// recall.
        #[test]
        fn allowed_chunk_denied_source_composed() {
            let dir = tempfile::tempdir().expect("tempdir");
            let fixture = fixture(dir.path());
            put_denied_source_chunk(
                &fixture,
                "chunk-denied-source-matrix",
                "parse deniedsource token",
            );
            let recall = composed_recall_with_evidence(&fixture, denying_evidence(&fixture));
            let payload = block_on(recall.recall(request("deniedsource", 20))).expect("recall");
            let ids = target_ids(&payload);
            assert!(
                !ids.contains(&"chunk-denied-source-matrix".to_string()),
                "allowed chunk with denied source denies: {ids:?}"
            );
        }

        /// SOURCE-PROVENANCE-ONLY code dependency: an ordinary (non-repo)
        /// source whose provenance references code; the chunk itself has no
        /// dependency refs — only the walked source provenance surfaces the
        /// code dependency.
        #[test]
        fn source_provenance_only_code_dependency_composed() {
            use engram_knowledge::KnowledgeRepository as _;
            let dir = tempfile::tempdir().expect("tempdir");
            let fixture = fixture(dir.path());
            let put_source_with_provenance = |id: &str, refs: Vec<EvidenceRef>| {
                let source = engram_domain::KnowledgeSource {
                    id: Id::from(id),
                    kind: engram_domain::SourceKind::Upload,
                    scope: scope(),
                    name: format!("{id} name"),
                    uri: None,
                    version: None,
                    repository_key: None,
                    git_ref: None,
                    policy: policy(),
                    provenance: provenance_with(refs),
                    created_at: chrono::Utc::now(),
                    updated_at: None,
                    metadata: None,
                };
                block_on(fixture.knowledge.put_source(source)).expect("put source");
            };
            put_source_with_provenance(
                "source-prov-stale",
                vec![evidence_ref(EvidenceTargetType::Chunk, "chunk-stale")],
            );
            put_source_with_provenance(
                "source-prov-eligible",
                vec![evidence_ref(EvidenceTargetType::Chunk, "chunk-eligible")],
            );
            let mut stale_chunk = chunk(
                "chunk-source-prov-stale",
                "parse srcprovstale body",
                KnowledgeChunkKind::Paragraph,
            );
            stale_chunk.source_id = Id::from("source-prov-stale");
            let mut eligible_chunk = chunk(
                "chunk-source-prov-eligible",
                "parse srcproveligible body",
                KnowledgeChunkKind::Paragraph,
            );
            eligible_chunk.source_id = Id::from("source-prov-eligible");
            block_on(fixture.knowledge.put_chunk(stale_chunk)).expect("put chunk");
            block_on(fixture.knowledge.put_chunk(eligible_chunk)).expect("put chunk");

            let payload =
                block_on(composed_recall(&fixture).recall(request("srcprov", 20))).expect("recall");
            let ids = target_ids(&payload);
            assert!(
                !ids.contains(&"chunk-source-prov-stale".to_string()),
                "source-provenance-only stale code dependency denies: {ids:?}"
            );
            assert!(
                ids.contains(&"chunk-source-prov-eligible".to_string()),
                "source-provenance-only eligible code dependency survives: {ids:?}"
            );
        }

        /// ENTITY `source_refs`-ONLY code dependency: an ordinary entity
        /// whose typed `source_refs` (not provenance) reference code.
        #[test]
        fn entity_source_refs_only_code_dependency_composed() {
            let dir = tempfile::tempdir().expect("tempdir");
            let fixture = fixture(dir.path());
            let mut stale_entity = symbol_entity("entity-sr-stale", "parseSrStale");
            stale_entity.kind = EntityKind::Concept;
            stale_entity.source_refs = vec![evidence_ref(EvidenceTargetType::Chunk, "chunk-stale")];
            let mut eligible_entity = symbol_entity("entity-sr-eligible", "parseSrEligible");
            eligible_entity.kind = EntityKind::Concept;
            eligible_entity.source_refs =
                vec![evidence_ref(EvidenceTargetType::Chunk, "chunk-eligible")];
            block_on(fixture.knowledge.put_entity(stale_entity)).expect("put entity");
            block_on(fixture.knowledge.put_entity(eligible_entity)).expect("put entity");

            let payload =
                block_on(composed_recall(&fixture).recall(request("parseSr", 20))).expect("recall");
            let ids = target_ids(&payload);
            assert!(
                !ids.contains(&"entity-sr-stale".to_string()),
                "entity source_refs-only stale code dependency denies: {ids:?}"
            );
            assert!(
                ids.contains(&"entity-sr-eligible".to_string()),
                "entity source_refs-only eligible code dependency survives: {ids:?}"
            );
        }

        /// OWN-LINEAGE row for Memory and Belief (empty provenance), plus
        /// REFERENCES to own-code records: published lineage is eligible
        /// through the view; staged-only lineage denies; a reference to an
        /// own-code record carries its dependency.
        #[test]
        fn own_lineage_memory_belief_composed() {
            let dir = tempfile::tempdir().expect("tempdir");
            let fixture = fixture(dir.path());
            let own_memory =
                write_memory_with_refs(&fixture, "parse own lineage memory", Vec::new());
            let referencing = write_memory_with_refs(
                &fixture,
                "parse references own lineage",
                vec![evidence_ref(
                    EvidenceTargetType::Memory,
                    own_memory.id.as_str(),
                )],
            );
            put_belief_matrix(
                &fixture,
                "belief-own-lineage",
                "parse",
                "parse own lineage belief",
                Vec::new(),
                Vec::new(),
                Vec::new(),
            );
            // Published generation owning the memory, the referencing
            // chain root and the belief.
            publish_with_lineage(
                &fixture.bindings,
                "repo-a",
                "gen-a-3",
                2,
                Some("gen-a-2"),
                &[
                    own_memory.id.as_str(),
                    referencing.id.as_str(),
                    "belief-own-lineage",
                ],
            );
            // Staged-only (unpublished) lineage for a second memory.
            let staged_memory =
                write_memory_with_refs(&fixture, "parse staged lineage memory", Vec::new());
            {
                let repo_id = engram_domain::RepoId("repo-b".to_string());
                block_on(fixture.bindings.begin_attempt(
                    &scope(),
                    &repo_id,
                    generation("gen-b-2"),
                    attempt("attempt-repo-b-gen-b-2", 0, None),
                ))
                .expect("begin staged attempt");
                block_on(fixture.bindings.put_lineage(
                    &scope(),
                    staged_memory.id.as_str(),
                    engram_domain::CodeLineage {
                        repo_id,
                        generation_id: "gen-b-2".to_string(),
                        source_path: "src/staged.rs".to_string(),
                        contributors: vec![staged_memory.id.as_str().to_string()],
                    },
                ))
                .expect("staged lineage");
            }

            let payload =
                block_on(composed_recall(&fixture).recall(request("parse", 30))).expect("recall");
            let ids = target_ids(&payload);
            assert!(
                ids.iter().any(|id| id == own_memory.id.as_str()),
                "own-lineage memory with EMPTY provenance under a published generation is view-eligible: {ids:?}"
            );
            assert!(
                ids.contains(&"belief-own-lineage".to_string()),
                "own-lineage belief with EMPTY provenance under a published generation is view-eligible: {ids:?}"
            );
            assert!(
                ids.iter().any(|id| id == referencing.id.as_str()),
                "a reference to an own-code record carries its dependency (eligible here): {ids:?}"
            );
            assert!(
                !ids.iter().any(|id| id == staged_memory.id.as_str()),
                "staged-only lineage denies: {ids:?}"
            );
        }

        /// Lane projection binding: the lexical entity `"{name} {kind:?}"`
        /// projection and a long chunk bind; ALTERED payloads deny.
        #[test]
        fn lane_projection_binding_composed() {
            use engram_knowledge::KnowledgeRepository as _;
            let dir = tempfile::tempdir().expect("tempdir");
            let fixture = fixture(dir.path());
            // Lexical entity projection positive (the bound lexical lane
            // publishes "{name} {kind:?}"). The lane's rehydration gate
            // requires a generation stamp on symbol entities (stamps are NOT
            // authority — durable lineage decides below); stamp the fixture
            // entity consistently with its lineage.
            {
                use engram_knowledge::KnowledgeRepository as _;
                let mut stamped = symbol_entity("entity-published", "parsePublished");
                let mut metadata = Metadata::new();
                metadata.insert(
                    engram_retrieval::bound_filter::CODE_GENERATION_KEY.to_string(),
                    serde_json::Value::String("gen-a-2".to_string()),
                );
                stamped.metadata = Some(metadata);
                block_on(fixture.knowledge.put_entity(stamped)).expect("put stamped entity");
            }
            let recall = fixture
                .bound_recall()
                .with_candidate_evidence(fixture.candidate_evidence());
            let payload = block_on(recall.recall(request("parsePublished", 20))).expect("recall");
            let ids = target_ids(&payload);
            assert!(
                ids.contains(&"entity-published".to_string()),
                "the lexical entity projection binds (legitimate lexical Entity positive): {ids:?}"
            );

            // Long chunk positive through the graph lane (full text).
            let long_text = format!("parse longchunk {}", "x".repeat(3000));
            let mut long_chunk = chunk("chunk-long", &long_text, KnowledgeChunkKind::Paragraph);
            long_chunk.source_id = Id::from("source-ordinary-task-013");
            block_on(fixture.knowledge.put_chunk(long_chunk)).expect("put long chunk");
            let payload = block_on(composed_recall(&fixture).recall(request("longchunk", 20)))
                .expect("recall");
            let ids = target_ids(&payload);
            assert!(
                ids.contains(&"chunk-long".to_string()),
                "a long graph chunk binds through the full-text projection: {ids:?}"
            );

            // Resolver-level projection asserts for the pgvector clipping
            // shapes (SQLite lanes do not clip; the binding helper is
            // shared): clipped-at-2000 and capped-at-2048 bind, altered
            // content denies.
            let record = block_on(
                fixture
                    .knowledge
                    .get_chunk(&Id::from("chunk-long"), &scope()),
            )
            .expect("get long chunk")
            .expect("chunk exists");
            let resolver = fixture.candidate_evidence();
            let clipped_2000: String = record.text.chars().take(2000).collect();
            let capped_2048: String = record.text.chars().take(2048).collect();
            for (label, content, expected) in [
                (
                    "clipped-2000 projection binds",
                    clipped_2000.clone(),
                    CandidateEvidence::Ordinary,
                ),
                (
                    "capped-2048 projection binds",
                    capped_2048.clone(),
                    CandidateEvidence::Ordinary,
                ),
                (
                    "altered projection denies",
                    format!("{}!", clipped_2000),
                    CandidateEvidence::Unknown,
                ),
                (
                    "altered cap denies",
                    format!("{}?", capped_2048),
                    CandidateEvidence::Unknown,
                ),
            ] {
                let candidate = RetrievalResult {
                    id: "projection-candidate".to_string(),
                    target_type: RetrievalTargetType::Chunk,
                    target_id: "chunk-long".to_string(),
                    content,
                    score: engram_domain::RetrievalScore {
                        total: 1.0,
                        relevance: Some(1.0),
                        recency: None,
                        confidence: None,
                        cue_match: None,
                        hierarchical_fit: None,
                        policy_fit: Some(1.0),
                    },
                    provenance: record.provenance.clone(),
                    policy: record.policy.clone(),
                    explanation: None,
                    fusion_trace: None,
                    metadata: None,
                };
                let classification = block_on(resolver.evidence(&candidate, &request("parse", 10)))
                    .expect("evidence resolution");
                assert_eq!(classification, expected, "{label}");
            }
        }

        /// Altered-payload NEGATIVE through composed recall: a lane that
        /// injects a candidate whose payload does not match the canonical
        /// record (or its deterministic projections) is denied by the
        /// post-lane guard.
        #[test]
        fn altered_payload_candidate_denied_composed() {
            let dir = tempfile::tempdir().expect("tempdir");
            let fixture = fixture(dir.path());
            let record = block_on(engram_knowledge::KnowledgeRepository::get_entity(
                fixture.knowledge.as_ref(),
                &Id::from("entity-published"),
                &scope(),
            ))
            .expect("get entity")
            .expect("entity exists");
            let forged = RetrievalResult {
                id: "forged-entity".to_string(),
                target_type: RetrievalTargetType::Entity,
                target_id: "entity-published".to_string(),
                content: format!("{} {:?} FORGED", record.name, record.kind),
                score: engram_domain::RetrievalScore {
                    total: 1.0,
                    relevance: Some(1.0),
                    recency: None,
                    confidence: None,
                    cue_match: None,
                    hierarchical_fit: None,
                    policy_fit: Some(1.0),
                },
                provenance: record.provenance.clone(),
                policy: policy(),
                explanation: None,
                fusion_trace: None,
                metadata: None,
            };
            struct ForgedLane(RetrievalResult);
            #[async_trait::async_trait]
            impl RetrievalIndex for ForgedLane {
                async fn retrieve_candidates(
                    &self,
                    _request: &RetrievalRequest,
                ) -> CoreResult<Vec<RetrievalResult>> {
                    Ok(vec![self.0.clone()])
                }
            }
            let recall = SqlUnifiedRecall::new(
                fixture.memory.clone(),
                vec![Arc::new(ForgedLane(forged)) as Arc<dyn RetrievalIndex>],
                fixture.beliefs.clone(),
            )
            .with_code_eligibility(
                fixture.eligibility() as Arc<dyn engram_retrieval::CodeEligibilityResolver>
            )
            .with_candidate_evidence(fixture.candidate_evidence());
            let payload = block_on(recall.recall(request("parse", 20))).expect("recall");
            let ids = target_ids(&payload);
            assert!(
                !ids.iter().any(|id| id == "entity-published"),
                "an altered-payload candidate denies through the composed guard: {ids:?}"
            );
        }

        /// Wide inputs are bounded by INSTRUMENTED counters, not elapsed
        /// time: a beyond-budget reference array exhausts terminally with
        /// counters inside the documented bounds, and a hostile long-id
        /// array is rejected by the ADMISSION scan before any frame is
        /// enqueued.
        ///
        /// LABELING (review r5, finding 11): this test is RESOLVER-ONLY —
        /// it drives `classify_with_dependencies`/`evidence` directly, not
        /// composed recall. The composed-recall equivalents live in
        /// `r5_walk::wide_input_counters_bounded_composed` and
        /// `r5_walk::cyclic_input_counters_bounded_composed`.
        #[test]
        fn wide_input_counters_bounded() {
            use engram_retrieval::CandidateEvidenceResolver;
            let dir = tempfile::tempdir().expect("tempdir");
            let fixture = fixture(dir.path());
            let resolver = fixture.candidate_evidence();
            let counters = resolver.walk_counters();
            counters.reset();

            // Beyond-budget wide array of ordinary refs (all the same id —
            // one keyed lookup after dedupe).
            let ordinary_target = write_memory_with_refs(&fixture, "wide dep target", Vec::new());
            let wide = write_memory_with_refs(
                &fixture,
                "wide beyond budget",
                std::iter::repeat_n(
                    evidence_ref(EvidenceTargetType::Memory, ordinary_target.id.as_str()),
                    2000,
                )
                .collect(),
            );
            let classification = block_on(resolver.classify_with_dependencies(
                &RetrievalResult {
                    id: "wide".to_string(),
                    target_type: RetrievalTargetType::Memory,
                    target_id: wide.id.to_string(),
                    content: wide.content.text.clone(),
                    score: engram_domain::RetrievalScore {
                        total: 1.0,
                        relevance: Some(1.0),
                        recency: None,
                        confidence: None,
                        cue_match: None,
                        hierarchical_fit: None,
                        policy_fit: Some(1.0),
                    },
                    provenance: wide.provenance.clone(),
                    policy: wide.policy.clone(),
                    explanation: None,
                    fusion_trace: None,
                    metadata: None,
                },
                &request("parse", 10),
            ))
            .expect("classification");
            assert_eq!(
                classification.evidence,
                CandidateEvidence::Unknown,
                "a beyond-budget array is terminally Unknown"
            );
            assert!(
                !classification.complete,
                "exhaustion is distinct from a fully evaluated result (incomplete)"
            );
            let stats = counters.snapshot();
            assert!(
                stats.references_examined <= 1024,
                "reference examinations bounded by the work budget: {stats:?}"
            );
            assert!(stats.budget_exhaustions >= 1, "{stats:?}");
            assert!(
                stats.ownership_lookups <= stats.references_examined + 2,
                "keyed lookups never exceed references (dedupe + own identity): {stats:?}"
            );
            assert!(
                stats.max_queue_depth <= 16,
                "queue depth stays bounded: {stats:?}"
            );
            assert!(
                stats.visited_records <= 256,
                "visited set bounded by the record budget: {stats:?}"
            );
            assert!(
                stats.bytes_accounted <= 128 * 1024,
                "byte accounting bounded: {stats:?}"
            );

            // Hostile long ids: rejected BEFORE enqueueing (no frame, no
            // examination).
            counters.reset();
            let long_ids: Vec<EvidenceRef> = (0..300)
                .map(|index| {
                    evidence_ref(
                        EvidenceTargetType::Memory,
                        &format!("hostile-{index}-{}", "i".repeat(600)),
                    )
                })
                .collect();
            let hostile = write_memory_with_refs(&fixture, "hostile long ids", long_ids);
            let classification = block_on(resolver.evidence(
                &RetrievalResult {
                    id: "hostile".to_string(),
                    target_type: RetrievalTargetType::Memory,
                    target_id: hostile.id.to_string(),
                    content: hostile.content.text.clone(),
                    score: engram_domain::RetrievalScore {
                        total: 1.0,
                        relevance: Some(1.0),
                        recency: None,
                        confidence: None,
                        cue_match: None,
                        hierarchical_fit: None,
                        policy_fit: Some(1.0),
                    },
                    provenance: hostile.provenance.clone(),
                    policy: hostile.policy.clone(),
                    explanation: None,
                    fusion_trace: None,
                    metadata: None,
                },
                &request("parse", 10),
            ))
            .expect("classification");
            assert_eq!(classification, CandidateEvidence::Unknown);
            let stats = counters.snapshot();
            assert_eq!(
                stats.frames_enqueued, 0,
                "the hostile frame is rejected BEFORE enqueueing: {stats:?}"
            );
            assert_eq!(
                stats.references_examined, 0,
                "no reference is examined for a rejected frame: {stats:?}"
            );
            assert!(stats.budget_exhaustions >= 1, "{stats:?}");
        }

        /// Cyclic inputs are bounded and instrumented: back-edges are
        /// counted, the visited set and queue stay bounded, and repeated
        /// references reuse the keyed lookup instead of re-materializing.
        ///
        /// LABELING (review r5, finding 11): RESOLVER-ONLY — the
        /// composed-recall equivalent is
        /// `r5_walk::cyclic_input_counters_bounded_composed`.
        #[test]
        fn cyclic_input_counters_bounded() {
            use engram_retrieval::CandidateEvidenceResolver;
            let dir = tempfile::tempdir().expect("tempdir");
            let fixture = fixture(dir.path());
            let resolver = fixture.candidate_evidence();
            let counters = resolver.walk_counters();
            counters.reset();

            // A ⇄ B cycle, each side repeating the sibling reference many
            // times (within the total work budget: 3 × 300 ≤ 1024): the
            // FIRST visit resolves; the rest are back-edges.
            put_memory_with_id(
                &fixture,
                "memory-counter-b",
                "counter cycle beta",
                std::iter::repeat_n(
                    evidence_ref(EvidenceTargetType::Memory, "memory-counter-a"),
                    300,
                )
                .collect(),
            );
            let a = put_memory_with_id(
                &fixture,
                "memory-counter-a",
                "counter cycle alpha",
                std::iter::repeat_n(
                    evidence_ref(EvidenceTargetType::Memory, "memory-counter-b"),
                    300,
                )
                .collect(),
            );

            let classification = block_on(resolver.evidence(
                &RetrievalResult {
                    id: "cycle".to_string(),
                    target_type: RetrievalTargetType::Memory,
                    target_id: a.id.to_string(),
                    content: a.content.text.clone(),
                    score: engram_domain::RetrievalScore {
                        total: 1.0,
                        relevance: Some(1.0),
                        recency: None,
                        confidence: None,
                        cue_match: None,
                        hierarchical_fit: None,
                        policy_fit: Some(1.0),
                    },
                    provenance: a.provenance.clone(),
                    policy: a.policy.clone(),
                    explanation: None,
                    fusion_trace: None,
                    metadata: None,
                },
                &request("parse", 10),
            ))
            .expect("classification");
            assert_eq!(
                classification,
                CandidateEvidence::Ordinary,
                "an ordinary cycle stays ordinary within budget"
            );
            let stats = counters.snapshot();
            assert!(
                stats.cycle_backedges >= 1,
                "cycle back-edges are instrumented: {stats:?}"
            );
            assert!(
                stats.ownership_lookups <= 3,
                "repeated references reuse the keyed lookup (own + two records): {stats:?}"
            );
            assert!(
                stats.visited_records <= 256 && stats.max_queue_depth <= 16,
                "set and queue bounded: {stats:?}"
            );
            assert_eq!(
                stats.budget_exhaustions, 0,
                "a within-budget cycle does not exhaust: {stats:?}"
            );
        }
        /// Review r5 walk corrections (findings 1/4/5/6/7/9/11-walk) — decisive
        /// cases through REAL composed recall unless a case is explicitly
        /// resolver-level and labeled as such.
        mod r5_walk {
            use super::*;

            /// F1 decisive (the combination the A8 matrix missed): an OWN-CODE
            /// candidate (eligible published lineage) with an UNRESOLVED
            /// dependency denies — own identity never converts an Unknown walk
            /// into accepted code evidence. The resolved twin with an ordinary
            /// dependency stays accepted.
            #[test]
            fn own_code_with_unresolved_dependency_denies_composed() {
                use engram_retrieval::CandidateEvidenceResolver;
                let dir = tempfile::tempdir().expect("tempdir");
                let fixture = fixture(dir.path());
                let unresolved = write_memory_with_refs(
                    &fixture,
                    "owncode unresolved dep",
                    vec![evidence_ref(
                        EvidenceTargetType::Memory,
                        "memory-never-written",
                    )],
                );
                let resolved = write_memory_with_refs(
                    &fixture,
                    "owncode resolved dep",
                    vec![evidence_ref(
                        EvidenceTargetType::Memory,
                        fixture.memory_id.as_str(),
                    )],
                );
                publish_with_lineage(
                    &fixture.bindings,
                    "repo-a",
                    "gen-a-3",
                    2,
                    Some("gen-a-2"),
                    &[unresolved.id.as_str(), resolved.id.as_str()],
                );

                // Resolver-level: the unresolved member keeps the classification
                // Unknown (finding 1 — merge_own preserves Unknown).
                let resolver = fixture.candidate_evidence();
                let candidate_of = |record: &engram_domain::MemoryRecord| RetrievalResult {
                    id: "owncode".to_string(),
                    target_type: RetrievalTargetType::Memory,
                    target_id: record.id.to_string(),
                    content: record.content.text.clone(),
                    score: engram_domain::RetrievalScore {
                        total: 1.0,
                        relevance: Some(1.0),
                        recency: None,
                        confidence: None,
                        cue_match: None,
                        hierarchical_fit: None,
                        policy_fit: Some(1.0),
                    },
                    provenance: record.provenance.clone(),
                    policy: record.policy.clone(),
                    explanation: None,
                    fusion_trace: None,
                    metadata: None,
                };
                assert_eq!(
                    block_on(resolver.evidence(&candidate_of(&unresolved), &request("parse", 10)))
                        .expect("classification"),
                    CandidateEvidence::Unknown,
                    "own code identity cannot convert an unresolved walk into code evidence"
                );

                // Composed: the unresolved twin denies, the resolved twin stays.
                let payload = block_on(composed_recall(&fixture).recall(request("owncode", 20)))
                    .expect("recall");
                let ids = target_ids(&payload);
                assert!(
                    !ids.iter().any(|id| id == unresolved.id.as_str()),
                    "own-code + unresolved dependency denies through composed recall: {ids:?}"
                );
                assert!(
                    ids.iter().any(|id| id == resolved.id.as_str()),
                    "the resolved twin stays accepted (CodeDependent with eligible deps): {ids:?}"
                );
            }

            /// F6 decisive: an AGGREGATE-ONLY evidence resolver (trait default
            /// `classify_with_dependencies` = explicitly incomplete) can never
            /// pass the DEGRADED path — its Ordinary aggregate is not complete,
            /// positively ordinary evidence, so nothing flows while the code
            /// authority is unavailable.
            #[test]
            fn degraded_recall_rejects_aggregate_only_ordinary() {
                struct AggregateOnlyOrdinary;
                #[async_trait::async_trait]
                impl engram_retrieval::CandidateEvidenceResolver for AggregateOnlyOrdinary {
                    async fn evidence(
                        &self,
                        _candidate: &RetrievalResult,
                        _request: &RetrievalRequest,
                    ) -> CoreResult<CandidateEvidence> {
                        // Aggregate-only stub: claims Ordinary for everything.
                        Ok(CandidateEvidence::Ordinary)
                    }
                }
                let dir = tempfile::tempdir().expect("tempdir");
                let fixture = fixture(dir.path());
                let graph_lane: Arc<dyn RetrievalIndex> =
                    Arc::new(engram_store_sqlite::knowledge::GraphRetrievalIndex::new(
                        fixture.knowledge.clone(),
                    ));
                let recall = SqlUnifiedRecall::new(
                    fixture.memory.clone(),
                    vec![graph_lane],
                    fixture.beliefs.clone(),
                )
                .with_code_eligibility(Arc::new(ErroringAuthority))
                .with_candidate_evidence(Arc::new(AggregateOnlyOrdinary));
                let payload = block_on(recall.recall(request("parse", 20))).expect("recall");
                assert!(
                    payload.items.is_empty(),
                    "an aggregate-only resolver cannot pass the degraded completeness contract: {:?}",
                    target_ids(&payload)
                );
                assert!(
                    payload
                        .source_failures
                        .iter()
                        .any(|failure| failure.source == "code_eligibility"),
                    "the authority error stays surfaced: {:?}",
                    payload.source_failures
                );
            }

            /// F4 decisive matrix through REAL composed recall: relationship
            /// subject/object endpoints resolve through the bounded canonical
            /// Entity path — permitted ordinary and eligible-code endpoints
            /// accept, stale-code / denied-policy / missing / foreign-scope /
            /// ID-less endpoints deny, and a cycle through an endpoint settles.
            #[test]
            fn relationship_endpoint_matrix_composed() {
                use engram_knowledge::KnowledgeRepository as _;
                let dir = tempfile::tempdir().expect("tempdir");
                let fixture = fixture(dir.path());

                // Ordinary endpoint entities (canonical graph).
                for id in ["entity-ep-a", "entity-ep-b", "entity-ep-cyc"] {
                    let mut concept = symbol_entity(id, id);
                    concept.kind = EntityKind::Concept;
                    block_on(fixture.knowledge.put_entity(concept)).expect("put endpoint");
                }
                // A stale-code endpoint entity (durable lineage of gen-a-1).
                block_on(
                    fixture
                        .knowledge
                        .put_entity(symbol_entity("entity-ep-stale", "epStale")),
                )
                .expect("put stale endpoint");
                block_on(fixture.bindings.put_lineage(
                    &scope(),
                    "entity-ep-stale",
                    CodeLineage {
                        repo_id: RepoId("repo-a".to_string()),
                        generation_id: "gen-a-1".to_string(),
                        source_path: "src/ep-stale.rs".to_string(),
                        contributors: vec!["entity-ep-stale".to_string()],
                    },
                ))
                .expect("stale endpoint lineage");
                // A denied-policy endpoint entity (High-sensitivity graph).
                let mut high_policy = policy();
                high_policy.sensitivity = Some(engram_domain::Sensitivity::High);
                block_on(fixture.knowledge.put_graph(KnowledgeGraph {
                    id: Id::from("graph-ep-high"),
                    scope: scope(),
                    name: "high endpoint graph".to_string(),
                    uri: None,
                    version: None,
                    ontology_refs: Vec::new(),
                    policy: high_policy,
                    provenance: provenance(),
                    created_at: chrono::Utc::now(),
                    updated_at: None,
                    metadata: None,
                }))
                .expect("put high graph");
                let mut high_endpoint = symbol_entity("entity-ep-high", "epHigh");
                high_endpoint.kind = EntityKind::Concept;
                high_endpoint.graph_id = Some(Id::from("graph-ep-high"));
                block_on(fixture.knowledge.put_entity(high_endpoint)).expect("put high endpoint");
                // A foreign-scope endpoint entity (same id, different scope →
                // the canonical request-scope read must not resolve it).
                let mut foreign_scope = scope();
                foreign_scope.workspace = Some("endpoint-foreign".to_string());
                let mut foreign_endpoint = symbol_entity("entity-ep-foreign", "epForeign");
                foreign_endpoint.kind = EntityKind::Concept;
                foreign_endpoint.scope = foreign_scope;
                block_on(fixture.knowledge.put_entity(foreign_endpoint))
                    .expect("put foreign endpoint");

                let rel =
                    |id: &str, subject: Option<&str>, object: Option<&str>, object_idless: bool| {
                        let endpoint = |entity_id: Option<&str>| engram_domain::EntityRef {
                            id: entity_id.map(Id::from),
                            kind: None,
                            name: entity_id.map(|id| id.to_string()),
                            aliases: Vec::new(),
                        };
                        let mut object = endpoint(object);
                        if object_idless {
                            object.id = None;
                        }
                        engram_domain::KnowledgeRelationship {
                            id: engram_domain::RelationshipId::from(id),
                            graph_id: Some(Id::from("graph-task-013")),
                            predicate: "mentions".to_string(),
                            subject: endpoint(subject),
                            object,
                            scope: scope(),
                            evidence: Vec::new(),
                            confidence: None,
                            provenance: provenance(),
                            created_at: chrono::Utc::now(),
                            updated_at: None,
                            code_resolution: None,
                            archived_at: None,
                        }
                    };
                let ordinary = rel(
                    "rel-ep-ordinary",
                    Some("entity-ep-a"),
                    Some("entity-ep-b"),
                    false,
                );
                let eligible = rel(
                    "rel-ep-eligible",
                    Some("entity-ep-a"),
                    Some("entity-published"),
                    false,
                );
                let stale = rel(
                    "rel-ep-stale",
                    Some("entity-ep-a"),
                    Some("entity-ep-stale"),
                    false,
                );
                let missing = rel(
                    "rel-ep-missing",
                    Some("entity-ep-a"),
                    Some("entity-ep-never-written"),
                    false,
                );
                let foreign = rel(
                    "rel-ep-foreign",
                    Some("entity-ep-a"),
                    Some("entity-ep-foreign"),
                    false,
                );
                let idless = rel("rel-ep-idless", Some("entity-ep-a"), None, true);
                for relationship in [ordinary, eligible, stale, missing, foreign, idless] {
                    block_on(fixture.knowledge.put_relationship(relationship))
                        .expect("put relationship");
                }
                // CYCLIC endpoints: the endpoint entity's own provenance
                // references the very memory that references the relationship —
                // the walk must settle through the visited-set back-edge.
                let cycle_memory =
                    write_memory_with_refs(&fixture, "endpoint cycle memory", Vec::new());
                let mut cyc_endpoint = symbol_entity("entity-ep-cyc", "entity-ep-cyc");
                cyc_endpoint.kind = EntityKind::Concept;
                cyc_endpoint.provenance = provenance_with(vec![evidence_ref(
                    EvidenceTargetType::Memory,
                    cycle_memory.id.as_str(),
                )]);
                block_on(fixture.knowledge.put_entity(cyc_endpoint)).expect("put cyclic endpoint");
                let cyclic = rel(
                    "rel-ep-cyclic",
                    Some("entity-ep-a"),
                    Some("entity-ep-cyc"),
                    false,
                );
                block_on(fixture.knowledge.put_relationship(cyclic)).expect("put cyclic rel");

                let chained = |rel_id: &str| {
                    write_memory_with_refs(
                        &fixture,
                        &format!("endpoint chain {rel_id}"),
                        vec![evidence_ref(EvidenceTargetType::Relationship, rel_id)],
                    )
                };
                let ordinary_mem = chained("rel-ep-ordinary");
                let eligible_mem = chained("rel-ep-eligible");
                let stale_mem = chained("rel-ep-stale");
                let missing_mem = chained("rel-ep-missing");
                let foreign_mem = chained("rel-ep-foreign");
                let idless_mem = chained("rel-ep-idless");
                let cyclic_mem = write_memory_with_refs(
                    &fixture,
                    "endpoint chain cyclic",
                    vec![evidence_ref(
                        EvidenceTargetType::Relationship,
                        "rel-ep-cyclic",
                    )],
                );
                let _ = cycle_memory;

                // Permitted view (real authorizer): ordinary/eligible/cyclic
                // chains accept; stale/missing/foreign/idless deny.
                let payload = block_on(composed_recall(&fixture).recall(request("endpoint", 20)))
                    .expect("recall");
                let ids = target_ids(&payload);
                for (label, case_id, expected_present) in [
                    ("ordinary endpoints accept", ordinary_mem.id.as_str(), true),
                    (
                        "eligible-code endpoint accepts as code-dependent",
                        eligible_mem.id.as_str(),
                        true,
                    ),
                    ("cyclic endpoint settles", cyclic_mem.id.as_str(), true),
                    ("stale-code endpoint denies", stale_mem.id.as_str(), false),
                    ("missing endpoint denies", missing_mem.id.as_str(), false),
                    (
                        "foreign-scope endpoint denies",
                        foreign_mem.id.as_str(),
                        false,
                    ),
                    ("ID-less endpoint denies", idless_mem.id.as_str(), false),
                ] {
                    assert_eq!(
                        ids.iter().any(|id| id == case_id),
                        expected_present,
                        "{label}: {ids:?}"
                    );
                }

                // DENIED-policy endpoint through a denying owning authorizer:
                // the high-graph endpoint chain denies while the ordinary chain
                // survives the same page.
                let high_mem = write_memory_with_refs(
                    &fixture,
                    "endpoint chain high",
                    vec![evidence_ref(
                        EvidenceTargetType::Relationship,
                        "rel-ep-high",
                    )],
                );
                let _ = high_mem;
                let mut high_rel = rel(
                    "rel-ep-high",
                    Some("entity-ep-a"),
                    Some("entity-ep-high"),
                    false,
                );
                high_rel.predicate = "mentions".to_string();
                block_on(fixture.knowledge.put_relationship(high_rel)).expect("put high rel");
                let high_chain = write_memory_with_refs(
                    &fixture,
                    "endpoint chain high",
                    vec![evidence_ref(
                        EvidenceTargetType::Relationship,
                        "rel-ep-high",
                    )],
                );
                let denying = block_on(
                    composed_recall_with_evidence(&fixture, denying_evidence(&fixture))
                        .recall(request("endpoint", 20)),
                )
                .expect("denying recall");
                let denied_ids = target_ids(&denying);
                assert!(
                    !denied_ids.iter().any(|id| id == high_chain.id.as_str()),
                    "a denied-policy endpoint denies the chain: {denied_ids:?}"
                );
                assert!(
                    denied_ids.iter().any(|id| id == ordinary_mem.id.as_str()),
                    "the ordinary-endpoint chain survives the same denying page: {denied_ids:?}"
                );
            }

            /// F7 decisive matrix through REAL composed recall: a chunk's
            /// canonical SOURCE is processed as a bounded typed SOURCE
            /// dependency — an unbound ordinary source accepts, a
            /// durably-code-owned CURRENT source accepts as code-dependent, a
            /// durably-code-owned STALE source denies, and a repository-bound
            /// source without settled ownership fails closed.
            #[test]
            fn chunk_source_matrix_composed() {
                use engram_knowledge::KnowledgeRepository as _;
                let dir = tempfile::tempdir().expect("tempdir");
                let fixture = fixture(dir.path());
                let owned_source = |id: &str, repository_key: Option<&str>| KnowledgeSource {
                    id: Id::from(id),
                    kind: SourceKind::GitRepository,
                    scope: scope(),
                    name: id.to_string(),
                    uri: None,
                    version: None,
                    repository_key: repository_key.map(str::to_string),
                    git_ref: repository_key.map(|_| "refs/heads/main".to_string()),
                    policy: policy(),
                    provenance: provenance(),
                    created_at: chrono::Utc::now(),
                    updated_at: None,
                    metadata: None,
                };
                // DURABLY CODE-OWNED sources: current generation (gen-a-2) and
                // stale generation (gen-a-1).
                block_on(
                    fixture
                        .knowledge
                        .put_source(owned_source("source-current-owned", Some("repo-a"))),
                )
                .expect("put current-owned source");
                block_on(
                    fixture
                        .knowledge
                        .put_source(owned_source("source-stale-owned", Some("repo-a"))),
                )
                .expect("put stale-owned source");
                for (source, generation) in [
                    ("source-current-owned", "gen-a-2"),
                    ("source-stale-owned", "gen-a-1"),
                ] {
                    block_on(fixture.bindings.put_lineage(
                        &scope(),
                        source,
                        CodeLineage {
                            repo_id: RepoId("repo-a".to_string()),
                            generation_id: generation.to_string(),
                            source_path: format!("src/{source}.rs"),
                            contributors: vec![source.to_string()],
                        },
                    ))
                    .expect("source lineage");
                }
                let src_chunk = |id: &str, text: &str, source: &str| {
                    let mut record = chunk(id, text, KnowledgeChunkKind::Paragraph);
                    record.source_id = Id::from(source);
                    record
                };
                // repository-bound WITHOUT settled ownership (source-task-013
                // is fixture-seeded with repository_key repo-a and no lineage).
                block_on(fixture.knowledge.put_chunk(src_chunk(
                    "chunk-src-unsettled",
                    "srcmatrix unsettl",
                    "source-task-013",
                )))
                .expect("put unsettled-source chunk");
                block_on(fixture.knowledge.put_chunk(src_chunk(
                    "chunk-src-stale",
                    "srcmatrix stale",
                    "source-stale-owned",
                )))
                .expect("put stale-source chunk");
                block_on(fixture.knowledge.put_chunk(src_chunk(
                    "chunk-src-current",
                    "srcmatrix current",
                    "source-current-owned",
                )))
                .expect("put current-source chunk");
                block_on(fixture.knowledge.put_chunk(src_chunk(
                    "chunk-src-unbound",
                    "srcmatrix unbound",
                    "source-ordinary-task-013",
                )))
                .expect("put unbound-source chunk");

                let payload = block_on(composed_recall(&fixture).recall(request("srcmatrix", 20)))
                    .expect("recall");
                let ids = target_ids(&payload);
                assert!(
                    ids.contains(&"chunk-src-unbound".to_string()),
                    "an unbound ordinary source accepts: {ids:?}"
                );
                assert!(
                    ids.contains(&"chunk-src-current".to_string()),
                    "a durably-code-owned CURRENT source accepts as code-dependent: {ids:?}"
                );
                assert!(
                    !ids.contains(&"chunk-src-stale".to_string()),
                    "an ordinary chunk of a durably-code-owned STALE source denies (finding 7): {ids:?}"
                );
                assert!(
                    !ids.contains(&"chunk-src-unsettled".to_string()),
                    "a repository-bound source without settled ownership fails closed (finding 7): {ids:?}"
                );
            }

            /// F9 decisive boundaries (RESOLVER-LEVEL, labeled): every
            /// deterministic projection is evaluated independently at the exact
            /// 2000/2001/2048/2049-character boundaries, including multibyte
            /// text, for BOTH lane shapes (graph-lane 2000-char clip and
            /// vector-lane 2048-char cap). The 2001-case is the interval the old
            /// helper rejected.
            #[test]
            fn chunk_projection_boundaries_bind_independently() {
                use engram_knowledge::KnowledgeRepository as _;
                let dir = tempfile::tempdir().expect("tempdir");
                let fixture = fixture(dir.path());
                let resolver = fixture.candidate_evidence();
                use engram_retrieval::CandidateEvidenceResolver;
                let case = |label: &str, chars: usize, multibyte: bool| {
                    let text: String = if multibyte {
                        std::iter::repeat_n('é', chars).collect()
                    } else {
                        std::iter::repeat_n('a', chars).collect()
                    };
                    let id = format!("chunk-proj-{label}");
                    let mut record = chunk(&id, &text, KnowledgeChunkKind::Paragraph);
                    record.source_id = Id::from("source-ordinary-task-013");
                    let record_provenance = record.provenance.clone();
                    let record_policy = record.policy.clone();
                    block_on(fixture.knowledge.put_chunk(record)).expect("put projection chunk");
                    let candidate = |content: String| RetrievalResult {
                        id: id.clone(),
                        target_type: RetrievalTargetType::Chunk,
                        target_id: id.clone(),
                        content,
                        score: engram_domain::RetrievalScore {
                            total: 1.0,
                            relevance: Some(1.0),
                            recency: None,
                            confidence: None,
                            cue_match: None,
                            hierarchical_fit: None,
                            policy_fit: Some(1.0),
                        },
                        provenance: record_provenance.clone(),
                        policy: record_policy.clone(),
                        explanation: None,
                        fusion_trace: None,
                        metadata: None,
                    };
                    let full = block_on(
                        resolver.evidence(&candidate(text.clone()), &request("parse", 10)),
                    )
                    .expect("full-text projection");
                    assert_eq!(
                        full,
                        CandidateEvidence::Ordinary,
                        "{label}: full text binds"
                    );
                    // Graph-lane projection: first 2000 characters — evaluated
                    // INDEPENDENTLY. For >2000-char texts this is the decisive
                    // interval the old helper rejected (2001..=2048); for ≤2000
                    // it equals the full text and binds trivially.
                    let clipped: String = text.chars().take(2000).collect();
                    let graph_lane =
                        block_on(resolver.evidence(&candidate(clipped), &request("parse", 10)))
                            .expect("graph projection");
                    assert_eq!(
                        graph_lane,
                        CandidateEvidence::Ordinary,
                        "{label}: the graph-lane 2000-char projection binds independently"
                    );
                    // Vector-lane projection: first 2048 characters — also
                    // independent (binds for every boundary length).
                    let capped: String = text.chars().take(2048).collect();
                    let vector_lane =
                        block_on(resolver.evidence(&candidate(capped), &request("parse", 10)))
                            .expect("vector projection");
                    assert_eq!(
                        vector_lane,
                        CandidateEvidence::Ordinary,
                        "{label}: the vector-lane 2048-char projection binds independently"
                    );
                    // An altered projection still denies (anti-forgery intact).
                    let forged = format!("{}x", text.chars().take(2000).collect::<String>());
                    if forged != text {
                        let denied =
                            block_on(resolver.evidence(&candidate(forged), &request("parse", 10)))
                                .expect("forged projection");
                        assert_ne!(
                            denied,
                            CandidateEvidence::Ordinary,
                            "{label}: altered denies"
                        );
                    }
                };
                case("2000", 2000, false);
                case("2001", 2001, false);
                case("2048", 2048, false);
                case("2049", 2049, false);
                case("2001-multibyte", 2001, true);
                case("2049-multibyte", 2049, true);
            }

            /// F5 composed-recall counter equivalent (labeled COMPOSED, unlike
            /// the resolver-only `wide_input_counters_bounded`): a wide
            /// dependency array on a REAL facts-lane candidate exhausts by
            /// INSTRUMENTED counters — admission scan included — through the
            /// full composed recall, and the candidate denies.
            #[test]
            fn wide_input_counters_bounded_composed() {
                let dir = tempfile::tempdir().expect("tempdir");
                let fixture = fixture(dir.path());
                let resolver = fixture.candidate_evidence();
                let counters = resolver.walk_counters();
                counters.reset();
                let ordinary_target =
                    write_memory_with_refs(&fixture, "wide composed dep target", Vec::new());
                let wide = write_memory_with_refs(
                    &fixture,
                    "widecomposed beyond budget",
                    std::iter::repeat_n(
                        evidence_ref(EvidenceTargetType::Memory, ordinary_target.id.as_str()),
                        2000,
                    )
                    .collect(),
                );
                let recall = SqlUnifiedRecall::new(
                    fixture.memory.clone(),
                    Vec::new(),
                    fixture.beliefs.clone(),
                )
                .with_code_eligibility(
                    fixture.eligibility() as Arc<dyn engram_retrieval::CodeEligibilityResolver>
                )
                .with_candidate_evidence(resolver.clone());
                let payload = block_on(recall.recall(request("widecomposed", 20))).expect("recall");
                let ids = target_ids(&payload);
                assert!(
                    !ids.iter().any(|id| id == wide.id.as_str()),
                    "the beyond-budget wide candidate denies through composed recall: {ids:?}"
                );
                let stats = counters.snapshot();
                assert!(
                    stats.references_examined <= 1024,
                    "composed reference examinations stay inside the work budget: {stats:?}"
                );
                assert!(
                    stats.admission_examined <= 1024 + 16,
                    "admission-scan work is bounded by the remaining budgets (early-terminating): {stats:?}"
                );
                assert!(
                    stats.budget_exhaustions >= 1,
                    "terminal exhaustion attested through composed recall: {stats:?}"
                );
                assert!(
                    stats.bytes_accounted <= 128 * 1024,
                    "ALL byte charges (references, visited keys, code deps) bounded: {stats:?}"
                );
            }

            /// F5 composed-recall counter equivalent (labeled COMPOSED): an
            /// ordinary cycle through REAL composed recall settles with
            /// bounded, instrumented counters.
            #[test]
            fn cyclic_input_counters_bounded_composed() {
                let dir = tempfile::tempdir().expect("tempdir");
                let fixture = fixture(dir.path());
                let resolver = fixture.candidate_evidence();
                let counters = resolver.walk_counters();
                counters.reset();
                put_memory_with_id(
                    &fixture,
                    "memory-composed-cycle-b",
                    "composedcycle beta",
                    std::iter::repeat_n(
                        evidence_ref(EvidenceTargetType::Memory, "memory-composed-cycle-a"),
                        300,
                    )
                    .collect(),
                );
                let a = put_memory_with_id(
                    &fixture,
                    "memory-composed-cycle-a",
                    "composedcycle alpha",
                    std::iter::repeat_n(
                        evidence_ref(EvidenceTargetType::Memory, "memory-composed-cycle-b"),
                        300,
                    )
                    .collect(),
                );
                let recall = SqlUnifiedRecall::new(
                    fixture.memory.clone(),
                    Vec::new(),
                    fixture.beliefs.clone(),
                )
                .with_code_eligibility(
                    fixture.eligibility() as Arc<dyn engram_retrieval::CodeEligibilityResolver>
                )
                .with_candidate_evidence(resolver.clone());
                let payload =
                    block_on(recall.recall(request("composedcycle", 20))).expect("recall");
                let ids = target_ids(&payload);
                assert!(
                    ids.iter().any(|id| id == a.id.as_str()),
                    "the ordinary cyclic candidate flows through composed recall: {ids:?}"
                );
                let stats = counters.snapshot();
                assert!(
                    stats.cycle_backedges >= 1,
                    "back-edges instrumented: {stats:?}"
                );
                assert!(
                    stats.visited_records <= 256 && stats.max_queue_depth <= 16,
                    "set and queue bounded through composed recall: {stats:?}"
                );
                assert_eq!(
                    stats.budget_exhaustions, 0,
                    "a within-budget composed cycle does not exhaust: {stats:?}"
                );
            }
        }
    }

    /// rerank_context_inputs: eligibility carries through the reranker seam
    /// and the composed context (recording double at the rerank port
    /// boundary; the rerank scorer is deployer-provided).
    #[test]
    fn rerank_context_inputs() {
        use std::collections::BTreeSet;

        struct RecordingReranker {
            seen: Mutex<Vec<Vec<String>>>,
        }
        impl RetrievalReranker for RecordingReranker {
            fn rerank(
                &self,
                _request: &RetrievalRequest,
                candidates: Vec<RetrievalResult>,
            ) -> CoreResult<Vec<RetrievalResult>> {
                self.seen
                    .lock()
                    .expect("reranker lock")
                    .push(candidates.iter().map(|c| c.target_id.clone()).collect());
                Ok(candidates)
            }
        }

        let dir = tempfile::tempdir().expect("tempdir");
        let fixture = fixture(dir.path());
        let recorder = Arc::new(RecordingReranker {
            seen: Mutex::new(Vec::new()),
        });

        let lane = engram_integration::sqlite::lexical_recall_lane_bound(
            fixture.knowledge.clone(),
            fixture.lexical.clone(),
            fixture.eligibility() as Arc<dyn engram_retrieval::CodeEligibilityResolver>,
        );
        let recall = SqlUnifiedRecall::with_reranker(
            fixture.memory.clone(),
            vec![lane as Arc<dyn RetrievalIndex>],
            fixture.beliefs.clone(),
            engram_retrieval::ReciprocalFusionConfig::default(),
            Some(recorder.clone() as Arc<dyn RetrievalReranker>),
        )
        .with_code_eligibility(
            fixture.eligibility() as Arc<dyn engram_retrieval::CodeEligibilityResolver>
        )
        .with_candidate_evidence(fixture.candidate_evidence());

        let payload = block_on(recall.recall(request("parse", 10))).expect("recall");
        let captured = recorder.seen.lock().expect("reranker lock").clone();
        assert_eq!(
            captured.len(),
            1,
            "compose_context must run the reranker once"
        );
        let rerank_inputs: BTreeSet<String> = captured[0].iter().cloned().collect();
        assert!(
            !rerank_inputs.iter().any(|id| id.contains("stale")
                || id.contains("repo-b")
                || id.contains("forged")
                || id.contains("legacy")),
            "rerank input must exclude ineligible code: {rerank_inputs:?}"
        );
        assert!(
            rerank_inputs.contains("chunk-eligible"),
            "rerank input must include eligible code: {rerank_inputs:?}"
        );
        let ids = target_ids(&payload);
        assert!(
            !ids.iter().any(|id| id.contains("stale")),
            "composed context must exclude ineligible code: {ids:?}"
        );
    }
}
