# Spec: Knowledge-graph identity and consolidation

- **Status:** Draft
- **Owner:** phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** RFC-0014 (canonical knowledge-graph identity and consolidation), ADR-0022 (engine neutrality)
- **Brief:** none
- **Contract:** none — the new types (EntityIdentityMode, EntityWriteOutcome, EntityMergePolicy) are Rust domain types in `engram-domain`; the port operations live on a focused trait in `engram-knowledge`. Generated contract schemas are additive to `contracts/v1/` when the capability ships.
- **Shape:** mixed — `data` (identity model + merge policy) + `service` (port operations + SQLite atomic impl) + `integration` (facade + N-API parity)

> **Spec contract:** this document defines what "done" means. The implementing
> PR must match this spec, or update it.

## Objective

Engram provides storage-neutral, caller-policy-driven identity operations for
knowledge-graph entities and exact relationships, plus transactional duplicate
consolidation. A host application writes an entity with a declared identity
policy (`IdOnly`, `StableKey`, or `ScopedKindAndNormalizedName`); engram
atomically resolves it to one canonical record — matching an existing entity
under the policy, creating a new one if none exists, or merging if the policy
permits. The host discovers duplicate IDs under a policy via dry-run collision
discovery, then consolidates them transactionally with full relationship
redirection, provenance preservation, and an audit result. Existing ID-only
`put_entity` and `put_relationship` behavior is unchanged; the new operations
are additive.

Success: no duplicate entity IDs for case variants under the same scope, graph,
and kind; concurrent resolve-or-create writes converge on one canonical ID;
consolidation redirects every relationship endpoint without dangling references
or provenance loss; the capability is reachable from the Rust facade, N-API
binding, and capability report (surface parity).

## Boundaries

### Always do
- Identity never crosses scope or kind boundaries unless the caller explicitly
  selects a broader boundary.
- Include `graph_id` in the identity key by default; the caller can opt out.
- Normalization is deterministic and versioned: Unicode NFC, case folding,
  whitespace collapse, stable separators. Honorifics, tickers, abbreviations,
  and domain synonyms remain caller policy.
- Merge aliases, `source_refs`, `concept_refs`, `ontology_class_refs`,
  provenance, evidence, temporal fields (`valid_from`/`valid_until`,
  `created_at`/`updated_at`); report conflicting values rather than silently
  discarding them.
- Consolidation redirects all inbound and outbound relationship endpoints,
  coalesces exact duplicates, and preserves provenance from every contributing
  record.
- Expose identity + consolidation through `EngramProvider` typed handles,
  `CapabilityReport`, and the N-API binding per the surface-parity rule.
- Keep the identity port focused — do not overload `KnowledgeRepository` with
  merge policy, collision discovery, and audit behavior.

### Ask first
- Changing the default identity mode from `IdOnly` to normalized (would affect
  existing callers).
- Auto-merging during schema migration (must be explicit dry-run + consolidate,
  never arbitrary row-order resolution).
- Introducing a new normalization version (requires a migration test).

### Never do
- Merge entities across scope, graph, or kind unless the caller explicitly
  selects a broader boundary. [structural]
- Infer that arbitrary predicates are synonymous — predicate equivalence and
  inverse rules belong to a caller-supplied ontology policy, not engram. Engram
  may expose a canonicalization hook; the caller maps `used_by` → inverse `uses`
  before the exact key is evaluated. [structural]
- Break existing ID-only `put_entity` or `put_relationship` behavior — new
  operations are additive; existing records do not become implicitly merged. [structural]
- Put SQL, locking, indexes, or backend-specific identity implementation in
  `engram-domain` or the knowledge port crates (engine neutrality, ADR-0022). [structural]

## Testing Strategy

- **TDD** — pure normalization and merge logic: case folding, Unicode NFC,
  whitespace, alias matching, stable keys, scope/kind/graph separation,
  deterministic ordering, conflict reporting, idempotency. These have
  compressible invariants (same input → same normalized key; same merge inputs
  → same merged entity). Lives in `core/knowledge/src/identity.rs` + tests.
- **Goal-based check** — SQLite identity indexes + uniqueness constraints:
  migration adds indexed identity columns or a dedicated identity table;
  backfill does not auto-select winners for existing collisions. Verified by
  `cargo test -p engram-store-sqlite` (concurrent convergence, reopen, rollback).
