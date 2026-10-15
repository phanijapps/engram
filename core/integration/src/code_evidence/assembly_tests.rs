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
    let config = CodeIntelligenceConfig::default();
    let mut config = config;
    config.evidence.max_per_file = 100;
    let resolved = resolve_code_config(None, None, None).unwrap();
    let admission = Admission {
        config,
        resolved,
        requested: CodeEvidenceReranker::CrossEncoder,
        scorer: None,
        identity: None,
        state: CodeScorerState::Ready,
    };
    let binding:CodeCalibrationArtifact=serde_json::from_value(serde_json::json!({"schemaVersion":1,"id":"test","configHash":"test","queryRepresentationId":"q","passageRepresentationId":"p","embeddingManifestId":"e","scorerManifestId":"s","tokenizerIdentity":"t","scoreTransformation":"raw_logit","scoreSpace":"cross_encoder","threshold":0,"developmentLabelsSha256":"ab".repeat(32)})).unwrap();
    let mut primary = vec![];
    let mut parents = vec![];
    let mut views = vec![];
    for index in 0..names {
        let id = format!("unit-{index}");
        let parent:KnowledgeChunk=serde_json::from_value(serde_json::json!({"id":id,"documentId":"doc","sourceId":"source","kind":"code_block","text":"abc","contentHash":"hash","policy":{"visibility":"workspace","retention":"durable","allowedUses":["retrieval"]},"provenance":{"source":"test","actor":{"id":"actor","kind":"agent"},"observedAt":"2026-10-14T00:00:00Z"},"createdAt":"2026-10-14T00:00:00Z","location":{"anchor":"a".repeat(length)},"code":{"role":"symbol","file":{"scope":{"tenant":"t"},"repository":"repo","path":"a.ts"},"generation":"g","sourceHash":"hash","fingerprint":"f","ranges":[{"start":0,"end":3}],"complete":true,"parseQuality":"clean","unit":id}})).unwrap();
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
            parent: id,
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
    let ranking = Ranking {
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
