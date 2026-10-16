//! Admitted model manifests and cache-confined verified offline byte loading.
mod loading;
mod manifest;
pub use loading::{VerifiedModelAssets, default_cache_root, load};
pub use manifest::{AdmittedModel, ModelAsset, ModelManifest};
