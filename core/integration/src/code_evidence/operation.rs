//! Shared evidence journey; transports provide trusted scope and requester only.
use super::{
    assembly::{self, PacketInput},
    calibration, eligibility, preflight, ranking,
};
use crate::{EngramProvider, code_candidates};
use engram_domain::*;
use engram_runtime::{CoreError, CoreResult};
use std::{collections::BTreeMap, time::Instant};

fn source_failure(source: &str, reason: &str) -> RetrievalSourceFailure {
    RetrievalSourceFailure {
        source: source.into(),
        mode: None,
        severity: SourceFailureSeverity::Warning,
        reason: reason.into(),
        message: Some("code evidence preparation incomplete".into()),
        degraded: true,
    }
}
async fn unchanged(
    provider: &EngramProvider,
    primary: &[code_candidates::CodeCandidate],
    supporting: &[code_candidates::CodeCandidate],
    scope: &Scope,
    requester: &Requester,
    representation: &str,
    revision: u64,
) -> CoreResult<bool> {
    let port = provider
        .code_segments()
        .ok_or_else(|| CoreError::ProviderUnavailable {
            provider: "code segment authority".into(),
        })?;
    if port.code_index_revision().await? != revision {
        return Ok(false);
    }
    for candidate in primary.iter().chain(supporting) {
        if !eligibility::current(provider, candidate, scope, requester, representation).await? {
            return Ok(false);
        }
    }
    Ok(port.code_index_revision().await? == revision)
}
/// Builds the identity fields for a development calibration artifact. The
/// returned template is deliberately unsealed and has no label digest: it is
/// never a validated runtime threshold until fitted and explicitly installed.
pub fn calibration_template(
    provider: &EngramProvider,
    query: &CodeEvidenceQuery,
) -> CoreResult<CodeCalibrationArtifact> {
    let admission = preflight::prepare(provider, query)?;
    calibration::binding(provider, query, &admission)
}
impl EngramProvider {
    pub async fn code_evidence(
        &self,
        query: &CodeEvidenceQuery,
        scope: &Scope,
        requester: &Requester,
    ) -> CoreResult<CodeEvidencePacket> {
        let start = Instant::now();
        let mut admission = preflight::prepare(self, query)?;
        for (present, key) in [
            (query.result_limit.is_some(), "retrieval.result_limit"),
            (
                query.candidates_per_lane.is_some(),
                "retrieval.candidates_per_lane",
            ),
            (
                query.evidence_token_budget.is_some(),
                "evidence.token_budget",
            ),
        ] {
            if present {
                admission
                    .resolved
                    .origins
                    .insert(key.into(), "request".into());
            }
        }
        let binding = calibration::binding(self, query, &admission)?;
        let preflight_ms = start.elapsed().as_secs_f64() * 1000.0;
        let mut discarded_pairs = 0u32;
        for attempt in 0..2 {
            let mut set =
                code_candidates::collect(self, query, scope, requester, &admission.config).await?;
            let (primary, supporting): (Vec<_>, Vec<_>) = set
                .candidates
                .into_iter()
                .partition(|c| c.segment.source_category == CodeSourceCategory::Implementation);
            set.candidate_counts
                .insert("primary_segments".into(), primary.len() as u32);
            set.candidate_counts
                .insert("supporting_segments".into(), supporting.len() as u32);
            let scoring = Instant::now();
            let ranking = match ranking::rank(
                self,
                &primary,
                &query.query,
                &admission,
                scope,
                requester,
                &set.representation_id,
            )
            .await
            {
                Ok(ranking) => ranking,
                Err(CoreError::Conflict { .. }) => continue,
                Err(error) => return Err(error),
            };
            set.phase_ms
                .insert("rerank".into(), scoring.elapsed().as_secs_f64() * 1000.0);
            if !unchanged(
                self,
                &primary,
                &supporting,
                scope,
                requester,
                &set.representation_id,
                set.source_revision,
            )
            .await?
            {
                discarded_pairs += ranking.attempted_pairs;
                continue;
            }
            let lane_failed = set
                .source_failures
                .iter()
                .any(|f| f.source != "code_sources");
            let validated = if lane_failed || set.reindex_required {
                None
            } else {
                calibration::validated(self.code_calibration(), &binding, &ranking)
            };
            if admission.requested == CodeEvidenceReranker::CrossEncoder
                && ranking.effective != CodeEvidenceReranker::CrossEncoder
            {
                set.source_failures.push(source_failure(
                    "code_scorer",
                    if ranking.state == CodeScorerState::Failed {
                        "code_scorer_failed"
                    } else {
                        "code_scorer_unavailable"
                    },
                ));
            }
            let assembly_start = Instant::now();
            let mut packet = assembly::assemble(
                self.require_embedding_provider()?.as_ref(),
                PacketInput {
                    primary: &primary,
                    supporting: &supporting,
                    ranking: &ranking,
                    admission: &admission,
                    binding: &binding,
                    calibration: validated,
                    counts: set.candidate_counts,
                    phases: set.phase_ms,
                    failures: set.source_failures,
                    reindex_required: set.reindex_required,
                },
            )?;
            packet.diagnostics.phase_ms.insert(
                "assembly".into(),
                assembly_start.elapsed().as_secs_f64() * 1000.0,
            );
            let final_start = Instant::now();
            if !unchanged(
                self,
                &primary,
                &supporting,
                scope,
                requester,
                &set.representation_id,
                set.source_revision,
            )
            .await?
            {
                discarded_pairs += ranking.attempted_pairs;
                continue;
            }
            packet
                .diagnostics
                .candidate_counts
                .insert("discarded_attempt_model_pairs".into(), discarded_pairs);
            packet.diagnostics.phase_ms.insert(
                "final_authorization".into(),
                final_start.elapsed().as_secs_f64() * 1000.0,
            );
            packet
                .diagnostics
                .phase_ms
                .insert("preflight".into(), preflight_ms);
            packet
                .diagnostics
                .phase_ms
                .insert("total".into(), start.elapsed().as_secs_f64() * 1000.0);
            packet
                .diagnostics
                .candidate_counts
                .insert("source_retries".into(), attempt);
            // Added diagnostics are included in the same hard serialized bound.
            if serde_json::to_vec(&packet)
                .map_err(|_| preflight::invalid("invalid evidence packet"))?
                .len()
                > 262144
            {
                return Err(preflight::invalid(
                    "evidence diagnostics exceed serialized packet bound",
                ));
            }
            return Ok(packet);
        }
        // A second concurrent mutation does not inherit any prior scores or
        // references. A subsequent request can observe a stable indexed head.
        let ranking = ranking::rank(
            self,
            &[],
            &query.query,
            &admission,
            scope,
            requester,
            &binding.passage_representation_id,
        )
        .await?;
        let mut counts = BTreeMap::new();
        counts.insert("source_retries".into(), 1);
        counts.insert("discarded_attempt_model_pairs".into(), discarded_pairs);
        let mut phases = BTreeMap::new();
        phases.insert("total".into(), start.elapsed().as_secs_f64() * 1000.0);
        assembly::assemble(
            self.require_embedding_provider()?.as_ref(),
            PacketInput {
                primary: &[],
                supporting: &[],
                ranking: &ranking,
                admission: &admission,
                binding: &binding,
                calibration: None,
                counts,
                phases,
                failures: vec![source_failure(
                    "code_sources",
                    "code_sources_changed_during_query",
                )],
                reindex_required: false,
            },
        )
    }
}
