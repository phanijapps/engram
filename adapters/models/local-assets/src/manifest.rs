//! Compiled-in admission list; callers cannot supply an arbitrary manifest URL.
use engram_runtime::{CoreError, CoreResult};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Copy)]
pub enum AdmittedModel {
    BgeSmallEnV15,
    JinaRerankerV1TurboEn,
}
impl AdmittedModel {
    pub fn slug(self) -> &'static str {
        match self {
            Self::BgeSmallEnV15 => "bge-small-en-v1.5",
            Self::JinaRerankerV1TurboEn => "jina-reranker-v1-turbo-en",
        }
    }
    pub fn manifest(self) -> CoreResult<(ModelManifest, String)> {
        let raw: &[u8] = match self {
            Self::BgeSmallEnV15 => include_bytes!("../manifests/bge-small-en-v1.5.json"),
            Self::JinaRerankerV1TurboEn => {
                include_bytes!("../manifests/jina-reranker-v1-turbo-en.json")
            }
        };
        let manifest: ModelManifest =
            serde_json::from_slice(raw).map_err(|_| CoreError::ValidationFailed {
                reason: "invalid admitted model manifest".into(),
            })?;
        manifest.validate()?;
        Ok((manifest, format!("model-manifest-v1/{}", digest(raw))))
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModelManifest {
    pub schema_version: u32,
    pub model_id: String,
    pub artifact_repository: String,
    pub revision: String,
    pub license: String,
    pub license_source: String,
    pub native_window: u32,
    pub dimensions: Option<u32>,
    pub pooling: Option<String>,
    pub normalization: Option<String>,
    pub input_contract: String,
    pub score_contract: Option<String>,
    pub external_data: Vec<String>,
    pub assets: Vec<ModelAsset>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModelAsset {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
}
impl ModelManifest {
    pub(crate) fn validate(&self) -> CoreResult<()> {
        let invalid = || CoreError::ValidationFailed {
            reason: "invalid admitted model asset contract".into(),
        };
        if self.schema_version != 1
            || self.revision.len() != 40
            || !self.revision.bytes().all(|c| c.is_ascii_hexdigit())
            || self.native_window == 0
            || self.assets.len() > 16
            || self.assets.is_empty()
        {
            return Err(invalid());
        }
        let mut paths = std::collections::HashSet::new();
        for asset in &self.assets {
            if !safe_asset_path(&asset.path)
                || !paths.insert(asset.path.as_str())
                || asset.bytes == 0
                || asset.bytes > 1_000_000_000
                || asset.sha256.len() != 64
                || !asset.sha256.bytes().all(|c| c.is_ascii_hexdigit())
            {
                return Err(invalid());
            }
        }
        for path in &self.external_data {
            if !paths.contains(path.as_str()) {
                return Err(invalid());
            }
        }
        Ok(())
    }
}
pub(crate) fn safe_asset_path(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.contains('\\')
        && !path.contains(':')
        && path.split('/').all(|part| !matches!(part, "" | "." | ".."))
}

pub(crate) fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
