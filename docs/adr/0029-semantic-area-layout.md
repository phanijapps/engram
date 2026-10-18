# ADR-0029: Semantic area directories over a flat `crates/` tree

- **Status:** Accepted
- **Date:** 2026-10-17
- **Related:** ADR-0010 (behavior port split), ADR-0022 (engine grid vs
  backend recipe), ADR-0028 (engram-code crate), `AGENTS.md` § Target
  Repository Shape

## Context

The workspace grew to 36 Rust crates across six top-level areas (`core/`,
`adapters/`, `backends/`, `bindings/`, `mcp/`, `codegraph/`) plus
`examples/rust-integration`. A review question asked whether all Rust code
should instead live under a single conventional `crates/` directory, as many
Cargo workspaces do, to reduce root clutter.

Verified before deciding (2026-10-17, `main`):

- All 36 crates are workspace members already listed in the root
  `Cargo.toml`; there is no scattering problem — every crate sits in exactly
  one semantic area.
- The CI enforcement gates pattern-match the area paths:
  `scripts/ci/check-engine-neutrality.sh` scans an explicit list of
  `core/*/src` directories; `scripts/ci/check-surface-parity.sh` mirrors the
  same structure for the Rust facade ↔ N-API binding parity rule.
- `AGENTS.md` § Target Repository Shape documents the layout as the boundary
  contract, including per-area rules ("engram-domain must not depend on SQL…",
  "engines live on a capability × engine grid at `adapters/<capability>/<engine>`").

## Decision

Keep the semantic area directories. Do not introduce a flat `crates/` tree.

Rules that follow from this decision:

1. **A crate's directory IS its layer declaration.** `adapters/sqlite`
   announces "engine cell behind a port" in a way `crates/engram-store-sqlite`
   cannot. New crates join their semantic area, never a flat bucket.
2. **Layout changes are ADR-level decisions.** Moving an area (or flattening
   to `crates/`) rewrites the neutrality/parity scan lists and the AGENTS.md
   boundary contract — that is an architecture change, not housekeeping.
3. **The scan lists stay honest.** The neutrality gate's directory list must
   contain only paths that exist; a retired crate's path is removed in the
   same change that retires the crate (this was violated once —
   `core/orchestration/src` lingered after the crate was deleted — and is
   the reason this rule is written down).

## Consequences

- Root carries six semantic directories instead of one; the cost is accepted
  because the directories are self-documenting and few.
- Tooling that wants "all Rust crates" enumerates workspace members
  (`cargo metadata`), never the filesystem shape.
- If a future reorganization is proposed (e.g. grouping under `rust/`), this
  ADR is the document to supersede, and the enforcement-gate coupling above
  is the checklist of what must move with it.
