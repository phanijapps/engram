//! Pinned model/calibration descriptors; loading and inference belong to adapters.
use crate::CodeScoreSpace;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CodeModelAsset {
    pub path: String,
    /// Lowercase hex SHA-256 of asset bytes, separate from opaque source hashes.
    pub sha256: String,
    pub role: CodeModelAssetRole,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CodeModelAssetRole {
    Onnx,
    OnnxData,
    Tokenizer,
    TokenizerConfig,
    SpecialTokens,
    ModelConfig,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CodeScoreTransformation {
    RawLogit,
    Sigmoid,
    None,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CodeModelManifest {
    pub schema_version: u32,
    pub id: String,
    pub model_id: String,
    pub revision: String,
    pub license: String,
    /// Versioned pair order, token preparation and output interpretation contract.
    pub input_contract_id: String,
    pub assets: Vec<CodeModelAsset>,
    pub tokenizer_identity: String,
    pub tokenizer_window: u32,
    pub score_transformation: CodeScoreTransformation,
}

/// Development-only fit with explicit identities; runtime never receives labels.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CodeCalibrationArtifact {
    pub schema_version: u32,
    pub id: String,
    pub config_hash: String,
    pub query_representation_id: String,
    pub passage_representation_id: String,
    pub embedding_manifest_id: String,
    #[serde(deserialize_with = "crate::code_evidence::required_nullable")]
    pub scorer_manifest_id: Option<String>,
    pub tokenizer_identity: String,
    pub score_transformation: CodeScoreTransformation,
    pub score_space: CodeScoreSpace,
    pub threshold: f32,
    pub development_labels_sha256: String,
}
