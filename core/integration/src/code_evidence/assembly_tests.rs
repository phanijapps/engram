use super::*;
use crate::{EmbeddingProvider, code_configuration::resolve_code_config};
use engram_retrieval::{CodeIntelligenceConfig, TextTokenization};
struct Counter;
impl EmbeddingProvider for Counter {
    fn is_local(&self) -> bool {
        true
    }
    fn provider_id(&self) -> &str {
        "test"
    }
    fn model_id(&self) -> &str {
        "test"
    }
    fn dimensions(&self) -> u32 {
        3
    }
    fn embedding_space(&self) -> EmbeddingSpace {
        EmbeddingSpace::new("test", "test", 3, "passage", None::<String>)
    }
    fn embed_query(&self, _: &str) -> CoreResult<Vec<f32>> {
        unreachable!()
    }
    fn embed_passage(&self, _: &str) -> CoreResult<Vec<f32>> {
        unreachable!()
    }
    fn tokenize_untruncated(&self, text: &str, special: bool) -> CoreResult<TextTokenization> {
        Ok(TextTokenization {
            offsets: text
                .char_indices()
                .map(|(i, c)| CodeByteRange {
                    start: i as u64,
                    end: (i + c.len_utf8()) as u64,
                })
                .collect(),
            special_tokens: if special { 2 } else { 0 },
            window: 512,
            tokenizer_id: "test".into(),
        })
    }
}
fn packet(names: usize, length: usize) -> CodeEvidencePacket {
    packet_with(names, length, |_, _, _, _| {})
}
fn packet_with(
    names: usize,
    length: usize,
    edit: impl FnOnce(
        &mut Admission,
        &mut Vec<CodeCandidate>,
        &mut Ranking,
        &mut CodeCalibrationArtifact,
    ),
) -> CodeEvidencePacket {
    let config = CodeIntelligenceConfig::default();
    let mut config = config;
    config.evidence.max_per_file = 100;
    let resolved = resolve_code_config(None, None, None).unwrap();
    let mut admission = Admission {
        config,
        resolved,
        requested: CodeEvidenceReranker::CrossEncoder,
        scorer: None,
        identity: None,
        state: CodeScorerState::Ready,
    };
    let mut binding:CodeCalibrationArtifact=serde_json::from_value(serde_json::json!({"schemaVersion":1,"id":"test","configHash":"test","queryRepresentationId":"q","passageRepresentationId":"p","embeddingManifestId":"e","scorerManifestId":"s","tokenizerIdentity":"t","scoreTransformation":"raw_logit","scoreSpace":"cross_encoder","threshold":0,"developmentLabelsSha256":"ab".repeat(32)})).unwrap();
    let mut primary = vec![];
    let mut parents = vec![];
    let mut views = vec![];
    for index in 0..names {
        let id = format!("unit-{index}");
        let parent:KnowledgeChunk=serde_json::from_value(serde_json::json!({"id":format!("chunk-{index}"),"documentId":"doc","sourceId":"source","kind":"code_block","text":"abc","contentHash":"hash","policy":{"visibility":"workspace","retention":"durable","allowedUses":["retrieval"]},"provenance":{"source":"test","actor":{"id":"actor","kind":"agent"},"observedAt":"2026-10-14T00:00:00Z"},"createdAt":"2026-10-14T00:00:00Z","location":{"anchor":"a".repeat(length)},"code":{"role":"symbol","file":{"scope":{"tenant":"t"},"repository":"repo","path":"a.ts"},"generation":"g","sourceHash":"hash","fingerprint":"f","ranges":[{"start":0,"end":3}],"complete":true,"parseQuality":"clean","unit":id}})).unwrap();
        let code = parent.code.as_ref().unwrap();
        let segment = CodeEmbeddingSegment {
            id: format!("segment-{index}"),
            parent_chunk_id: parent.id.clone(),
            parent_unit_id: id.clone(),
            file: code.file.clone(),
            generation: "g".into(),
            attempt_id: "a".into(),
            source_hash: "hash".into(),
            representation_id: "p".into(),
            ranges: code.ranges.clone(),
            continuation: false,
            source_category: CodeSourceCategory::Implementation,
            context: vec![],
            embedding_input: CodeTokenCoverage {
                code_tokens: 3,
                context_tokens: 0,
                query_tokens: 0,
                special_tokens: 2,
                total_tokens: 5,
                truncated: false,
            },
        };
        views.push(super::super::views::View {
            id: format!("view-{index}"),
            original: index,
            ranges: segment.ranges.clone(),
            context: vec![],
            text: "abc".into(),
            coverage: CodeTokenCoverage {
                code_tokens: 3,
                context_tokens: 0,
                query_tokens: 1,
                special_tokens: 4,
                total_tokens: 8,
                truncated: false,
            },
        });
        parents.push(super::super::ranking::RankedParent {
            parent: parent.id.to_string(),
            segments: vec![(index, Some(index), 1.0)],
            score: 1.0,
        });
        let trace: FusionTrace =
            serde_json::from_value(serde_json::json!({"source":"vector","sourceRank":index+1}))
                .unwrap();
        primary.push(CodeCandidate {
            indexed_revision: None,
            parent,
            segment,
            fusion_score: 0.01,
            fusion_traces: vec![trace],
        });
    }
    let mut ranking = Ranking {
        parents,
        views,
        scores: vec![1.0; names],
        state: CodeScorerState::Ready,
        effective: CodeEvidenceReranker::CrossEncoder,
        omitted_segments: 0,
        omitted_parents: 0,
        remaining_continuations: 0,
        prepared_views: names as u32,
        unscored_views: 0,
        attempted_pairs: names as u32,
        pool_omitted_segments: 0,
    };
    edit(&mut admission, &mut primary, &mut ranking, &mut binding);
    assemble(
        &Counter,
        PacketInput {
            primary: &primary,
            supporting: &[],
            ranking: &ranking,
            admission: &admission,
            binding: &binding,
            calibration: Some(&binding),
            counts: BTreeMap::new(),
            phases: BTreeMap::new(),
            failures: vec![],
            reindex_required: false,
        },
    )
    .unwrap()
}
#[test]
fn byte_cap_discards_entire_winning_records_and_preserves_the_rest() {
    let packet = packet(10, 40000);
    assert!(!packet.items.is_empty() && packet.items.len() < 10);
    assert!(serde_json::to_vec(&packet).unwrap().len() <= 262144);
    assert_eq!(
        packet.diagnostics.omitted_units,
        10 - packet.items.len() as u32
    );
    assert!(packet.diagnostics.truncated);
    for item in packet.items {
        assert_eq!(item.rerank_score, Some(1.0));
        assert_eq!(item.excerpts[0].text, "abc");
        let segment = &item.segments[0];
        assert_eq!(segment.rerank_score, Some(1.0));
        assert_eq!(
            segment.rerank_sources.as_ref().unwrap()[0].unit_id,
            item.parent_unit_id
        );
        assert!(segment.rerank_input.as_ref().unwrap().is_valid());
        assert!(segment.rerank_context.is_some());
    }
}
#[test]
fn byte_budget_removing_every_primary_recomputes_insufficient_status() {
    let packet = packet(1, 270000);
    assert!(packet.items.is_empty());
    assert_eq!(
        packet.evidence_status,
        CodeEvidenceStatus::InsufficientEvidence
    );
    assert_eq!(
        packet.diagnostics.calibration_state,
        CodeCalibrationState::Validated
    );
    assert_eq!(packet.diagnostics.omitted_units, 1);
}

