//! Immutable development-artifact matching; scores are never probabilities.
use super::{preflight::Admission, ranking::Ranking, views};
use crate::{
    EngramProvider, code_query::prepare_code_query, code_representation::code_representation,
};
use engram_domain::*;
use engram_runtime::{CoreError, CoreResult};
use sha2::{Digest, Sha256};
fn hash<T: serde::Serialize>(value: &T) -> CoreResult<String> {
    let bytes = serde_json::to_vec(value).map_err(|_| CoreError::ValidationFailed {
        reason: "invalid code calibration identity".into(),
    })?;
    Ok(Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}
/// Deterministic seal over all artifact fields except the seal itself. Loading
/// remains a trusted operator action; runtime never receives evaluation labels.
pub fn artifact_id(artifact: &CodeCalibrationArtifact) -> CoreResult<String> {
    if !artifact.threshold.is_finite() {
        return Err(CoreError::InvalidRequest {
            reason: "nonfinite calibration threshold".into(),
        });
    }
    let mut value = artifact.clone();
    value.id.clear();
    Ok(format!("code-calibration-v1/{}", hash(&value)?))
}
pub(super) fn binding(
    provider: &EngramProvider,
    query: &CodeEvidenceQuery,
    admission: &Admission,
) -> CoreResult<CodeCalibrationArtifact> {
    let model = provider.require_embedding_provider()?;
    let representation = code_representation(model.as_ref(), &admission.config.index)?;
    let prepared = prepare_code_query(
        model.as_ref(),
        &query.query,
        admission.config.query.instruction,
    )?;
    let config_hash = hash(&(
        "code-evidence-binding-v2/packet-max-confidence",
        &admission.config,
        model.asset_identity(),
        &representation,
        &prepared.representation_id,
        &admission.identity,
        views::CONTRACT,
    ))?;
    let tokenizer_identity = hash(&(
        &representation.tokenizer_id,
        admission
            .identity
            .as_ref()
            .map(|s| (&s.tokenizer_id, &s.input_contract_id, &s.score_contract_id)),
        views::CONTRACT,
    ))?;
    let cross = admission.requested == CodeEvidenceReranker::CrossEncoder;
    Ok(CodeCalibrationArtifact {
        schema_version: 1,
        id: String::new(),
        config_hash,
        query_representation_id: prepared.representation_id,
        passage_representation_id: representation.id,
        embedding_manifest_id: model.asset_identity().unwrap_or("unverified").into(),
        scorer_manifest_id: admission.identity.as_ref().map(|s| s.manifest_id.clone()),
        tokenizer_identity,
        score_transformation: if cross {
            CodeScoreTransformation::RawLogit
        } else {
            CodeScoreTransformation::None
        },
        score_space: if cross {
            CodeScoreSpace::CrossEncoder
        } else {
            CodeScoreSpace::Rrf
        },
        threshold: 0.0,
        development_labels_sha256: String::new(),
    })
}
pub(super) fn validated<'a>(
    artifact: Option<&'a CodeCalibrationArtifact>,
    expected: &CodeCalibrationArtifact,
    ranking: &Ranking,
) -> Option<&'a CodeCalibrationArtifact> {
    let artifact = artifact?;
    if expected.embedding_manifest_id == "unverified"
        || (expected.score_space == CodeScoreSpace::CrossEncoder
            && (ranking.effective != CodeEvidenceReranker::CrossEncoder
                || ranking.state != CodeScorerState::Ready))
    {
        return None;
    }
    if artifact.schema_version != 1
        || artifact.development_labels_sha256.len() != 64
        || !artifact
            .development_labels_sha256
            .bytes()
            .all(|b| b.is_ascii_hexdigit())
        || artifact
            .development_labels_sha256
            .bytes()
            .all(|b| b == b'0')
        || artifact_id(artifact).ok().as_deref() != Some(&artifact.id)
    {
        return None;
    }
    let mut actual = artifact.clone();
    actual.id.clear();
    actual.threshold = 0.0;
    actual.development_labels_sha256.clear();
    (&actual == expected).then_some(artifact)
}
