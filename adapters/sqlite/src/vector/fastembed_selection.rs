//! Catalogue-only model resolution: validates identity and dimensions before loading assets.
use engram_runtime::{CoreError, CoreResult};
use fastembed::{EmbeddingModel, TextEmbedding};

#[derive(Clone, Debug)]
pub struct FastEmbedModelSelection {
    pub dimensions: u32,
    pub model_id: String,
    pub(crate) model: EmbeddingModel,
}

impl FastEmbedModelSelection {
    /// Resolve a FastEmbed enum name, catalogue model code or supported BAAI alias.
    /// Zero dimensions selects the model's native output size.
    pub fn resolve(model: &str, dimensions: u32) -> CoreResult<Self> {
        let model = model.trim();
        let alias = match model.to_ascii_lowercase().as_str() {
            "baai/bge-small-en-v1.5" | "fastembed/bge-small-en-v1.5" | "bge-small-en-v1.5" => {
                Some(EmbeddingModel::BGESmallENV15)
            }
            "baai/bge-base-en-v1.5" => Some(EmbeddingModel::BGEBaseENV15),
            "baai/bge-large-en-v1.5" => Some(EmbeddingModel::BGELargeENV15),
            "baai/bge-small-zh-v1.5" => Some(EmbeddingModel::BGESmallZHV15),
            "baai/bge-large-zh-v1.5" => Some(EmbeddingModel::BGELargeZHV15),
            _ => None,
        };
        let mut catalogue = TextEmbedding::list_supported_models();
        // Some upstream model codes have quantized variants. Resolve those aliases
        // deterministically to the unsuffixed enum; enum names select variants exactly.
        catalogue.sort_by_key(|info| format!("{:?}", info.model));
        let info = catalogue
            .into_iter()
            .find(|info| {
                alias.as_ref() == Some(&info.model)
                    || info.model_code.eq_ignore_ascii_case(model)
                    || format!("{:?}", info.model).eq_ignore_ascii_case(model)
            })
            .ok_or_else(|| CoreError::InvalidRequest {
                reason: "unsupported FastEmbed model; select a catalogue model or served provider"
                    .into(),
            })?;
        if dimensions != 0 && dimensions as usize != info.dim {
            return Err(CoreError::InvalidRequest {
                reason: format!(
                    "FastEmbed model requires {} dimensions; arbitrary resizing is unsupported",
                    info.dim
                ),
            });
        }
        let model_id = match info.model {
            EmbeddingModel::BGESmallENV15 => "BAAI/bge-small-en-v1.5".into(),
            EmbeddingModel::BGEBaseENV15 => "BAAI/bge-base-en-v1.5".into(),
            EmbeddingModel::BGELargeENV15 => "BAAI/bge-large-en-v1.5".into(),
            EmbeddingModel::BGEM3 => "BAAI/bge-m3".into(),
            EmbeddingModel::BGESmallZHV15 => "BAAI/bge-small-zh-v1.5".into(),
            EmbeddingModel::BGELargeZHV15 => "BAAI/bge-large-zh-v1.5".into(),
            _ => format!("{:?}", info.model),
        };
        Ok(Self {
            dimensions: info.dim as u32,
            model_id,
            model: info.model,
        })
    }

    pub(crate) fn instructions(&self) -> (Option<String>, Option<String>) {
        let name = format!("{:?}", self.model);
        if name.starts_with("Nomic") {
            (
                Some("search_query: ".into()),
                Some("search_document: ".into()),
            )
        } else if name.starts_with("MultilingualE5") {
            (Some("query: ".into()), Some("passage: ".into()))
        } else if name.starts_with("BGE") && name != "BGEM3" {
            // Preserve Engram's historical BGE query convention.
            (Some("query: ".into()), None)
        } else if name.starts_with("Mxbai") {
            (
                Some("Represent this sentence for searching relevant passages: ".into()),
                None,
            )
        } else {
            (None, None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_presets_and_aliases_without_loading_models() {
        for (name, dim) in [
            ("BAAI/bge-small-en-v1.5", 384),
            ("BAAI/bge-base-en-v1.5", 768),
            ("BAAI/bge-m3", 1024),
            ("JinaEmbeddingsV2BaseCode", 768),
        ] {
            assert_eq!(
                FastEmbedModelSelection::resolve(name, 0)
                    .unwrap()
                    .dimensions,
                dim
            );
        }
        let a = FastEmbedModelSelection::resolve("BAAI/bge-small-en-v1.5", 384).unwrap();
        let b = FastEmbedModelSelection::resolve("Xenova/bge-small-en-v1.5", 0).unwrap();
        assert_eq!(a.model_id, b.model_id);
        assert_eq!(a.instructions(), (Some("query: ".into()), None));
    }

    #[test]
    fn rejects_unknown_model_and_wrong_dimensions_before_loading() {
        assert!(FastEmbedModelSelection::resolve("made-up/model", 384).is_err());
        assert!(FastEmbedModelSelection::resolve("BAAI/bge-small-en-v1.5", 768).is_err());
        assert!(FastEmbedModelSelection::resolve("BAAI/bge-m3", 8193).is_err());
    }
    #[test]
    fn forged_selection_is_rejected_before_model_load() {
        let mut selection = FastEmbedModelSelection::resolve("BGESmallENV15", 0).unwrap();
        selection.dimensions = 768;
        assert!(
            crate::vector::FastEmbedBgeSmallQueryProvider::from_selection(selection, None, None)
                .is_err()
        );
        let mut selection = FastEmbedModelSelection::resolve("BGESmallENV15", 0).unwrap();
        selection.model_id = "pretend-model".into();
        assert!(
            crate::vector::FastEmbedBgeSmallQueryProvider::from_selection(selection, None, None)
                .is_err()
        );
    }
}
