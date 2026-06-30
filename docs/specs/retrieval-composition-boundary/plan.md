# Plan: Retrieval Composition Boundary

- **Spec:** [`spec.md`](spec.md)
- **Status:** Done

> **Plan contract:** this is the implementation strategy. Unlike the spec, this
> document is allowed to change as you learn. When it changes substantially
> (a different approach, not just a re-ordering), note why in the changelog at
> the bottom.

## Approach

Move the canonical retrieval traits and composition service into
`engram-retrieval`, then migrate adapters to depend on that boundary instead of
using `engram-core` as the owner of retrieval behavior. Keep the first slice
deterministic: in-memory memory and knowledge fixtures provide test candidates,
vector indexes remain optional sources, and accepted v1 retrieval payloads stay
unchanged. Defer durable graph traversal and belief/hierarchy crate splits to
follow-up specs unless the code requires narrow compatibility shims.

## Constraints

- `docs/domain-data-model.md`: memory and knowledge remain distinct but
  composable; every retrieval path enforces policy.
- `docs/rfcs/0002-knowledge-source-extension.md`: retrieval routes across memory
  and knowledge without merging storage concerns.
- `docs/arch_divergence.md`: retrieval composition moving out of
  `engram-store-memory` is the active closure condition.
- `AGENTS.md`: crate roots stay facades, adapters stay behind ports, and
  in-memory memory/knowledge fixtures remain test-oriented.

## Construction tests

**Integration tests:** `cargo test -p engram-retrieval`, `cargo test -p
engram-store-memory --test retrieve_context`, `cargo test -p
engram-store-memory --test knowledge_retrieval`, `cargo test -p
engram-store-memory --test retrieval_indexes`, `cargo test -p
engram-store-vector --test retrieval_index`, and accepted v1 fixture tests.

**Manual verification:** none.

## Design (LLD)

### Design decisions

- `engram-retrieval` owns the canonical retrieval traits because retrieval
  composition changes for ranking and orchestration reasons, not for memory or
  knowledge persistence reasons. Traces to: AC1, AC2.
- `engram-core` keeps compatibility re-exports only for the migration window.
  Traces to: AC1, AC5.
- Store adapters produce candidates or implement storage ports; they do not own
  cross-source composition. Traces to: AC5, AC6.

### Data & schema

No portable v1 schema change is part of this slice. Existing
`RetrievalRequest`, `ContextPayload`, `RetrievalResult`,
`RetrievalExplanation`, `FusionTrace`, `RetrievalSourceFailure`, and
`OmittedResult` domain types remain the data contract. Traces to: AC7.

### Interfaces & contracts

- Move `RetrievalIndex` and `RetrievalFusion` canonical definitions from
  `engram-core` into `engram-retrieval`.
- Add a small composition interface in `engram-retrieval` for assembling
  candidate groups, omissions, degraded sources, fusion, and final context
  payloads.
- Re-export retrieval traits from `engram-core` for source compatibility while
  downstream crates migrate. Traces to: AC1, AC2, AC5.

### Component / module decomposition

- `crates/engram-retrieval/src/lib.rs`: facade and public re-exports only.
- `crates/engram-retrieval/src/ports.rs`: retrieval source, index, and fusion
  traits.
- `crates/engram-retrieval/src/composer.rs`: storage-neutral composition from
  candidate groups to `ContextPayload`.
- `crates/engram-retrieval/src/weighted.rs`: weighted fusion implementation.
- `crates/engram-store-memory/src/retrieval.rs`: quick fixture candidate
  extraction only, calling the retrieval composer for shared fan-in and fusion.
- `crates/engram-store-vector/src/retrieval.rs`: vector index implementation
  imports retrieval traits directly from `engram-retrieval`.

### State & control flow

1. A memory service receives a `RetrievalRequest`.
2. Store-local code extracts memory candidates and fixture knowledge candidates
   inside policy and scope boundaries.
3. Optional external indexes return additional candidates or source failures.
4. `engram-retrieval` receives candidate groups, omissions, source failures,
   fusion strategy, and request settings.
5. The composer applies fusion once, truncates once, and returns the final
   `ContextPayload`.

### Behavior & rules

- Candidate producers are responsible for policy and scope checks before
  returning candidates.
- The composer is responsible for shared fusion, final limit enforcement,
  omission merging, source-failure preservation, and deterministic context
  assembly.
- Optional source failures do not erase successful candidates from other
  sources.
- Compatibility re-exports do not make `engram-core` the canonical owner of new
  retrieval behavior.

### Failure, edge cases & resilience

- A failing optional vector or graph index returns a degraded-source entry and
  does not fail the whole retrieval response.
- Empty candidate groups produce an empty context with no synthetic results.
- Duplicate targets across sources collapse through fusion trace behavior.
- Budget truncation reports omitted candidates consistently after shared
  ranking.

### Quality attributes (NFRs)

- Determinism: identical candidate groups and request settings produce stable
  result ordering.
- Modularity: production adapter crates compile against canonical boundary
  crates and do not import test fixture crates.
- Compatibility: accepted v1 retrieval examples still deserialize and execute
  through existing service surfaces.

### Dependencies & integration

- `engram-retrieval` depends on `engram-domain` and `engram-runtime`, not on
  concrete store crates.
- `engram-store-memory` depends on `engram-retrieval` for composition while
  retaining quick fixture extraction.
- `engram-store-vector` depends on `engram-retrieval` for retrieval index
  traits.
