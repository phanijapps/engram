//! Independent lane budgets and timed orchestration before model reranking.
use super::{CodeCandidateSet, LaneHit, failure, fusion, graph, lanes, resolution::Resolver};
use crate::{
    EngramProvider, code_query::prepare_code_query, code_representation::code_representation,
};
use engram_domain::*;
use engram_retrieval::{CodeIntelligenceConfig, CodeRetrievalLane};
use engram_runtime::{CoreError, CoreResult};
use std::{
    collections::{BTreeMap, HashSet},
    time::Instant,
};

/// Trusted SDK stage. Public wire operations supply scope/requester from their
/// host and run scorer preflight before entering this candidate stage.
pub async fn collect(
    provider: &EngramProvider,
    query: &CodeEvidenceQuery,
    scope: &Scope,
    requester: &Requester,
    config: &CodeIntelligenceConfig,
) -> CoreResult<CodeCandidateSet> {
    query.validate().map_err(|e| CoreError::InvalidRequest {
        reason: e.to_string(),
    })?;
    let config = config.for_query(query)?;
    let mut phase_ms = BTreeMap::new();
    let mut candidate_counts = BTreeMap::new();
    let mut source_failures = Vec::new();
    let start = Instant::now();
    let model = provider.require_embedding_provider()?;
    let prepared = prepare_code_query(model.as_ref(), &query.query, config.query.instruction)?;
    let representation = code_representation(model.as_ref(), &config.index)?;
    phase_ms.insert(
        "query_tokenization".into(),
        start.elapsed().as_secs_f64() * 1000.0,
    );
    let request = RetrievalRequest {
        query: query.query.clone(),
        scope: scope.clone(),
        requester: requester.clone(),
        modes: vec![],
        filters: None,
        cues: vec![],
        limit: Some(config.retrieval.candidates_per_lane),
        budget: None,
        include_explanations: Some(true),
    };
    let start = Instant::now();
    let revision = provider
        .code_segments()
        .ok_or_else(|| CoreError::ProviderUnavailable {
            provider: "code segment authority".into(),
        })?
        .code_index_revision()
        .await?;
    let mut resolver = Resolver::new(
        provider,
        &request,
        &representation.id,
        query.repository.as_deref(),
    )
    .await?;
    phase_ms.insert(
        "source_authority".into(),
        start.elapsed().as_secs_f64() * 1000.0,
    );
    let cap = config.retrieval.candidates_per_lane as usize;
    candidate_counts.insert("unavailable_sources".into(), resolver.unavailable_sources);
    if resolver.unavailable_sources > 0 {
        let mut unavailable = failure("code_sources");
        unavailable.reason = "code_source_unavailable".into();
        source_failures.push(unavailable);
    }
    let mut hits = Vec::new();
    for lane in &config.retrieval.enabled_lanes {
        let start = Instant::now();
        let result = if *lane == CodeRetrievalLane::Vector {
            lanes::vector(
                &mut resolver,
                &prepared,
                &representation,
                cap,
                &mut phase_ms,
            )
            .await
        } else {
            lanes::indexed(&mut resolver, *lane, cap).await
        };
        phase_ms.insert(
            format!("{}_candidates", lane.as_str()),
            start.elapsed().as_secs_f64() * 1000.0,
        );
        match result {
            Ok(lane_hits) => {
                candidate_counts.insert(lane.as_str().into(), lane_hits.len() as u32);
                hits.extend(lane_hits);
            }
            Err(_) => {
                source_failures.push(failure(lane.as_str()));
                candidate_counts.insert(lane.as_str().into(), 0);
            }
        }
    }
    let start = Instant::now();
    let initial = fusion::fuse(&request, &config, hits)?;
    phase_ms.insert("fusion".into(), start.elapsed().as_secs_f64() * 1000.0);
    let start = Instant::now();
    let mut seen = HashSet::new();
    let anchors = initial
        .iter()
        .filter(|c| seen.insert(c.parent.id.clone()))
        .map(|c| c.parent.clone())
        .collect();
    let expanded = match graph::expand(&mut resolver, anchors, &config).await {
        Ok(hits) => hits,
        Err(_) => {
            source_failures.push(failure("graph_expansion"));
            vec![]
        }
    };
    candidate_counts.insert("graph_expansion".into(), expanded.len() as u32);
    phase_ms.insert(
        "graph_expansion".into(),
        start.elapsed().as_secs_f64() * 1000.0,
    );
    // Preserve original per-lane rank order while adding the expansion lane.
    let mut final_hits = Vec::new();
    for candidate in initial {
        for trace in candidate.fusion_traces {
            final_hits.push(LaneHit {
                resolved: engram_knowledge::ResolvedCodeSegment {
                    segment: candidate.segment.clone(),
                    parent: candidate.parent.clone(),
                },
                trace,
            });
        }
    }
    final_hits.sort_by(|a, b| {
        a.trace
            .source
            .cmp(&b.trace.source)
            .then_with(|| a.trace.source_rank.cmp(&b.trace.source_rank))
    });
    final_hits.extend(expanded);
    let start = Instant::now();
    let candidates = fusion::fuse(&request, &config, final_hits)?;
    *phase_ms.entry("fusion".into()).or_default() += start.elapsed().as_secs_f64() * 1000.0;
    candidate_counts.insert("fused_segments".into(), candidates.len() as u32);
    candidate_counts.insert(
        "fused_parents".into(),
        candidates
            .iter()
            .map(|c| &c.parent.id)
            .collect::<HashSet<_>>()
            .len() as u32,
    );
    Ok(CodeCandidateSet {
        candidates,
        source_failures,
        candidate_counts,
        phase_ms,
        query_representation_id: prepared.representation_id,
        representation_id: representation.id.clone(),
        source_revision: revision,
        reindex_required: resolver.reindex_required,
    })
}
