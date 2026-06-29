# Spec: Hierarchy Navigation

- **Status:** Shipped
- **Owner:** phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** ADR-0003, ADR-0004
- **Brief:** none
- **Contract:** none
- **Shape:** service

> **Spec contract:** this document defines what "done" means. The implementing
> PR must match this spec, or update it. Verification must be derivable from it.

## Objective

Engram can persist hierarchy nodes and relations, then navigate from memory or
knowledge seed targets to their containing hierarchy path without adding a
clustering algorithm or model provider. The first slice proves scope isolation,
auditable provenance preservation, and parent-chain traversal.

## Boundaries

### Always do

- Preserve hierarchy provenance, policy, status, layer, members, and source
  target references.
- Keep hierarchy storage behind `HierarchyRepository`.
- Keep construction and navigation distinct.

### Ask first

- Add clustering, taxonomy evolution, LLM summarization, or embeddings.
- Promote hierarchy types into accepted v1 JSON schemas.
- Change retrieval composition to include hierarchy-expanded results by default.

### Never do

- Cross tenant/workspace boundaries while building paths.
- Mutate older hierarchy nodes during path lookup.
- Hide hierarchy effects inside retrieval results without explanation.
- Create a hierarchy module that owns storage, clustering, retrieval, ranking,
  and consolidation at once.

## Testing Strategy

- Repository behavior: TDD through in-memory hierarchy tests that store nodes and
  relations.
- Path navigation: TDD through parent-chain path tests for one seed and multiple
  seeds with a common ancestor.
- Scope isolation: TDD through cross-workspace path tests.
- Workspace hygiene: goal-based Rust, contract, code-doc, and TypeScript gates.

## Acceptance Criteria

- [x] In-memory storage persists hierarchy nodes and relations through
  `HierarchyRepository`.
- [x] `path_for` returns visible nodes from seed target to ancestor within the
  requested max layer.
- [x] `path_for` reports a lowest common ancestor when seeds share one.
- [x] Path lookup does not return nodes or relations outside the requested
  scope.
- [x] Hierarchy behavior does not introduce vector, model, SQL, Node, or
  TypeScript dependencies into core/domain.

## Assumptions

- Technical: domain hierarchy types already model nodes, memberships,
  relations, build configs, and paths (source:
  `crates/engram-domain/src/hierarchy.rs`).
- Technical: `engram-core` already exposes `HierarchyRepository` with node,
  relation, and path operations (source: `crates/engram-core/src/lib.rs`).
- Research: hierarchy should separate construction from navigation and support
  temporal plus hierarchical views (source:
  `docs/research/hierarchical-taxonomy-research.md`).
- Process: crate roots stay facades and behavior lives in focused modules
  (source: `AGENTS.md`).
- Product: first hierarchy slice is navigation over explicit parent links, not
  autonomous clustering (source: user confirmation 2026-06-29).
