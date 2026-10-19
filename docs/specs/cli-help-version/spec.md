# Spec: CLI help and version flags

- **Status:** Draft <!-- Draft | Approved | Implementing | Shipped | Archived -->
- **Owner:** phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** none (the human-facing argument contract lives in `mcp/engram-mcp/HOW-TO.md` § "Launch + runtime arguments"; this spec keeps it true)
- **Brief:** none
- **Contract:** none — the CLI flag surface is documented in HOW-TO.md (the argument contract), which this spec touches but does not move under `contracts/`
- **Shape:** service

> **Spec contract:** this document defines what "done" means. The implementing
> PR must match this spec, or update it. Verification must be derivable from it.

## Objective

`engram-mcp --help` (and `-h`) prints the full launch-argument contract —
all 13 flags with their defaults, the one-shot subcommands, and the scope
rule — then exits 0. `engram-mcp --version` (and `-V`) prints the binary's
self-reported version (the release tag it was built from, falling back to
the Cargo workspace version for local builds) as a single line, then exits
0. The one-shot subcommands accept `--help` too (`engram-mcp recall --help`)
and print their own usage. Users debugging installs (the "what version is
this binary?" class, e.g. the Windows lockstep incident) and scripts wrapping
the binary get unambiguous, machine-checkable answers; genuine usage errors
remain exit 2 with usage on stderr, distinguishable from help by exit code.

## Boundaries

### Always do

- Read the version from the single `ENGRAM_VERSION` constant (`main.rs`) — no new version sources.
- Print help to **stdout**, exit 0; keep usage errors on **stderr**, exit 2 — the existing convention, now actually distinguishable.
- Keep `--help`/`--version` handling **before** MCP server startup so stdout stays JSON-RPC-only once the server runs.
- Update the HOW-TO argument contract in the same change if the flag surface it lists changes.

### Ask first

- Any new flag beyond `--help`/`-h` and `--version`/`-V`.
- Changing existing exit codes (0/2) or moving usage errors off stderr.
- Machine-readable help output (`--help --json` or similar).

### Never do

- No new dependencies (no `clap`, no help框架 — the parser stays hand-rolled).
- No new modules — the dispatch lives in `main.rs` next to `ENGRAM_VERSION`; at most a `usage()` helper function in the same file.
- Do not duplicate the argument table in code: `--help` output is generated from (or kept in lockstep by test with) the `launch_flag_surface_is_pinned` test's flag set and the HOW-TO contract table.
- No version computation at runtime (no git calls, no file reads) — the constant is compile-time only.

## Testing Strategy

Goal-based (each Objective outcome gets a check; TDD where a unit test is natural):

- `--help`/`-h` exit 0 and stdout contains every contract flag name — **unit test** (stdout capture is awkward in-crate; assert the usage string constant contains all 13 flags + both subcommands) plus **live check** in the eval battery (spawn the built binary, assert exit code + flag names in stdout).
- `--version`/`-V` exit 0 and stdout equals `ENGRAM_VERSION` + newline — **TDD unit test** (pure function of the constant) plus live spawn check.
- One-shot `recall --help` / `status --help` exit 0 with their usage — unit test in `oneshot.rs`.
- Usage errors still exit 2, stderr, and are distinguishable from help — pin with a test that asserts the two paths' exit codes differ.
- Server path unpolluted: `initialize` handshake JSON-RPC on stdout still parses after the flags exist — existing server test suite is the regression net; a live probe asserts stdout is valid JSON when the server runs.

## Acceptance Criteria

- [ ] `engram-mcp --help` exits 0; stdout lists all 13 launch flags, the one-shot subcommands, and points to the HOW-TO contract
- [ ] `engram-mcp -h` behaves identically
- [ ] `engram-mcp --version` exits 0; stdout is exactly `<ENGRAM_VERSION>\n`
- [ ] `engram-mcp -V` behaves identically
- [ ] `engram-mcp recall --help` and `engram-mcp status --help` exit 0 with per-subcommand usage
- [ ] `engram-mcp --bogus-flag` still exits 2 with usage on stderr (help and errors distinguishable by exit code alone)
- [ ] A tagged CI build's `--version` output equals the tag (v-stripped) — verified by the release workflow or a stamped local build
- [ ] All checks green: `cargo test -p engram-mcp` (0 FAILED across binaries), docs gate, launcher evals unchanged-green

## Assumptions

- Technical: `ENGRAM_VERSION` exists at `main.rs:52` as `option_env!(ENGRAM_BUILD_VERSION) || CARGO_PKG_VERSION` — the single compile-time version source (mcp/engram-mcp/src/main.rs:52, shipped in 69572b8).
- Technical: unknown args (including today's `--help`/`--version`) exit 2 with usage on stderr — the convention to extend, not invent (probe: `engram-mcp --bogus-flag` → exit 2; config.rs:124).
- Technical: one-shot subcommands already exit 2 on usage errors (probe: `recall` without `--query` → exit 2).
- Technical: flag parsing precedes `server::run`; stdout stays JSON-RPC-only once serving (main.rs dispatch order 63→115).
- Product: short forms `-h`/`-V` are included (user confirmation 2026-10-19).
- Product: `--help` prints the full contract table, not the one-liner (user confirmation 2026-10-19).
- Product: `--help` applies to one-shot subcommands as well (user confirmation 2026-10-19).
- Product: scope is the CLI flag surface only — version constant + CI stamping already shipped (user confirmation 2026-10-19).
- Process: specs live at `docs/specs/<feature>/` as spec.md + plan.md; docs gate covers the tree green (existing specs tree; `scripts/ci/check-docs.sh`).
