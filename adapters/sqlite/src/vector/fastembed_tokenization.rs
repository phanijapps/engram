//! Exact tokenizer contract from the same installed model used for inference.
use super::fastembed_provider::adapter_error;
use engram_domain::CodeByteRange;
use engram_retrieval::TextTokenization;
use engram_runtime::{CoreError, CoreResult};
use sha2::{Digest, Sha256};

pub(super) struct LocalTokenizer {
    tokenizer: tokenizers::Tokenizer,
    identity: String,
    window: u32,
}

fn canonical_json(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            let ordered: std::collections::BTreeMap<_, _> = map
                .into_iter()
                .map(|(key, value)| (key, canonical_json(value)))
                .collect();
            serde_json::Value::Object(ordered.into_iter().collect())
        }
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.into_iter().map(canonical_json).collect())
        }
        other => other,
    }
}
impl LocalTokenizer {
    pub(super) fn new(model: &fastembed::TextEmbedding) -> CoreResult<Self> {
        let window = model
            .tokenizer
            .get_truncation()
            .and_then(|t| u32::try_from(t.max_length).ok())
            .filter(|v| *v > 0)
            .ok_or_else(|| CoreError::ValidationFailed {
                reason: "embedding tokenizer has no declared positive input window".into(),
            })?;
        let mut tokenizer = model.tokenizer.clone();
        tokenizer.with_padding(None);
        tokenizer.with_truncation(None).map_err(adapter_error)?;
        let value = serde_json::from_str(&tokenizer.to_string(false).map_err(adapter_error)?)
            .map_err(adapter_error)?;
        let bytes = serde_json::to_vec(&canonical_json(value)).map_err(adapter_error)?;
        let digest: String = Sha256::digest(bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        let identity = format!("tokenizer-json-sha256:{digest}/window-{window}");
        Ok(Self {
            tokenizer,
            identity,
            window,
        })
    }
    pub(super) fn tokenize(
        &self,
        text: &str,
        special_tokens: bool,
    ) -> CoreResult<TextTokenization> {
        let encoded = self
            .tokenizer
            .encode(text, special_tokens)
            .map_err(adapter_error)?;
        let offsets = encoded
            .get_offsets()
            .iter()
            .zip(encoded.get_special_tokens_mask())
            .filter(|(_, mask)| **mask == 0)
            .map(|(&(start, end), _)| CodeByteRange {
                start: start as u64,
                end: end as u64,
            })
            .collect();
        let special_tokens = encoded
            .get_special_tokens_mask()
            .iter()
            .filter(|v| **v != 0)
            .count() as u32;
        Ok(TextTokenization {
            offsets,
            special_tokens,
            window: self.window,
            tokenizer_id: self.identity.clone(),
        })
    }
}
