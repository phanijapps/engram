# Spec: Surreal identity cell

- **Status:** Draft
- **Owner:** phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** RFC-0014 (canonical knowledge-graph identity), ADR-0022 (engine neutrality), [`knowledge-graph-identity`](../knowledge-graph-identity/spec.md) (parent spec — defines the port + pure logic)
- **Brief:** none
- **Contract:** none — implements the `EntityIdentityRepository` port defined by the parent spec; SURQL is adapter-private.
- **Shape:** integration — adapter cell implementing a neutral port behind SURQL

> **Spec contract:** this document defines what "done" means for the SurrealDB
> identity adapter. The implementing PR must match this spec, or update it.

## Objective

The `engram-store-surreal` crate provides a `SurrealIdentityStore` that
implements the `EntityIdentityRepository` port (defined by
[`knowledge-graph-identity`](../knowledge-graph-identity/spec.md) E0–E1) over
embedded SurrealKV, using SURQL-native semantics for identity resolution,
atomic resolve-or-create, exact relationship identity, and transactional
consolidation. The cell shares one `SurrealConnection` with the existing
knowledge/belief/hierarchy/vector cells and is wired into `bootstrap_surreal`.

Success: the SurrealDB identity cell passes the same cross-adapter conformance
fixtures as the SQLite cell (entity identity, exact relationship identity,
concurrent convergence, dry-run discovery, consolidation integrity, unchanged
ID-only puts); `CapabilityReport` advertises identity `Supported` when the
Surreal backend is active.

## Boundaries

### Always do
- Implement the port contract exactly as defined by `knowledge-graph-identity`
  E0 — no Surreal-specific behavior leaks through the trait surface.
- Use SURQL-native primitives: `DEFINE INDEX … UNIQUE` for identity keys,
  `BEGIN TRANSACTION … COMMIT` for atomic resolve-or-create + consolidation,
  graph-native edge semantics for relationship identity, `MERGE` for
  consolidation endpoint redirection.
- Share the existing `SurrealConnection` (lazy-open `OnceCell`) with the other
  Surreal cells — one connection per backend, cloned via `Arc`.
- Coalesce exact relationships created by endpoint redirection, preserving
  evidence, provenance, and confidence — same merge semantics as the SQLite cell.

### Ask first
- Adding a SurrealDB-specific identity feature not in the port contract (e.g.
  SURQL graph traversal for fuzzy identity matching).
- Changing the normalization version or merge policy in a Surreal-specific way
  (must match the pure logic in `core/knowledge/src/identity.rs`).

### Never do
- Put SURQL, `surrealdb` crate types, or SurrealDB-specific index/table
  definitions in `engram-domain` or `engram-knowledge` (engine neutrality,
  ADR-0022). [structural]
- Reimplement normalization or merge logic in the adapter — reuse the pure
  functions from `core/knowledge/src/identity.rs` (E1 of the parent spec). [structural]
- Break the existing `SurrealKnowledgeStore` `put_entity` / `put_relationship`
  behavior — identity operations are additive. [structural]

## Testing Strategy

- **Goal-based check** — SURQL identity indexes + uniqueness: `DEFINE INDEX …
  UNIQUE` on the identity key; backfill does not auto-select winners; concurrent
  writers converge via UPSERT + unique constraint. Verified by
  `cargo test -p engram-store-surreal --features surreal` (the existing
  `#[tokio::test]` pattern from the Surreal bootstrap tests).
- **Integration** — transactional consolidation over SurrealKV: endpoint
  redirection (inbound, outbound, self-loop), relationship coalescing,
  evidence/provenance preservation, cross-scope rejection, dry-run discovery,
  idempotent repeated apply, rollback-as-a-unit, reopen integrity. Exercised
  by `core/integration/src/surreal/bootstrap.rs` tests (same `#[tokio::test]`
  pattern as the existing 6 Surreal cell tests).
- **Goal-based check** — cross-adapter fixtures from `knowledge-graph-identity`
  E6 pass against the Surreal backend (same fixtures as SQLite). Verified by
  running the identity + consolidation fixtures with the Surreal provider.
- **Goal-based check** — `CapabilityReport` advertises identity `Supported`
  when `bootstrap_surreal` wires the cell; `EngremProvider` exposes the typed
  handle. Verified by `cargo test -p engram-integration --features surreal`.

## Acceptance Criteria

- [ ] `SurrealIdentityStore` implements `EntityIdentityRepository` with
  `resolve_or_put_entity`, `resolve_or_put_relationship`,
  `discover_collisions`, and `consolidate_entities`.
- [ ] Case variants under the same scope, graph, kind, and normalized-name
  policy converge on one canonical entity via SURQL `DEFINE INDEX … UNIQUE` +
  UPSERT.
- [ ] Concurrent `resolve_or_put_entity` requests converge on one canonical ID
  (SurrealKV ACID + unique constraint, not application-level probe-then-insert).
- [ ] `resolve_or_put_relationship` enforces the exact canonical key (scope +
  graph + subject + object + predicate) via a SURQL unique index on the
  relationship edge.
- [ ] Dry-run collision discovery reports duplicate IDs without mutation.
- [ ] Entity consolidation redirects every inbound and outbound relationship
  endpoint via SURQL `UPDATE … MERGE` and leaves no dangling references.
- [ ] Relationships made identical by redirection are coalesced with evidence,
  provenance, and confidence preserved.
- [ ] Consolidation is transactional (`BEGIN TRANSACTION … COMMIT`) and
  idempotent when repeated.
- [ ] Existing `SurrealKnowledgeStore::put_entity` / `put_relationship` calls
  retain their current behavior (compatibility).
- [ ] The Surreal identity cell passes the same cross-adapter conformance
  fixtures as the SQLite cell.
- [ ] `CapabilityReport` advertises identity `Supported` when the Surreal
  backend is active; the `EngramProvider` facade exposes the typed handle.

## Assumptions

- Technical: `engram-store-surreal` has `knowledge.rs` implementing
  `KnowledgeRepository` + `KnowledgeGraphRepository`. (source:
  adapters/surreal/src/knowledge.rs)
- Technical: `EntityIdentityRepository` port does not exist yet — defined by
  `knowledge-graph-identity` spec E0, not implemented. This spec is downstream.
  (source: grep core/knowledge/src/ → NOT FOUND)
- Technical: `DEFINE INDEX` is already used in the adapter (MTREE for vectors);
  `UNIQUE` index is standard SURQL. (source: adapters/surreal/src/vector.rs:77)
- Technical: No explicit transactions in the Surreal adapter today (lazy-open
  `OnceCell` pattern); transactional resolve-or-create + consolidation is new
  SURQL usage. (source: grep adapters/surreal/src/ → no BEGIN/transaction)
- Technical: One crate per backend (ADR-0022) — the identity cell lives in
  `engram-store-surreal` as `identity.rs`, same pattern as the other 5 cells.
  (source: adapters/surreal/src/lib.rs)
- Technical: Engine neutrality + surface parity: identity is a port contract;
  the Surreal cell implements it behind SURQL. (source: ADR-0022, AGENTS.md)
- Process: This spec depends on `knowledge-graph-identity` E0–E1 (port
  definition + pure normalization/merge logic). The user will switch to Surreal
  for testing later. (source: user confirmation 2026-07-21)
