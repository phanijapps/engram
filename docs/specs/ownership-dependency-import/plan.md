# Plan: ownership-dependency-import

## Design (LLD)

Two post-index MCP tools, each a focused module in `mcp/engram-mcp/src/`,
mirroring `scan_protocols` (`protocols.rs`): a `pub fn scan_*(app, args)`
handler that reads files under the repo root and writes entities + edges via
`app.provider.require_knowledge()`.

### `dependencies.rs`

Pure parsers (TDD):
- `parse_cargo_deps(toml_text, self_name) -> Vec<(String /*consumer*/, String /*dep*/)>`
  - Track the current section (`[package]` → read `name`; `[dependencies]` /
    `[dev-dependencies]` → each `key = …` line is a dep named `key`).
  - Workspace member self-name comes from the `[package] name` of each member's
    Cargo.toml; the caller passes it in.
- `parse_package_json_deps(json_text) -> Option<(String /*name*/, Vec<String>/*deps*/)>`
  - serde_json: `name` + union of `dependencies` + `devDependencies`.

Tool `scan_dependencies {path}`:
1. Walk the repo (ignore crate) for `Cargo.toml` + `package.json` (skip
   `target/`, `node_modules/`).
2. For each manifest: collect `(consumer, dep)` pairs.
3. Build a `Module` entity per distinct package name (consumers + deps) +
   `depends_on` relationship per pair. Ids: `"mod-{name}"` and
   `"{consumer}\u{1f}depends_on\u{1f}{dep}"`.
4. Write via `require_knowledge()`; soft-fail per file; summarize counts.

### `ownership.rs`

Pure parser (TDD):
- `parse_codeowners(text) -> Vec<(String /*path*/, Vec<String> /*owners*/)>`
  - Skip blank/`#` comment lines; split each line into path + `@owner` tokens;
    strip the leading `@` (keep `org/team` or `user` as the owner name).

Tool `scan_ownership {path}`:
1. Discover CODEOWNERS at `.github/CODEOWNERS`, `CODEOWNERS`, or
   `docs/CODEOWNERS`; if none → return a no-op note.
2. For each rule: create `Organization` (if owner contains `/`, i.e. `org/team`)
   or `Person` entities; create a `Module`/`File` entity for the path; emit
   `owner -[owns]-> path-entity`. Ids: `"owner-{name}"`, `"path-{path}"`,
   `"{owner}\u{1f}owns\u{1f}{path}"`.
3. Write via `require_knowledge()`; summarize counts.

### Stack / boundaries

Rust, existing `serde_json` + `ignore` (walker) + `futures::executor::block_on`.
No new dependency. MCP-layer only (no `core/` or adapter changes). No
`EntityKind` change (`Module`/`Organization`/`Person` reused). Engine-neutral
(writes go through the provider handle).

## Tempted to add, declining

- The `toml` crate — regex/line parsing of `[dependencies]` is enough for v1 and
  keeps the dependency budget flat. Revisit if parsing proves brittle.
- Symbol-level ownership resolution (CODEOWNERS path → exact code entities) —
  v1 links owners to path Modules; deep resolution deferred.
- A `Package` EntityKind variant — `Module` is the established analogue.
- Folding the importers into `scan_repo` itself — keeping them as separate
  post-index tools mirrors `scan_protocols` and stays opt-in.

## Tasks

### T1 — `dependencies.rs` parsers (TDD)
Depends on: none
Spec mapping: AC#1, AC#2 (parsing half)
Verification: TDD
Tests (red first):
- `cargo_parses_dependencies_and_dev_dependencies` — `[dependencies]` + `[dev-dependencies]` keys extracted.
- `cargo_reads_package_name` — `[package] name = "x"` captured as the consumer.
- `package_json_parses_deps` — `dependencies` + `devDependencies` unioned; `name` returned.
- `package_json_malformed_is_none` — bad JSON → `None` (soft-fail).
Approach: create `dependencies.rs` with the two pure fns + a `#[cfg(test)] mod tests`; red, then green.

### T2 — `scan_dependencies` tool wiring
Depends on: T1
Spec mapping: AC#1, AC#2, AC#4, AC#6
Verification: goal-based + manual
Approach: walk manifests, build Module entities + depends_on edges (stable ids), write via `require_knowledge()`, soft-fail, summarize. Register in `main.rs`. Run against the mem-alpha repo; `graph_neighbors` on a crate shows its deps.

### T3 — `ownership.rs` parser + `scan_ownership` tool (TDD + goal-based)
Depends on: none
Spec mapping: AC#3, AC#4, AC#5, AC#6
Tests: `codeowners_parses_rules_and_owners` (path + `@org/team` + `@user`, comments skipped); `codeowners_empty_is_empty`.
Approach: parser fn + tool (discover CODEOWNERS, create org/person + path Module entities + owns edges, stable ids, soft-fail). No-op note when absent. Register in `main.rs`.

### T4 — Gates + PR
Depends on: T3
Spec mapping: AC#7
Verification: the gate suite.
Approach: fmt, check (0 warnings), test engram-mcp, neutrality, parity, docs. Commit grouped, push `feat/ownership-dependency-import`, open PR (no merge). Tick ACs + Shipped in the spec.

## Rollout

Single PR against `main`, branch `feat/ownership-dependency-import`. Logical
commits: feat(mcp) dependencies tool; feat(mcp) ownership tool; docs spec.
Left open for review — not auto-merged.
