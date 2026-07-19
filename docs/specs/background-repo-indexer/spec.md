# Spec: background-repo-indexer (Rust rayon scan + background jobs)

- **Status:** Shipped
- **Shape:** mixed (data + service + ui)
- **Constrained by:** the Slice-1 security controls (path confinement, secret blocklist, size bound, .gitignore) — ported to Rust, not relaxed; AGENTS.md boundary (Rust owns deterministic ingest behavior); the panic fix in `extractor.rs` (prerequisite, done)
- **Contract:** none (new demo endpoints; transport `unknown`-typed)

## Objective

Indexing a large repo is **parallel, background, and tracked** instead of a single blocking request that dies if the connection drops. A Rust `RepositoryScanner` walks a root (`.gitignore`-aware via the `ignore` crate, with path confinement, secret-file blocklist, and size bound ported from `demo/backend/src/{scan,decide}.ts`), then **ingests files in parallel with rayon**, sharing the thread-safe `SqlKnowledgeStore`. The scan runs as a **background job**: `POST /ingest/jobs` starts it and returns `{jobId}` immediately; `GET /ingest/jobs/:id` polls `{status, progress, summary}`. The content-hash manifest gives incremental resume across restarts. The old blocking `POST /ingest/scan` (TS) is replaced by the Rust parallel path; the Slice-1 TS scan code is removed once the Rust path covers it.

The panic that aborted the agentzero index (multi-byte `mentions` slice) is already fixed; this slice makes large-repo indexing fast + non-blocking on top of that.

## Decision

Parallelism lives in **Rust** (`rayon` over the file list after the walk), per the user's choice — TS only kicks off the job + polls. Background-ness is **Rust-side**: the binding spawns the scan on a Rust thread and stores job state in a process-wide `Mutex<HashMap<JobId, JobState>>`; `startScanJob` returns the id immediately, `getScanJob` reads the shared state. No Node worker_threads. The walk uses the `ignore` crate (handles `.gitignore`, `.git`, hidden files). Secret detection + path confinement are reimplemented in Rust behind unit tests so the Slice-1 data-custody controls are preserved exactly.

## Assumptions

- Technical: the panic on multi-byte text is fixed (`extractor.rs` `mentions` advances by match length, not 1 byte). (verified — `37703c6` + regression test)
- Technical: `SqlKnowledgeStore` is `Arc<Mutex<Connection>>` (Send+Sync) → safe to share across rayon workers; writes serialize on the mutex. (verified — Slice 3 service.rs)
- Technical: `KnowledgeIngestor` + `GraphExtractor` are stateless (`&self` methods over a `KnowledgeRepository`) → safe to call from multiple rayon threads. (to verify in T1)
- Technical: the `ignore` crate (ripgrep) walks with `.gitignore` + hidden-file semantics; `sha2` is already a workspace dep for the manifest. (community-standard)
- Product: Rust rayon scan + poll tracking confirmed by user. (user confirmation)
- Process: lighter single-pass adversarial review. (user standing preference)

## Boundaries

**Always do**
- Preserve the Slice-1 security controls in the Rust port: canonicalize every path under the root (reject `..`/symlink escape), skip secret-laden files (`.env`, `*.key`, `*.pem`, `id_rsa`, …) by name/extension, enforce a per-file size bound, honor `.gitignore`.
- Run the scan on a background Rust thread; never block the N-API caller. Persist the manifest per file so a crash/restart resumes.
- Keep the API key + secret-blocklist logic server-side; never read past a secret file's name/size.
- Scope-filter persisted entities (the existing store already does).

**Ask first**
- Cancellation of a running job; multi-repo queuing; persistent job history across restarts.

**Never do**
- Relax the path-confinement / secret-blocklist / size-bound controls; send file contents of secret files anywhere; change Rust domain types or contracts; block the event loop on a scan.

## Testing Strategy

- **TDD (unit, Rust):** the scanner's pure helpers — path-confinement (rejects `..`/symlink escape), secret-file blocklist (`.env`/`*.key`/`id_rsa` skipped; `.env.example` allowed), size bound, classification (code/text by extension), manifest unchanged-skip. Plus an end-to-end scan over a temp fixture tree (mixed code/text/secret/oversized) asserting entity counts + that secrets were never read.
- **Goal-based (build):** `cargo fmt --all && cargo check --workspace && cargo test -p engram-ingest`; rebuild native binding; backend + frontend typecheck/build.
- **Goal-based (plumbing):** `POST /ingest/jobs` → `{jobId}`; `GET /ingest/jobs/:id` progresses → `done` with a summary.
- **Manual QA:** index a large repo (e.g. agentzero) — non-blocking, progress advances, completes without crashing; re-index skips unchanged files.

## Acceptance Criteria

- [x] A Rust `RepositoryScanner` walks a root (`.gitignore`-aware) with path confinement, secret blocklist, size bound (+ unit tests for each control).
- [x] Ingest runs in parallel via `rayon`, sharing the `SqlKnowledgeStore`; the manifest gives incremental resume.
- [x] `POST /ingest/jobs` starts a background scan → `{jobId}` (non-blocking); `GET /ingest/jobs/:id` polls `{status, progress, summary}`.
- [x] The old blocking TS `/ingest/scan` + `scan.ts`/`decide.ts`/`manifest.ts` are removed (Rust path covers them); the `/index` route uses the job API + polls.
- [x] Backend + frontend typecheck/build/test green; agentzero-scale repo indexes without crashing.
