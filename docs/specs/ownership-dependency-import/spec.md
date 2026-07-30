# Spec: ownership-dependency-import

Status: Shipped
Mode: light (structural — two new post-index MCP tools + a new extraction; no new workspace dependency; no security boundary beyond reading repo files the scanner already reads)
Shape: service
Constrained by: RFC-0012 (codegraph/on-top layer), ADR-0022 (engine neutrality — tools use existing provider handles), AGENTS.md (MCP tools route through the provider, no store bypass)
- **Contract:** none — no `contracts/<type>/` artifact; the extracted edges use existing ontology predicates (`depends_on` within, `owns` across).

## Objective

Two post-index MCP tools that turn the knowledge graph from "a code call-graph"
into "a graph that also knows **what depends on what** and **who owns what**" —
the multi-team / multi-repo program view. Both run AFTER `scan_repo`, walk the
repo for manifest/ownership files, and write entities + edges through the
provider's knowledge handle (same pattern as `scan_protocols`).

- **`scan_dependencies {path}`** — parse `Cargo.toml` (workspace root + each
  workspace member) and `package.json` files; emit `Module` entities for each
  package/crate and a `depends_on` edge from each consumer to each dependency.
  Internal workspace deps (a `path = "…"` dep on another member) link to that
  member's Module entity; external deps create a new Module entity for the
  external package.
- **`scan_ownership {path}`** — parse `CODEOWNERS` (`.github/CODEOWNERS`,
  `CODEOWNERS`, `docs/CODEOWNERS`); for each rule, create `Organization` (team)
  or `Person` (individual) entities for the owners and an `owns` edge to the
  path's Module/File entity. Missing CODEOWNERS → no-op with a clear note.

## Assumptions

- Technical: `EntityKind` has `Module`, `Organization`, `Person` (verified — `core/domain/src/knowledge.rs:174`). `scan_protocols` establishes the post-index-tool pattern: `require_knowledge()` + `put_entity`/`put_relationship` (verified).
- Technical: the workspace `Cargo.toml` lists `[workspace] members` + each member has a `[package] name`; `package.json` has `name` + `dependencies`/`devDependencies` (verified in this repo).
- Process: Cargo.toml is parsed with a lightweight section/line scan (no new `toml` dependency — keep the dependency budget flat, mirroring scan_protocols' regex approach); package.json uses the existing `serde_json`. (user confirmation 2026-07-30 — go for it)
- Process: ownership path→entity resolution is intentionally simple in v1 (Module/File entity per distinct CODEOWNERS path); deep symbol-level ownership is deferred.

## Boundaries

### Always do
- Route every write through the provider's `require_knowledge()` handle — no direct concrete-store access (mirror scan_protocols).
- Reuse existing ontology predicates only: `depends_on` (within), `owns` (across). Do not invent predicates.
- Make both tools idempotent on re-run (entity/relationship ids derived from stable name+predicate keys, like scan_protocols' `"{a}\u{1f}predicate\u{1f}{b}"` id scheme).
- Soft-fail on parse problems (skip the file, continue) — a malformed manifest never aborts the scan; the result text summarizes counts.

### Ask first
- Adding the `toml` crate as a dependency (currently avoided; revisit if regex parsing proves brittle on real-world Cargo.toml).
- Changing `EntityKind` (e.g. adding a `Package` variant) — v1 reuses `Module`.

### Never do
- Add a new workspace dependency for v1.
- Put storage/SQL or concrete-store types in the tool (engine neutrality).
- Block on a missing file: `scan_ownership` with no CODEOWNERS and `scan_dependencies` with no manifests are documented no-ops, not errors.
- Invent link targets: external dependency Modules are created in the same batch they're linked from.

## Testing Strategy

- **TDD** for the pure parsers — the contract is compressible (manifest text → `(consumer, dep)` / `(path, owners)` pairs). Red-green on: Cargo.toml `[dependencies]` + `[dev-dependencies]` + workspace `path` deps; package.json `dependencies`/`devDependencies`; CODEOWNERS comments + multiple owners + glob paths.
- **Goal-based check** for the tool wiring — each tool runs over a temp fixture repo and the result reports the expected entity/edge counts; verify an edge via `graph_neighbors`.
- **Manual/QA** — run both tools against the mem-alpha repo itself (dependency tool has real input; ownership is a no-op here since no CODEOWNERS) and confirm `graph_neighbors` on a crate shows its `depends_on` edges.

## Acceptance Criteria

- [x] `scan_dependencies` parses workspace `Cargo.toml` + member `Cargo.toml`s + `package.json`s into `Module` entities + `depends_on` edges.
- [x] A workspace `path` dependency links to the member's Module (by package name); an external dependency creates a new Module.
- [x] `scan_ownership` parses CODEOWNERS into `Organization`/`Person` entities + `owns` edges; absent CODEOWNERS is a documented no-op.
- [x] Both tools are idempotent (stable ids) and soft-fail on malformed input.
- [x] Pure parsers are unit-tested (Cargo.toml / package.json / CODEOWNERS).
- [x] Both tools registered in `main.rs` with input schemas; route through `require_knowledge()`.
- [x] `cargo fmt --all`, `cargo check --workspace` (0 warnings), `cargo test -p engram-mcp`, engine-neutrality, surface-parity all green.
