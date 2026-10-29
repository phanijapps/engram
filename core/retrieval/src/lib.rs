//! Retrieval fusion algorithms for Engram.
//!
//! This crate owns deterministic retrieval ports and collaborators that operate
//! on already-produced candidate results. It does not call stores, embedding
//! providers, policy engines, graph databases, or model rerankers.

mod artifacts;
pub mod bound_filter;
pub mod code_eligibility;
mod composer;
mod config;
mod graph_cache;
mod outcomes;
mod ports;
mod predict;
mod reciprocal;
mod router;
mod tokens;
mod vector_index;
mod weighted;

pub use artifacts::{CODE_ARTIFACT_KINDS, is_code_artifact, suppresses_code_artifacts};
pub use bound_filter::{
    BoundBinding, BoundCodeEligibility, BoundCodeView, CodeEligibilityResolver, DurableLineage,
    RecordEligibility, binding_required, resolve_view,
};
pub use code_eligibility::{
    CodeCandidateLane, CodeEligibilityToken, CodeLaneState, authorize_before_selection,
};
pub use composer::{RetrievalCompositionInput, compose_context};
pub use config::{
    KNOWN_LANE_TAGS, RecallFusionConfig, ReciprocalFusionConfig, RerankConfig, TrustWeights,
    WeightedFusionConfig, unknown_lane_keys,
};
pub use graph_cache::{GraphCache, GraphSnapshot, InMemoryGraphCache, scope_key};
pub use outcomes::OutcomeLedger;
pub use ports::{ContextComposer, RetrievalFusion, RetrievalIndex, RetrievalReranker};
pub use predict::{AgentState, PredictiveRetriever, RecentActivityPredictor, RetrievalHints};
pub use reciprocal::{DEFAULT_RRF_K, ReciprocalRankFusion};
pub use router::{RetrievalRoute, RetrievalRouteMode, RetrievalRouter, RoutedRetrieval};
pub use tokens::query_tokens;
pub use vector_index::VectorIndex;
pub use weighted::WeightedRetrievalFusion;
