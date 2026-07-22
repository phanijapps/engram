# Plan: Surreal identity cell

- **Spec:** [`spec.md`](spec.md)
- **Status:** Drafting

> **Strategy — mirror `SqlIdentityStore` with SURQL primitives.** The port
> (`EntityIdentityRepository`) and pure functions (`normalize_name`,
> `compute_identity_key`, `compute_relationship_key`, `merge_entities`) are
> already shipped in `core/knowledge/src/identity.rs`. This plan implements only
> the SURQL adapter cell + wires it into `bootstrap_surreal`.

## Reference implementation to mirror

`adapters/sqlite/src/knowledge/identity.rs` (`SqlIdentityStore`) is the
reference. The Surreal cell mirrors its logic 1:1 but replaces rusqlite with
SURQL via the `surrealdb` crate's async query API:

| SQLite | SurrealDB (SURQL) |
| --- | --- |
| `CREATE UNIQUE INDEX … ON knowledge_entities(identity_key)` | `DEFINE INDEX … ON knowledge_entities FIELDS identity_key UNIQUE` |
| `SELECT record_json FROM knowledge_entities WHERE identity_key = ?1` | `SELECT data FROM type::thing('knowledge_entities', $id) WHERE identity_key = $key` or `SELECT data FROM knowledge_entities WHERE identity_key = $key` |
| `INSERT INTO knowledge_entities … ON CONFLICT(id) DO UPDATE` | `UPSERT type::thing('knowledge_entities', $id) SET data = $record, identity_key = $key` |
| `conn.transaction()` + `tx.execute()` | `BEGIN TRANSACTION; …; COMMIT;` via `db.query("BEGIN TRANSACTION")` … `db.query("COMMIT")` |
| `UPDATE knowledge_relationships SET subject_id = ?1 WHERE subject_id = ?2` | `UPDATE knowledge_relationships SET subject_id = $canonical WHERE subject_id = $duplicate` |
| `DELETE FROM knowledge_entities WHERE id = ?1` | `DELETE type::thing('knowledge_entities', $id)` |

The Surreal adapter stores records as `data` fields (the data-wrapper pattern
already used by `SurrealKnowledgeStore`), so the identity module follows the
same `SELECT data FROM …` / `UPSERT … SET data = $record` shape.

## Constraints
- Reuse `core/knowledge/src/identity.rs` pure functions — no reimplementation.
- ADR-0022: SURQL is adapter-private; no engine types in domain/knowledge crates.
- Same `SurrealConnection` (lazy-open `OnceCell`) shared with all Surreal cells.
- The identity conformance fixture (`adapters/integration/src/fixtures/identity.rs`)
  from E6 must pass against the Surreal provider.

## Tasks

### S0: Scaffold SurrealIdentityStore + SURQL indexes
**Depends on:** knowledge-graph-identity E0–E6 (shipped) · **Mode:** goal-based (compiles)
- Create `adapters/surreal/src/identity.rs`:
  - `pub struct SurrealIdentityStore { conn: Arc<SurrealConnection> }`
  - `impl SurrealIdentityStore { pub fn new(conn: Arc<SurrealConnection>) -> Self }`
  - `#[async_trait] impl EntityIdentityRepository for SurrealIdentityStore` — all
    four methods as `todo!()`.
- In the constructor (or a separate `ensure_indexes` called lazily on first use):
  ```sql
  DEFINE INDEX IF NOT EXISTS idx_entity_identity
    ON knowledge_entities FIELDS identity_key UNIQUE;
  DEFINE INDEX IF NOT EXISTS idx_relationship_exact
    ON knowledge_relationships FIELDS relationship_key UNIQUE;
  ```
  (Mirror the `DEFINE INDEX vec_idx … MTREE` idempotent pattern from
  `adapters/surreal/src/vector.rs`.)
- Export from `adapters/surreal/src/lib.rs`:
  `pub mod identity; pub use identity::SurrealIdentityStore;`
- **Done when:** `cargo check -p engram-store-surreal --features surreal` green.

### S1: Implement resolve_or_put_entity
**Depends on:** S0 · **Mode:** goal-based (`#[tokio::test]` green)
- Compute the identity key using `compute_identity_key(&request.entity, &request.identity)`
  from `engram_knowledge::identity`.
