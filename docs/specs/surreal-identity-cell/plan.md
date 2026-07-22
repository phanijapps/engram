# Plan: Surreal identity cell

- **Spec:** [`spec.md`](spec.md)
- **Status:** Drafting

> **Strategy — mirror the SQLite cell with SURQL primitives.** The port
> contract + pure normalization/merge logic are defined by the parent spec
> (`knowledge-graph-identity` E0–E1). This plan implements the adapter cell:
> SURQL unique indexes for identity, UPSERT for atomic resolve, graph-native
> edges for relationship identity, and `BEGIN TRANSACTION … COMMIT` +
> `MERGE` for consolidation.

## Constraints
- Parent spec `knowledge-graph-identity` E0–E1 must land first (port + pure
  logic). This cell reuses `core/knowledge/src/identity.rs` functions — no
  reimplementation.
- ADR-0022: SURQL is adapter-private; no engine types in domain/knowledge crates.
- Same `SurrealConnection` (lazy-open `OnceCell`) shared with all Surreal cells.
- Cross-adapter fixtures from `knowledge-graph-identity` E6 must pass.

## Tasks

### S0: Scaffold SurrealIdentityStore + SURQL identity indexes
**Depends on:** knowledge-graph-identity E0–E1 · **Mode:** goal-based (compiles)
- Create `adapters/surreal/src/identity.rs`: `SurrealIdentityStore` struct
  holding `Arc<SurrealConnection>` + the shared `EmbeddingSpace`.
- Implement `EntityIdentityRepository` trait shell (all methods `todo!()`).
- Add SURQL `DEFINE INDEX … UNIQUE` on the identity key for entity records;
  add a unique index on the relationship edge key
  (`scope + graph + subject + predicate + object`).
- Export from `lib.rs`: `pub mod identity; pub use identity::SurrealIdentityStore;`
- **Done when:** `cargo check -p engram-store-surreal --features surreal` green.

### S1: Implement resolve_or_put_entity (SURQL UPSERT + UNIQUE)
**Depends on:** S0 · **Mode:** goal-based (Surreal tests green)
- Compute the normalized identity key using the pure functions from
  `core/knowledge/src/identity.rs` (E1).
- Query the entity table by the identity key (SURQL `SELECT … WHERE
  identity_key = $key`); if found → `Matched`; if not → insert via
  `UPSERT type::thing('entity', $id) SET …` → `Created`; if the caller's
  merge policy permits merging aliases/refs into an existing match → `Merged`.
- Concurrent convergence: SurrealKV ACID + `DEFINE INDEX … UNIQUE` → duplicate
  insert fails → retry resolves to the existing record.
- **Tests** (`#[tokio::test]` in `core/integration/src/surreal/bootstrap.rs`):
  fresh create, repeated match, case variant convergence, stable-key rename,
  separate scope/graph/kind, concurrent writers, existing-collision report.
- **Done when:** entity identity tests pass against embedded SurrealKV.

### S2: Implement resolve_or_put_relationship (exact edge key)
**Depends on:** S1 · **Mode:** goal-based (Surreal tests green)
- Compute the exact canonical relationship key (scope + graph + canonical
  subject + canonical object + caller-canonicalized predicate).
- Query the relationship edge by the exact key (SURQL `SELECT … WHERE
  rel_key = $key`); if found → merge evidence/provenance/confidence; if not →
  insert. Use the same UPSERT pattern as S1.
- Preserve different predicates and opposite directions unless the caller
  canonicalization hook maps them.
- **Tests:** repeat edge, canonical predicate spelling, caller inverse map,
  distinct predicates, opposite directions, concurrent writes, idempotency.
- **Done when:** relationship identity tests pass.

### S3: Implement transactional consolidation
**Depends on:** S1, S2 · **Mode:** goal-based (integration tests green)
- `discover_collisions(policy)` → SURQL `SELECT` grouping by identity key,
  returning collision groups (duplicate IDs under the policy). No mutation.
- `consolidate_entities(request)` → inside `BEGIN TRANSACTION … COMMIT`:
  1. Verify canonical + duplicate IDs against scope.
  2. Merge entity data (reuse E1 pure merge).
  3. Redirect relationship endpoints: `UPDATE relationship SET subject = $canonical WHERE subject = $duplicate` (+ object) via SURQL.
  4. Handle self-loops (skip if subject == object post-redirect).
  5. Coalesce exact relationships made identical by redirection (SURQL `DELETE`
     duplicates after merging their evidence/provenance into the survivor).
  6. Delete or tombstone losing entity records per policy.
  7. Return remaps, counts, conflicts, audit identifier.
  Roll back on any error; idempotent on repeat.
- **Tests:** inbound/outbound/self-loop redirection, relationship coalescing,
  evidence/provenance preservation, conflicting metadata, cross-scope rejection,
  missing IDs, rollback, dry run, repeated apply, reopen integrity.
- **Done when:** consolidation integration tests pass.

### S4: Wire bootstrap_surreal + CapabilityReport
**Depends on:** S1–S3 · **Mode:** goal-based (integration green)
- In `core/integration/src/surreal/bootstrap.rs`: clone the shared
  `SurrealConnection` into `SurrealIdentityStore`, add it to
  `EngramProviderBuilder`, and flip the identity capability to `Supported`
  in the `CapabilityReport`.
- Add `#[tokio::test]` in bootstrap.rs confirming identity is `Supported` +
  the handle is wired (same pattern as the existing 6 cell tests).
- **Done when:** `cargo test -p engram-integration --features surreal` green;
  capability report shows identity `Supported`.

### S5: Cross-adapter fixtures pass against Surreal
**Depends on:** S4 + knowledge-graph-identity E6 · **Mode:** goal-based (fixtures green)
- Run the cross-adapter identity + consolidation fixtures from
  `adapters/integration/src/fixtures/` (E6 of the parent spec) against the
  Surreal provider.
- **Done when:** the same fixtures that pass against SQLite also pass against
  Surreal (entity identity, exact relationship identity, concurrent convergence,
  dry-run discovery, consolidation integrity, unchanged ID-only puts).

## Rollout
- S0–S3 land after the parent spec's E0–E1 (port + pure logic) ship.
- S4 gates the Surreal capability as `Supported`.
- S5 gates cross-engine parity (the "fully shipped" status).

## Changelog
- 2026-07-21: drafted from RFC-0014 + the `knowledge-graph-identity` parent
  spec. Shape `integration`. SURQL-native: UNIQUE indexes, UPSERT, BEGIN
  TRANSACTION, MERGE. User will switch to Surreal later for testing.
