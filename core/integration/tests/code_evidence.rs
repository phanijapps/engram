#![cfg(feature = "sqlite")]
#[path = "support/code_authority.rs"]
mod authority;
use authority::*;
use engram_domain::*;
use engram_integration::{
    CapabilityReport, EngramProvider, EngramProviderBuilder,
    code_configuration::resolve_code_config,
};
use engram_retrieval::{
    CodeScorerFailurePolicy, RerankScorer, RerankScorerIdentity, TextTokenization,
};
use engram_runtime::{CoreError, CoreResult};
use futures::executor::block_on;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

struct Scorer {
    local: bool,
    invalid: bool,
    inspected: AtomicUsize,
    code_inspected: AtomicUsize,
    identity_calls: AtomicUsize,
    identity_hook: Option<Box<dyn Fn() + Send + Sync>>,
    pairs: AtomicUsize,
    factor: usize,
    length_scores: bool,
    hook: Option<Box<dyn Fn() + Send + Sync>>,
}
impl Scorer {
    fn new(local: bool, invalid: bool) -> Self {
        Self {
            local,
            invalid,
            inspected: AtomicUsize::new(0),
            code_inspected: AtomicUsize::new(0),
            identity_calls: AtomicUsize::new(0),
            identity_hook: None,
            pairs: AtomicUsize::new(0),
            factor: 1,
            length_scores: false,
            hook: None,
        }
    }
}
fn tokens(text: &str, special: u32) -> TextTokenization {
    TextTokenization {
        offsets: text
            .char_indices()
            .map(|(i, c)| CodeByteRange {
                start: i as u64,
                end: (i + c.len_utf8()) as u64,
            })
            .collect(),
        special_tokens: special,
        window: 8192,
        tokenizer_id: "pair-test-v1".into(),
    }
}
impl RerankScorer for Scorer {
    fn supports_code_input(&self) -> bool {
        true
    }
    fn score(&self, _: &str, _: &str) -> CoreResult<f32> {
        panic!("batch scorer required")
    }
    fn is_local(&self) -> bool {
        self.local
    }
    fn identity(&self) -> Option<RerankScorerIdentity> {
        if self.identity_calls.fetch_add(1, Ordering::SeqCst) == 1 {
            if let Some(hook) = &self.identity_hook {
                hook();
            }
        }
        Some(RerankScorerIdentity {
            model_id: "jinaai/jina-reranker-v1-turbo-en".into(),
            manifest_id: "test-scorer-manifest".into(),
            tokenizer_id: "pair-test-v1".into(),
            input_contract_id: "complete-pair-test-v1".into(),
            score_contract_id: "logits-column-0/raw-v1".into(),
            window: 8192,
        })
    }
    fn inspect_query(&self, text: &str) -> CoreResult<TextTokenization> {
        assert!(self.local);
        self.inspected.fetch_add(1, Ordering::SeqCst);
        Ok(tokens(text, 0))
    }
    fn inspect_document(&self, text: &str) -> CoreResult<TextTokenization> {
        assert!(self.local);
        self.code_inspected.fetch_add(1, Ordering::SeqCst);
        self.inspected.fetch_add(1, Ordering::SeqCst);
        let mut t = tokens(text, 0);
        t.offsets = t
            .offsets
            .into_iter()
            .flat_map(|r| std::iter::repeat_n(r, self.factor))
            .collect();
        Ok(t)
    }
    fn inspect_pair(
        &self,
        query: &str,
        document: &str,
        prefix: usize,
    ) -> CoreResult<CodeTokenCoverage> {
        assert!(self.local);
        self.code_inspected.fetch_add(1, Ordering::SeqCst);
        assert!(!document.contains("PRIVATE_MARKER"));
        self.inspected.fetch_add(1, Ordering::SeqCst);
        let context_tokens = (document[..prefix].chars().count() * self.factor) as u32;
        let code_tokens = (document[prefix..].chars().count() * self.factor) as u32;
        let query_tokens = query.chars().count() as u32;
        Ok(CodeTokenCoverage {
            context_tokens,
            code_tokens,
            query_tokens,
            special_tokens: 4,
            total_tokens: context_tokens + code_tokens + query_tokens + 4,
            truncated: false,
        })
    }
    fn score_batch(&self, _: &str, documents: &[String], batch: usize) -> CoreResult<Vec<f32>> {
        assert!(self.local);
        assert!(batch > 0);
        assert!(documents.len() <= 200);
        assert!(documents.iter().all(|t| !t.contains("PRIVATE_MARKER")));
        self.pairs.fetch_add(documents.len(), Ordering::SeqCst);
        if let Some(hook) = &self.hook {
            hook();
        }
        if self.invalid {
            return Ok(vec![f32::NAN]);
        }
        Ok(documents
            .iter()
            .map(|t| {
                if self.length_scores {
                    t.chars().count() as f32
                } else if t.contains("run9") {
                    9.0
                } else {
                    0.0
                }
            })
            .collect())
    }
}
fn provider(
    f: &Fixture,
    scorer: Arc<Scorer>,
    config: engram_retrieval::CodeIntelligenceConfig,
) -> EngramProvider {
    provider_calibrated(f, scorer, config, None)
}
fn provider_calibrated(
    f: &Fixture,
    scorer: Arc<Scorer>,
    config: engram_retrieval::CodeIntelligenceConfig,
    calibration: Option<CodeCalibrationArtifact>,
) -> EngramProvider {
    let overlay = serde_json::from_value(serde_json::to_value(config).unwrap()).unwrap();
    let mut builder = EngramProviderBuilder::new(CapabilityReport::new(CapabilityState::Supported))
        .knowledge(f.store.clone())
        .knowledge_query(f.store.clone())
        .graph(f.expansion.clone())
        .embedding_provider(f.model.clone())
        .vectors(f.provider.vectors().unwrap().clone())
        .code_vectors(f.provider.code_vectors().unwrap().clone())
        .code_retrieval_lane(
            engram_retrieval::CodeRetrievalLane::Lexical,
            f.lexical.clone(),
        )
        .code_configuration(resolve_code_config(None, None, Some(&overlay)).unwrap())
        .code_scorer(scorer);
    if let Some(artifact) = calibration {
        builder = builder.code_calibration(artifact);
    }
    builder.build()
}

