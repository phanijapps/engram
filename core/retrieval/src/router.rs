//! Mode-aware retrieval routing.
//!
//! The router fans a request out to registered `RetrievalIndex` implementations
//! without knowing whether an index is backed by SQL, graph traversal, vector
//! search, lexical matching, or another mechanism. It reports missing requested
//! modes as degraded source failures and leaves ranking/fusion to the existing
//! composition pipeline.

use std::{collections::BTreeSet, sync::Arc};

use engram_domain::*;
use engram_runtime::CoreResult;

use crate::RetrievalIndex;

/// Internal route mode understood by the storage-neutral router.
///
/// `Vector` is not an accepted v1 `RetrievalMode`; it is routed when callers ask
/// for semantic retrieval because vector similarity is currently represented as
/// a semantic retrieval specialization in the v1 contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RetrievalRouteMode {
    Temporal,
    Cue,
    Hierarchical,
    Semantic,
    Graph,
    Keyword,
    Vector,
}

impl RetrievalRouteMode {
    fn requested_mode(&self) -> RetrievalMode {
        match self {
            Self::Temporal => RetrievalMode::Temporal,
            Self::Cue => RetrievalMode::Cue,
            Self::Hierarchical => RetrievalMode::Hierarchical,
            Self::Semantic | Self::Vector => RetrievalMode::Semantic,
            Self::Graph => RetrievalMode::Graph,
            Self::Keyword => RetrievalMode::Keyword,
        }
    }

    fn label(&self) -> &'static str {
        match self {
            Self::Temporal => "temporal",
            Self::Cue => "cue",
            Self::Hierarchical => "hierarchical",
            Self::Semantic => "semantic",
            Self::Graph => "graph",
            Self::Keyword => "keyword",
            Self::Vector => "vector",
        }
    }
}

impl From<RetrievalMode> for RetrievalRouteMode {
    fn from(mode: RetrievalMode) -> Self {
        match mode {
            RetrievalMode::Temporal => Self::Temporal,
            RetrievalMode::Cue => Self::Cue,
            RetrievalMode::Hierarchical => Self::Hierarchical,
            RetrievalMode::Semantic => Self::Semantic,
            RetrievalMode::Graph => Self::Graph,
            RetrievalMode::Keyword => Self::Keyword,
        }
    }
}

/// One registered candidate source for the retrieval router.
pub struct RetrievalRoute {
    source: String,
    mode: RetrievalRouteMode,
    index: Arc<dyn RetrievalIndex>,
}

impl RetrievalRoute {
    /// Creates a route for one mode and candidate source.
    pub fn new(
        source: impl Into<String>,
        mode: RetrievalRouteMode,
        index: Arc<dyn RetrievalIndex>,
    ) -> Self {
        Self {
            source: source.into(),
            mode,
            index,
        }
    }
}

/// Candidate and degraded-source output from one routed request.
#[derive(Debug, Clone, PartialEq)]
pub struct RoutedRetrieval {
    pub candidates: Vec<RetrievalResult>,
    pub source_failures: Vec<RetrievalSourceFailure>,
}

/// Storage-neutral fan-out router over registered retrieval indexes.
pub struct RetrievalRouter {
    routes: Vec<RetrievalRoute>,
}

impl RetrievalRouter {
    /// Creates a router from ordered routes.
    ///
    /// Route order is preserved when collecting candidates. Final ranking still
    /// belongs to `RetrievalFusion` and `compose_context`.
    pub fn new(routes: Vec<RetrievalRoute>) -> Self {
        Self { routes }
    }

    /// Retrieves candidates from routes selected by the request's modes.
    ///
    /// Empty `request.modes` means all registered routes are eligible. When a
    /// caller requests a mode with no route, the router reports a degraded
    /// `unsupported_mode` source failure and continues with the routes it can
    /// execute.
    pub async fn retrieve(&self, request: &RetrievalRequest) -> CoreResult<RoutedRetrieval> {
        let requested = requested_route_modes(request);
        let mut candidates = Vec::new();
        let mut failures = unsupported_mode_failures(request, &requested, &self.routes);

        for route in &self.routes {
            if !route_is_selected(route.mode, &requested) {
                continue;
            }
            match route.index.retrieve_candidates(request).await {
                Ok(mut results) => candidates.append(&mut results),
                Err(error) => {
                    failures.push(RetrievalSourceFailure {
                        source: route.source.clone(),
                        mode: Some(route.mode.requested_mode()),
                        severity: SourceFailureSeverity::Error,
                        reason: "source_error".to_owned(),
                        message: Some(error.to_string()),
                        degraded: true,
                    });
                }
            }
        }

        Ok(RoutedRetrieval {
            candidates,
            source_failures: failures,
        })
    }
}

fn requested_route_modes(request: &RetrievalRequest) -> BTreeSet<RetrievalRouteMode> {
    request
        .modes
        .iter()
        .cloned()
        .map(RetrievalRouteMode::from)
        .collect()
}

fn route_is_selected(mode: RetrievalRouteMode, requested: &BTreeSet<RetrievalRouteMode>) -> bool {
    requested.is_empty()
        || requested.contains(&mode)
        || (mode == RetrievalRouteMode::Vector && requested.contains(&RetrievalRouteMode::Semantic))
}

fn unsupported_mode_failures(
    request: &RetrievalRequest,
    requested: &BTreeSet<RetrievalRouteMode>,
    routes: &[RetrievalRoute],
) -> Vec<RetrievalSourceFailure> {
    if request.modes.is_empty() {
        return Vec::new();
    }
    requested
        .iter()
        .filter(|mode| {
            !routes
                .iter()
                .any(|route| route_is_selected(route.mode, &BTreeSet::from([**mode])))
        })
        .map(|mode| RetrievalSourceFailure {
            source: format!("router.{}", mode.label()),
            mode: Some(mode.requested_mode()),
            severity: SourceFailureSeverity::Warning,
            reason: "unsupported_mode".to_owned(),
            message: Some(format!(
                "no retrieval route registered for {} mode",
                mode.label()
            )),
            degraded: true,
        })
        .collect()
}
