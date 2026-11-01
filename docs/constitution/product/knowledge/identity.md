# What engram IS

<!-- prerequisite: none -->

## Knowledge — what engram IS

Engram is a contract-first, local-first **agentic memory and knowledge engine**.
Its customer is an agent/host that must remember across sessions, retrieve
grounded context, revise evidence-backed beliefs, and understand code without
confusing observations with truth.

**The four substrates (distinct — never merged):**
1. **Memory** — governed experiential records: write/recall/forget with scope,
   policy, lifecycle events, tombstones (not SQL deletes)
2. **Knowledge** — source/document/chunk-grounded graph: entities, relations,
   taxonomy + ontology packs (FIBO, schema.org, +33); source-bound supersession
   retires stale assertions
3. **Belief** — derived, revisable stances with valid-time ≠ record-time;
   contradictions are reviewable signals, never silent overwrites
4. **Code intelligence** — generation-bound indexes (engram-code tree-sitter):
   symbols, typed edges, blast radius; branch-bound per project

Rust owns all deterministic behavior behind ports; TypeScript (N-API + SDK)
owns ergonomics; one unified MCP server (`mcp/engram-mcp`) is the agent
surface; `packages/launcher` ships the installable agent bundle. Retrieval
fuses all substrates into a bounded, explainable ContextPayload.

**Engram is NOT:** an LLM host, a vector database, an app server, or a hosted
control plane. Vector search is a retrieval mechanism, not the identity.
Full product document absorbed below.

## Source: product/engram.md (absorbed)

# Engram — Product Overview

> The single consolidated view of the engram product **today**, synthesized
> from the shipped implementation (the historical feature specs were consolidated
> here). The code map is the
> [architecture overview](../architecture/overview.md); direction is the
> [roadmap](roadmap.md). This is a *living* doc — drift is a bug.

**Status:** pre-1.0. SQLite is the reference backend and is production-shaped
across the core capabilities. A second engine (SurrealDB) and the agent-host
adapter are in flight. (Legend: ✅ shipped · 🚧 draft / in-progress)