#[test]
fn nonlocal_scorer_receives_no_query_or_code_under_either_failure_policy() {
    let f = fixture();
    for policy in [
        CodeScorerFailurePolicy::Degrade,
        CodeScorerFailurePolicy::Error,
    ] {
        let scorer = Arc::new(Scorer::new(false, false));
        let mut config = f.config.clone();
        config.local_scorer.on_failure = policy;
        let p = provider(&f, scorer.clone(), config);
        let before = f.model.queries.load(Ordering::SeqCst);
        let result = block_on(p.code_evidence(&query(), &f.scope, &f.requester));
        assert_eq!(scorer.inspected.load(Ordering::SeqCst), 0);
        assert_eq!(scorer.pairs.load(Ordering::SeqCst), 0);
        match policy {
            CodeScorerFailurePolicy::Error => {
                assert!(result.is_err());
                assert_eq!(f.model.queries.load(Ordering::SeqCst), before);
            }
            CodeScorerFailurePolicy::Degrade => {
                let packet = result.unwrap();
                assert_eq!(
                    packet.evidence_status,
                    CodeEvidenceStatus::InsufficientEvidence
                );
                assert_eq!(
                    packet.diagnostics.scorer_state,
                    CodeScorerState::Unavailable
                );
                assert!(packet.items.iter().all(|i| i.rerank_score.is_none()));
            }
        }
    }
}
#[test]
fn byte_and_scorer_query_admission_precede_all_inference() {
    let f = fixture();
    let scorer = Arc::new(Scorer::new(true, false));
    let p = provider(&f, scorer.clone(), f.config.clone());
    let mut q = query();
    q.query = "q".repeat(16385);
    assert!(block_on(p.code_evidence(&q, &f.scope, &f.requester)).is_err());
    assert_eq!(scorer.inspected.load(Ordering::SeqCst), 0);
    q.query = "q".repeat(129);
    assert!(matches!(
        block_on(p.code_evidence(&q, &f.scope, &f.requester)),
        Err(CoreError::InvalidRequest { .. })
    ));
    assert_eq!(f.model.queries.load(Ordering::SeqCst), 0);
    assert_eq!(scorer.pairs.load(Ordering::SeqCst), 0);
}
#[test]
fn successful_scoring_has_exact_winning_ranges_and_remains_insufficient_without_calibration() {
    let f = fixture();
    let scorer = Arc::new(Scorer::new(true, false));
    let p = provider(&f, scorer.clone(), f.config.clone());
    let packet = block_on(p.code_evidence(&query(), &f.scope, &f.requester)).unwrap();
    assert!(!packet.items.is_empty());
    assert_eq!(
        packet.evidence_status,
        CodeEvidenceStatus::InsufficientEvidence
    );
    assert_eq!(packet.diagnostics.scorer_state, CodeScorerState::Ready);
    assert_eq!(
        packet.diagnostics.effective_reranker,
        CodeEvidenceReranker::CrossEncoder
    );
    assert_eq!(
        packet.diagnostics.scored_pairs as usize,
        scorer.pairs.load(Ordering::SeqCst)
    );
    assert!(packet.diagnostics.scored_pairs <= 50);
    assert!(packet.items.len() <= 10);
    assert_eq!(
        packet.diagnostics.calibration_state,
        CodeCalibrationState::Unvalidated
    );
    for item in &packet.items {
        assert!(item.rerank_score.is_some());
        assert_eq!(item.source_category, CodePrimaryCategory::Implementation);
        for segment in &item.segments {
            assert!(segment.rerank_input.as_ref().unwrap().is_valid());
            assert!(segment.rerank_sources.as_ref().unwrap().len() > 0);
            assert!(segment.rerank_context.is_some());
            for part in &item.excerpts {
                let text = std::fs::read_to_string(f._root.path().join(&part.source.path)).unwrap();
                assert_eq!(
                    &text[part.source.start_byte as usize..part.source.end_byte as usize],
                    part.text
                );
                assert_eq!(part.source.unit_id, item.parent_unit_id);
            }
        }
    }
    assert!(serde_json::to_vec(&packet).unwrap().len() <= 262144);
}
#[test]
fn malformed_scores_discard_all_partial_results_and_report_real_fallback() {
    let f = fixture();
    let scorer = Arc::new(Scorer::new(true, true));
    let p = provider(&f, scorer, f.config.clone());
    let packet = block_on(p.code_evidence(&query(), &f.scope, &f.requester)).unwrap();
    assert!(!packet.items.is_empty());
    assert_eq!(packet.diagnostics.scorer_state, CodeScorerState::Failed);
    assert_eq!(
        packet.diagnostics.effective_reranker,
        CodeEvidenceReranker::None
    );
    assert_eq!(packet.diagnostics.scored_pairs, 0);
    assert!(packet.diagnostics.candidate_counts["prepared_scoring_views"] > 0);
    assert_eq!(
        packet.diagnostics.candidate_counts["prepared_scoring_views"],
        packet.diagnostics.candidate_counts["prepared_but_unscored_views"]
    );
    assert_eq!(
        packet.diagnostics.candidate_counts["attempted_scoring_pairs"],
        packet.diagnostics.candidate_counts["prepared_scoring_views"]
    );
    assert_eq!(
        packet.evidence_status,
        CodeEvidenceStatus::InsufficientEvidence
    );
    for item in packet.items {
        assert!(item.rerank_score.is_none());
        for s in item.segments {
            assert!(s.rerank_score.is_none());
            assert!(s.rerank_input.is_none());
            assert!(s.rerank_sources.is_none());
            assert!(s.rerank_context.is_none());
        }
    }
}

