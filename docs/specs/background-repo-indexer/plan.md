# Plan: background-repo-indexer (Rust rayon scan + background jobs)

Rust parallel repo scanner + background job system + poll API + job UI. Replaces
the blocking TS scan. Big feature; phased across commits.

## Tasks

### T1 — Rust `RepositoryScanner` (walk + filter + rayon ingest) [Commit A]
- **Tests (TDD):** `adapters/ingest/tests/scanner.rs` — path confinement (rejects `..` + symlink escape), secret blocklist (`.env`/`*.key`/`id_rsa` skipped; `.env.example` allowed), size bound, classification, end-to-end scan over a temp fixture (code/text/secret/oversized) asserting entity counts + secrets never read.
- **Depends on:** none
- **Approach:** Add `ignore` + `rayon` workspace deps. New module `adapters/ingest/src/scanner.rs`: `RepositoryScanner::scan(root, opts, store, progress_fn) -> ScanSummary` where opts = {scope, policy, actor, max_bytes, secret patterns}. Walk via `ignore::WalkBuilder` (.gitignore + hidden + .git aware). For each entry: canonicalize + assert within root (path confinement), secret-name/extension blocklist (port `decide.ts` SECRET_FILE_RE/DENY), size bound, classify code/text by extension (port `classifyFile`). Collect the readable files, then `rayon::par_iter` over them: read text (UTF-8 lossy), `KnowledgeIngestor::ingest` + `GraphExtractor::extract_into` into the shared store, content-hash manifest (skip unchanged). `progress_fn(file, kind)` per file. Return summary {scanned, ingested, unchanged, skipped, entities, relationships, errors}. Pure helpers (confinement/secret/classify) are `pub(crate)` + unit-tested.

### T2 — Rust job system + binding [Commit B]
- **Tests:** goal-based — cargo check/test; rebuild native.
- **Depends on:** T1
- **Approach:** Process-wide job state in the native module: `Lazy<Mutex<HashMap<String, ScanJobState>>>` (ScanJobState = {status: Queued|Running|Done|Error, progress counts, current file, summary, error}). `scan_repository_start(root, opts, store) -> JobId` clones the store + opts, `std::thread::spawn`s the scanner (updating the shared state), returns the id immediately. `scan_repository_status(job_id) -> ScanJobState` reads the map. Binding: `startScanJobJson(request) -> jobId`, `getScanJobJson({jobId}) -> state`.

### T3 — Backend `/ingest/jobs` routes [Commit C]
- **Tests:** goal-based — typecheck; curl smoke (start → poll → done).
- **Depends on:** T2
- **Approach:** `demo/backend/src/app.ts`: `POST /ingest/jobs { root, scope?, policy?, maxBytes? }` → `startScanJob` → `{jobId}`; `GET /ingest/jobs/:id` → `getScanJob` → state. (Hono `c.req.param("id")`.) Keep `/ingest/scan` until T5 removes it.

### T4 — `/index` route uses the job API + polls [Commit D]
- **Tests:** goal-based — frontend typecheck/build; manual QA on a large repo.
- **Depends on:** T3
- **Approach:** `src/routes/repo-index.tsx`: replace the NDJSON streaming fetch with `POST /ingest/jobs` → store `jobId` → `setInterval` poll `GET /ingest/jobs/:id` every ~1.2s → render status/progress/summary + the accumulated Graph3D. Stop polling when status is Done/Error. Keep the LLM-enhance note (Rust scan is deterministic; enhance stays on /ingest for now).

### T5 — Remove the old blocking TS scan + validate [Commit E]
- **Tests:** `cargo fmt/check/test --workspace`; rebuild native; backend + frontend typecheck/build; agentzero-scale smoke; single-pass review focused on the ported security controls + job-state thread-safety.
- **Depends on:** T4
- **Approach:** Delete `demo/backend/src/{scan,decide,manifest}.ts` + `POST /ingest/scan` + their tests once the Rust path covers them. Gate: `git grep` shows no `/ingest/scan` callers. Full validation suite.

## Out of scope (logged)
- Job cancellation; multi-repo queuing; persistent job history across restarts; LLM-enhance during the parallel scan (stays per-doc on /ingest); server-side clustering.