- `engram-core` depends on `engram-retrieval` only to re-export migration
  compatibility surfaces.

## Tasks

### T1: Retrieval traits are canonical in `engram-retrieval`

**Depends on:** none

**Touches:** `crates/engram-retrieval/src/*.rs`, `crates/engram-core/src/lib.rs`,
`crates/engram-retrieval/tests/*.rs`

**Tests:**
- `cargo test -p engram-retrieval` verifies weighted fusion still satisfies
  AC1 and AC7.
- `rg -n "pub trait Retrieval(Index|Fusion)" crates/engram-core/src/lib.rs`
  returns no canonical trait definitions and proves AC1 migration.

**Approach:**
- Move `RetrievalIndex` and `RetrievalFusion` trait definitions into
  `crates/engram-retrieval/src/ports.rs`.
- Re-export the traits from `engram-retrieval/src/lib.rs`.
- Re-export the traits from `engram-core/src/lib.rs` for compatibility.
- Update `WeightedRetrievalFusion` to import traits from its own crate.

**Done when:** `engram-retrieval` owns the trait definitions and all existing
fusion tests pass.

### T2: Storage-neutral composer returns accepted context payloads

**Depends on:** T1

**Touches:** `crates/engram-retrieval/src/composer.rs`,
`crates/engram-retrieval/src/lib.rs`, `crates/engram-retrieval/tests/*.rs`

**Tests:**
- TDD tests prove empty candidates, duplicate target fusion, budget omissions,
  and degraded-source preservation for AC2, AC4, and AC7.

**Approach:**
- Add a composer input type for candidate groups, omissions, degraded sources,
  fusion strategy, and request settings.
- Add a composer function or thin service struct that applies fusion once,
  enforces the request limit once, and assembles `ContextPayload`.
- Keep domain payload construction storage-neutral.

**Done when:** composer tests cover the shared fan-in behavior without importing
store crates.

### T3: In-memory memory fixture delegates shared composition

**Depends on:** T2

**Touches:** `crates/engram-store-memory/src/retrieval.rs`,
`crates/engram-store-memory/src/service.rs`,
`crates/engram-store-memory/tests/retrieve_context.rs`,
`crates/engram-store-memory/tests/knowledge_retrieval.rs`,
`crates/engram-store-memory/tests/retrieval_indexes.rs`

**Tests:**
- `cargo test -p engram-store-memory --test retrieve_context` verifies memory
  retrieval compatibility for AC2, AC3, and AC7.
- `cargo test -p engram-store-memory --test knowledge_retrieval` verifies
  quick fixture knowledge retrieval for AC2, AC3, and AC5.
- `cargo test -p engram-store-memory --test retrieval_indexes` verifies
  optional index degradation and truncation for AC4.

**Approach:**
- Keep store-local candidate extraction in focused modules.
- Replace local final context assembly and truncation with the
  `engram-retrieval` composer.
- Keep `engram-store-memory` documented as a quick fixture rather than a
  production composition owner.

**Done when:** in-memory retrieval tests pass and composition behavior lives in
`engram-retrieval`.

### T4: Adapter imports use retrieval boundary directly

**Depends on:** T1-T3

**Touches:** `crates/engram-store-memory/**/*.rs`,
`crates/engram-store-vector/**/*.rs`, `crates/engram-retrieval/**/*.rs`,
`docs/arch_divergence.md`

**Tests:**
- `cargo check --workspace` verifies compile-time compatibility for AC6.
- `rg -n "engram_core::.*Retrieval(Index|Fusion)" crates/engram-store-memory crates/engram-store-vector`
  returns no production imports and verifies AC5 and AC6.

**Approach:**
- Migrate production imports from `engram_core::RetrievalIndex` and
  `engram_core::RetrievalFusion` to `engram_retrieval`.
- Keep tests compatible through either direct retrieval imports or temporary
  core re-exports.
- Update divergence tracking with the new retrieval score and remaining gaps.

**Done when:** production adapter crates compile against the retrieval boundary
directly.

### T5: Full gates and documentation close the slice

**Depends on:** T1-T4

**Touches:** `README.md`, `docs/architecture.md`, `docs/arch_divergence.md`,
`docs/specs/retrieval-composition-boundary/*`

**Tests:**
- `cargo fmt --all --check`
- `cargo check --workspace`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- `pnpm run check`
- `.codex/hooks/check-contracts.sh`
- `.codex/hooks/check-docs.sh`

**Approach:**
- Update docs that describe crate ownership and the remaining architecture
  divergence.
- Mark acceptance criteria only after the code and gates prove them.

**Done when:** all repository gates pass and the spec status is ready to move
from Draft to Implementing or Shipped according to the implementation state.

## Rollout

This is a source-compatible internal Rust boundary migration. `engram-core`
keeps compatibility re-exports during the slice, so existing downstream imports
can move gradually. No database migration, TypeScript package change, or
portable contract migration is part of this rollout.

## Risks

- Moving traits can create circular dependencies if `engram-retrieval` imports
  orchestration behavior from `engram-core`.
- A composer that knows too much about memory or knowledge record internals can
  recreate the same coupling under a new crate name.
- Compatibility re-exports can hide unfinished migrations unless import greps
  are part of the acceptance checks.

## Changelog

- 2026-06-30: initial plan.
- 2026-06-30: moved retrieval traits and context composition into
  `engram-retrieval`, migrated adapter imports, and closed the slice.