fn fitted(provider: &EngramProvider, threshold: f32) -> CodeCalibrationArtifact {
    let mut artifact =
        engram_integration::code_evidence::calibration_template(provider, &query()).unwrap();
    artifact.threshold = threshold;
    artifact.development_labels_sha256 = "ab".repeat(32);
    artifact.id = engram_integration::code_evidence::artifact_id(&artifact).unwrap();
    artifact
}
#[test]
fn matching_calibration_filters_parent_scores_and_operator_overrides_invalidate_it() {
    let f = fixture();
    let scorer = Arc::new(Scorer::new(true, false));
    let original = provider(&f, scorer.clone(), f.config.clone());
    let artifact = fitted(&original, 8.0);
    let p = provider_calibrated(&f, scorer, f.config.clone(), Some(artifact));
    let packet = block_on(p.code_evidence(&query(), &f.scope, &f.requester)).unwrap();
    assert_eq!(
        packet.diagnostics.calibration_state,
        CodeCalibrationState::Validated
    );
    assert_eq!(
        packet.evidence_status,
        CodeEvidenceStatus::CandidatesAvailable
    );
    assert!(packet.items.iter().all(|i| i.rerank_score.unwrap() >= 8.0));
    let mut reduced = query();
    reduced.candidates_per_lane = Some(20);
    let changed = block_on(p.code_evidence(&reduced, &f.scope, &f.requester)).unwrap();
    assert_eq!(
        changed.diagnostics.calibration_state,
        CodeCalibrationState::Unvalidated
    );
    assert_eq!(
        changed.evidence_status,
        CodeEvidenceStatus::InsufficientEvidence
    );
    assert_ne!(
        packet.diagnostics.config_hash,
        changed.diagnostics.config_hash
    );
    assert_eq!(
        changed.diagnostics.config_origins["retrieval.candidates_per_lane"],
        "request"
    );
}
#[test]
fn calibrated_fast_mode_has_its_own_rrf_space_and_zero_scorer_calls() {
    let f = fixture();
    let scorer = Arc::new(Scorer::new(true, false));
    let config =
        engram_retrieval::CodeIntelligenceConfig::for_profile(CodeIntelligenceProfile::FastV1);
    let first = provider(&f, scorer.clone(), config.clone());
    let artifact = fitted(&first, 0.0);
    assert_eq!(artifact.score_space, CodeScoreSpace::Rrf);
    assert!(artifact.scorer_manifest_id.is_none());
    let p = provider_calibrated(&f, scorer.clone(), config, Some(artifact));
    let packet = block_on(p.code_evidence(&query(), &f.scope, &f.requester)).unwrap();
    assert_eq!(
        packet.evidence_status,
        CodeEvidenceStatus::CandidatesAvailable
    );
    assert_eq!(packet.diagnostics.scorer_state, CodeScorerState::Disabled);
    assert_eq!(
        packet.diagnostics.threshold_score_space,
        Some(CodeScoreSpace::Rrf)
    );
    assert_eq!(scorer.inspected.load(Ordering::SeqCst), 0);
    assert_eq!(scorer.pairs.load(Ordering::SeqCst), 0);
}
#[test]
fn scorer_continuations_keep_exact_pair_inputs_and_bound_preparation_work() {
    let f = fixture();
    let mut scorer = Scorer::new(true, false);
    scorer.factor = 2;
    let scorer = Arc::new(scorer);
    let mut config = f.config.clone();
    config.local_scorer.query_token_limit = 32;
    config.local_scorer.pair_token_limit = 128;
    let p = provider(&f, scorer.clone(), config);
    let packet = block_on(p.code_evidence(&query(), &f.scope, &f.requester)).unwrap();
    assert!(!packet.items.is_empty());
    assert!(packet.diagnostics.scored_pairs <= 50);
    assert!(packet.diagnostics.candidate_counts["segments_with_unprepared_scoring_tails"] > 0);
    assert!(
        scorer.inspected.load(Ordering::SeqCst) < 50 * 60,
        "preparation must be bounded before inference"
    );
    let mut partial = false;
    for item in &packet.items {
        for s in &item.segments {
            let ranges = s.rerank_sources.as_ref().unwrap();
            let facts = s.rerank_context.as_ref().unwrap();
            let mut context = engram_knowledge::code_context_text(facts);
            if !context.is_empty() {
                context.push_str("\n\n");
            }
            let mut texts = vec![];
            for range in ranges {
                let source = std::fs::read_to_string(f._root.path().join(&range.path)).unwrap();
                texts.push(source[range.start_byte as usize..range.end_byte as usize].to_owned());
            }
            let body = texts.join("");
            let actual = scorer
                .inspect_pair(&query().query, &format!("{context}{body}"), context.len())
                .unwrap();
            assert_eq!(s.rerank_input.as_ref(), Some(&actual));
            assert!(actual.total_tokens <= 128);
            partial |= ranges != &s.sources;
        }
    }
    assert!(
        partial,
        "scorer-specific ranges must distinguish complete embedding coverage"
    );
}
#[test]
fn revocation_during_inference_never_publishes_old_scores_or_source_ranges() {
    let f = fixture();
    let mut scorer = Scorer::new(true, false);
    let store = f.store.clone();
    let once = AtomicUsize::new(0);
    scorer.hook = Some(Box::new(move || {
        if once.fetch_add(1, Ordering::SeqCst) == 0 {
            store.lock().unwrap().execute("UPDATE knowledge_chunks_records SET record_json=json_set(record_json,'$.policy.allowedUses',json('[]')) WHERE json_extract(record_json,'$.code.file.path')='public.ts'",[]).unwrap();
        }
    }));
    let p = provider(&f, Arc::new(scorer), f.config.clone());
    let packet = block_on(p.code_evidence(&query(), &f.scope, &f.requester)).unwrap();
    assert!(packet.items.is_empty());
    assert!(packet.supporting_units.is_empty());
    assert_eq!(
        packet.evidence_status,
        CodeEvidenceStatus::InsufficientEvidence
    );
    assert_eq!(packet.diagnostics.candidate_counts["source_retries"], 1);
}
#[test]
fn second_revision_change_after_retry_drops_every_scored_view() {
    let f = fixture();
    let mut scorer = Scorer::new(true, false);
    let store = f.store.clone();
    scorer.hook = Some(Box::new(move || {
        store
            .lock()
            .unwrap()
            .execute(
                "UPDATE code_index_revision SET revision=revision+1 WHERE singleton=1",
                [],
            )
            .unwrap();
    }));
    let scorer = Arc::new(scorer);
    let p = provider(&f, scorer.clone(), f.config.clone());
    let packet = block_on(p.code_evidence(&query(), &f.scope, &f.requester)).unwrap();
    assert!(packet.items.is_empty());
    assert!(packet.supporting_units.is_empty());
    assert_eq!(
        packet.evidence_status,
        CodeEvidenceStatus::InsufficientEvidence
    );
    assert_eq!(packet.diagnostics.scored_pairs, 0);
    assert!(scorer.pairs.load(Ordering::SeqCst) > 50);
    assert!(
        packet
            .source_failures
            .iter()
            .any(|f| f.reason == "code_sources_changed_during_query")
    );
}