#[test]
fn parent_decisions_explain_threshold_pool_and_selection() {
    let selected = packet(1, 3);
    assert_eq!(selected.diagnostics.parent_decisions.len(), 1);
    let decision = &selected.diagnostics.parent_decisions[0];
    assert_eq!(decision.reason, CodeParentDecisionReason::Selected);
    assert_eq!(decision.rerank_score, Some(1.0));
    assert_eq!(decision.applied_threshold, Some(0.0));
    assert_eq!(
        (decision.retrieved_segments, decision.scored_segments),
        (1, 1)
    );
    let threshold = packet_with(1, 3, |_, _, _, binding| binding.threshold = 2.0);
    assert!(threshold.items.is_empty());
    let decision = &threshold.diagnostics.parent_decisions[0];
    assert_eq!(decision.reason, CodeParentDecisionReason::BelowThreshold);
    assert_eq!(decision.applied_threshold, Some(2.0));
    let pool = packet_with(2, 3, |_, _, ranking, _| {
        ranking.parents.truncate(1);
        ranking.views.truncate(1);
        ranking.scores.truncate(1);
    });
    let decision = pool
        .diagnostics
        .parent_decisions
        .iter()
        .find(|d| d.parent_unit_id == "unit-1")
        .unwrap();
    assert_eq!(decision.reason, CodeParentDecisionReason::ScorerPool);
    assert_eq!(decision.rerank_score, None);
    assert_eq!(decision.scored_segments, 0);
}

#[test]
fn parent_decisions_distinguish_output_limits() {
    for (kind, expected) in [
        (0, CodeParentDecisionReason::ResultLimit),
        (1, CodeParentDecisionReason::PerFileLimit),
        (2, CodeParentDecisionReason::EvidenceBudget),
    ] {
        let p = packet_with(2, 3, |a, _, _, _| match kind {
            0 => a.config.retrieval.result_limit = 1,
            1 => a.config.evidence.max_per_file = 1,
            _ => a.config.evidence.token_budget = 2,
        });
        assert!(
            p.diagnostics
                .parent_decisions
                .iter()
                .any(|d| d.reason == expected)
        );
    }
    let p = packet(10, 40000);
    for d in &p.diagnostics.parent_decisions {
        assert_eq!(
            d.reason == CodeParentDecisionReason::Selected,
            p.items.iter().any(|i| i.parent_unit_id == d.parent_unit_id)
        );
    }
    assert!(
        p.diagnostics
            .parent_decisions
            .iter()
            .any(|d| d.reason == CodeParentDecisionReason::SerializedByteLimit)
    );
}

