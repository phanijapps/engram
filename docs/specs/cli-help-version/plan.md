# Plan: CLI help and version flags

- **Spec:** [`spec.md`](spec.md)
- **Status:** Drafting <!-- Drafting | Executing | Executing | Done -->

> **Plan contract:** this is the implementation strategy. Unlike the spec, this
> document is allowed to change as you learn. When it changes substantially
> (a different approach, not just a re-ordering), note why in the changelog
> at the bottom.

## Approach

Pure dispatch work in `main.rs`, ahead of every existing path. `--help`/`-h`
and `--version`/`-V` are intercepted before the one-shot check and before
config parsing, print to stdout, and `exit(0)`. The help text is a `const`
string in `main.rs` (generated from the contract table — kept in lockstep by
a test asserting every flag in `launch_flag_surface_is_pinned`'s set appears
in it). One-shot subcommands grow a `--help` arm in `oneshot.rs` that prints
per-subcommand usage. Nothing else moves: no parser rewrite, no dependency,
no new module. Riskiest part is nothing functional — it's drift: the help
const must not fork from the HOW-TO table, which the lockstep test pins.

## Constraints

- `ENGRAM_VERSION` is the only version source (`main.rs:52`; tag-stamped in CI via `ENGRAM_BUILD_VERSION`).
- Hand-rolled parser stays (AGENTS.md: no new deps without decision; spec Never-do).
- Exit-code convention: 0 = help/version success, 2 = usage error, stderr.
- HOW-TO argument contract is the human-facing source of truth — updated in the same change.

## Construction tests

Cross-cutting: the **lockstep test** — the help text contains every flag the
config parser accepts (assert against the same flag list the surface pin
uses, or parse it from the test's own table so both derive from one constant
in the test file). Live spawns (exit codes + stdout/stderr split) run in the
existing launcher eval battery as one new case each.

## Tasks

### Task 1 — `--version` / `-V`

Add the dispatch in `main.rs` before `oneshot::is_oneshot`: print
`{ENGRAM_VERSION}\n` to stdout, `exit(0)`.

Tests: unit — a `version_line()` helper returns `format!("{ENGRAM_VERSION}")`;
live — `engram-mcp --version` exits 0, stdout exactly the version line.

### Task 2 — `--help` / `-h` with the contract table

Add `const HELP_TEXT: &str` in `main.rs` — the 13-flag table (flag, effect,
default), one-shot usage, scope rule one-liner, HOW-TO pointer. Dispatch
prints it to stdout, `exit(0)`.

Tests: lockstep unit test (every flag in the pin test's argv list appears in
`HELP_TEXT`; both subcommand names appear); live — exit 0, contains
`--pg-connection-string` (the easiest-to-forget flag).

### Task 3 — one-shot `--help`

In `oneshot.rs`: `run_recall`/`run_status` treat a `--help` arg as
print-usage + exit 0 (before `--query` validation, so it works without
`--storage`).

Tests: unit — `recall --help` exits 0 without storage; usage-error path
still exits 2.

### Task 4 — docs + battery

HOW-TO contract table gains the `--help`/`--version` rows. Launcher eval
battery gains the live flag checks (spawn the dev binary via
`ENGRAM_MCP_BIN`).

Tests: docs gate; `pnpm --filter @engramlabs/mcp test` green.

## Risks

- **Help-text drift** from HOW-TO/config — mitigated by the lockstep test (fails the build when a flag exists in one and not the other).
- **stdout pollution** breaking MCP hosts — impossible by ordering (flags exit before the server loop); the existing server tests + a live handshake probe are the net.

## Changelog

- 2026-10-19: drafted on branch `versioning` after assumption sign-off; approach is pure `main.rs`/`oneshot.rs` dispatch + a drift-pinning test.
