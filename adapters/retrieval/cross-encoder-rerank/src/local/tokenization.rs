//! Exact unpadded, untruncated query/document pair accounting.
use engram_domain::{CodeByteRange, CodeTokenCoverage};
use engram_retrieval::TextTokenization;
use engram_runtime::{CoreError, CoreResult};
use sha2::{Digest, Sha256};
use tokenizers::Tokenizer;

pub(super) struct PairTokenizer {
    tokenizer: Tokenizer,
    pub identity: String,
    window: u32,
}
fn invalid() -> CoreError {
    CoreError::ValidationFailed {
        reason: "local scorer tokenizer contract is incompatible".into(),
    }
}
fn ordered(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => serde_json::Value::Object(
            map.into_iter()
                .map(|(k, v)| (k, ordered(v)))
                .collect::<std::collections::BTreeMap<_, _>>()
                .into_iter()
                .collect(),
        ),
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.into_iter().map(ordered).collect())
        }
        other => other,
    }
}
impl PairTokenizer {
    pub fn new(tokenizer: &Tokenizer, window: u32) -> CoreResult<Self> {
        let mut tokenizer = tokenizer.clone();
        tokenizer.with_padding(None);
        tokenizer.with_truncation(None).map_err(|_| invalid())?;
        let json: serde_json::Value =
            serde_json::from_str(&tokenizer.to_string(false).map_err(|_| invalid())?)
                .map_err(|_| invalid())?;
        let bytes = serde_json::to_vec(&ordered(json)).map_err(|_| invalid())?;
        let hash: String = Sha256::digest(bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        Ok(Self {
            tokenizer,
            identity: format!("tokenizer-json-sha256:{hash}/roberta-pair-pad1/window-{window}"),
            window,
        })
    }
    pub fn text(&self, text: &str) -> CoreResult<TextTokenization> {
        let encoded = self.tokenizer.encode(text, false).map_err(|_| invalid())?;
        Ok(TextTokenization {
            offsets: encoded
                .get_offsets()
                .iter()
                .map(|&(start, end)| CodeByteRange {
                    start: start as u64,
                    end: end as u64,
                })
                .collect(),
            special_tokens: 0,
            window: self.window,
            tokenizer_id: self.identity.clone(),
        })
    }
    pub fn pair(
        &self,
        query: &str,
        document: &str,
        prefix: usize,
    ) -> CoreResult<CodeTokenCoverage> {
        if prefix > document.len() || !document.is_char_boundary(prefix) {
            return Err(CoreError::InvalidRequest {
                reason: "scorer context prefix must be a document UTF-8 boundary".into(),
            });
        }
        let encoded = self
            .tokenizer
            .encode((query, document), true)
            .map_err(|_| invalid())?;
        let sequences = encoded.get_sequence_ids();
        let mut coverage = CodeTokenCoverage {
            code_tokens: 0,
            context_tokens: 0,
            query_tokens: 0,
            special_tokens: 0,
            total_tokens: encoded.len() as u32,
            truncated: false,
        };
        for (index, &(_, end)) in encoded.get_offsets().iter().enumerate() {
            if encoded.get_special_tokens_mask()[index] != 0 {
                coverage.special_tokens += 1;
                continue;
            }
            match sequences[index] {
                Some(0) => coverage.query_tokens += 1,
                Some(1) if prefix > 0 && end <= prefix => coverage.context_tokens += 1,
                Some(1) => coverage.code_tokens += 1,
                _ => return Err(invalid()),
            }
        }
        if !coverage.is_valid() {
            return Err(CoreError::InvalidRequest {
                reason:
                    "scorer input exceeds portable token coverage bounds; split document ranges"
                        .into(),
            });
        }
        Ok(coverage)
    }
}
