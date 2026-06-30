# Spec: Retrieval Composition Boundary

- **Status:** Draft
- **Owner:** phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** RFC-0002, `docs/domain-data-model.md`, `docs/arch_divergence.md`
- **Brief:** none
- **Contract:** `contracts/v1/schemas/engram-v1.schema.json`, `crates/engram-retrieval` public Rust API
- **Shape:** mixed

> **Spec contract:** this document defines what "done" means. The implementing
> PR must match this spec, or update it. Verification must be derivable from it.

## Objective

Retrieval composition exists as a storage-neutral Rust boundary that combines
memory, knowledge, vector, graph, hierarchy, and belief candidates without
making any store adapter own cross-source orchestration. Application callers see
the accepted `RetrievalRequest` to `ContextPayload` behavior, while memory and
knowledge backends remain independently replaceable.

## Boundaries

The three-tier guard that keeps an implementing agent inside the lines.
*Always do* applies without asking; *Ask first* requires human sign-off before
proceeding; *Never do* is a hard rule, even under time pressure.

### Always do

- Keep `RetrievalRequest`, `ContextPayload`, `RetrievalResult`,
  `RetrievalExplanation`, `FusionTrace`, `RetrievalSourceFailure`, and
  `OmittedResult` semantics aligned with the accepted v1 schema.
- Keep memory, knowledge, vector, graph, hierarchy, and belief candidate
  production behind focused traits or adapters.
- Preserve policy, scope, provenance, omission, and degraded-source reporting
  across every composed retrieval source.

### Ask first

- Add or change portable v1 retrieval schema fields.
- Make a production storage adapter depend on a test fixture crate.
- Promote hierarchy, belief, graph, or consolidation ports out of `engram-core`
  in the same implementation slice.

### Never do

- Do not make `engram-store-memory`, `engram-store-sql`, or any future store
  adapter the canonical owner of multi-source retrieval composition.
- Do not merge memory records and knowledge chunks into one persistence model
  for retrieval convenience.
- Do not introduce provider-specific embedding, graph database, SQL, or Node
  assumptions into `engram-domain`, `engram-memory`, or `engram-knowledge`.

## Testing Strategy

Retrieval composition uses TDD for source fan-in, ranking, omission, and
degraded-source invariants because each behavior has deterministic inputs and
outputs. Adapter boundary migration uses goal-based checks because successful
compilation and targeted import searches prove that store crates no longer own
composition traits. Existing v1 contract fixtures remain goal-based integration
checks through the memory service surface so callers keep the same accepted
payload behavior.

## Acceptance Criteria

- [ ] `engram-retrieval` owns the canonical retrieval composition traits and
  fusion strategy without depending on concrete memory, knowledge, SQL, vector,
  graph, Node, or TypeScript adapters.
- [ ] A composed retrieval service accepts memory candidates, knowledge
  candidates, and external index candidates through ports, applies shared fusion
  once, and returns one `ContextPayload`.
- [ ] Policy and scope checks stay visible before a candidate enters fusion, and
  denied candidates become `OmittedResult` entries instead of hidden leaks.
- [ ] Failed optional indexes become `RetrievalSourceFailure` entries without
  suppressing successful local memory or knowledge results.
- [ ] `engram-store-memory` keeps quick local retrieval fixtures but no longer
  defines the canonical composition traits or owns the production composition
  path.
- [ ] `engram-store-sql`, `engram-store-vector`, and future graph/knowledge
  adapters can participate in retrieval composition without depending on each
  other.
- [ ] Existing accepted retrieval fixtures and TypeScript contract checks pass
  without portable v1 schema changes.
- [ ] `docs/arch_divergence.md` records the updated alignment score and any
  remaining retrieval modularity gaps.

## Assumptions

- Technical: memory and knowledge are distinct but composable, and every
  retrieval path enforces policy (source: `docs/domain-data-model.md`).
- Technical: retrieval routes across memory and knowledge without merging their
  storage concerns (source: `docs/rfcs/0002-knowledge-source-extension.md`).
- Technical: architecture names semantic search, metadata filters, graph
  traversal, recency, confidence, and policy-aware ranking as retrieval
  responsibilities (source: `docs/architecture.md`).
- Technical: current divergence is that retrieval composition still partly lives
  in the in-memory memory service tests and fixture implementation (source:
  `docs/arch_divergence.md`).
- Technical: `engram-retrieval` already exists and currently provides weighted
  fusion while canonical retrieval traits still come from `engram-core` (source:
  `crates/engram-retrieval`, `crates/engram-core`).
- Process: specs under `docs/specs/` define acceptance criteria and plans for
  implementation slices (source: existing `docs/specs/*/spec.md` and
  `docs/specs/*/plan.md`).
- Product: the user has selected memory/knowledge separation and Rust crate
  modularity as the next closure targets (source: user confirmation
  2026-06-30).
