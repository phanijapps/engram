//! Retrieval fusion algorithms for Engram.
//!
//! This crate owns deterministic retrieval ports and collaborators that operate
//! on already-produced candidate results. It does not call stores, embedding
//! providers, policy engines, graph databases, or model rerankers.

mod composer;
mod config;
mod ports;
mod weighted;

pub use composer::{RetrievalCompositionInput, compose_context};
pub use config::WeightedFusionConfig;
pub use ports::{ContextComposer, RetrievalFusion, RetrievalIndex};
pub use weighted::WeightedRetrievalFusion;
