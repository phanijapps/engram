//! Codegraph queries over knowledge-graph call edges.
//!
//! The first on-top codegraph crate (RFC-0012): code-specific queries —
//! [`dead_code`], [`blast_radius`], [`dependency_path`] — over
//! `KnowledgeRelationship` `calls` edges, delegating the graph math to
//! `engram-graph-analytics`. Depends only on `engram-domain` +
//! `engram-graph-analytics`; no storage or infrastructure.

mod queries;

pub use queries::{
    HttpEndpoint, SymbolContext, blast_radius, bridge_symbols, call_communities, call_edges,
    central_symbols, cyclomatic_complexity, dead_code, dependency_path, entity_key, find_endpoints,
    symbol_context,
};
