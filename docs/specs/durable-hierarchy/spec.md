# Spec: Durable hierarchy backend (SQLite)

- **Status:** Shipped
- **Mode:** light — pattern-proven adapter (mirrors `engram-store-belief-sqlite` / `engram-store-knowledge-sqlite`); the `path_for` traversal is replicated from the in-memory adapter. Single new crate; no contract change.
- **Constrained by:** [ADR-0005](../../adr/0005-storage-adapter-semantics.md) (storage adapter semantics — idempotent upsert, scope isolation, lossless contract JSON).
- **Gap:** `docs/arch_divergence.md` "Hierarchy construction/navigation separation" — no durable hierarchy backend exists (in-memory only).

## Objective

A durable SQLite `HierarchyRepository` adapter so hierarchy nodes/relations persist beyond process lifetime, mirroring the existing SQLite adapter pattern. Closes the "no durable hierarchy backend" gap and unblocks the construction/navigation split (the durable backend lets the build/navigation concern move out of the in-memory fixture).

## Acceptance Criteria

- [x] **AC1 — crate.** `engram-store-hierarchy-sqlite` exists at `adapters/hierarchy/sqlite`, implements `HierarchyRepository` (`put_node`, `put_relation`, `path_for`), is file-backed (`open_file` + `open_in_memory`), and is a workspace member. Depends on `engram-hierarchy` (not `engram-core`).
- [x] **AC2 — durable + lossless.** `put_node`/`put_relation` upsert into `hierarchy_nodes`/`hierarchy_relations` as contract JSON with scope columns; re-`put` updates (idempotent). Round-trips preserve every field.
- [x] **AC3 — path_for matches in-memory semantics.** Seeds by node id or `source_target_id` (within `max_layer`), walks `parent_id` chains to the root, computes the LCA across chains, and returns in-scope relations whose endpoints are in the path. Scope-isolated — a cross-tenant node never appears.
- [x] **AC4 — gates + regression.** `cargo fmt`/`clippy (--workspace --all-targets -D warnings)`/`test` + `pnpm typecheck` green; tests cover round-trip/upsert, parent-chain navigation, `max_layer` cap, LCA, relation inclusion, and cross-tenant isolation.

## Boundaries

- `adapters/hierarchy/sqlite` only. No contract change (`HierarchyRepository` + domain types already exist). Depends on `engram-hierarchy` + `engram-domain` + `engram-runtime`.
- The `path_for` traversal is replicated (not yet extracted to `engram-hierarchy`); extracting it to share with the in-memory adapter is a follow-up.
- **Scaling (fixture-grade):** `path_for` loads the in-scope graph into memory and filters in Rust (consistent with the other SQLite adapters' reads), and holds the connection lock across the traversal. Pushing tenant predicates into the SQL `WHERE` clause and dropping the lock before the LCA walk are future optimizations if hierarchy volume outgrows fixture scale.

## Testing Strategy

- New tests under `adapters/hierarchy/sqlite/tests/`: round-trip + upsert, 3-node parent-chain navigation with `max_layer` cap, relation inclusion, and cross-tenant isolation. The existing suite is the regression gate.
- Single adversarial pass (user preference).

## Changelog

- 2026-07-01 — spec opened.
