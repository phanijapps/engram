use super::EmbeddingProviderConfig;
use engram_domain::EmbeddingSpace;
use sha2::{Digest, Sha256};

impl EmbeddingProviderConfig {
    /// Full semantic identity; transport tuning and credential references are excluded.
    /// Preserve the original identity when no semantic options are configured.
    pub fn embedding_space(&self) -> EmbeddingSpace {
        let options = &self.options;
        // Earlier bootstrap code always loaded BGE Small even when another
        // local model was declared. Fence those historical mislabeled indexes.
        let configured_local_model =
            self.provider_type == "fastembed" && self.model != "BAAI/bge-small-en-v1.5";
        let profile = if !configured_local_model
            && options.endpoint.is_none()
            && options.query_instruction.is_none()
            && options.passage_instruction.is_none()
        {
            self.prompt_profile.clone()
        } else {
            let semantic = serde_json::to_vec(&(
                options.endpoint.as_ref().map(|s| s.trim_end_matches('/')),
                &options.query_instruction,
                &options.passage_instruction,
            ))
            .expect("string-only semantic tuple serializes");
            format!(
                "{}/embedding-config-v1/{}",
                self.prompt_profile,
                Sha256::digest(semantic)
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<String>()
            )
        };
        EmbeddingSpace::new(
            &self.provider_type,
            &self.model,
            self.dimensions,
            profile,
            self.normalization.clone(),
        )
    }
}
