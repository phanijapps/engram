# Plan: Knowledge-graph identity and consolidation

- **Spec:** [`spec.md`](spec.md)
- **Status:** Drafting

> **Strategy — contract-first, then pure-core, then SQLite, then surface.**
> Settle the identity + merge domain contract first (E0), implement the pure
> normalization + merge logic behind tests (E1), add the SQLite atomic identity
> layer (E2–E3), transactional consolidation (E4), wire the facade + transports
> (E5), and gate with cross-adapter fixtures (E6). Each task ships behind a
> feature flag or capability gate until surface parity is complete.

## Constraints
- RFC-0014: identity is a storage-neutral port contract; SQLite indexes/tables
  are adapter-private.
- ADR-0022: no engine types in `engram-domain` or the knowledge port crates.
- AGENTS.md: surface parity — Rust facade + N-API + CapabilityReport agree.
- Existing `put_entity` / `put_relationship` behavior is unchanged (additive only).
- Focused identity port, not `KnowledgeRepository` extension.

## Tasks

### E0: Settle identity and compatibility contracts
**Depends on:** none · **Mode:** goal-based (contracts green)
- Define `EntityIdentityMode` (`IdOnly` | `StableKey(String)` |
  `ScopedKindAndNormalizedName`), `EntityWriteRequest`, `EntityWriteOutcome`
  (`Created` | `Matched` | `Merged`), `EntityMergePolicy`, `EntityMergeConflict`,
  `EntityMergeRequest`, `EntityMergeResult` in `core/domain/src/knowledge.rs`.
- Define `RelationshipIdentityKey` (scope + graph + canonical subject + object +
  predicate) and `RelationshipWriteOutcome`.
- Define a focused `EntityIdentityRepository` port in `core/knowledge/src/identity.rs`
  with: `resolve_or_put_entity`, `resolve_or_put_relationship`,
  `discover_collisions`, `consolidate_entities`.
- Classify the contract additions as compatible (additive types + new port;
  no rename/removal of existing types).
- **Done when:** domain types serde round-trip; port compiles; generated
  contracts reproducible; `cargo check --workspace` green.

### E1: Add pure normalization and merge behavior
**Depends on:** E0 · **Mode:** TDD
- Implement deterministic versioned normalization (Unicode NFC, case fold,
  whitespace collapse, stable separators) in `core/knowledge/src/identity.rs`.
- Implement pure merge functions for aliases, `source_refs`, `concept_refs`,
  `ontology_class_refs`, provenance, temporal fields, metadata. Surface
  conflicts via `EntityMergeConflict`.
- Compute exact relationship identity keys (scope + graph + endpoints + canonical
  predicate). Provide a canonicalization hook the caller can supply.
- **Tests:** case folding, Unicode NFC, whitespace, alias matching, stable keys,
  scope/kind/graph separation, deterministic ordering, conflict detection,
  idempotent merge, exact-key computation for distinct predicates/directions.
- **Done when:** `cargo test -p engram-knowledge` green; normalization is pure
  (no SQL, no I/O).

### E2: Implement atomic SQLite entity identity
**Depends on:** E1 · **Mode:** goal-based (SQLite tests green)
- Add indexed identity columns or a dedicated `entity_identity` table in
  `adapters/sqlite/src/knowledge/`. Backfill identity material for existing
  rows without auto-selecting winners (report collisions).
- Implement `resolve_or_put_entity` transactionally: compute the identity key,
  look up by uniqueness constraint, insert/update/merge inside one transaction,
  return `Created`/`Matched`/`Merged`. Concurrent writers converge via
  `ON CONFLICT` retry or equivalent.
- **Tests:** fresh create, repeated match, case variant convergence, stable-key
  rename, separate scope/graph/kind, concurrent writers, existing-collision
  report, rollback, reopen.
- **Done when:** `cargo test -p engram-store-sqlite` identity tests green.

### E3: Implement atomic exact relationship identity
**Depends on:** E2 · **Mode:** goal-based (SQLite tests green)
- Implement `resolve_or_put_relationship` transactionally with the exact
  canonical key. Merge evidence, provenance, confidence, temporal fields
  deterministically when an exact match is found.
- Preserve different predicates and opposite directions unless a caller-supplied
  canonicalization hook maps them.
- **Tests:** repeat edge, canonical predicate spelling, caller inverse map,
  distinct predicates, opposite directions, concurrent writes, idempotency.
- **Done when:** relationship identity tests green.

### E4: Implement transactional entity consolidation
**Depends on:** E2, E3 · **Mode:** goal-based (integration tests green)
- Add dry-run `discover_collisions(policy) -> Vec<CollisionGroup>` and
  transactional `consolidate_entities(request) -> EntityMergeResult`.
- Consolidation: verify canonical + duplicate IDs against scope, merge entity
  data, redirect inbound/outbound relationship endpoints, handle self-loops,
  coalesce exact relationships, preserve evidence/provenance, enforce scope,
  delete or tombstone losing records per policy, return remaps + counts +
  conflicts + audit identifier, roll back as a unit, remain idempotent.
- **Tests:** inbound/outbound/self-loop redirection, relationship coalescing,
  evidence/provenance preservation, conflicting metadata, cross-scope rejection,
  missing IDs, rollback, dry run, repeated apply, reopen integrity.
- **Done when:** consolidation integration tests green.

### E5: Wire facade, capability, and transport surfaces
**Depends on:** E2–E4 · **Mode:** goal-based (parity green)
- Expose typed identity + consolidation handles on `EngramProvider` (or a
  sub-handle) in `core/integration/`. Add a `CapabilityReport` entry.
- Add equivalent N-API operations in `bindings/node/` with stable typed error
  mapping. Update generated contracts in `packages/contracts/`.
- **Tests:** capability/handle agreement, unsupported-backend behavior, Rust
  and TypeScript parity, generated contracts.
- **Done when:** `cargo test -p engram-integration` + `pnpm run typecheck` green.

### E6: Add cross-adapter conformance fixtures
**Depends on:** E2–E5 · **Mode:** goal-based (fixtures green)
- Add reusable fixtures in `adapters/integration/src/fixtures/` covering entity
  identity, exact relationship identity, concurrent convergence (where supported),
  dry-run discovery, consolidation integrity, and unchanged ID-only puts.
- Run fixtures against the SQLite adapter + the conformance stub.
- **Done when:** `cargo test -p engram-conformance` green; fixtures are
  portable for future adapters.

## Rollout
- E0–E1 land first (pure contracts + logic, no storage change).
- E2–E4 land behind a capability gate (SQLite identity + consolidation; existing
  callers unaffected).
- E5–E6 gate the "shipped" status (surface parity + fixtures).

## Changelog
- 2026-07-21: drafted from RFC-0014. Shape `mixed`. All six decisions (D1–D6)
  in one spec. Focused `EntityIdentityRepository` port per RFC OQ1 + user
  confirmation. E0–E6 task decomposition follows the RFC's implementation plan.
