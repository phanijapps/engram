# ADR 0007: N-API binding public-surface extension

## Status

Accepted

## Context

`engram-node` (the Node-API binding at `bindings/node`) currently exposes only
memory operations: `NativeMemoryEngine` with `writeMemoryJson` / `retrieveJson` /
`forgetJson` over `SqlMemoryService`. Knowledge ingestion, the knowledge graph,
taxonomy, and retrieval are reachable from Rust but not from TypeScript.

RFC 0003 (demo program) Slice 1 needs the binding to expose knowledge, taxonomy,
and (in later slices) ingest and retrieval so a browser demo can drive them
through the Node layer. Extending the N-API public surface and the native binding
technology is an *"Ask first"* item under `docs/specs/workspace-responsibility-
layout` and `docs/specs/typescript-native-surface`, so it needs a recorded
decision before Slice 1 lands.

The binding is a JSON transport over Rust behavior; TypeScript owns ergonomics.
That role must not change when the surface grows.

## Decision

Extend `engram-node` with **focused native structs, one per behavior domain**,
alongside the existing `NativeMemoryEngine`:

- `NativeMemoryEngine` — memory write / retrieve / forget (exists, unchanged).
- `NativeKnowledgeEngine` — knowledge graph + taxonomy (Slice 1).
- `NativeIngestEngine` — source ingestion (Slice 2).
- `NativeRetrievalEngine` — fused retrieval (Slice 3).
- `NativeTaxonomyEngine` — concept scheme / concept maintenance (Slice 1, or
  folded into `NativeKnowledgeEngine` if the surface stays small).

Each struct owns one connection surface and one trait family, constructed and
composed in the Node layer (the demo backend). The JSON round-trip pattern
(`serde_json` in, `serde_json` out, unchanged by Rust) is preserved for every
new method.

A single `NativeEngramEngine` god-struct that owns all domains is explicitly
rejected — it would violate the no-god-object / crate-roots-as-facades rule in
`AGENTS.md` (one struct owning construction, state, and orchestration across five
domains).

## Consequences

- The N-API public surface grows additively. No existing method or payload
  changes; existing `NativeMemoryEngine` consumers are unaffected.
- Each new struct is a focused facade, matching the existing memory precedent and
  the repo's module-responsibility rules. Future surface additions follow the
  same one-struct-per-domain pattern.
- Rust remains a library; HTTP and composition continue to live only in the Node
  layer (ADR 0003).
- The TypeScript `@engram/node` transport widens to mirror the new structs; the
  client stays transport-agnostic and typed from generated contracts.
- `engram-node` gains direct dependencies on the crates whose traits the new
  structs expose (`engram-knowledge`, later `engram-ingest`, `engram-retrieval`,
  and the relevant adapter crates) — these stay behind the binding boundary and
  never enter `engram-domain` or `engram-runtime`.
