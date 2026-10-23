# Spec: knowledge change log, scan checkpoints, and staleness

**Status:** accepted (slice 3A + 3B + the subscribe/staleness part of 3D of
the wicked-estate top-3 adoption). The filesystem watcher (plan slice 3C)
and `ingest_watch` capability states are explicitly NOT in this slice.

## Problem

Long-lived agents had no way to know what changed since they last looked
(short of rescanning) and no truthful freshness signal on code-graph
answers — a graph from an old commit was indistinguishable from a current
one.

## Accepted design (this slice)

1. **Persisted change log** in the knowledge adapter: entity/relationship
   upserts (no-op rule: identical re-puts log nothing), entity/relationship
   removals. Deletion events record the target id + scope only — never the
   deleted payload. Sequences are table rowids serialized as decimal
   strings; cursors are opaque; `resetRequired` replaces silent resets.
2. **Ports without an append forge**: `KnowledgeChangeLog` (read) and
   `ScanCheckpointStore` (write, scan-coordinator-only) in `engram-knowledge`.
   No `append_change` port exists — mutation logging is the adapter's
   obligation inside its existing operations.
3. **Scan checkpoints** written by the MCP scan tool after each scan (full
   OID, partial-vs-completed status, dirty flag, canonical scan root).
   Stale runs never overwrite newer ones.
4. **Truthful staleness** (`derive_staleness`, pure, in `engram-domain`):
   unindexed / current / behind / diverged / unknown / partial. Unknown git
   facts stay unknown; dirty worktrees are never `current`; diverged
   histories (rebase/reset/shallow) are never a guessed zero-behind.
5. **Surfaces (parity complete)**: MCP `subscribe_changes` + `code_staleness`
   tools; the `scan` tool records checkpoints; `symbol_context` responses
   carry a visible `STALENESS:` line (R5). Rust facade
   `require_change_log()` / `require_checkpoints()`; N-API
   `requireChangeLogApi` / `requireCheckpointsApi`; TS bindings.

## Deferred (recorded honestly)

- The notify-based filesystem watcher (3C): watch/subscribe lifecycle,
  backpressure, initial-scan race. The durable cursor + checkpoint +
  staleness layers it builds on are complete.
- `ingest_watch` / `knowledge_changes` / `code_graph_staleness`
  CapabilityReport states (the capabilities surface via
  `ProviderUnavailable` until then).
- Archive/restore/merge instrumentation (maintenance paths) and
  document/chunk-level events: scan-driven upserts/removals are covered.
- Nothing transport-side: N-API proxies (`requireChangeLogApi`,
  `requireCheckpointsApi`) and TS bindings ship WITH this slice — parity
  lint green at 24 capabilities.

## Acceptance criteria (verified)

- [x] No-op rule: identical re-puts log nothing; changed re-puts log once.
- [x] Resumable paging with opaque cursor + hasMore + highWatermark.
- [x] Removals log `remove` events (target id + scope only).
- [x] Stale scan runs never overwrite newer checkpoints.
- [x] Staleness matrix: unindexed/current(+dirty)/behind/diverged/unknown/
      partial — no fabricated zeros.
