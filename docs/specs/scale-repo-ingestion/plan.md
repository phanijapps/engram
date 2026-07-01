# Plan: scale-repo-ingestion (RFC 0004 Slice 1 / PHASE58)

Follows RFC-0004 D4 + Security controls. TS scan job in `demo/backend`; Rust untouched.

## Tasks

### T1 — Pure decide functions (`decide.ts`) + unit tests
- **Tests (TDD, written first):** `demo/backend/src/decide.test.ts` — confinement (`isWithinRoot`), secret blocklist (`isSecretFile`), size bound (`isOverSize`), extension allowlist + code/text classification (`classifyFile`), builtin denylist (`isDenylisted`). Each pure function against fixtures.
- **Depends on:** none
- **Verification mode:** TDD (`vitest`)
- **Approach:** New `demo/backend/src/decide.ts`. No fs. `isWithinRoot(target, root)` uses `path.relative` + a leading-`..`/absolute check. `isSecretFile` matches `.env*`, `*.key`, `*.pem`, `id_rsa`, `*.cert`, `*.p12`, `*.pfx`. `classifyFile(name)` → `{ include, kind: "code"|"text"|"skip" }` from an extension map + a denylist.

### T2 — Walker (`scan.ts`) applying decisions + `.gitignore`
- **Tests:** fixture-tree test (`demo/backend/src/scan.test.ts`) walks a `tmp` tree, asserts included/skipped sets (node_modules skipped, .env skipped, oversized skipped, nested .gitignore respected).
- **Depends on:** T1
- **Verification mode:** TDD + goal-based
- **Approach:** Async generator `walk(root, opts)` over `fs.promises.readdir` (withFileTypes). Maintain an `ignore` instance, `.add()` per-directory `.gitignore`. For each file: realpath-confine, then decide (denylist/secret/size/classify). Yield `ScanFile { absPath, relPath, size, kind, skipped?, reason? }`. Never read contents in the walker — only `stat`.

### T3 — `/ingest/scan` route (streaming NDJSON + ingest)
- **Tests:** goal-based — typecheck + build; manual QA against `demo/`.
- **Depends on:** T2
- **Verification mode:** goal-based + manual QA
- **Approach:** New route in `app.ts` (`POST /ingest/scan`, body `{ path, scope?, policy? }`). Returns `text/plain` NDJSON stream. Per included file: read text (utf8), call `getIngestTransport().ingestExtract(request)`, emit `{type:"progress", index, total, file, entities, relationships}`; accumulate. Skips emit `{type:"skip", file, reason}`. End with `{type:"done", summary}`. Reuses the panel's SCOPE/POLICY/actor defaults.

### T4 — Incremental manifest (content-hash sidecar)
- **Tests:** unit test manifest get/set/skip-unchanged (`manifest.test.ts`).
- **Depends on:** T3
- **Verification mode:** TDD
- **Approach:** `demo/backend/src/manifest.ts` — JSON sidecar at `${ENGRAM_DB}.manifest.json` (or `demo-scan.manifest.json`): `{ [relPath]: sha256(content) }`. `loadManifest()`/`saveManifest()`. In the route, before ingesting a file, if `manifest[relPath] === currentHash` → skip with reason `"unchanged"` (still counted as scanned). Persist after each successful ingest (or at done).

### T5 — Frontend `ScanPanel`
- **Tests:** goal-based — `demo-frontend` typecheck + build.
- **Depends on:** T3
- **Verification mode:** goal-based + manual QA
- **Approach:** New `demo/frontend/src/ScanPanel.tsx`: path input + "Index" button. `fetch("/ingest/scan", {method:POST, body})` then read the NDJSON `ReadableStream` line-by-line; render live progress (current file, counts) + skip reasons + final summary. On done, surface a "X entities / Y edges indexed" line. Wire into `App.tsx` above/beside `IngestPanel`.

### T6 — Validate + lighter adversarial pass
- **Tests:** `pnpm --filter demo-backend typecheck && build && test`; `pnpm --filter demo-frontend typecheck && build`; single-pass adversarial review.
- **Depends on:** T5
- **Verification mode:** goal-based + manual QA

## Out of scope (logged)
- Binary/PDF/office ingestion; LLM extraction (Slice 2); provenance/confidence viz (Slice 4); the durable belief/hierarchy work (Slice 5).