#[test]
fn generated_sources_remain_supporting_and_never_consume_scoring_slots() {
    let f = fixture_with_text("// @generated\nexport function generatedOnly() { return 1; }\n");
    let scorer = Arc::new(Scorer::new(true, false));
    let first = provider(&f, scorer.clone(), f.config.clone());
    let artifact = fitted(&first, -100.0);
    let p = provider_calibrated(&f, scorer.clone(), f.config.clone(), Some(artifact));
    let packet = block_on(p.code_evidence(&query(), &f.scope, &f.requester)).unwrap();
    assert!(packet.items.is_empty());
    assert!(!packet.supporting_units.is_empty());
    assert!(
        packet
            .supporting_units
            .iter()
            .all(|s| s.source_category == CodeSourceCategory::Generated)
    );
    assert_eq!(scorer.pairs.load(Ordering::SeqCst), 0);
    assert_eq!(
        packet.evidence_status,
        CodeEvidenceStatus::InsufficientEvidence
    );
}
#[test]
fn stale_or_modified_calibration_and_scoring_failure_cannot_borrow_a_threshold() {
    let f = fixture();
    let scorer = Arc::new(Scorer::new(true, false));
    let first = provider(&f, scorer.clone(), f.config.clone());
    let artifact = fitted(&first, -100.0);
    let mut stale = artifact.clone();
    stale.tokenizer_identity.push_str("-different");
    stale.id = engram_integration::code_evidence::artifact_id(&stale).unwrap();
    let p = provider_calibrated(&f, scorer, f.config.clone(), Some(stale));
    let packet = block_on(p.code_evidence(&query(), &f.scope, &f.requester)).unwrap();
    assert_eq!(
        packet.diagnostics.calibration_state,
        CodeCalibrationState::Unvalidated
    );
    assert_eq!(
        packet.evidence_status,
        CodeEvidenceStatus::InsufficientEvidence
    );
    let bad = provider_calibrated(
        &f,
        Arc::new(Scorer::new(true, true)),
        f.config.clone(),
        Some(artifact),
    );
    let failed = block_on(bad.code_evidence(&query(), &f.scope, &f.requester)).unwrap();
    assert_eq!(
        failed.diagnostics.calibration_state,
        CodeCalibrationState::Unvalidated
    );
    assert!(failed.diagnostics.threshold_score_space.is_none());
    assert!(failed.diagnostics.calibration_id.is_none());
}
#[test]
fn output_token_budget_counts_duplicate_emitted_context_and_keeps_pair_records_intact() {
    use engram_integration::EmbeddingProvider;
    let f = fixture();
    let scorer = Arc::new(Scorer::new(true, false));
    let mut config = f.config.clone();
    config.evidence.token_budget = 256;
    let p = provider(&f, scorer.clone(), config);
    let packet = block_on(p.code_evidence(&query(), &f.scope, &f.requester)).unwrap();
    let mut total = 0;
    for item in &packet.items {
        for part in &item.excerpts {
            total += f
                .model
                .tokenize_untruncated(&part.text, false)
                .unwrap()
                .total_tokens();
        }
        for facts in std::iter::once(&item.context).chain(
            item.segments
                .iter()
                .filter_map(|s| s.rerank_context.as_ref()),
        ) {
            for fact in facts {
                total += f
                    .model
                    .tokenize_untruncated(
                        &engram_knowledge::code_context_text(std::slice::from_ref(fact)),
                        false,
                    )
                    .unwrap()
                    .total_tokens();
            }
        }
        for segment in &item.segments {
            assert!(segment.rerank_input.is_some());
            assert!(segment.rerank_sources.is_some());
            assert!(segment.rerank_context.is_some());
        }
    }
    for supporting in &packet.supporting_units {
        for part in &supporting.excerpts {
            total += f
                .model
                .tokenize_untruncated(&part.text, false)
                .unwrap()
                .total_tokens();
        }
    }
    assert!(total <= 256);
    assert!(packet.diagnostics.truncated);
    assert!(packet.diagnostics.omitted_units > 0);
}
#[test]
fn dropping_the_maximum_view_for_budget_cannot_leave_positive_evidence() {
    let f = fixture_with_text(&format!(
        "export function giant() {{ return '{}'; }}",
        "a".repeat(2000)
    ));
    // Force the maximum score onto a full 384-token view, rather than tying
    // it with a short tail that legitimately fits the output budget.
    let mut scorer = Scorer::new(true, false);
    scorer.length_scores = true;
    let scorer = Arc::new(scorer);
    let mut config = f.config.clone();
    config.evidence.token_budget = 256;
    let first = provider(&f, scorer.clone(), config.clone());
    let artifact = fitted(&first, -100.0);
    let p = provider_calibrated(&f, scorer, config, Some(artifact));
    let packet = block_on(p.code_evidence(&query(), &f.scope, &f.requester)).unwrap();
    assert!(packet.items.is_empty());
    assert_eq!(
        packet.evidence_status,
        CodeEvidenceStatus::InsufficientEvidence
    );
    assert_eq!(
        packet.diagnostics.calibration_state,
        CodeCalibrationState::Validated
    );
    assert!(packet.diagnostics.omitted_units > 0);
}

