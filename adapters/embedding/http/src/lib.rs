//! Bounded Ollama and OpenAI-compatible embedding transport.
mod config;
mod protocol;
mod provider;

pub use config::{HttpEmbeddingConfig, HttpEmbeddingProtocol};
pub use provider::HttpEmbeddingProvider;
