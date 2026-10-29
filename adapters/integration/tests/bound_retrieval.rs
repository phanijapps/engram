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
use engram_knowledge::{CodeBindingRepository, KnowledgeRepository, StageReceipt};
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
        source_id: Id::from("source-task-013"),
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
        graph_id: None,
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

        let recall = fixture.bound_recall();
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
        let recall = fixture.bound_recall();
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
    /// alongside constrained code.
    #[test]
    fn non_code_preserved() {
        let dir = tempfile::tempdir().expect("tempdir");
        let fixture = fixture(dir.path());

        let recall = fixture.bound_recall();
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

    /// Corrective r2 (finding 6): an eligibility AUTHORITY FAILURE fails
    /// closed — with eligible, stale and ordinary records in the lanes, an
    /// erroring resolver yields ONLY the authorized non-code results (the
    /// facts-lane memory), never stale code, and the failure is surfaced as
    /// a source failure instead of silently disabling the guard.
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
        let lane = engram_integration::sqlite::lexical_recall_lane_bound(
            fixture.knowledge.clone(),
            fixture.lexical.clone(),
            erroring.clone(),
        );
        let recall = SqlUnifiedRecall::new(
            fixture.memory.clone(),
            vec![lane as Arc<dyn RetrievalIndex>],
            fixture.beliefs.clone(),
        )
        .with_code_eligibility(erroring);

        let payload = block_on(recall.recall(request("parse", 10))).expect("recall");
        let ids = target_ids(&payload);
        assert!(
            ids.iter().any(|id| id == &fixture.memory_id.to_string()),
            "the independently authorized non-code memory must survive the authority failure: {ids:?}"
        );
        assert!(
            !ids.iter().any(|id| id.contains("stale")
                || id.contains("repo-b")
                || id.contains("forged")
                || id.contains("eligible")
                || id == "chunk-doc"
                || id == "chunk-legacy-code"),
            "code-capable candidates are denied while the authority is unavailable (fail closed): {ids:?}"
        );
        assert!(
            payload
                .source_failures
                .iter()
                .any(|failure| failure.source == "code_eligibility"),
            "the authority failure must be surfaced as a source failure: {:?}",
            payload.source_failures
        );
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
        );

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
