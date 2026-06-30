//! Retrieval fusion algorithms for Engram.
//!
//! This crate owns deterministic retrieval collaborators that operate on
//! already-produced candidate results. It does not call stores, vector indexes,
//! embedding providers, policy engines, or model rerankers.

mod config;
mod weighted;

pub use config::WeightedFusionConfig;
pub use weighted::WeightedRetrievalFusion;
