//! Stable host-facing request/response DTOs.
//!
//! These types give host applications a stable, ergonomic surface that does not
//! require importing `engram-domain` directly. Each DTO converts into the
//! underlying domain type via `From`/`Into`. Hosts that prefer the domain types
//! can use those instead — the DTOs are a convenience layer, not a replacement.

use engram_domain::identity::{Actor, ActorKind, Requester};
use engram_domain::{EntityKind, RetrievalMode, RetrievalRequest, Scope, types::ActorId};

// ---------------------------------------------------------------------------
// Scope helpers
// ---------------------------------------------------------------------------

/// Create a scope with a tenant and optional workspace. Shorthand for hosts
/// that don't need the full `Scope` builder.
pub fn scope(tenant: &str, workspace: Option<&str>) -> Scope {
    Scope {
        tenant: tenant.to_string(),
        subject: None,
        workspace: workspace.map(|w| w.to_string()),
        session: None,
        environment: None,
    }
}

/// Create a workspace-scoped tenant.
pub fn workspace_scope(tenant: &str, workspace: &str) -> Scope {
    scope(tenant, Some(workspace))
}

// ---------------------------------------------------------------------------
// Request DTOs
// ---------------------------------------------------------------------------

/// Memory search request — the most common host operation.
///
/// Converts into a `RetrievalRequest` with sensible defaults for `requester`,
/// `filters`, and `cues`.
#[derive(Debug, Clone)]
pub struct MemorySearch {
    /// The scope to search within (tenant isolation).
    pub scope: Scope,
    /// The search text.
    pub text: String,
    /// Retrieval modes (lexical, vector, hybrid). Empty = all available modes.
    pub modes: Vec<RetrievalMode>,
    /// Maximum results. `None` = backend default.
    pub limit: Option<usize>,
}

impl Default for MemorySearch {
    fn default() -> Self {
        Self {
            scope: Scope {
                tenant: "default".to_string(),
                subject: None,
                workspace: None,
                session: None,
                environment: None,
            },
            text: String::new(),
            modes: Vec::new(),
            limit: None,
        }
    }
}

impl MemorySearch {
    /// Create a search with the given scope and text.
    pub fn new(scope: Scope, text: impl Into<String>) -> Self {
        Self {
            scope,
            text: text.into(),
            ..Default::default()
        }
    }

    /// Set the retrieval modes.
    pub fn modes(mut self, modes: Vec<RetrievalMode>) -> Self {
        self.modes = modes;
        self
    }

    /// Set the result limit.
    pub fn limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }
}

impl From<MemorySearch> for RetrievalRequest {
    fn from(dto: MemorySearch) -> Self {
        RetrievalRequest {
            query: dto.text,
            scope: dto.scope,
            requester: Requester {
                actor: Actor {
                    id: ActorId::from("host"),
                    kind: ActorKind::System,
                    display_name: None,
                    metadata: None,
                },
                roles: Vec::new(),
                permissions: Vec::new(),
                on_behalf_of: None,
            },
            modes: dto.modes,
            filters: None,
            cues: Vec::new(),
            limit: dto.limit.map(|l| l as u32),
            budget: None,
            include_explanations: None,
        }
    }
}

/// Graph query — list entities with optional filters.
#[derive(Debug, Clone)]
pub struct GraphQuery {
    /// The scope to query.
    pub scope: Scope,
    /// Filter by entity kind (Function, Struct, Trait, etc.). `None` = all.
    pub kind_filter: Option<EntityKind>,
    /// Maximum results. `None` = backend default.
    pub limit: Option<usize>,
}

impl GraphQuery {
    /// Create a query for a scope.
    pub fn new(scope: Scope) -> Self {
        Self {
            scope,
            kind_filter: None,
            limit: None,
        }
    }

    /// Filter to a specific entity kind.
    pub fn kind(mut self, kind: EntityKind) -> Self {
        self.kind_filter = Some(kind);
        self
    }

