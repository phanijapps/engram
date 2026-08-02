//! FastEmbed-backed query-vector provider.
//!
//! This module is feature-gated so local model initialization and asset
//! downloads remain opt-in. It produces query vectors only; vector storage,
//! target rehydration, and policy checks stay in their existing collaborators.
//!
//! The provider defaults to BGE-small-en-v1.5 but is configurable via
//! [`FastEmbedBgeSmallQueryProvider::with_model`] — callers can pass any
//! FastEmbed model (e.g., a code-specialised model) with its own label and
//! optional query prefix.

use std::sync::Mutex;

use engram_domain::RetrievalRequest;
use engram_runtime::{CoreError, CoreResult};
use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};

use crate::vector::VectorQueryProvider;

const ADAPTER_NAME: &str = "engram-store-vector.fastembed";

/// Query-vector provider backed by FastEmbed. Defaults to BGE-small-en-v1.5;
/// configurable to any FastEmbed model via [`Self::with_model`].
pub struct FastEmbedBgeSmallQueryProvider {
    model: Mutex<TextEmbedding>,
    model_label: &'static str,
    query_prefix: Option<&'static str>,
}

impl FastEmbedBgeSmallQueryProvider {
    /// Initializes the BGE-small provider with download progress disabled.
    pub fn new() -> CoreResult<Self> {
        Self::with_model(
            EmbeddingModel::BGESmallENV15,
            "fastembed/bge-small-en-v1.5",
            Some("query: "),
        )
    }

    /// Creates a provider with a custom FastEmbed model (e.g., a code-specialised
    /// model for codegraph embedding parity). `model_label` is the stable name
    /// callers store with generated vectors; `query_prefix` is prepended to query
    /// text (BGE models use `"query: "`; most code models use `None`).
    pub fn with_model(
        model: EmbeddingModel,
        model_label: &'static str,
        query_prefix: Option<&'static str>,
    ) -> CoreResult<Self> {
        let text_embedding =
            TextEmbedding::try_new(InitOptions::new(model).with_show_download_progress(false))
                .map_err(adapter_error)?;
        Ok(Self {
            model: Mutex::new(text_embedding),
            model_label,
            query_prefix,
        })
    }

    /// Returns the stable model label callers should store with generated vectors.
    pub fn model_name(&self) -> &'static str {
        self.model_label
    }

    /// Embeds one retrieval query, applying the model's query prefix if set.
    pub fn embed_query(&self, query: &str) -> CoreResult<Vec<f32>> {
        let prefixed = match self.query_prefix {
            Some(prefix) => format!("{prefix}{}", query.trim()),
            None => query.trim().to_owned(),
        };
        let mut model = self.model.lock().map_err(|_| CoreError::Adapter {
            adapter: ADAPTER_NAME.to_owned(),
            message: "FastEmbed model lock poisoned".to_owned(),
        })?;
        let mut embeddings = model.embed(vec![prefixed], None).map_err(adapter_error)?;
        embeddings.pop().ok_or_else(|| CoreError::Adapter {
            adapter: ADAPTER_NAME.to_owned(),
            message: "FastEmbed returned no query embedding".to_owned(),
        })
    }

    /// Embeds one passage (chunk) text without a query prefix.
    pub fn embed_passage(&self, text: &str) -> CoreResult<Vec<f32>> {
        let mut model = self.model.lock().map_err(|_| CoreError::Adapter {
            adapter: ADAPTER_NAME.to_owned(),
            message: "FastEmbed model lock poisoned".to_owned(),
        })?;
        let mut embeddings = model
            .embed(vec![text.trim().to_owned()], None)
            .map_err(adapter_error)?;
        embeddings.pop().ok_or_else(|| CoreError::Adapter {
            adapter: ADAPTER_NAME.to_owned(),
            message: "FastEmbed returned no passage embedding".to_owned(),
        })
    }

    /// Embeds many passage texts in one native model call.
    ///
    /// This uses FastEmbed's batched `TextEmbedding::embed` (which itself splits
    /// the input into internal batches) so ingest paths can embed a whole chunk
    /// batch through the model once instead of N times. Each input text is
    /// trimmed and prefixed exactly as [`embed_passage`](Self::embed_passage)
    /// does, so a batched embedding matches the per-item embedding for the same
    /// text (within floating-point tolerance). Empty inputs return an empty
    /// vector; empty-text items are skipped (no embedding for them) to mirror
    /// the skip-empty-text behavior of the ingest loop.
    pub fn embed_passage_batch(&self, texts: &[String]) -> CoreResult<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        // Trim each passage and remember its position so we can re-expand the
        // model's dense output back into a slot-aligned vector (empty trimmed
        // texts are dropped from the model call and left absent in the output).
        let trimmed: Vec<String> = texts.iter().map(|t| t.trim().to_owned()).collect();
        let live: Vec<(usize, String)> = trimmed
            .iter()
            .enumerate()
            .filter(|(_, t)| !t.is_empty())
            .map(|(i, t)| (i, t.clone()))
            .collect();
        if live.is_empty() {
            return Ok(vec![Vec::new(); texts.len()]);
        }
        let inputs: Vec<String> = live.iter().map(|(_, t)| t.clone()).collect();
        let mut model = self.model.lock().map_err(|_| CoreError::Adapter {
            adapter: ADAPTER_NAME.to_owned(),
            message: "FastEmbed model lock poisoned".to_owned(),
        })?;
        let embeddings = model.embed(inputs, None).map_err(adapter_error)?;
        if embeddings.len() != live.len() {
            return Err(CoreError::Adapter {
                adapter: ADAPTER_NAME.to_owned(),
                message: format!(
                    "FastEmbed batch returned {} embeddings for {} inputs",
                    embeddings.len(),
                    live.len()
                ),
            });
        }
        // Re-expand into the original slot order; empty-trimmed slots get an
        // empty vec so callers can skip them as in the per-item path.
        let mut out: Vec<Vec<f32>> = vec![Vec::new(); texts.len()];
        for (slot, emb) in live.iter().zip(embeddings.into_iter()) {
            out[slot.0] = emb;
        }
        Ok(out)
    }
}

impl VectorQueryProvider for FastEmbedBgeSmallQueryProvider {
    fn query_vector(&self, request: &RetrievalRequest) -> CoreResult<Vec<f32>> {
        self.embed_query(&request.query)
    }
}

fn adapter_error(error: impl std::fmt::Display) -> CoreError {
    CoreError::Adapter {
        adapter: ADAPTER_NAME.to_owned(),
        message: error.to_string(),
    }
}
