//! FastEmbed-backed query-vector provider.
//!
//! This module is feature-gated so local model initialization and asset
//! downloads remain opt-in. It produces query vectors only; vector storage,
//! target rehydration, and policy checks stay in their existing collaborators.

use std::sync::Mutex;

use engram_domain::RetrievalRequest;
use engram_runtime::{CoreError, CoreResult};
use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};

use crate::VectorQueryProvider;

const ADAPTER_NAME: &str = "engram-store-vector.fastembed";
const QUERY_PREFIX: &str = "query: ";

/// Query-vector provider backed by FastEmbed BGE-small.
pub struct FastEmbedBgeSmallQueryProvider {
    model: Mutex<TextEmbedding>,
}

impl FastEmbedBgeSmallQueryProvider {
    /// Initializes the BGE-small provider with download progress disabled.
    ///
    /// Construction may download model assets depending on the local FastEmbed
    /// cache. Callers must opt into the Cargo feature before this type exists.
    pub fn new() -> CoreResult<Self> {
        let model = TextEmbedding::try_new(
            InitOptions::new(EmbeddingModel::BGESmallENV15).with_show_download_progress(false),
        )
        .map_err(adapter_error)?;
        Ok(Self {
            model: Mutex::new(model),
        })
    }

    /// Returns the stable model label callers should store with generated vectors.
    ///
    /// The label matches the BGE-small provider selected by this type. It lets
    /// callers keep vector metadata consistent between query embeddings and
    /// passage embeddings without making model identity a domain contract.
    pub fn model_name(&self) -> &'static str {
        "fastembed/bge-small-en-v1.5"
    }

    /// Embeds one retrieval query using the BGE query prefix.
    ///
    /// BGE models distinguish query text from passage text through prompt
    /// prefixes. This method applies the `query:` prefix, serializes access to
    /// the local model, and translates FastEmbed failures into the portable core
    /// adapter error surface.
    pub fn embed_query(&self, query: &str) -> CoreResult<Vec<f32>> {
        let mut model = self.model.lock().map_err(|_| CoreError::Adapter {
            adapter: ADAPTER_NAME.to_owned(),
            message: "FastEmbed model lock poisoned".to_owned(),
        })?;
        let mut embeddings = model
            .embed(vec![format!("{QUERY_PREFIX}{}", query.trim())], None)
            .map_err(adapter_error)?;
        embeddings.pop().ok_or_else(|| CoreError::Adapter {
            adapter: ADAPTER_NAME.to_owned(),
            message: "FastEmbed returned no query embedding".to_owned(),
        })
    }

    /// Embeds one passage (chunk) text without the BGE query prefix.
    ///
    /// Passages are embedded asymmetrically from queries for BGE models: no
    /// `query:` prefix. This is used to vectorize ingested knowledge chunks so
    /// sqlite-vec nearest-neighbor search can match query embeddings.
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
