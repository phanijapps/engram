# Plan: Hierarchy Navigation

- **Spec:** [`spec.md`](spec.md)
- **Status:** Done

> **Plan contract:** this is the implementation strategy. Unlike the spec, this
> document is allowed to change as you learn. When it changes substantially
> (a different approach, not just a re-ordering), note why in the changelog at
> the bottom.

## Approach

Extend the in-memory adapter with hierarchy maps and implement
`HierarchyRepository` path navigation over explicit `parent_id` links. This
keeps PHASE10 deterministic and auditable while deferring construction,
clustering, summaries, and retrieval expansion.

Tempted to add `engram-hierarchy` construction algorithms; declining because
navigation over explicit nodes is the first contract. Tempted to integrate
hierarchy into retrieval ranking; declining until fusion and explanation policy
are specified. Tempted to derive hierarchy from embeddings; declining because
PHASE09 only established vector indexing.

## Constraints

- ADR-0003 keeps behavior in Rust crates and infrastructure behind adapters.
- ADR-0004 keeps domain docs and accepted contracts as the source of truth.
- The research requires construction/navigation separation.

## Construction tests

**Integration tests:** in-memory hierarchy repository tests cover node/relation
persistence, path traversal, common ancestor detection, and scope isolation.

**Manual verification:** none.

## Design (LLD)

### Data & schema

Hierarchy nodes and relations are stored in separate in-memory maps. Path
navigation returns existing domain `HierarchyPath` values without inventing a
new contract.

### Interfaces & contracts

`InMemoryMemoryService` implements `HierarchyRepository` for deterministic tests.
Future durable adapters must preserve the same path semantics.

### Component / module decomposition

- `state.rs` owns hierarchy maps.
- `hierarchy.rs` owns repository persistence and path traversal.
- Tests own fixture construction.

### Failure, edge cases & resilience

Unknown seeds return an empty path. Stale or archived nodes are not filtered in
this baseline because lifecycle policy is not yet specified. Scope filtering
happens before path composition.

## Tasks

### T1: In-memory hierarchy repository stores nodes and relations

**Depends on:** none

**Tests:**
- Store and retrieve path data for explicit nodes and relations.

**Approach:**
- Add hierarchy maps to in-memory state.
- Implement `put_node` and `put_relation`.

**Done when:** repository tests can persist hierarchy records.

### T2: Path navigation traverses parent chains inside scope

**Depends on:** T1

**Tests:**
- Single seed returns seed and ancestor nodes.
- Multiple seeds return a common ancestor.
- Cross-workspace path lookup returns no hidden nodes.

**Approach:**
- Implement `path_for` over source target IDs and hierarchy node IDs.
- Keep relation inclusion scoped to returned nodes.

**Done when:** hierarchy path tests pass.

## Rollout

This ships as in-memory adapter behavior and tests. No durable migration,
clustering job, model provider, or retrieval behavior changes ship here.

## Risks

- Parent-chain navigation is intentionally simpler than future hierarchy
  construction.
- Lifecycle filtering for stale/superseded hierarchy versions is deferred.
- Retrieval expansion still needs an explicit explanation contract.

## Changelog

- 2026-06-29: initial plan for in-memory hierarchy navigation baseline.
- 2026-06-29: implemented in-memory hierarchy repository and parent-chain path
  navigation.
