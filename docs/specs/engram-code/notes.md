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