#[test]
fn legacy_packets_decode_with_no_parent_decisions() {
    let mut json = serde_json::to_value(packet(1, 3)).unwrap();
    json["diagnostics"]
        .as_object_mut()
        .unwrap()
        .remove("parentDecisions");
    json["diagnostics"]
        .as_object_mut()
        .unwrap()
        .remove("omittedParentDecisions");
    let packet: CodeEvidencePacket = serde_json::from_value(json).unwrap();
    assert!(packet.diagnostics.parent_decisions.is_empty());
    assert_eq!(packet.diagnostics.omitted_parent_decisions, 0);
}

#[test]
fn parent_decision_limits_discard_diagnostics_before_evidence() {
    let bounded = packet(250, 3);
    assert_eq!(bounded.diagnostics.parent_decisions.len(), 200);
    assert_eq!(bounded.diagnostics.omitted_parent_decisions, 50);
    let pressure = packet(250, 23000);
    assert_eq!(pressure.items, packet(10, 23000).items);
    assert!(pressure.diagnostics.parent_decisions.len() < 200);
    assert_eq!(
        pressure.diagnostics.omitted_parent_decisions as usize,
        250 - pressure.diagnostics.parent_decisions.len()
    );
    assert!(serde_json::to_vec(&pressure).unwrap().len() <= 261632);
}

#[test]
fn query_confidence_keeps_lower_scored_parts_of_an_accepted_answer() {
    let p = packet_with(2, 3, |_, _, ranking, _| {
        ranking.parents[1].score = -1.0;
        ranking.parents[1].segments[0].2 = -1.0;
        ranking.scores[1] = -1.0;
    });
    assert_eq!(
        p.items.len(),
        2,
        "one passing parent must not hide other ranked implementation parts"
    );
    assert_eq!(p.items[1].rerank_score, Some(-1.0));
    assert_eq!(p.evidence_status, CodeEvidenceStatus::CandidatesAvailable);
    assert!(
        p.diagnostics
            .parent_decisions
            .iter()
            .all(|d| d.reason == CodeParentDecisionReason::Selected)
    );
}

#[test]
fn query_confidence_requires_a_returnable_passing_parent() {
    let p = packet_with(2, 3, |_, primary, ranking, _| {
        primary[0].parent.location.as_mut().unwrap().anchor = Some("a".repeat(270000));
        ranking.parents[1].score = -1.0;
        ranking.parents[1].segments[0].2 = -1.0;
        ranking.scores[1] = -1.0;
    });
    assert!(
        p.items.is_empty(),
        "a high score without returnable evidence cannot admit an answer"
    );
    assert_eq!(p.evidence_status, CodeEvidenceStatus::InsufficientEvidence);
    assert_eq!(
        p.diagnostics.parent_decisions[1].reason,
        CodeParentDecisionReason::BelowThreshold
    );
}

#[test]
fn calibrated_weak_tail_uses_lane_rank_without_reordering_confident_head() {
    let configure = |a: &mut Admission,
                     primary: &mut Vec<CodeCandidate>,
                     ranking: &mut Ranking,
                     _: &mut CodeCalibrationArtifact| {
        a.config.retrieval.result_limit = 2;
        ranking.parents[1].score = -0.1;
        ranking.parents[2].score = -0.2;
        primary[1].fusion_traces[0].source_rank = Some(50);
        primary[2].fusion_traces[0].source_rank = Some(1);
    };
    let p = packet_with(3, 3, configure);
    assert_eq!(
        p.items
            .iter()
            .map(|i| i.parent_unit_id.as_str())
            .collect::<Vec<_>>(),
        ["unit-0", "unit-2"]
    );
    assert_eq!(p.items[1].rerank_score, Some(-0.2));
    let old = packet_with(3, 3, |a, primary, ranking, c| {
        configure(a, primary, ranking, c);
        a.config.evidence.weak_parent_order = engram_retrieval::CodeWeakParentOrder::Scorer;
    });
    assert_eq!(old.items[1].parent_unit_id, "unit-1");
}

#[test]
fn graph_expansion_and_zero_weight_lanes_do_not_supply_a_fallback_rank() {
    for zero_vector in [false, true] {
        let p = packet_with(3, 3, |a, primary, ranking, _| {
            a.config.retrieval.result_limit = 2;
            ranking.parents[1].score = -0.1;
            ranking.parents[2].score = -0.2;
            primary[1].fusion_traces = vec![serde_json::from_value(serde_json::json!({"source":if zero_vector {"vector"} else {"graph_expansion"},"sourceRank":1})).unwrap()];
            primary[2].fusion_traces = vec![
                serde_json::from_value(serde_json::json!({"source":"lexical","sourceRank":2}))
                    .unwrap(),
            ];
            if zero_vector {
                a.config.fusion.source_weights.insert("vector".into(), 0.0);
            }
        });
        assert_eq!(p.items[0].parent_unit_id, "unit-0");
        assert_eq!(p.items[1].parent_unit_id, "unit-2");
    }
}