#[test]
fn revocation_between_collection_and_preparation_reaches_no_code_inspector() {
    let f = fixture();
    let mut scorer = Scorer::new(true, false);
    let store = f.store.clone();
    // The second identity lookup is ranking entry, after candidate collection
    // but before any source/context inspector or pair inference.
    scorer.identity_hook = Some(Box::new(move || {
        store.lock().unwrap().execute("UPDATE knowledge_chunks_records SET record_json=json_set(record_json,'$.policy.allowedUses',json('[]')) WHERE json_extract(record_json,'$.code.file.path')='public.ts'",[]).unwrap();
    }));
    let scorer = Arc::new(scorer);
    let p = provider(&f, scorer.clone(), f.config.clone());
    let packet = block_on(p.code_evidence(&query(), &f.scope, &f.requester)).unwrap();
    assert_eq!(scorer.code_inspected.load(Ordering::SeqCst), 0);
    assert_eq!(scorer.pairs.load(Ordering::SeqCst), 0);
    assert!(packet.items.is_empty());
    assert!(packet.supporting_units.is_empty());
    assert_eq!(
        packet.evidence_status,
        CodeEvidenceStatus::InsufficientEvidence
    );
}
#[test]
fn known_indexed_git_revision_survives_all_emitted_source_and_context_references() {
    let f = fixture_with_git("export function run9() { return 9; }\n");
    let out = std::process::Command::new("git")
        .current_dir(f._root.path())
        .args(["rev-parse", "--short=10", "HEAD"])
        .output()
        .unwrap();
    let expected = String::from_utf8(out.stdout).unwrap().trim().to_owned();
    let p = provider(&f, Arc::new(Scorer::new(true, false)), f.config.clone());
    let packet = block_on(p.code_evidence(&query(), &f.scope, &f.requester)).unwrap();
    assert!(!packet.items.is_empty());
    for item in packet.items {
        for part in item.excerpts {
            assert_eq!(
                part.source.indexed_revision.as_deref(),
                Some(expected.as_str())
            );
        }
        for fact in item.context {
            for source in fact.sources {
                assert_eq!(source.indexed_revision.as_deref(), Some(expected.as_str()));
            }
        }
        for segment in item.segments {
            for source in segment
                .sources
                .into_iter()
                .chain(segment.rerank_sources.unwrap_or_default())
            {
                assert_eq!(source.indexed_revision.as_deref(), Some(expected.as_str()));
            }
            for fact in segment.rerank_context.unwrap_or_default() {
                for source in fact.sources {
                    assert_eq!(source.indexed_revision.as_deref(), Some(expected.as_str()));
                }
            }
        }
    }
}