    /// Limit results.
    pub fn limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }
}

/// Unified recall request — one query across all semantic lanes.
#[derive(Debug, Clone)]
pub struct RecallRequest {
    /// The scope to recall within.
    pub scope: Scope,
    /// The search/recall text.
    pub query: String,
    /// Retrieval modes. Empty = all available.
    pub modes: Vec<RetrievalMode>,
}

impl Default for RecallRequest {
    fn default() -> Self {
        Self {
            scope: Scope {
                tenant: "default".to_string(),
                subject: None,
                workspace: None,
                session: None,
                environment: None,
            },
            query: String::new(),
            modes: Vec::new(),
        }
    }
}

impl RecallRequest {
    /// Create a recall request with scope and query.
    pub fn new(scope: Scope, query: impl Into<String>) -> Self {
        Self {
            scope,
            query: query.into(),
            ..Default::default()
        }
    }
}

impl From<RecallRequest> for RetrievalRequest {
    fn from(dto: RecallRequest) -> Self {
        RetrievalRequest {
            query: dto.query,
            scope: dto.scope,
            requester: Requester {
                actor: Actor {
                    id: ActorId::from("host"),
                    kind: ActorKind::System,
                    display_name: None,
                    metadata: None,
                },
                roles: Vec::new(),
                permissions: Vec::new(),
                on_behalf_of: None,
            },
            modes: dto.modes,
            filters: None,
            cues: Vec::new(),
            limit: None,
            budget: None,
            include_explanations: None,
        }
    }
}

// ---------------------------------------------------------------------------
// T5: N-API plan (documented here for traceability)
// ---------------------------------------------------------------------------

/// **T5 plan: N-API `NativeProvider`** (not yet implemented)
///
/// The goal is to expose the provider pattern to TypeScript consumers
/// (engram-viz, @engram/node) so they use typed handle proxies instead of
/// the flat `NativeKnowledgeEngine` JSON transport.
///
/// ## Design: layered approach (not 47 proxy methods)
///
/// Instead of rewriting all 47 `NativeKnowledgeEngine` methods as handle
/// proxies, the `NativeProvider` is a **thin constructor + capability gateway**:
///
/// ```ts
/// const provider = new NativeProvider(configJson);
/// const caps = provider.capabilitiesJson();  // capability report
/// const mem = provider.requireMemoryJson();  // → NativeMemoryApi | error
/// mem.searchJson(requestJson);               // delegates to the underlying engine
/// ```
///
/// **Why not full typed methods?** N-API passes strings (JSON), not Rust trait
/// objects. The "typed" methods are still JSON in/out at the boundary. The win
/// is **discoverability** (TypeScript sees the provider pattern, not a flat
/// 47-method engine) and **capability gating** (require_* returns a typed error
/// instead of empty results).
///
/// ## Implementation sketch
///
/// 1. `NativeProvider` wraps `EngramProvider::open(config_json)` via `block_on`.
/// 2. Exposes `capabilitiesJson()` + 13 `require_*_json()` methods, each
///    returning a handle proxy struct (or throwing N-API error).
/// 3. Each handle proxy (`NativeMemoryApi`, `NativeGraphApi`, etc.) holds an
///    `Arc<dyn Trait>` and exposes the key operations as `*_json()` methods.
/// 4. `NativeKnowledgeEngine` stays for backward compat — it's the flat engine
///    that `engram-viz` and the MCP server use today.
///
/// ## When to implement
///
/// T5 is a **consistency improvement** for engram-viz, not a blocker. The flat
/// JSON transport works. T5 adds typed discoverability + capability gating.
/// Priority: after engram-viz feature stability (T7-T10 are shipped).
///
/// ## Files
///
/// - `bindings/node/src/provider.rs` — `NativeProvider` + handle proxies
/// - `bindings/node/src/lib.rs` — re-export
/// - `packages/node/` — TypeScript types auto-generated from `#[napi]`
pub const T5_PLAN_NOTE: &str = "see module doc above";
