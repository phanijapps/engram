//! Exact model-tokenizer output; no model or tokenizer implementation lives here.
use engram_domain::CodeByteRange;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextTokenization {
    /// Non-special token offsets in the original UTF-8 input (overlap is valid
    /// for tokenizers that emit multiple tokens for one Unicode scalar).
    pub offsets: Vec<CodeByteRange>,
    pub special_tokens: u32,
    pub window: u32,
    pub tokenizer_id: String,
}

impl TextTokenization {
    pub fn total_tokens(&self) -> usize {
        self.offsets.len() + self.special_tokens as usize
    }
}
