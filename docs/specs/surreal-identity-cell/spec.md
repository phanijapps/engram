# Spec: Surreal identity cell

- **Status:** Draft
- **Owner:** phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** RFC-0014, ADR-0022 (engine neutrality), [`knowledge-graph-identity`](../knowledge-graph-identity/spec.md) (parent spec)
- **Brief:** none
- **Contract:** none — implements the already-shipped `EntityIdentityRepository` port; SURQL is adapter-private.
- **Shape:** integration

> **Spec contract:** this document defines what "done" means for the SurrealDB
> identity adapter. The implementing PR must match this spec, or update it.

## Context (what already shipped)

The parent spec (`knowledge-graph-identity`) shipped E0–E6 against SQLite:

- **Domain types** in `core/domain/src/knowledge.rs` — `EntityIdentityMode`,
  `EntityWriteRequest`, `EntityWriteOutcome`, `EntityMergePolicy`,
  `EntityMergeConflict`, `EntityMergeRequest`, `EntityMergeResult`,
  `CollisionGroup`.
- **Port + pure functions** in `core/knowledge/src/identity.rs` —
  `EntityIdentityRepository` trait, `normalize_name()`,
  `compute_identity_key()`, `compute_relationship_key()`, `merge_entities()`.
  These are **engine-neutral and shared** — the Surreal cell reuses them as-is.
- **SQLite reference** in `adapters/sqlite/src/knowledge/identity.rs` —
  `SqlIdentityStore` implements the full port.
- **Facade wiring** — `CapabilityReport.identity` (20th field),
  `EngramProvider.identity()` handle, `EngramProviderBuilder.identity()`.

This spec covers **only the SurrealDB adapter cell** — the SURQL-specific
implementation that sits behind the same port.

## Objective

The `engram-store-surreal` crate provides a `SurrealIdentityStore` that
implements `EntityIdentityRepository` over embedded SurrealKV, using SURQL-native
primitives. It shares one `SurrealConnection` with the existing knowledge cell
and is wired into `bootstrap_surreal`.

Success: the Surreal identity cell passes the same conformance fixture as the
SQLite cell; `CapabilityReport` advertises identity `Supported` when the Surreal
backend is active; existing `put_entity` / `put_relationship` behavior is
unchanged.

## Boundaries

### Always do
- Reuse the pure functions from `core/knowledge/src/identity.rs` — no
  reimplementation of normalization or merge logic in the adapter.
- Share the existing `SurrealConnection` (lazy-open `OnceCell`) with the other
  Surreal cells.
- Use SURQL-native primitives:
  - `DEFINE INDEX … UNIQUE` for entity identity keys.
  - `DEFINE INDEX … UNIQUE` on the relationship exact key.
  - `BEGIN TRANSACTION … COMMIT` for atomic resolve-or-create + consolidation.
  - `UPDATE … MERGE` / `UPDATE … SET` for consolidation endpoint redirection.
- The Surreal cell is async (uses `conn.db().await?` per the existing pattern);
  tests are `#[tokio::test]`.

### Ask first
- Adding a SurrealDB-specific identity feature not in the port contract.
- Changing the normalization version or merge policy in a Surreal-specific way.

### Never do
- Put SURQL or `surrealdb` crate types in `engram-domain` or `engram-knowledge`
  (engine neutrality, ADR-0022). [structural]
- Reimplement normalization or merge logic in the adapter — reuse the pure
  functions from `core/knowledge/src/identity.rs`. [structural]
- Break the existing `SurrealKnowledgeStore::put_entity` /
  `put_relationship` behavior — identity operations are additive. [structural]

## Testing Strategy

- **Goal-based check** — SURQL identity indexes: `DEFINE INDEX … UNIQUE` on the
  `knowledge_entities` table for `identity_key`; `DEFINE INDEX … UNIQUE` on
  `knowledge_relationships` for `relationship_key`. Verified by `#[tokio::test]`
  in `core/integration/src/surreal/bootstrap.rs`.
- **Integration** — the identity conformance fixture from `knowledge-graph-identity`
  E6, run against the Surreal provider (same fixture as SQLite).
- **Goal-based check** — `CapabilityReport` shows identity `Supported` when
  `bootstrap_surreal` wires the cell.

## Acceptance Criteria

- [ ] `SurrealIdentityStore` implements `EntityIdentityRepository` with
  `resolve_or_put_entity`, `resolve_or_put_relationship`,
  `discover_collisions`, and `consolidate_entities`.
- [ ] Case variants under the same scope, graph, kind, and normalized-name
  policy converge on one canonical entity via `DEFINE INDEX … UNIQUE` + UPSERT.
- [ ] Concurrent `resolve_or_put_entity` requests converge on one canonical ID.
- [ ] `resolve_or_put_relationship` enforces the exact canonical key via a SURQL
  unique index on the relationship edge.
- [ ] Dry-run collision discovery reports duplicate IDs without mutation.
- [ ] Entity consolidation redirects endpoints via `UPDATE … SET` and leaves no
  dangling references.
- [ ] Consolidation is transactional (`BEGIN TRANSACTION … COMMIT`) and
  idempotent.
- [ ] Existing `SurrealKnowledgeStore::put_entity` / `put_relationship` calls
  retain their current behavior.
- [ ] The Surreal identity cell passes the same conformance fixture as SQLite.
- [ ] `CapabilityReport` advertises identity `Supported` when Surreal is active.

## Assumptions

- Technical: `engram-store-surreal` has `knowledge.rs` implementing
  `KnowledgeRepository` + `KnowledgeGraphRepository`.
  (source: adapters/surreal/src/knowledge.rs)
- Technical: `EntityIdentityRepository` port + pure functions are already shipped
  in `core/knowledge/src/identity.rs` — the Surreal cell reuses them.
  (source: commit b3254c2)
- Technical: `SqlIdentityStore` in `adapters/sqlite/src/knowledge/identity.rs` is
  the reference implementation — the Surreal cell mirrors its logic with SURQL.
  (source: commit da642f9)
- Technical: `DEFINE INDEX` is already used in the adapter (MTREE for vectors);
  `UNIQUE` index is standard SURQL.
  (source: adapters/surreal/src/vector.rs)
- Technical: The Surreal adapter uses a lazy-open connection pattern
  (`SurrealConnection` with `OnceCell`); all methods are async.
  (source: adapters/surreal/src/connection.rs)
- Technical: One crate per backend (ADR-0022) — the identity cell lives in
  `engram-store-surreal` as `identity.rs`.
  (source: adapters/surreal/src/lib.rs)
- Process: User will switch to Surreal for testing later.
  (source: user confirmation 2026-07-21)
