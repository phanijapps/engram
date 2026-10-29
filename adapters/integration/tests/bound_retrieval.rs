//! TASK-013 focused proof: retrieval-lane eligibility applies BEFORE ranking,
//! limits and budgets (REQ-005 / AC-006).
//!
//! Every case drives REAL owned behavior on real adapter targets:
//! - the real `SqlKnowledgeStore` + real file-backed `SqlCodeBindingRepository`
//!   (published vs staged-but-unpublished generations),
//! - the real persistent Tantivy `LexicalIndex` (contributor-keyed documents),
//! - the real bound lexical lane (`lexical_recall_lane_bound`) and the real
//!   `SqlUnifiedRecall` with `with_code_eligibility` (facts filter + post-lane
//!   guard + rerank/compose inputs),
//! - the real `SqlMemoryService` facts lane and `SqlBeliefStore` beliefs lane.
//!
//! No lane uses a stub retrieval index: the ineligible candidate in every case
//! would OUTSCORE the eligible one without the constraint (that contrast is
//! asserted against the historical unconstrained lane).

use std::sync::{Arc, Mutex};

use engram_conformance::SqlUnifiedRecall;
use engram_domain::*;
use engram_integration::UnifiedRecall;
use engram_integration::code_binding::RepositoryEligibilitySource;
use engram_knowledge::{CodeBindingRepository, KnowledgeRepository, StageReceipt};
use engram_memory::MemoryService;
use engram_retrieval::{RetrievalIndex, RetrievalReranker};
use engram_runtime::CoreResult;
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

/// Scanner-equivalent generation stamps (`generation_stage.rs` shape).
fn generation_stamps(generation: Option<&str>) -> Option<Metadata> {
    let mut meta = Metadata::new();
    if let Some(generation) = generation {
        meta.insert(
            "codeGenerationId".to_string(),
            serde_json::Value::String(generation.to_string()),
        );
        meta.insert(
            "codeRepoId".to_string(),
            serde_json::Value::String("repo-a".to_string()),
        );
    }
    Some(meta)
}

fn chunk(id: &str, text: &str, generation: Option<&str>) -> KnowledgeChunk {
    KnowledgeChunk {
        id: Id::from(id),
        document_id: Id::from("document-task-013"),
        source_id: Id::from("source-task-013"),
        kind: match generation {
            Some(_) => KnowledgeChunkKind::CodeSymbol,
            None => KnowledgeChunkKind::Paragraph,
        },
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
        metadata: generation_stamps(generation),
    }
}

fn symbol_entity(id: &str, name: &str, generation: Option<&str>) -> KnowledgeEntity {
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
        metadata: generation_stamps(generation),
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
        // Wire form of `CodeReceiptKind` (serde snake_case: "source",
        // "knowledge", …) — the string the durable receipts table stores.
        lane: serde_json::to_value(&lane)
            .ok()
            .and_then(|value| value.as_str().map(str::to_owned))
            .unwrap_or_default(),
        digest: format!("digest-{attempt_id}-{}", receipt_digest_suffix(&lane)),
    }
}

fn receipt_digest_suffix(lane: &CodeReceiptKind) -> String {
    format!("{lane:?}").to_lowercase()
}