#[test]
fn readiness_uses_actual_prerequisites_and_separates_disabled_or_nonlocal_scorers() {
    let f = fixture();
    let local = provider(&f, Arc::new(Scorer::new(true, false)), f.config.clone());
    assert_eq!(
        local.capabilities().code_evidence,
        CapabilityState::Supported
    );
    assert_eq!(local.capabilities().code_scorer, CapabilityState::Supported);
    let nonlocal = provider(&f, Arc::new(Scorer::new(false, false)), f.config.clone());
    assert_eq!(
        nonlocal.capabilities().code_evidence,
        CapabilityState::Supported
    );
    assert_eq!(
        nonlocal.capabilities().code_scorer,
        CapabilityState::Unsupported {
            reason: CapabilityReason::ProviderUnavailable
        }
    );
    let fast = provider(
        &f,
        Arc::new(Scorer::new(true, false)),
        engram_retrieval::CodeIntelligenceConfig::for_profile(CodeIntelligenceProfile::FastV1),
    );
    assert_eq!(
        fast.capabilities().code_scorer,
        CapabilityState::Unsupported {
            reason: CapabilityReason::FeatureDisabled
        }
    );
    let absent =
        EngramProviderBuilder::new(CapabilityReport::new(CapabilityState::Supported)).build();
    assert_eq!(
        absent.capabilities().code_evidence,
        CapabilityState::Unsupported {
            reason: CapabilityReason::ProviderUnavailable
        }
    );
}

