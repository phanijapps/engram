# engram-code — loop notes

## 2026-08-23 (T2)

- **Pre-existing failure, not ours:** `cargo test -p engram-ingest --test
  contract_ingestion` fails 7/9 both **before and after** the T2 relocation
  (verified via stash). Coincides with in-flight worktree modifications to
  `adapters/sqlite/src/{belief/service.rs, memory/engine.rs, tests/service.rs}`
  from a prior session. The relocation regression net holds: all other ingest
  suites (61 + 4 + 1) pass, and the failing set is byte-identical pre/post.
  Follow-up: re-run once the sqlite in-flight work lands; if still red,
  root-cause separately (do not fix inside this spec).
- **Plan-test sharpening:** the T2 tree-sitter gate is
  `cargo tree -p engram-ingest --depth 1` (direct deps). Plain `cargo tree`
  shows tree-sitter transitively via `engram-code` — that is the design, not a
  violation. Plan/spec wording updated in the T2 commit.
- **Transitional contract home:** `ChunkCandidate` + the parsing `Chunker`
  trait moved to `engram_code::parser::chunking` with a re-export shim in
  `adapters/ingest/src/chunker.rs` so text/markdown chunkers keep one import
  path. Reshaped when the ADR-0028 `parse()`/`resolve()` API lands (T3+).

## 2026-08-23 (T1)

- **Pre-existing failure, not ours:** `pnpm run typecheck` fails in
  `prototype/frontend` (React JSX typing) both before and after T1;
  `prototype/frontend` declares no engram dependencies. Packages T1 touches
  (`packages/contracts`, `engram-cc/frontend`) typecheck clean.

## 2026-08-23 (T3)

- **Pre-existing failure, not ours:** `extractor::code_entities_carry_logical_names`
  fails at the T2 commit in a clean worktree (`Invalid column type Null …
  archived_at` on the second `extract_into` — a store-side ADR-0027 read,
  in-flight sqlite work territory). Same classification as the
  contract_ingestion set. The other three extractor tests — including the
  Phase-2-updated `cross_file_calls_resolve_after_qualification` — pass.
- `register_entities` / `resolve_call_refs` are `pub` on the extractor + lib
  whitelist (scanner-facing API; the integration test mirrors the scanner).
- Rust receiver chains required adding `impl_item → "impl"` to `rust_kinds`
  (parse_symbol already anticipated `impl` anchors). Trait impls name the
  concrete type (`type` field), so `impl Display for Foo` methods are
  `Foo::fmt`.

## 2026-08-23 (T4)

- **Root cause of the pre-existing failure cluster identified:** multi-file
  code scans share the per-source Repository entity; the second file's
  `put_entity` upsert of that shared entity fails with `Invalid column type
  Null … archived_at` (store read-back), aborting that file's whole
  persistence block. Instrumentation proved it: in a two-file scan, whichever
  file persists second fails, the first succeeds. This is the same bug behind
  the pre-existing `contract_ingestion` (7 fails) and
  `code_entities_carry_logical_names` failures — all predate engram-code and
  live in the in-flight ADR-0027 sqlite work's territory. **Blocks nothing in
  T4** (tests restructured to single-file scans); **will surface in T8**
  (multi-file cutover) unless the sqlite upsert lands first — flag for the
  human at the T8 gate.
- Structural wiring: `extract_with_calls` takes `structural:
  Option<&StructuralEdges>`; imports form File→Module entities (graph-only,
  like Repository); contains/extends/implements attach between declaration
  entities; `resolve_call_refs` generalized to `calls|extends|implements`;
  File/Module entities are excluded from symbol-table registration (import
  paths would pollute bare-name resolution).

## 2026-08-23 (T8)

- **TS-side MCP check (AC3):** `@engram/runtime` has 3 failing tests, all
  10-second boot timeouts on "real addon" tests (`mcp.smoke` + two
  `mcp.test` dispatch cases). The worktree carries in-flight uncommitted
  `packages/runtime` changes (`index.ts`, `maintenance/llm.ts`) from prior
  sessions; nothing in this spec touches the addon boot path. Suspect
  pre-existing/environmental (stale native build). **T10 gate depends on
  these** — needs resolution before the cross-server parity check ships.

## 2026-08-23 (final review, T5–T12)

Findings routed apply (5 Blockers, 6 Concerns, 5 Nits):
- **B2 — bundling accident:** a3940ad's `git add packages/runtime` swept
  two UNCOMMITTED in-flight files (llm.ts completeAgent, index.ts export)
  into the T10b commit. Unbundled in f2fa59c: both files restored to their
  pre-bundle committed content in history, in-flight changes reapplied to
  the working tree uncommitted (exactly the pre-accident state).
- **B3:** `routes_to` joined RESOLVABLE — framework handlers now resolve
  cross-file or ledger like every other reference.
- **B4:** the React fixture actually uses JSX (`onClick={handleClick}`) and
  the component→handler calls edge is asserted.
- **B1/B5:** AC7 amended to the fixture+intersection parity contract with
  the full-equality deferral anchored in docs/backlog.md; spec → Shipped.
- **C6 — import-scope rung removed:** raw import strings never
  suffix-matched real file paths (Rust `crate::x`, Python `.x`) except as
  cross-directory false positives; the rung was dead code with tests that
  passed without exercising it. Ladder is same-doc > same-repo > unique >
  ambiguous. Revisit with importer-relative path normalization if needed.
- **C8:** explore is a true BFS now (FIFO deque); the plan's "lexical seed"
  wording deviates from the implemented identifier-token seeding — recorded
  in the plan changelog.
- **C9:** framework parser skips stacked decorators/comments (no handler
  theft) and rejects inline arrow handlers instead of mis-attributing them.
- **C10:** docstring harvesting rejects preprocessor/shebang lines.
- **C11:** same-stem module resolution stays unresolved on ambiguity (no
  silent first-match pick).
- **C7:** list_unresolved_refs applies the same scope post-filter as every
  sibling list. **N12:** the sweep reuses engram-code's (now-pub)
  split/hint helpers. **N13:** subjectless references skip the ledger
  instead of collapsing onto an "unknown" id. **N14:** Rust explore clamps
  mirror the TS contract. **N16:** SQL literal spacing. **N15:** kernel-
  scale benchmark gap recorded as a backlog deferral.
