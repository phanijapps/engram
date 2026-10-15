//! Verified offline bytes for the admitted BGE encoder, never HF runtime fetch.
use engram_local_model_assets::{AdmittedModel, default_cache_root, load};
use engram_runtime::{CoreError, CoreResult};
use fastembed::{
    EmbeddingModel, InitOptionsUserDefined, Pooling, QuantizationMode, TextEmbedding,
    TokenizerFiles, UserDefinedEmbeddingModel,
};

pub(super) fn open(model: EmbeddingModel) -> CoreResult<(TextEmbedding, String)> {
    open_at(model, &default_cache_root()?)
}

fn open_at(model: EmbeddingModel, cache: &std::path::Path) -> CoreResult<(TextEmbedding, String)> {
    if !matches!(model, EmbeddingModel::BGESmallENV15) {
        return Err(CoreError::CapabilityUnsupported {
            capability: "offline_embedding_model".into(),
            reason: "model needs an admitted offline asset manifest".into(),
        });
    }
    let mut assets = load(AdmittedModel::BgeSmallEnV15, cache)?;
    let manifest = assets.manifest().clone();
    if !manifest.external_data.is_empty()
        || manifest.pooling.as_deref() != Some("cls")
        || manifest.dimensions != Some(384)
    {
        return Err(CoreError::ValidationFailed {
            reason: "admitted BGE input/output contract is incompatible".into(),
        });
    }
    let model = UserDefinedEmbeddingModel::new(
        assets.take("onnx/model.onnx")?,
        TokenizerFiles {
            tokenizer_file: assets.take("tokenizer.json")?,
            config_file: assets.take("config.json")?,
            special_tokens_map_file: assets.take("special_tokens_map.json")?,
            tokenizer_config_file: assets.take("tokenizer_config.json")?,
        },
    )
    .with_pooling(Pooling::Cls)
    .with_quantization(QuantizationMode::None);
    let model = TextEmbedding::try_new_from_user_defined(
        model,
        InitOptionsUserDefined::new()
            .with_max_length(manifest.native_window as usize)
            .with_intra_threads(1),
    )
    .map_err(|_| CoreError::ProviderUnavailable {
        provider: "verified offline BGE initialization failed".into(),
    })?;
    Ok((model, assets.identity().into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn missing_offline_encoder_assets_fail_before_model_construction() {
        let cache = tempfile::tempdir().unwrap();
        assert!(open_at(EmbeddingModel::BGESmallENV15, cache.path()).is_err());
        assert_eq!(std::fs::read_dir(cache.path()).unwrap().count(), 0);
        assert!(matches!(
            open_at(EmbeddingModel::AllMiniLML6V2, cache.path()),
            Err(CoreError::CapabilityUnsupported { .. })
        ));
    }
}
