# Spec: Workspace Responsibility Layout

- **Status:** Shipped
- **Owner:** phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** `docs/domain-data-model.md`, `docs/architecture.md`, `docs/rfcs/0001-memory-layer-scope.md`, `docs/rfcs/0002-knowledge-source-extension.md`, `AGENTS.md`
- **Brief:** none
- **Contract:** workspace layout, crate manifests, group-local and adapter-local
  `AGENTS.md`; no v1 JSON schema change
- **Shape:** mixed

> **Spec contract:** this document defines what "done" means. The implementing
> PR must match this spec, or update it. Verification must be derivable from it.

## Objective

The workspace is grouped by responsibility so contributors can understand the
architecture from paths before opening code. Storage-neutral Rust contracts and
engines live under `core/`, replaceable infrastructure lives under `adapters/`,
runtime language bridges live under `bindings/`, TypeScript remains under
`packages/`, and developer tooling remains under `tools/` or existing script
locations. Crate package names remain stable during the first migration so
behavior, imports, and TypeScript bindings can be tested without conflating path
movement with public crate renaming.

## Boundaries

The three-tier guard that keeps an implementing agent inside the lines.
*Always do* applies without asking; *Ask first* requires human sign-off before
proceeding; *Never do* is a hard rule, even under time pressure.

### Always do

- Move crates by architectural responsibility while keeping package names,
  public Rust APIs, generated TypeScript contracts, and v1 JSON schemas stable.
- Add scoped `AGENTS.md` files for new top-level responsibility groups and for
  every adapter crate because adapters are the highest-risk boundary.
- Keep old behavior tests and examples passing through workspace path changes.
- Update docs that mention `crates/` as the canonical Rust location.

### Ask first

- Rename published crate package names such as `engram-store-sql` to
  `engram-memory-sqlite`.
- Delete compatibility crates or public re-exports.
- Split new behavior crates such as `engram-memory-engine`,
  `engram-hierarchy`, `engram-belief`, or `engram-consolidation` in the same
  path-migration slice.
- Change N-API or TypeScript public configuration surfaces.

### Never do

- Do not move adapter-specific SQL, in-memory state, vector, Node, or TypeScript
  code into `core/`.
- Do not make a path move also rewrite behavior, persistence semantics, or v1
  contract fields.
- Do not leave root or group `AGENTS.md` files contradictory about crate
  ownership.
- Do not create catch-all groups such as `misc/`, `shared/`, or `common/` for
  code whose responsibility is unclear.

## Testing Strategy

This spec uses goal-based checks for workspace membership, package-name
stability, path references, and documentation because those outcomes are best
proved by manifest reads, `cargo metadata`, grep checks, and docs hooks. Existing
Rust and TypeScript behavior suites remain the integration verification because
the migration is not allowed to change memory, retrieval, ingestion, vector, or
binding behavior. TDD is reserved for a later engine-extraction spec; this slice
does not introduce new runtime logic.

## Acceptance Criteria

- [x] Rust workspace members are grouped under responsibility directories:
  `core/`, `adapters/`, and `bindings/`.
- [x] Package names remain stable for the first migration; `cargo metadata`
  reports the same Rust package names before and after the path move.
- [x] `core/` contains only storage-neutral Rust crates: domain, runtime,
  memory ports, knowledge ports, retrieval, orchestration, and evaluation.
- [x] `adapters/` contains replaceable infrastructure crates, including memory
  in-memory, memory SQLite, knowledge in-memory, retrieval sqlite-vec, and the
  current mixed ingest crate while it still owns filesystem and Git readers.
- [x] `bindings/` contains the N-API bridge crate.
- [x] New `AGENTS.md` files exist at `core/AGENTS.md`, `adapters/AGENTS.md`,
  `bindings/AGENTS.md`, and inside each adapter crate after movement.
- [x] Root `Cargo.toml`, crate path dependencies, examples, docs, and local
  scripts resolve the new paths without relying on stale `crates/` assumptions.
- [x] Existing Rust gates, TypeScript gates, contract hooks, and docs hooks pass
  without v1 schema changes.
- [x] `docs/arch_divergence.md` and README architecture sections describe the
  grouped layout and remaining staged renames or engine extraction work.

## Assumptions

- Technical: Engram currently uses a flat Rust workspace under `crates/` with
  stable package names such as `engram-domain`, `engram-store-memory`, and
  `engram-store-sql` (source: `Cargo.toml`, `find crates -maxdepth 2 -name Cargo.toml`).
- Technical: memory and knowledge are distinct but composable, and domain
  contracts are independent of storage engines and language bindings (source:
  `docs/domain-data-model.md`).
- Technical: architecture already distinguishes domain, memory ports,
  knowledge ports, retrieval, storage adapters, and connectors (source:
  `docs/architecture.md`).
- Technical: storage adapters are replaceable behind ports, and code/document
  knowledge must not become special cases inside core memory (source:
  `docs/rfcs/0002-knowledge-source-extension.md`).
- Process: root `AGENTS.md` requires crate roots and package entry points to be
  facades and prohibits god modules/packages (source: `AGENTS.md`).
- Product: grouped top-level responsibility directories are the desired
  direction, and this spec should be written before implementation (source:
  user confirmation 2026-06-30).

## Adversarial Review Notes

- **Path churn can hide behavior changes.** The implementation must keep path
  movement separate from crate renames and runtime refactors.
- **Top-level grouping can become theater.** The groups need local `AGENTS.md`
  constraints and import checks, otherwise code can still violate boundaries
  from cleaner-looking paths.
- **Compatibility shims can become permanent.** Any deferred compatibility crate
  or old package name needs an explicit follow-up condition, not an unbounded
  promise.
- **Engine extraction is tempting but risky.** `engram-memory-engine` belongs in
  a later spec unless duplicated orchestration makes the extraction mechanical
  and testable.
- **TypeScript backend selection is a separate surface.** Native backend
  selection should be specified after Rust adapter paths are stable, not bundled
  into the path migration.