/// Binds `repo-a` (first bind: epoch 0, no active generation) and publishes
/// `published` (full receipts → ready → publish bumps the binding epoch and
/// activates the generation), leaving `staged` durable but UNPUBLISHED at
/// Ready against the post-publish epoch. Returns the store.
fn bound_store(dir: &std::path::Path, published: &str, staged: &str) -> SqlCodeBindingRepository {
    let store =
        SqlCodeBindingRepository::open(dir.join("bindings.sqlite")).expect("open binding store");
    let binding = CodeBinding {
        scope: scope(),
        repo_id: RepoId("repo-a".to_string()),
        branch_ref: "refs/heads/main".to_string(),
        version: 1,
        active_generation: None,
        epoch: 0,
    };
    block_on(store.bind(binding)).expect("bind");

    for (generation_id, attempt_id, publish) in [
        (published, "attempt-published", true),
        (staged, "attempt-staged", false),
    ] {
        // The staged attempt resumes against the POST-publish binding state
        // (epoch bumped, active generation set) — the same state machine the
        // coordinator's resume path asserts.
        let (expected_epoch, expected_active) = if publish {
            (0, None)
        } else {
            (1, Some(published))
        };
        block_on(store.begin_attempt(
            &scope(),
            &RepoId("repo-a".to_string()),
            generation(generation_id),
            attempt(attempt_id, expected_epoch, expected_active),
        ))
        .expect("begin attempt");
        block_on(store.record_receipt(receipt(attempt_id, generation_id, CodeReceiptKind::Source)))
            .expect("source receipt");
        block_on(store.record_receipt(receipt(
            attempt_id,
            generation_id,
            CodeReceiptKind::Knowledge,
        )))
        .expect("knowledge receipt");
        let (_, stored) =
            block_on(store.get_attempt(&scope(), &RepoId("repo-a".to_string()), attempt_id))
                .expect("get attempt")
                .expect("attempt exists");
        block_on(store.mark_ready(
            &scope(),
            &RepoId("repo-a".to_string()),
            attempt_id,
            stored.fencing_token,
        ))
        .expect("mark ready");
        if publish {
            block_on(store.publish(
                &scope(),
                &RepoId("repo-a".to_string()),
                attempt_id,
                expected_epoch,
            ))
            .expect("publish");
        }
    }
    store
}

/// Real seeding: published + stale code chunks/entities, an ordinary document
/// chunk, a legacy unbound symbol, and a real Tantivy index fed with the same
/// contributor keys the bound feed port uses.
struct Fixture {
    memory: Arc<SqlMemoryService>,
    beliefs: Arc<SqlBeliefStore>,
    knowledge: Arc<SqlKnowledgeStore>,
    lexical: Arc<LexicalIndex>,
    eligibility: Arc<RepositoryEligibilitySource>,
    memory_id: Id,
}

fn fixture(dir: &std::path::Path) -> Fixture {
    let memory = Arc::new(SqlMemoryService::open_in_memory().expect("memory open"));
    let beliefs = Arc::new(SqlBeliefStore::open_in_memory().expect("beliefs open"));
    let knowledge = Arc::new(SqlKnowledgeStore::open_in_memory().expect("knowledge open"));

    // Real durable binding store: gen-published is the active generation;
    // gen-staged is durable but NOT published.
    let bindings = bound_store(dir, "gen-published", "gen-staged");
    let eligibility = Arc::new(RepositoryEligibilitySource::new(Arc::new(bindings)));

    // Real seeding: published + stale code chunks/entities, an ordinary document
    // chunk, a legacy unbound symbol, and a real Tantivy index fed with the same
    // contributor keys the bound feed port uses.
    //
    // Chunks inherit visibility from their SOURCE scope (get_chunk joins
    // chunk → source), so the parent source + document must exist first.
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

    // Chunks: the stale generation would OUTSCORE the published one on BM25
    // (higher term frequency on the shared token).
    block_on(knowledge.put_chunk(chunk(
        "chunk-eligible",
        "parse parse eligible published",
        Some("gen-published"),
    )))
    .expect("put eligible chunk");
    block_on(knowledge.put_chunk(chunk(
        "chunk-stale",
        "parse parse parse parse parse staletoken",
        Some("gen-staged"),
    )))
    .expect("put stale chunk");
    block_on(knowledge.put_chunk(chunk("chunk-doc", "parse ordinary note", None)))
        .expect("put doc chunk");

    // Entities: a published symbol, and a legacy unbound symbol (no stamps).
    block_on(knowledge.put_entity(symbol_entity(
        "entity-published",
        "parsePublished",
        Some("gen-published"),
    )))
    .expect("put published entity");
    block_on(knowledge.put_entity(symbol_entity("entity-legacy", "parseLegacy", None)))
        .expect("put legacy entity");

    // Real Tantivy index fed with the same contributor keys the bound feed
    // port stamps (`code:<generation>` / ordinary `doc`).
    let lexical = Arc::new(LexicalIndex::new().expect("lexical index"));
    lexical
        .upsert_batch_stamped(&[
            (
                "chunk-eligible".to_string(),
                "parse parse eligible published".to_string(),
                engram_store_lexical::code_contributor_key("gen-published"),
            ),
            (
                "chunk-stale".to_string(),
                "parse parse parse parse parse staletoken".to_string(),
                engram_store_lexical::code_contributor_key("gen-staged"),
            ),
            (
                "chunk-doc".to_string(),
                "parse ordinary note".to_string(),
                "doc".to_string(),
            ),
            (
                "entity-published".to_string(),
                "parsePublished Function".to_string(),
                engram_store_lexical::code_contributor_key("gen-published"),
            ),
            // The legacy symbol was fed by the pre-binding path: no
            // generation stamp exists for it.
            (
                "entity-legacy".to_string(),
                "parseLegacy Function".to_string(),
                "doc".to_string(),
            ),
        ])
        .expect("feed lexical index");

    // One ordinary memory in the facts lane (non-code must survive).
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
        eligibility,
        memory_id: written.record.id.clone(),
    }
}