#[test]
fn explicitly_loaded_calibration_is_an_immutable_snapshot_and_malformed_selection_fails() {
    use engram_integration::{
        CapabilityPolicy, EmbeddingProviderConfig, EngramConfig, MigrationMode,
    };
    let f = fixture();
    let p = provider(&f, Arc::new(Scorer::new(true, false)), f.config.clone());
    let mut artifact =
        engram_integration::code_evidence::calibration_template(&p, &query()).unwrap();
    artifact.development_labels_sha256 = "a".repeat(64);
    artifact.id = engram_integration::code_evidence::artifact_id(&artifact).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("calibration.json");
    std::fs::write(&path, serde_json::to_vec(&artifact).unwrap()).unwrap();
    let config = EngramConfig::new(
        dir.path().join("store"),
        dir.path(),
        ScopeMappingStrategy::Strict,
        EmbeddingProviderConfig {
            provider_type: "none".into(),
            model: "none".into(),
            dimensions: 3,
            prompt_profile: "query".into(),
            normalization: None,
        },
        MigrationMode::DryRun,
        CapabilityPolicy::FailClosed,
    )
    .with_code_calibration_path(&path);
    let p = p.with_code_calibration(config.load_code_calibration().unwrap().unwrap());
    std::fs::write(&path, b"{}").unwrap();
    assert_eq!(p.code_calibration().unwrap(), &artifact);
    assert!(config.load_code_calibration().is_err());
}

