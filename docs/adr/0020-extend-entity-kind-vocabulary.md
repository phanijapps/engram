# ADR-0020: Extend EntityKind vocabulary for code-structural symbols

- **Status:** Accepted
- **Date:** 2026-07-08
- **Decision-makers:** phanijapps
- **Supersedes:** none
- **Related:** RFC-0012 (code-structural graph layer), `docs/codegraph-parity-roadmap.md` (A2), ADR-0019 (bi-temporal entities), domain-data-model §EntityKind

## Decision summary

- **Decision:** Add six `EntityKind` values — `Struct`, `Interface`, `Trait`, `TypeAlias`, `Enum`, `Endpoint` — so code-structural extraction can tag the symbol kinds a memtrace-style code graph needs (the codegraph data layer, C2 taxonomy).
- **Because:** the current `EntityKind` (person / organization / ... / function / class / method / variable / api / ...) lacks struct / interface / trait / type_alias / enum / endpoint, the kinds needed to represent rich code graphs across Rust, TS, Python, etc.
- **Applies to:** the **draft-extension** `EntityKind` enum (not in the frozen v1 schema). Additive enum values, permitted by the contract-freeze policy ("Add new enum values only when consumers are expected to tolerate unknown values"); consumers already tolerate unknown values via wildcard match arms (e.g. `kind_label`).
- **Tradeoff accepted:** `Api` (OpenAPI/contract node, per `contract-first-ingestion`) and `Endpoint` (code-declared HTTP endpoint) are intentionally distinct kinds — a code route and its published contract are separate entities.
- **Revisit if:** a richer symbol taxonomy (memtrace's 41 kinds) or a typed relationship-predicate enum is needed.

## Context

`EntityKind` models what a knowledge-graph entity is. The code-structural graph
extractor emits `Function`, `Class`, `Method`, `Variable` today, but
language symbol tables also contain structs, interfaces, traits, type aliases,
enums, and HTTP endpoints. Without those kinds, the graph collapses them into
`Class`/`Unknown`, losing the structural distinction the codegraph data layer
(C2) and downstream queries (complexity, blast-radius) rely on. RFC-0012 item
A2 / the codegraph-parity roadmap call this out.

## Decision

Add `Struct`, `Interface`, `Trait`, `TypeAlias`, `Enum`, `Endpoint` to
`EntityKind`:

- `snake_case` serialization (`struct`, `interface`, `trait`, `type_alias`,
  `enum`, `endpoint`), matching the enum's existing `rename_all`.
- Additive only — no v1 schema regeneration (`EntityKind` is not in the frozen
  v1 schema); consumers tolerate unknown values via wildcard arms.
- `KnowledgeRelationship.predicate` is already a free-form `String`, so the
  companion edge vocabulary (`overrides`, `annotated_with`) needs **no** enum
  change — callers simply use those predicate strings.

## Consequences

- Code-structural extraction (C2) and the AST extractor can tag these symbol
  kinds; existing `EntityKind` matches keep working (wildcards).
- No v1 breaking change and no v1 schema regeneration.
- Follow-up: wire the AST extractor (`adapters/ingest`) to emit the new kinds;
  optionally a typed predicate vocabulary if string predicates prove error-prone.