impl Fixture {
    /// The REAL bound recall: bound lexical lane + facts/beliefs lanes, with
    /// the resolver wired into the recall composition.
    fn bound_recall(&self) -> SqlUnifiedRecall {
        let lane = engram_integration::sqlite::lexical_recall_lane_bound(
            self.knowledge.clone(),
            self.lexical.clone(),
            self.eligibility.clone() as Arc<dyn engram_retrieval::CodeEligibilityResolver>,
        );
        SqlUnifiedRecall::new(
            self.memory.clone(),
            vec![lane as Arc<dyn RetrievalIndex>],
            self.beliefs.clone(),
        )
        .with_code_eligibility(
            self.eligibility.clone() as Arc<dyn engram_retrieval::CodeEligibilityResolver>
        )
    }
}

fn target_ids(payload: &ContextPayload) -> Vec<String> {
    payload.items.iter().map(|i| i.target_id.clone()).collect()
}

// ---------- task_013 focused cases -----------------------------------------

mod task_013 {
    use super::*;

    /// ineligible_outscores_eligible: a stale-generation chunk that BEATS every
    /// eligible candidate on BM25 must never appear in the fused page, and the
    /// eligible candidates must still fill the request budget. The historical
    /// unconstrained lane is asserted first to prove the ineligible candidate
    /// really outscores the eligible ones — the constraint, not scoring,
    /// removes it.
    #[test]
    fn ineligible_outscores_eligible() {
        let dir = tempfile::tempdir().expect("tempdir");
        let fixture = fixture(dir.path());

        // Contrast: the historical unconstrained lexical lane ranks the stale
        // generation FIRST on the shared token.
        let unconstrained_lane = engram_integration::sqlite::lexical_recall_lane(
            fixture.knowledge.clone(),
            fixture.lexical.clone(),
        );
        let unconstrained = block_on(unconstrained_lane.retrieve_candidates(&request("parse", 10)))
            .expect("unconstrained lane");
        assert!(
            unconstrained
                .first()
                .is_some_and(|hit| hit.target_id == "chunk-stale"),
            "fixture must actually outrank eligible candidates unconstrained: {:?}",
            unconstrained
                .iter()
                .map(|h| h.target_id.clone())
                .collect::<Vec<_>>()
        );

        // The real bound recall: stale never appears; eligible code + the
        // ordinary document fill the budget.
        let recall = fixture.bound_recall();
        let payload = block_on(recall.recall(request("parse", 2))).expect("bound recall");
        let ids = target_ids(&payload);
        assert!(
            !ids.iter().any(|id| id.contains("stale")),
            "stale-generation code must never reach the fused page: {ids:?}"
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

    /// fallback_injection_denied: when only an ineligible target matches the
    /// query, the lane returns nothing and the recall composition injects NO
    /// scope-wide fallback record — an empty (or non-code-only) page is the
    /// honest result, never stale code.
    #[test]
    fn fallback_injection_denied() {
        let dir = tempfile::tempdir().expect("tempdir");
        let fixture = fixture(dir.path());

        let recall = fixture.bound_recall();
        let payload = block_on(recall.recall(request("staletoken", 10))).expect("recall");
        let ids = target_ids(&payload);
        assert!(
            !ids.iter()
                .any(|id| id.contains("stale") || id.contains("legacy")),
            "no ineligible code may be injected as a fallback: {ids:?}"
        );
        // The lane degraded to an empty contribution, not an error: recall
        // still succeeds and reports no lexical lane failure.
        assert!(
            payload
                .source_failures
                .iter()
                .all(|failure| !failure.source.contains("lexical")),
            "a clean empty lane must not be recorded as a failure: {:?}",
            payload.source_failures
        );

        // Legacy unbound code is equally denied: the symbol exists in the
        // store and the index, but a bound lane never serves it.
        let legacy = block_on(recall.recall(request("parseLegacy", 10))).expect("recall");
        let legacy_ids = target_ids(&legacy);
        assert!(
            !legacy_ids.contains(&"entity-legacy".to_string()),
            "legacy unbound code must never leak through a documents fallback: {legacy_ids:?}"
        );
    }

    /// non_code_preserved: ordinary documents and memories keep flowing
    /// alongside constrained code — the constraint only removes code.
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

        // And in the mixed query, eligible code + non-code coexist.
        let mixed = block_on(recall.recall(request("parse", 10))).expect("mixed recall");
        let mixed_ids = target_ids(&mixed);
        assert!(mixed_ids.contains(&"chunk-eligible".to_string()));
        assert!(mixed_ids.contains(&"chunk-doc".to_string()));
    }

    /// rerank_context_inputs: eligibility carries through the reranker seam and
    /// the composed context — the reranker receives only eligible candidates
    /// (recorded by a deterministic port double at the rerank boundary), and
    /// the final composed context excludes ineligible code entirely.
    #[test]
    fn rerank_context_inputs() {
        use std::collections::BTreeSet;

        /// Deterministic recording double at the `RetrievalReranker` port
        /// boundary (the real cross-encoder scorer is deployer-provided; MMR
        /// is exercised in its own crate). It records the candidate set the
        /// composition hands it and passes candidates through unchanged.
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
            fixture.eligibility.clone() as Arc<dyn engram_retrieval::CodeEligibilityResolver>,
        );
        let recall = SqlUnifiedRecall::with_reranker(
            fixture.memory.clone(),
            vec![lane as Arc<dyn RetrievalIndex>],
            fixture.beliefs.clone(),
            engram_retrieval::ReciprocalFusionConfig::default(),
            Some(recorder.clone() as Arc<dyn RetrievalReranker>),
        )
        .with_code_eligibility(
            fixture.eligibility.clone() as Arc<dyn engram_retrieval::CodeEligibilityResolver>
        );

        let payload = block_on(recall.recall(request("parse", 10))).expect("recall");
        let captured = recorder.seen.lock().expect("reranker lock").clone();
        assert!(
            captured.len() == 1,
            "compose_context must run the reranker once: {captured:?}"
        );
        let rerank_inputs: BTreeSet<String> = captured[0].iter().cloned().collect();
        assert!(
            !rerank_inputs.iter().any(|id| id.contains("stale")),
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
