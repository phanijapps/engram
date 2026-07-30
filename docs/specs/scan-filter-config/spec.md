# Spec: scan-filter-config

Status: Shipped
Mode: light (no risk trigger fired beyond structural — single new module in one crate, no new dependency, no security boundary; the scanner already performs file I/O)
Shape: service
Constrained by: ADR-0022 (engine neutrality — `engram-ingest` stays neutral; config discovery lives in `engram-mcp`), AGENTS.md boundary rules (crate roots are facades; behavior in focused modules)
- **Contract:** none — the `ScanFilterConfig` JSON shape is a Rust serde struct in `adapters/ingest`; no `contracts/<type>/` artifact.

## Objective

A user can steer the repository scanner's two hardcoded tuning lists — the
cross-document **concept-link filter** (`should_link_concept`) and the **file
denylist** (`is_denylisted`) — through an external JSON config file, without
editing Rust. Today both lists are `const` arrays in `adapters/ingest`, so an
enterprise that wants to block a project-specific generic term (e.g.
`"KafkaConsumer"`) or skip a project-specific directory (`generated/`,
`vendor/`) must fork the scanner.

Concretely, after this change:

- A `.engram/scan.json` placed at a scanned repo's root (or a path passed via
  the `scan_repo` tool's new `scan_config` arg) is loaded and merged with the
  built-in defaults.
- `concepts.blocklist` adds terms to the generic-word filter; any matching
  concept name is **not** cross-document-linked.
- `concepts.allowlist` forces a concept to be linked even if it is short or in
  the blocklist (override).
- `concepts.min_name_length` raises/lowers the ≤N-char threshold (default 8).
- `deny.dirs` / `deny.extensions` add to the always-skip denylist.

Default behavior (no config file, no arg) is byte-identical to today.

## Assumptions

- Technical: scanner uses the `ignore` crate walker + a hardcoded denylist in `adapters/ingest/src/classifier.rs` (DENY_DIRS, DENY_FILE_EXT); concept filter is `should_link_concept` in `adapters/ingest/src/scanner.rs` (verified this session).
- Technical: `ScanOptions` is a struct literal constructed at `mcp/engram-mcp/src/codegraph.rs:131`, `bindings/node/src/ingest_ops.rs:117`, and ~6 test sites (verified this session).
- Process: `.npmignore` honoring is OUT of scope — the `ignore` crate does not recognize it and adding it is a separate, larger change. Deferred.
- Process: config discovery (reading the file from disk) belongs in `engram-mcp` (transport/host), not `engram-ingest` (which receives a ready `ScanFilter`). user confirmation 2026-07-30.

## Boundaries

### Always do
- Preserve current behavior exactly when no config is present (regression-safe: every existing `should_link_concept` / `is_denylisted` test outcome unchanged).
- Keep `engram-ingest` engine-neutral and free of host assumptions — it takes a `ScanFilter` value; it does not read `~/.engram` or assume a home dir.
- Put the new logic in a focused `scan_filter` module; keep `lib.rs` a facade.

### Ask first
- Changing the JSON schema shape after it ships (it becomes a user-facing contract).
- Honoring `.npmignore` or other non-`ignore`-crate formats.

### Never do
- Add a new workspace dependency (use existing `serde`/`serde_json`).
- Move denylist decisions into `engram-domain` or any core port crate (stay in `adapters/ingest`).
- Make config discovery (file reads from a home/repo path) the scanner's job — that stays in the MCP host.
- Change `classify_file` (Code/Text) or `is_secret_file` semantics — only `is_denylisted` is tunable.

## Testing Strategy

- **TDD** for the pure merge/decision logic in `scan_filter` — the contract is compressible (input name/path + config → bool). Red-green on: allowlist override, blocklist merge, min_name_length override, deny dir/ext merge, default==builtin, JSON round-trip. (Pairs with the AC outcomes below.)
- **Goal-based check** for the MCP wiring + config discovery. The discovery
  ladder itself is unit-tested at the helper layer (`resolve_scan_filter`:
  explicit arg, repo-local discovery, builtin fallback, soft-fail, arg-overrides-
  discovered). The end-to-end *application* of a merged filter through
  `ScanOptions` is proven at the ingest layer by `scan_honors_custom_deny_filter`
  (a custom deny dir skips a file the builtin would index). The thin wiring
  between the `scan_repo` handler and `resolve_scan_filter` is not covered by a
  full App-provider integration test — bootstrapping a provider is
  disproportionate for a 2-line call; the helper + ingest tests cover the
  contract in spirit.
- Existing `should_link_concept` / `is_denylisted` unit tests stay green (regression net).

## Acceptance Criteria

- [x] A `scan_filter` module exposes `ScanFilter { should_link_concept, is_denylisted }` + `ScanFilterConfig` (serde) + `builtin()` + `merge()`.
- [x] `ScanFilter::default()` / `builtin()` reproduces today's filter exactly (existing tests unchanged; `PartialEq` asserts `merge({}) == builtin()`).
- [x] `concepts.allowlist` forces a link for a name that the builtin would reject (e.g. a 5-char name or a blocklisted term).
- [x] `concepts.blocklist` adds a term that the builtin would accept (e.g. `"KafkaConsumer"`), and it is then rejected.
- [x] `concepts.min_name_length` overrides the 8-char threshold.
- [x] `deny.dirs` / `deny.extensions` cause an otherwise-indexed file to be skipped (`scan_honors_custom_deny_filter`).
- [x] `scan_repo` loads `<repo>/.engram/scan.json` when present; an explicit `scan_config` arg overrides the discovery path; absent config → builtin (`resolve_scan_filter` tests).
- [x] `cargo fmt --all`, `cargo check --workspace` (0 warnings), `cargo test -p engram-ingest -p engram-mcp`, engine-neutrality, surface-parity all green.
