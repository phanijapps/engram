//! Retrieval fusion algorithms for Engram.
//!
//! This crate owns deterministic retrieval ports and collaborators that operate
//! on already-produced candidate results. It does not call stores, embedding
//! providers, policy engines, graph databases, or model rerankers.

mod composer;
mod config;
mod ports;
mod predict;
mod reciprocal;
mod router;
mod weighted;

pub use composer::{RetrievalCompositionInput, compose_context};
pub use config::{ReciprocalFusionConfig, WeightedFusionConfig};
pub use ports::{ContextComposer, RetrievalFusion, RetrievalIndex};
pub use predict::{AgentState, PredictiveRetriever, RecentActivityPredictor, RetrievalHints};
pub use reciprocal::{DEFAULT_RRF_K, ReciprocalRankFusion};
pub use router::{RetrievalRoute, RetrievalRouteMode, RetrievalRouter, RoutedRetrieval};
pub use weighted::WeightedRetrievalFusion;