- If `None` (IdOnly): UPSERT the entity with `identity_key = NONE` → `Created`.
- If `Some(key)`:
  - Query: `SELECT data FROM knowledge_entities WHERE identity_key = $key LIMIT 1`
  - If found → `Matched` (or `Merged` if `merge_entities` produces changed fields).
  - If not found → UPSERT with `identity_key = $key` → `Created`.
- Use the data-wrapper pattern: `UPSERT type::thing('knowledge_entities', $id)
  SET data = $record, identity_key = $key` where `$record` is the serialized
  `KnowledgeEntity` JSON. Deserialize via `DataWrapper<T>` (from `util.rs`).
- Concurrent convergence: `DEFINE INDEX … UNIQUE` + UPSERT retries on conflict.
- **Tests** (in `core/integration/src/surreal/bootstrap.rs`):
  fresh create, repeated match, case variant (`FastIndex` → `fastindex`),
  stable-key rename, separate scope/graph/kind, IdOnly compatibility.
- **Done when:** entity identity `#[tokio::test]`s pass.

### S2: Implement resolve_or_put_relationship
**Depends on:** S1 · **Mode:** goal-based (tests green)
- Compute the exact key using `compute_relationship_key(&relationship)`.
- Query: `SELECT data FROM knowledge_relationships WHERE relationship_key = $key LIMIT 1`
- If found → merge evidence/provenance/confidence into the survivor (reuse the
  merge logic from `SqlIdentityStore::resolve_or_put_relationship`).
- If not found → UPSERT with `relationship_key = $key`.
- **Tests:** repeat edge, different predicate, opposite direction, concurrent writes.
- **Done when:** relationship identity tests pass.

### S3: Implement discover_collisions + consolidate_entities
**Depends on:** S1, S2 · **Mode:** goal-based (integration tests green)
- `discover_collisions`:
  ```sql
  SELECT identity_key, array::group(id) AS ids
  FROM knowledge_entities
  WHERE identity_key IS NOT NONE AND tenant = $tenant
  GROUP BY identity_key
  ```
  Return `Vec<CollisionGroup>`.
- `consolidate_entities`:
  ```sql
  BEGIN TRANSACTION;
  -- merge entity data (reuse merge_entities)
  -- redirect: UPDATE knowledge_relationships SET subject_id = $canonical WHERE subject_id = $duplicate
  -- coalesce: DELETE duplicate relationships by relationship_key
  -- DELETE type::thing('knowledge_entities', $duplicate)
  COMMIT;
  ```
  Wrap in `BEGIN TRANSACTION … COMMIT`; on error, `CANCEL TRANSACTION` (rollback).
- **Tests:** inbound/outbound/self-loop redirection, relationship coalescing,
  evidence preservation, cross-scope rejection, missing IDs, rollback, dry run,
  idempotent repeated apply.
- **Done when:** consolidation tests pass.

### S4: Wire bootstrap_surreal + CapabilityReport
**Depends on:** S1–S3 · **Mode:** goal-based (integration green)
- In `core/integration/src/surreal/bootstrap.rs`:
  ```rust
  let identity_store = SurrealIdentityStore::new(conn.clone());
  // …
  .identity(Arc::new(identity_store))
  ```
  + `.identity(CapabilityState::Supported)` in the capability report builder.
- Add `#[tokio::test]` confirming identity is `Supported` + handle wired.
- **Done when:** `cargo test -p engram-integration --features surreal` green.

### S5: Cross-adapter fixture passes against Surreal
**Depends on:** S4 · **Mode:** goal-based (fixture green)
- Run `run_identity_fixture()` from `adapters/integration/src/fixtures/identity.rs`
  against the Surreal provider (parameterize the fixture to accept either
  `SqlIdentityStore` or `SurrealIdentityStore` behind the trait).
- **Done when:** same fixture that passes SQLite also passes Surreal.

## Rollout
- S0–S3 land after the parent spec's E0–E1 (already shipped).
- S4 gates the Surreal capability as `Supported`.
- S5 gates cross-engine parity (fully shipped).

## Changelog
- 2026-07-21: initial draft (pre-implementation).
- 2026-07-22: rewritten to reflect shipped E0–E6 (port + pure logic + SQLite
  reference + facade wiring all exist). The Surreal cell mirrors
  `SqlIdentityStore` with SURQL. Added SQLite↔SurrealQL mapping table.