#[test]
fn lexical_only_readiness_does_not_require_vectors_and_missing_enabled_vector_is_degraded() {
    let f = fixture();
    let mut config = f.config.clone();
    config.retrieval.enabled_lanes = vec![engram_retrieval::CodeRetrievalLane::Lexical];
    config.retrieval.graph_hops = 0;
    let build = |config: &engram_retrieval::CodeIntelligenceConfig| {
        let overlay = serde_json::from_value(serde_json::to_value(config).unwrap()).unwrap();
        EngramProviderBuilder::new(CapabilityReport::new(CapabilityState::Supported))
            .knowledge(f.store.clone())
            .embedding_provider(f.model.clone())
            .code_retrieval_lane(
                engram_retrieval::CodeRetrievalLane::Lexical,
                f.lexical.clone(),
            )
            .code_configuration(resolve_code_config(None, None, Some(&overlay)).unwrap())
            .build()
    };
    let lexical = build(&config);
    assert!(lexical.code_vectors().is_none());
    assert_eq!(
        lexical.capabilities().code_evidence,
        CapabilityState::Supported
    );
    let packet = block_on(lexical.code_evidence(&query(), &f.scope, &f.requester)).unwrap();
    assert!(!packet.items.is_empty());
    config
        .retrieval
        .enabled_lanes
        .push(engram_retrieval::CodeRetrievalLane::Vector);
    let missing = build(&config);
    assert_eq!(
        missing.capabilities().code_evidence,
        CapabilityState::Degraded {
            reason: CapabilityReason::ProviderUnavailable
        }
    );
}

#[test]
fn local_identity_alone_cannot_advertise_the_code_input_contract() {
    struct GenericIdentityOnly;
    impl RerankScorer for GenericIdentityOnly {
        fn is_local(&self) -> bool {
            true
        }
        fn identity(&self) -> Option<RerankScorerIdentity> {
            Scorer::new(true, false).identity()
        }
        fn score(&self, _: &str, _: &str) -> CoreResult<f32> {
            panic!("generic scorer must not receive code")
        }
    }
    let f = fixture();
    let p = EngramProviderBuilder::new(CapabilityReport::new(CapabilityState::Supported))
        .code_scorer(Arc::new(GenericIdentityOnly))
        .build();
    assert_eq!(
        p.capabilities().code_scorer,
        CapabilityState::Unsupported {
            reason: CapabilityReason::ProviderUnavailable
        }
    );
    assert!(f.model.queries.load(Ordering::SeqCst) == 0);
}
