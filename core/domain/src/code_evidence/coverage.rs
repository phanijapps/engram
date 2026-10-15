use serde::{Deserialize, Serialize};

/// Actual tokenizer coverage; counts include special tokens and never hide truncation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CodeTokenCoverage {
    pub code_tokens: u32,
    pub context_tokens: u32,
    pub query_tokens: u32,
    pub special_tokens: u32,
    pub total_tokens: u32,
    pub truncated: bool,
}

impl CodeTokenCoverage {
    pub fn is_valid(&self) -> bool {
        !self.truncated
            && self.code_tokens <= 8192
            && self.context_tokens <= 1024
            && self.query_tokens <= 2048
            && self.special_tokens <= 64
            && self.total_tokens <= 16384
            && self.code_tokens + self.context_tokens + self.query_tokens + self.special_tokens
                == self.total_tokens
    }
}
