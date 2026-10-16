//! Real local ONNX scorer with admitted assets and complete-pair preflight.
mod results;
mod tokenization;

use engram_domain::CodeTokenCoverage;
use engram_local_model_assets::{AdmittedModel, VerifiedModelAssets};
use engram_retrieval::{
    CodeLocalScorerConfig, RerankScorer, RerankScorerIdentity, TextTokenization,
};
use engram_runtime::{CoreError, CoreResult};
use fastembed::{
    RerankInitOptionsUserDefined, TextRerank, TokenizerFiles, UserDefinedRerankingModel,
};
use std::{path::Path, sync::Mutex};
use tokenization::PairTokenizer;

pub struct LocalOnnxScorer {
    model: Mutex<TextRerank>,
    tokenizer: PairTokenizer,
    identity: RerankScorerIdentity,
    query_limit: u32,
    pair_limit: u32,
    batch_limit: usize,
    candidate_limit: usize,
}
fn unavailable() -> CoreError {
    CoreError::ProviderUnavailable {
        provider: "local ONNX scorer initialization/inference failed".into(),
    }
}
fn invalid(reason: &str) -> CoreError {
    CoreError::InvalidRequest {
        reason: reason.into(),
    }
}
fn files(assets: &mut VerifiedModelAssets) -> CoreResult<TokenizerFiles> {
    Ok(TokenizerFiles {
        tokenizer_file: assets.take("tokenizer.json")?,
        config_file: assets.take("config.json")?,
        special_tokens_map_file: assets.take("special_tokens_map.json")?,
        tokenizer_config_file: assets.take("tokenizer_config.json")?,
    })
}
impl LocalOnnxScorer {
    pub fn open_default(config: &CodeLocalScorerConfig) -> CoreResult<Self> {
        Self::open(&engram_local_model_assets::default_cache_root()?, config)
    }
    pub fn open(cache: &Path, config: &CodeLocalScorerConfig) -> CoreResult<Self> {
        let mut assets =
            engram_local_model_assets::load(AdmittedModel::JinaRerankerV1TurboEn, cache)?;
        let manifest = assets.manifest().clone();
        if config.model_id != manifest.model_id
            || config.query_token_limit < 16
            || config.query_token_limit > 2048
            || config.pair_token_limit < 128
            || config.query_token_limit.saturating_add(4) >= config.pair_token_limit
            || config.pair_token_limit > manifest.native_window
            || !(1..=64).contains(&config.batch_size)
            || !(1..=200).contains(&config.max_candidates)
            || !manifest.external_data.is_empty()
        {
            return Err(invalid(
                "unsupported local scorer model/input configuration",
            ));
        }
        let model_bytes = assets.take("onnx/model.onnx")?;
        let tokenizer_files = files(&mut assets)?;
        let mut model = TextRerank::try_new_from_user_defined(
            UserDefinedRerankingModel::new(model_bytes, tokenizer_files),
            RerankInitOptionsUserDefined::new()
                .with_max_length(config.pair_token_limit as usize)
                .with_intra_threads(1),
        )
        .map_err(|_| unavailable())?;
        // Jina tokenizer_config says512 despite the larger model window.
        // Disable silent truncation; complete pairs are checked before inference.
        model
            .tokenizer
            .with_truncation(None)
            .map_err(|_| unavailable())?;
        let pad = model
            .tokenizer
            .token_to_id("<pad>")
            .filter(|id| *id == 1)
            .ok_or_else(unavailable)?;
        let padding = model.tokenizer.get_padding_mut().ok_or_else(unavailable)?;
        padding.pad_id = pad;
        padding.pad_token = "<pad>".into();
        padding.strategy = tokenizers::PaddingStrategy::BatchLongest;
        let tokenizer = PairTokenizer::new(&model.tokenizer, manifest.native_window)?;
        let identity = RerankScorerIdentity {
            model_id: manifest.model_id,
            manifest_id: assets.identity().into(),
            tokenizer_id: tokenizer.identity.clone(),
            input_contract_id: manifest.input_contract,
            score_contract_id: manifest.score_contract.ok_or_else(unavailable)?,
            window: manifest.native_window,
        };
        Ok(Self {
            model: Mutex::new(model),
            tokenizer,
            identity,
            query_limit: config.query_token_limit,
            pair_limit: config.pair_token_limit,
            batch_limit: config.batch_size as usize,
            candidate_limit: config.max_candidates as usize,
        })
    }
}
impl RerankScorer for LocalOnnxScorer {
    fn supports_code_input(&self) -> bool {
        true
    }
    fn is_local(&self) -> bool {
        true
    }
    fn identity(&self) -> Option<RerankScorerIdentity> {
        Some(self.identity.clone())
    }
    fn inspect_query(&self, query: &str) -> CoreResult<TextTokenization> {
        if query.len() > 16384 {
            return Err(invalid("scorer query byte bound exceeded"));
        }
        self.tokenizer.text(query)
    }
    fn inspect_document(&self, document: &str) -> CoreResult<TextTokenization> {
        if document.len() > 1_048_576 {
            return Err(invalid("scorer document byte bound exceeded"));
        }
        self.tokenizer.text(document)
    }
    fn inspect_pair(
        &self,
        query: &str,
        document: &str,
        prefix: usize,
    ) -> CoreResult<CodeTokenCoverage> {
        if query.len() > 16384 || document.len() > 1_048_576 {
            return Err(invalid("scorer input byte bound exceeded"));
        }
        self.tokenizer.pair(query, document, prefix)
    }
    fn score(&self, query: &str, document: &str) -> CoreResult<f32> {
        self.score_batch(query, &[document.into()], 1)?
            .into_iter()
            .next()
            .ok_or_else(unavailable)
    }
    fn score_batch(
        &self,
        query: &str,
        documents: &[String],
        batch_size: usize,
    ) -> CoreResult<Vec<f32>> {
        if batch_size == 0
            || documents.len() > self.candidate_limit
            || query.len() > 16384
            || query.trim().is_empty()
        {
            return Err(invalid(
                "local scorer query/candidate/batch bounds exceeded",
            ));
        }
        let query_tokens = self.inspect_query(query)?;
        if query_tokens.total_tokens() > self.query_limit as usize {
            return Err(invalid("query exceeds local scorer token limit"));
        }
        // Validate every pair before running even the first model batch.
        for document in documents {
            if document.len() > 1_048_576 {
                return Err(invalid("scorer document byte bound exceeded"));
            }
            let pair = self.inspect_pair(query, document, 0)?;
            if pair.query_tokens > self.query_limit || pair.total_tokens > self.pair_limit {
                return Err(invalid("complete pair exceeds local scorer input limits"));
            }
        }
        if documents.is_empty() {
            return Ok(vec![]);
        }
        let mut model = self.model.lock().map_err(|_| unavailable())?;
        let docs: Vec<_> = documents.iter().map(String::as_str).collect();
        let results = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            model.rerank(query, &docs, false, Some(batch_size.min(self.batch_limit)))
        }))
        .map_err(|_| unavailable())?
        .map_err(|_| unavailable())?;
        results::remap(
            documents.len(),
            results
                .into_iter()
                .map(|result| (result.index, result.score)),
        )
    }
}
