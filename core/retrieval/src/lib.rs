//! Retrieval fusion algorithms for Engram.
//!
//! This crate owns deterministic retrieval ports and collaborators that operate
//! on already-produced candidate results. It does not call stores, embedding
//! providers, policy engines, graph databases, or model rerankers.

mod composer;
mod config;
mod graph_cache;
mod ports;
mod predict;
mod reciprocal;
mod router;
mod vector_index;
mod weighted;

pub use composer::{RetrievalCompositionInput, compose_context};
pub use config::{
    KNOWN_LANE_TAGS, RecallFusionConfig, ReciprocalFusionConfig, RerankConfig,
    WeightedFusionConfig, unknown_lane_keys,
};
pub use graph_cache::{GraphCache, GraphSnapshot, InMemoryGraphCache, scope_key};
pub use ports::{ContextComposer, RetrievalFusion, RetrievalIndex, RetrievalReranker};
pub use predict::{AgentState, PredictiveRetriever, RecentActivityPredictor, RetrievalHints};
pub use reciprocal::{DEFAULT_RRF_K, ReciprocalRankFusion};
pub use router::{RetrievalRoute, RetrievalRouteMode, RetrievalRouter, RoutedRetrieval};
pub use vector_index::VectorIndex;
pub use weighted::WeightedRetrievalFusion;
