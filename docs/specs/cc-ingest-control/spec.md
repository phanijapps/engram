# Spec: engram-cc Ingestion Control

Status: **Ready to plan**
Related: [`ts-runtime-ingest`](../ts-runtime-ingest/spec.md) (shipped ingest CLI), ADR-0022

## Context

engram-cc (Control Center) can *view* the graph/memory but cannot *feed* it from the UI —
ingestion is CLI-only today. Add a **4th "Ingest" tab** to drive code (treesitter) **and**
document scans from the UI, with a job monitor and a sources/counts view. Milestone 1 of
the next batch (independent of pi-mono maintenance).

## Decisions (finalized)

- **Scope**: code (treesitter) **+ document** ingestion.
- **Progress model**: **terminal-only** — the scan runs to completion and returns final
  counts; the UI shows a *"Running — this may take a while…"* state until done. **No
  live per-file progress** in this milestone.
- **Surface**: BFF job-wrapper over the **provider-facade `scan()`** (writes to the
  agentzero store — the store the user is viewing).

## Current state (grounded)

- **Provider-facade scan** (`NativeProviderTransport.scan` → `scanRepositoryJson` →
  `scan_via_provider`, `bindings/node/src/provider.rs:1178`): treesitter-indexes a repo
  into the provider's store. **Synchronous**, returns `ScanSummary`
  `{ scanned, ingested, unchanged, skipped, entities, relationships, errors,
  git_remote/branch/sha }`. No job id, no incremental progress.
- **Chunkers**: `TreeSitterChunker` / `CodeSymbolChunker` (code) + `MarkdownChunker` /
  `PlainTextChunker` (docs). Readers: `FilesystemSourceReader`, `GitSourceReader`.
- **Counts**: `Observability::diagnostics().record_counts` has
  sources/documents/chunks/entities/relationships (`documents` + `memories` report **0 in
  v1** — degraded, not error). Reached via TS `diagnostics()`.
- **engram-cc backend** (`engram-cc/backend`): Hono BFF over `@engram/node`
  (`NativeProviderTransport`). Frontend: React + Vite, 3 tabs (Memory/Observatory/Graph).

## Design

### Backend (`engram-cc/backend/src/routes/ingest.ts`, new)
- `POST /ingest/scan` → `{ root: string, kind: "code"|"doc"|"auto", force? }` → starts the
  provider-facade `scan()`; returns `{ jobId }` immediately.
  - `kind` selects chunker: `code` → TreeSitter, `doc` → Markdown/Plain, `auto` → detect
    by extension.
  - Scope: agentzero tenant/workspace (the BFF scope seam).
- `GET /ingest/jobs/:jobId` → `{ status: "running"|"done"|"error", summary?: ScanSummary,
  error?: string }` (polled by the frontend).
- `GET /ingest/counts` → `record_counts` subset (sources, documents, chunks, entities,
  relationships) via `diagnostics()`.
- `GET /ingest/sources` → paged source list (observability gives counts only; a
  sources-list path is added on the facade — small surface extension, see Plan).
- **Threading**: `scan()` is a **blocking** N-API call. To return `{ jobId }` at once and
  show the "this will take a while" state, run it in a Node **`worker_thread`** (load
  `@engram/node` in the worker). Jobs live in an in-process `Map` (single BFF process;
  lost on restart — acceptable for a dev Control Center). *Plan decides*: worker_thread
  (live-ish state) vs a long POST the client awaits with a "please wait" message. Default:
  worker_thread.

### Frontend (`engram-cc/frontend/src/features/ingest/`, new 4th tab)
- `IngestTab.tsx` — `IngestForm` (root path input + `code`/`doc`/`auto` toggle + Start),
  `JobMonitor` (polls `/ingest/jobs/:id`; shows *"Running — this may take a while…"* then
  the final `ScanSummary` counts), `SourcesPanel` (counts + source list).
- `api.ts` — `startScan`, `getScanJob`, `ingestCounts`, `listSources`.
- `App.tsx` — add **"Ingest"** to nav (4th tab) + route.

## Acceptance criteria

1. User enters a root path + kind (code/doc/auto), starts a scan; the UI shows a running
   state with a *"this may take a while"* message.
2. On completion, the `ScanSummary` counts (scanned/ingested/entities/relationships) display.
3. Graph-tab entity/source/chunk counts rise and new entities appear in the agentzero scope.
4. Sources panel lists ingested sources.
5. Honest empty / loading / **error** states (a failed scan shows the error, not a silent hang).
6. Scope respected — ingest lands in the viewed (agentzero) store; **no new `node:sqlite`**
   in the BFF for the scan path (uses the facade `scan()`).

## Verification

- `engram-cc-backend` typecheck + route tests (`/ingest/scan` starts a job, `/ingest/jobs/:id`
  returns state, `/ingest/counts`).
- Manual: ingest a small repo from the UI; confirm Graph-tab counts rise + entities appear.