- **Integration** — transactional consolidation: endpoint redirection (inbound,
  outbound, self-loop), relationship coalescing, evidence/provenance
  preservation, cross-scope rejection, dry-run discovery, repeated-apply
  idempotency, rollback-as-a-unit, reopen integrity. Exercised by
  `adapters/sqlite/tests/` + cross-adapter fixtures in `adapters/integration/`.
- **Goal-based check** — facade + N-API parity: `EngramProvider` typed handle +
  capability report entry + N-API binding expose the same operations with stable
  typed error mapping. Verified by `cargo test -p engram-integration` +
  `pnpm run typecheck`.

## Acceptance Criteria

- [ ] Case variants under the same scope, graph, kind, and selected
  normalized-name policy converge on one canonical entity via
  `resolve_or_put_entity`.
- [ ] Identical names in different scopes, graphs, kinds, or stable keys remain
  distinct entities.
- [ ] Concurrent `resolve_or_put_entity` requests for the same identity
  converge on one canonical ID (database uniqueness, not application-level
  probe-then-insert).
- [ ] Stable-key identity joins renamed entities without relying on display
  names.
- [ ] `resolve_or_put_entity` returns `Created`, `Matched`, or `Merged` with
  the canonical entity and ID; retries are idempotent.
- [ ] `resolve_or_put_relationship` enforces the exact canonical key (scope +
  graph boundary + canonical subject + canonical object + caller-canonicalized
  predicate); different predicates and directions remain distinct.
- [ ] Dry-run collision discovery under a declared identity policy reports
  duplicate IDs without mutation.
- [ ] Entity consolidation redirects every inbound and outbound relationship
  endpoint and leaves no dangling references.
- [ ] Relationships made identical by endpoint redirection are coalesced with
  evidence, provenance, and confidence preserved.
- [ ] Consolidation is transactional (rolls back as a unit on any failure) and
  idempotent when repeated.
- [ ] Conflicting entity field values during merge are reported (not silently
  discarded).
- [ ] Existing ID-only `put_entity` and `put_relationship` calls and stored
  records retain their current behavior (compatibility).
- [ ] Cross-adapter fixtures cover entity identity, exact relationship identity,
  concurrent convergence, dry-run discovery, consolidation integrity, and
  unchanged ID-only puts.
- [ ] `CapabilityReport` advertises the identity capability; the Rust facade
  (`EngramProvider`) and the N-API binding expose equivalent operations.

## Assumptions

- Technical: `KnowledgeEntity` carries `name`, `aliases`, `kind`, `scope`,
  `graph_id` (Option), `provenance`, `source_refs`, `concept_refs`,
  `ontology_class_refs`, temporal fields, `metadata` — sufficient inputs for a
  caller-defined identity policy. (source: core/domain/src/knowledge.rs:203-228)
- Technical: `KnowledgeRepository::put_entity` is ID-keyed with no identity
  resolution; SQLite uses `ON CONFLICT(id)`. (source:
  core/knowledge/src/repository.rs:29; RFC-0014 §Problem)
- Technical: `KnowledgeRelationship` carries `subject: EntityRef`, `predicate:
  String`, `object: EntityRef`, `scope`, `graph_id` — identity is record-ID-based,
  no exact key. (source: core/domain/src/knowledge.rs:232-248)
- Technical: `core/knowledge/src/` has no `identity.rs` module — identity is
  net-new. (source: ls core/knowledge/src/)
- Technical: Engine neutrality requires identity as a port contract, not
  SQLite-specific (ADR-0022; AGENTS.md boundary rules).
- Technical: Surface parity requires every capability through `EngramProvider`
  + N-API + `CapabilityReport` (AGENTS.md).
- Process: RFC-0014 is Draft; the user confirmed it is accepted enough to spec
  against. (source: user confirmation 2026-07-21)
- Product: This spec covers all six decisions (D1–D6) in one feature spec; the
  plan decomposes into E0–E6 tasks. (source: user confirmation 2026-07-21)
- Process: Identity and consolidation use a focused port (e.g.
  `EntityIdentityRepository`) rather than extending `KnowledgeRepository`, per
  RFC-0014 open question 1. (source: user confirmation 2026-07-21)
