# Spec: Graph maintenance API

- **Status:** Draft
- **Owner:** phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** ADR-0027 (archive+restore durable state; supersedes ADR-0018's storage-mode consequence — *Accepted*), ADR-0022 (engine neutrality + surface parity), RFC-0014 (transactional consolidation — apply precedent), ADR-0018 (retraction ports + convergence remain in force over the active set)
- **Brief:** none
- **Contract:** none — the interface is a Rust port surfaced through the N-API binding and engram-mcp tools (no `contracts/<type>/` artifact)
- **Shape:** mixed — a data-model change (archive state) plus a service port plus multi-surface integration wiring

> **Spec contract:** this document defines what "done" means. The implementing
> PR must match this spec, or update it. Verification must be derivable from it.

## Objective

A host application or integration — `engram-cc`, an agent gateway, a future
ingestion adapter — repairs its knowledge graph through a first-class
**graph maintenance API** instead of writing directly to SQLite. Every ingestion
path is imperfect, so the graph accumulates duplicate, low-confidence, orphaned,
or unsupported topology; the maintenance API makes that damage
reversible. The host supplies policy (its ontology vocabulary, confidence
threshold, retention rules, and which candidate sets to act on); Engram supplies
the generic data-management primitives. The host selects a graph and scope,
lists and filters candidates, builds a dry-run plan that shows the exact
per-entity and per-relationship mutations, applies the reviewed plan, and verifies
the result — with every mutation attributable from existing `Provenance` and
reversible through archive/restore. Success is an integration that never needs
direct store access to repair its graph, sees exact previews before application,
reapplies a completed plan idempotently, and never loses data to an irreversible
default.

## Boundaries

The three-tier guard that keeps an implementing agent inside the lines.
*Always do* applies without asking; *Ask first* requires human sign-off
before proceeding; *Never do* is a hard rule, even under time pressure.

### Always do

- Default to non-mutating: any operation that can mutate returns a dry-run
  preview unless the caller passes an explicit apply flag (`ApplyMode::Apply`).
- Apply a maintenance plan inside a single backend transaction where the backend
  supports it (e.g., a transactional backend), and surface the backend's
  atomicity guarantee level in the apply result so the caller knows what holds.
- Attribute every mutation through the existing `Provenance` (actor, method,
  observed_at, confidence) already carried by each entity and relationship; carry
  review status where a human reviewed a candidate.
- Treat archive as the reversible path and permanent delete as an explicit,
  escalated operation — never hard-delete as the default maintenance action.
- Reuse the existing graph primitives rather than reimplementing them:
  `EntityIdentityRepository` (`consolidate_entities`, `discover_collisions`,
  `resolve_or_put`), `KnowledgeRepository` (`delete_*`), and
  `OntologyRepository::validate_graph` (advisory integrity).

### Ask first

- Extending v1 export coverage (hierarchy/belief) — only if a maintenance plan
  must roll back cross-family changes; deferred by ADR-0027, so raise before
  touching it.
- Introducing an LLM into candidate detection — out of scope (ADR-0027); raise
  before adding any non-deterministic detection path.
- Deciding the retention/reaping policy for archived rows — ADR-0018 convergence
  operates on the active set, so archived rows sit outside convergence; ask before
  committing a reaping rule.

### Never do

- Never give a host direct SQLite/SQL access to repair its graph — the API is
  the only path. *(structural)*
- Never claim or imply cross-store ACID or cross-store rollback. The atomicity
  guarantee is backend-dependent (ADR-0022); all-or-nothing holds within one
  transactional backend only. *(structural — matches the `atomic-batch-ingest`
  invariant)*
- Never introduce a separate durable audit table or a new export format for
  rollback. ADR-0027 rejected both: attribution rides `Provenance`, reversibility
  rides archive/restore.
- Never fold graph maintenance into the `consolidation` crate or copy its
  `Plan`/`Gate`/`Run` shapes into a god-module. It is a new port alongside
  `EntityIdentityRepository`, mirroring those shapes as a template only.
  *(structural — no god-module)*
- Never add product-specific rules (e.g. "remove z-Bot unknowns") or a fixed
  ontology into the core — candidate sets are policy-derived and generic.
- Never bypass the maintenance port's dry-run/atomic-apply envelope by calling
  store internals directly from a host.

## Testing Strategy

- **Plan semantics, idempotency, and atomicity** — TDD at the port level against
  a deterministic stub store, plus an integration test on the SQLite adapter that
  forces a verification failure and asserts the graph is byte-for-byte unchanged
  (single-transaction rollback). Why: the all-or-nothing guarantee is the core
  invariant and must prove out across the port boundary and the real engine.
- **Archive/restore reversibility and referential integrity** — TDD on the store:
  archived rows are hidden from active reads and from convergence; a node and its
  incident edges archive and restore together so no dangling edge ever appears in
  the active graph. Why: reversibility is the point of the feature and the
  referential rule is a compressible invariant.
- **Candidate detection** — TDD with fixture graphs asserting the candidate set
  per policy threshold (orphan, low-confidence, unsupported, duplicate), reusing
  `discover_collisions` and `validate_graph`. Why: detection is pure
  deterministic logic over the graph.
- **Merge and relationship rewiring** — TDD extending the `consolidate_entities`
  contract: redirected/coalesced/deleted counts match and `Provenance` is
  preserved on the surviving entity. Why: merge correctness is a typed invariant
  already under test.
- **Default-non-mutating safety** — TDD: invoking any mutating operation without
  an apply flag returns a preview and mutates nothing. Why: the safety default
  must be mechanical, not aspirational.
- **Surface parity** — goal-based check (integration surface): the capability is
  reachable from the Rust `EngramProvider` facade, the N-API binding,
  `@engram/node`, and engram-mcp tools, and `CapabilityReport` lists a
  `graph_maintenance` entry. Why: parity is a wiring outcome verifiable by a
  gate; until the deleted parity lint is reinstated, it is reviewer-checked.

## Acceptance Criteria

- [ ] A host lists and filters entities and relationships by graph, scope,
  source, kind, and confidence with cursor pagination, without direct store
  access.
- [ ] Every mutating operation returns an exact dry-run preview — per-entity and
  per-relationship before/after — when invoked without an explicit apply flag;
  only an explicit apply flag (`ApplyMode::Apply`) commits mutations.
- [ ] On a transactional backend, applying a maintenance plan runs inside one
  transaction; a verification failure leaves the graph byte-for-byte unchanged,
  and the apply result surfaces the backend's atomicity guarantee level.
- [ ] Reapplying a completed plan is idempotent — a second apply produces no new
  mutations.
- [ ] Archive moves an entity or relationship to a hidden, non-contractual state
  and is reversed by restore; the active graph (reads and convergence) excludes
  archived rows.
- [ ] Archiving a node archives its incident edges with it, and restore returns
  both, so the active graph never shows a dangling edge to an archived node.
- [ ] Merge combines two entities into one, rewiring subject/object references
  and coalescing duplicate relationships, and preserves `Provenance` on the
  survivor.
- [ ] Alias add/remove and normalized-exact-identity resolution are exposed
  through the port, reusing `resolve_or_put` semantics.
- [ ] Relationship rewrite edits a predicate or endpoint in place without a full
  record re-put.
- [ ] Candidate detection identifies orphan (no incident edges or no
  `source_refs`), low-confidence (below a policy threshold), unsupported
  (ontology-violating via `validate_graph`), and duplicate (via
  `discover_collisions`) candidates — deterministically, with no LLM.
- [ ] Every mutation is attributable from the affected entity or relationship's
  `Provenance` (actor, method, observed_at) and reversible through
  archive/restore; merge archives absorbed entities (recoverable via restore),
  and escalated Delete is the one permanent mutation.
- [ ] Graph health metrics report, per graph and source, candidate volume
  (orphan, low-confidence, unsupported, duplicate counts) and archived-row count
  as point-in-time read-only aggregates.
- [ ] The capability is reachable from the Rust `EngramProvider` facade, the
  N-API binding, `@engram/node`, and engram-mcp tools (candidates, build/apply
  plan, archive, restore, delete, merge, rewrite, alias, list/filter, health), and is
  reflected in `CapabilityReport` as a `graph_maintenance` entry distinct from
  the existing backend-neutral `maintenance` field. The atomicity guarantee level
  rides in `MaintenanceApplyResult`, not the report.

## Assumptions

Audit trail for the assumption-surfacing checkpoint run when this spec was
drafted. This section is the frame the contract was written under, not the
contract itself.

- Technical: Graph repair is a new capability, not an extension of consolidation
  — consolidation owns strategy→synthesis/decay planning for memory/belief and is
  non-durable; its `Plan`/`Gate`/`Run` shapes are reused as a template only.
  (source: consolidation-overlap check; `core/consolidation/src/lib.rs:31`,
  `core/domain/src/operations.rs:268`)
- Technical: Entities and relationships carry `Provenance` (actor, method,
  observed_at, confidence, source/evidence refs) but no archive/status field
  today; deletion is hard. (source: knowledge-model check;
  `core/domain/src/knowledge.rs:201-248`; ADR-0018)
- Technical: Reusable ops exist (`delete_*`, `delete_graph` cascade-transactional,
  `consolidate_entities` merge+rewire transactional, `discover_collisions`
  dry-run duplicates, `resolve_or_put` normalized identity); missing are
  port-level paginate/filter, archive/restore, alias port, relationship rewrite,
  candidate detection, and a generic plan/apply-transactionally port. (source:
  knowledge-model check; `core/knowledge/src/{repository,graph,identity}.rs`)
- Technical: No public transaction/batch port exists; atomicity is implicit
  inside `delete_graph` and `consolidate_entities` via `conn.transaction()`.
  (source: knowledge-model check; `adapters/sqlite/src/knowledge/{graph,identity}.rs`)
- Technical: A new capability wires through ~11 surface files (Rust facade →
  N-API → `@engram/node` → engram-mcp → `CapabilityReport`); the surface-parity
  lint is currently absent (deleted 2026-08-01), so parity is reviewer-checked.
  (source: surface-plumbing check)
- Technical: An `ExportImport`/`MigrationService` rollback-snapshot seam exists
  but is rejected for this feature — coarse, partial v1 coverage, and additive
  upsert rather than wipe-then-restore. (source: surface-plumbing check; ADR-0027)
- Process: Durable state (archive+restore, backend-dependent atomicity,
  deterministic candidates, no audit table, no export rollback) is locked in
  ADR-0027, which partially supersedes ADR-0018. (source: ADR-0027)
- Process: AGENTS.md requires `docs/domain-data-model.md` to be updated before
  contract/schema changes, and surface parity (ADR-0022) governs both the Rust
  facade and the N-API binding. (source: AGENTS.md)
- Product: The reversibility primitive is archive/restore only — no durable audit
  table, no export-snapshot rollback; candidate detection is deterministic with no
  LLM; atomicity is backend-dependent. (source: user confirmation 2026-08-07)
- Product: Near-term consumers are host integrations (`engram-cc`, agent
  gateways, future ingestion adapters) repairing knowledge-graph topology without
  direct store access. (source: user confirmation 2026-08-07; originating writeup
  `engram-graph-maintenance-api-writeup.md` — external, not in repo)
