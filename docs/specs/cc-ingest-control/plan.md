# Plan: engram-cc Ingestion Control

Spec: [`spec.md`](./spec.md) · Status: **Ready to implement** · Milestone 1 of the batch.

## Approach

A 4th **"Ingest"** tab that starts a code/doc scan of a path and shows terminal progress
(*"this may take a while…"*) + final counts. The scan runs in a **separate child process**
— not in-process, not a worker_thread — because the sync facade's `block_on` panics under a
nested runtime (cited: `docs/guides/how-to/extend-storage.md:213`,
`docs/guides/how-to/build-a-surrealdb-store.md:123`). This matches the repo's established
pattern (`engram-ingest`, `engram-mcp` are each their own process).

The BFF reuses the shipped **`engram-ingest`** CLI for the scan itself: add a `--json` mode
(emits the `ScanSummary` as one JSON line on stdout), then `/ingest/scan` spawns it with the
BFF's agentzero config and tracks the child as a job. **No new `node:sqlite`**; the scan
writes to the agentzero store via the facade (the store the user is viewing).

**Scope guardrails** (keep M1 TS/UI-only — no Rust):
- Counts come from `diagnostics()` (already on the facade) — **no new port**.
- The **paged source list** is **deferred** (needs a `listSources` facade port → Rust).
  M1 ships a **CountsPanel** (sources/chunks/entities/relationships; `documents` is honestly
  `0` in v1). The list is a noted follow-up (fold into the read-facade migration).
- **Code+doc**: `scan()` walks a path and chunks per file (treesitter for code; markdown/plain
  for docs). **Verify** it handles mixed content; `kind` is then a client-side filter
  (`code`/`doc`/`auto`). If `scan()` is code-only, T-doc adds a doc-chunker path (the one task
  that may need a small facade touch — flagged).

## Task breakdown

### T1 — `engram-ingest --json` (reusable scan subprocess)
**Files**: `packages/runtime/src/ingest/cli.ts` (`parseIngestArgs` + `runIngest`).
- Add a `--json` flag. When set, after the one-shot scan, print the `ScanSummary` as a single
  JSON line on **stdout** (no other stdout noise — route progress/logs to stderr). Exit 0 on
  success, non-zero + a JSON `{ error }` line on failure.
- Keep the existing human-log behavior when `--json` is absent (back-compat).
- **Acc**: `runIngest({…, json:true})` resolves to the summary and the bin prints one JSON line;
  `pnpm --filter @engram/runtime test` stays green; rebuild dist.

### T2 — BFF `/ingest/*` routes
**Files**: `engram-cc/backend/src/routes/ingest.ts` (new), wire into `src/index.ts`.
- `POST /ingest/scan` → `{ root, kind, force? }`. Builds the agentzero config JSON (reuse
  `engram/provider.ts::buildConfigJson`), spawns
  `node packages/runtime/dist/ingest/bin.js --config <json> --path <root> --tenant <t>
  --workspace <ws> --json` as a child, stores `{ jobId, child, status:"running", startedAt }`
  in an in-process `Map`, returns `{ jobId }` immediately.
  - `kind` validation: `code`|`doc`|`auto` (422 otherwise). Passed through as a scan filter /
    client hint (see T-doc note).
- `GET /ingest/jobs/:jobId` → `{ status: "running"|"done"|"error", summary?, error? }`.
  `done` = child exited 0 + parsed JSON line; `error` = non-zero exit / no JSON / stderr.
- `GET /ingest/counts` → `record_counts` subset via `getProvider(cfg).diagnostics()`.
- Job map is in-process (single BFF; lost on restart — acceptable for a dev Control Center).
  Job ids are opaque (`job-<n>`).
- **Acc**: route tests — scan starts a job (use a tiny fixture repo + a stubbed/mocked child or
  a `PI/INGEST_DRY` path), `/ingest/jobs/:id` transitions running→done, `/ingest/counts` returns
  the subset. No `node:sqlite` on the scan path.

### T3 — Frontend "Ingest" tab (4th)
**Files**: `engram-cc/frontend/src/features/ingest/IngestTab.tsx` (+ `IngestForm`,
`JobMonitor`, `CountsPanel`), `src/lib/api.ts` (`startScan`, `getScanJob`, `ingestCounts`),
`src/App.tsx` (nav + route).
- `IngestForm`: root-path input + `code`/`doc`/`auto` toggle + **Start**. Client validation
  (non-empty path).
- `JobMonitor`: polls `GET /ingest/jobs/:id` (~1s) while `running`; shows
  *"Running — this may take a while…"*; on `done` shows `ScanSummary` counts
  (scanned/ingested/unchanged/skipped/entities/relationships/errors); on `error` shows the
  message. Stop polling on `done`/`error`.
- `CountsPanel`: live `record_counts` (sources/chunks/entities/relationships; `documents`
  shown as an honest `—`/`0` empty-state).
- **Acc**: tab renders; start → monitor → done/error states; Playwright E2E (start a scan of a
  fixture path, assert the done counts render). Honest empty/error states.

### T4 — Verify scan lands in agentzero + Graph reflects it
- Manual/CI: from the UI, ingest a small path; confirm the **Graph** tab's entity/source/chunk
  counts rise and new entities appear in the agentzero scope (the same store the BFF reads).
- **Acc**: post-ingest, `/api/graph/stats` counts increase and the Graph tab shows new data.

### T-doc — Document ingest (code+doc completeness)
- **Verify** `scan()` chunks mixed content per file (treesitter for code, markdown/plain for
  `.md`/`.txt`). If yes → `kind` is a pure client/filter; done.
- If `scan()` is **code-only** → add a doc-chunker path (point at a path, chunk with
  `MarkdownChunker`/`PlainTextChunker`, ingest as sources/documents/chunks). Prefer routing
  through the facade; if that needs a small port, **flag it** (may push doc to a fast follow-up
  rather than block M1).
- **Acc**: a `.md`/`.txt` path ingests as documents/chunks (or doc is an explicitly-deferred
  follow-up with a note in the UI).

## Gate sequence (per work-loop)

1. `pnpm --filter @engram/runtime run build` (T1 dist) → `pnpm --filter @engram/runtime test`.
2. `pnpm --filter engram-cc-backend run typecheck` + `test` (T2 routes).
3. `pnpm --filter engram-cc-frontend run typecheck` + e2e (T3).
4. Manual ingest round-trip → Graph-tab counts rise (T4).
5. Review pass (light — this is TS/UI + a small CLI flag; no Rust, no contracts).

## Risks / caveats

- **Nested-executor panic** → mitigated by the **child-process** scan (not worker_thread).
- **`scan()` code-vs-doc** → T-doc verifies; doc may need a small path/port (flagged, may defer).
- **Source list deferred** → M1 ships counts only (no `listSources` port = no Rust in M1).
- **Long scans block the child, not the BFF** → the BFF stays responsive (separate process);
  the job map is in-process (lost on BFF restart — acceptable for a dev tool).
- **Scope** → all ingest lands in the agentzero tenant/workspace (the BFF scope seam); the
  browser never speaks engram-mcp (ADR-0022).
