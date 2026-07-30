# Plan: scan-filter-config

## Design (LLD)

### Data flow

`engram-mcp` (host) discovers + loads a JSON config → builds a `ScanFilter` →
passes it via `ScanOptions.scan_filter` into `engram-ingest::scan_repository`,
which threads it to the two decision points:

- walk filter (`scanner.rs:210`): `scan_filter.is_denylisted(&rel)`
- concept resolver (`scanner.rs:453`): `scan_filter.should_link_concept(name)`

`engram-ingest` never reads a config file from disk; it receives a ready value.
This keeps the ingest crate host-neutral (ADR-0022) and testable with pure
constructors.

### `scan_filter` module (`adapters/ingest/src/scan_filter.rs`)

- `ScanFilter` (the ready-to-use filter): holds `HashSet`s for blocklist,
  allowlist, deny_dirs, deny_exts + `min_concept_name_len`. Methods:
  `should_link_concept(&self, name) -> bool`, `is_denylisted(&self, rel) -> bool`.
- `ScanFilter::builtin()` — moves the current `GENERIC` list (from
  `scanner.rs`) and `DENY_DIRS`/`DENY_FILE_EXT` (from `classifier.rs`) here as
  the single source; `min_concept_name_len = 8`.
- `ScanFilter::default()` → `builtin()` (impl `Default`).
- `ScanFilter::merge(config: &ScanFilterConfig) -> Self` — start from `builtin()`,
  then: `min_concept_name_len = config.min_name_length.unwrap_or(8)`; extend
  blocklist/deny sets; allowlist is additive and checked first (override).
- `ScanFilterConfig` (serde, `#[derive(Default)]`, all fields optional):
  ```
  { concepts: { min_name_length: Option<usize>, blocklist: Vec<String>, allowlist: Vec<String> },
    deny: { dirs: Vec<String>, extensions: Vec<String> } }
  ```
  All `Vec` default to empty; sections default to empty structs.

### Backward-compat delegation

- Free `fn is_denylisted(rel)` in `classifier.rs` → delegates to
  `ScanFilter::builtin().is_denylisted(rel)`.
- `DENY_DIRS`/`DENY_FILE_EXT` consts stay in `classifier.rs` (re-exported /
  reused by `ScanFilter::builtin()` via `pub(crate)`), avoiding a cross-module
  move that would touch `classify_file`'s neighbors. Decision logic moves to
  `scan_filter`; the consts are referenced, not duplicated.
- **Deviation from T1 (recorded):** the original plan kept the free
  `fn should_link_concept(name)` in `scanner.rs` as a delegating alias. In
  implementation it was *removed* instead — it was private (no external
  callers), the resolver now calls `opts.scan_filter.should_link_concept`
  directly, and its 5 tests were relocated to `scan_filter::tests` (the logic's
  new home). Cleaner than a dead delegating alias; behavior identical.

### `ScanOptions`

Add `pub scan_filter: ScanFilter`. Update the 8 struct-literal sites:
- `mcp/engram-mcp/src/codegraph.rs:131` — build from discovered config.
- `bindings/node/src/ingest_ops.rs:117` — `ScanFilter::default()` (N-API uses
  builtin; config discovery is MCP-first, noted in PR).
- 6 test sites — `ScanFilter::default()`.

### `scan_repo` MCP tool (`mcp/engram-mcp/src/codegraph.rs`)

- Add optional arg `scan_config` (path) to the tool's input schema.
- Discovery ladder: explicit `scan_config` arg → else `<repo>/.engram/scan.json`
  if it exists → else `ScanFilter::builtin()`.
- Load errors are **soft**: a malformed/missing file logs and falls back to
  builtin (a bad config file must never abort a scan). Surfaced in the result
  text.

### Stack

Rust + `serde`/`serde_json` (existing deps). No new dependency.

## Tempted to add, declining

- A `.npmignore` reader — the `ignore` crate doesn't support it; it's a separate
  slice with its own design (custom `OverrideBuilder` or a pre-filter pass). Declining now.
- An env-var fallback (`ENGRAM_SCAN_CONFIG`) — the repo-root convention + tool arg
  cover the real use cases; an env var adds a hidden global. Declining.
- Making `classify_file` / `is_secret_file` tunable — out of the stated scope;
  `classify_file` is extension-driven already, `is_secret_file` is a security
  control that must stay fixed. Declining.

## Tasks

### T1 — `ScanFilter` + `ScanFilterConfig` module (TDD)
Depends on: none
Spec mapping: AC#1, AC#2 (builtin==today)
Verification: TDD
Tests (red first):
- `default_rejects_short_and_generic` — builtin rejects "Config", "authentication"; accepts "RetrievalIndex".
- `allowlist_overrides_blocklist_and_length` — allowlist forces a 5-char name + a blocklisted term to link.
- `extra_blocklist_merges` — config blocklist adds "KafkaConsumer"; it is then rejected while a builtin-accepted name still passes.
- `min_name_length_override` — set to 12; a 10-char specific name is rejected.
- `deny_dirs_and_exts_merge` — config adds dir "generated" + ext "map"; paths under them are denied; builtin-denied paths still denied.
- `config_json_round_trip` — `serde_json::from_str` into `ScanFilterConfig`, merge, assert merged behavior.
Approach:
1. Create `adapters/ingest/src/scan_filter.rs` with the struct, consts, methods, config serde type. Red tests first.
2. Reference `DENY_DIRS`/`DENY_FILE_EXT` from `classifier` (make them `pub(crate)`); move `GENERIC` here.
3. Green; refactor.

### T2 — Wire `ScanFilter` through `ScanOptions` + scanner decision points
Depends on: T1
Spec mapping: AC#2 (default path), enables AC#3-6
Verification: TDD (regression) + goal-based
Tests:
- Existing `should_link_concept` / `is_denylisted` free-fn tests stay green (delegation).
- New: `scan_repository` honors `opts.scan_filter` — a fixture file in a custom-denied dir is not ingested (integration test in `adapters/ingest/tests/`).
Approach:
1. Add `scan_filter: ScanFilter` to `ScanOptions`; update 8 literal sites (default except MCP).
2. Scanner walk: `if opts.scan_filter.is_denylisted(&rel) || is_secret_file(&rel)`.
3. Resolver: `if opts.scan_filter.should_link_concept(name)`.
4. Free fns delegate to builtin.

### T3 — `scan_repo` MCP tool: config discovery + `scan_config` arg
Depends on: T2
Spec mapping: AC#7
Verification: goal-based check (exercises the built MCP path end-to-end via the existing scanner test harness) + manual
Approach:
1. Add `scan_config` to tool input schema (optional path).
2. Discovery ladder in `codegraph.rs::scan_repo`: arg → `<repo>/.engram/scan.json` → builtin. Soft-fail to builtin.
3. Re-export `ScanFilter`/`ScanFilterConfig` from `engram-ingest` lib facade.
4. Result text notes whether a config was applied.

### T4 — Gates + PR
Depends on: T3
Spec mapping: AC#8
Verification: goal-based (the gate suite)
Approach: fmt, check (0 warnings), test ingest+mcp, neutrality, parity, docs check. Commit grouped, push, open PR (no merge).

## Rollout

Single PR (#60) against `main`, branch `feat/scan-filter-config`. Three logical
commits: feat(ingest) scan_filter module + wiring; feat(mcp) config discovery;
docs spec. Left open for review — not auto-merged.
