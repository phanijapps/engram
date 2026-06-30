# Spec: scale-repo-ingestion (RFC 0004 Slice 1 / PHASE58)

- **Shape:** mixed (service + ui)
- **Constrained by:** RFC-0004 D4 + the Security & data-custody controls (path confinement, secret blocklist, file-size bounds); ADR-0007 (no binding change needed here — reuses `NativeIngestEngine`)
- **Contract:** none (new demo-only routes; consumes the existing ingest transport)

## Objective

The demo can **point at a folder or a repository and index it**: a backend scan job
walks the tree, applies `.gitignore` + a secret-file blocklist + per-file size bounds
and path confinement, ingests each text/code file through the existing Rust
`ingestExtract` path, and streams progress to the UI. A re-scan is **incremental**
(content-hash manifest) so only changed files are re-ingested.

All file reading and filtering lives in TypeScript (`demo/backend`); Rust is still a
library reached only through the existing `NativeIngestEngine`. No contract or Rust
change.

## Assumptions

- Technical: backend is Hono on Node ≥22 (`demo/backend`); ingest is `getIngestTransport().ingestExtract(request)` returning `IngestExtractResult` (`packages/node/src/transport.ts`). (verified)
- Technical: `DocumentIngestRequest` needs `sourceKind`, `sourceName`, `scope`, `documentKind` (code|text), `document.path`, `text`, `policy`, `actor`. (verified, `adapters/ingest/src/request.rs`)
- Technical: `.gitignore` filtering via the `ignore` npm package (standard, lightweight); Node 22 `fs.promises` + `fs.realpath` for walking + confinement. (verified — `ignore` is the conventional choice)
- Product: incremental state is a sidecar manifest (content hash per file path) next to the demo DB — durable, simple, swappable later. (author decision; conservative default)
- Process: lighter single-pass adversarial review. (user standing preference)

## Boundaries

**Always do**
- Canonicalize the root and confine every read under it (reject `..` / symlink escape).
- Apply `.gitignore` (root + per-directory) plus a builtin denylist (`.git`, `node_modules`, `target`, `dist`, `build`, `coverage`, `.fastembed_cache`, `*.db`, `*.sqlite*`, `*.node`, `*.log`).
- Skip secret-laden files (`.env*`, `*.key`, `*.pem`, `id_rsa`, `*.cert`, `*.p12`, `*.pfx`) by default.
- Enforce a per-file size bound before reading; skip oversized with a logged reason.
- Ingest only text/code files (extension allowlist); `documentKind=code` for code, `text` otherwise.
- Stream progress to the UI; make re-scan incremental via a content-hash manifest.

**Ask first**
- Changing the ingest request shape or the `NativeIngestEngine` binding.
- Indexing binaries (images, PDFs, office docs) — out of scope (text/code only).

**Never do**
- Read outside the configured root.
- Send file contents anywhere except the local Rust ingest path (no external calls in this slice).
- Persist secrets (skipped files are never read past their name/size).
- Change v1 contracts, generated types, or Rust.

## Testing Strategy

- **TDD (unit, deterministic):** the filter/decide logic — path confinement, secret blocklist, size bound, extension allowlist, `.gitignore` match — is pure and unit-tested with `vitest` (no filesystem needed for the decide functions; a small fixture tree for the walker).
- **Goal-based (build):** `pnpm --filter demo-backend typecheck` + `build`, and `demo-frontend` typecheck + build.
- **Manual QA:** point the scan panel at `demo/` itself (or the repo root), confirm progress streams, files are filtered (e.g., `node_modules`/`target`/`.env` skipped), the graph populates, and a re-scan is a no-op (incremental).

## Acceptance Criteria

- [x] A backend scan module decides per file (include/skip + reason) with: path-confinement, `.gitignore` + builtin denylist, secret blocklist, size bound, extension allowlist — each unit-tested.
- [x] `POST /ingest/scan` walks a root, streams NDJSON progress events (`{type:"progress"|"done", …}`), ingests each included file via the existing ingest transport, and ends with a summary (`{scanned, ingested, skipped, entities, relationships, errors}`).
- [x] Incremental: a content-hash manifest (sidecar) skips unchanged files on re-scan; changed/added files are (re-)ingested; persisted per-file so an aborted scan is not fully re-ingested.
- [x] No file outside the root is ever read (confinement enforced + tested, incl. symlink-escape); secret/binary/oversized files are skipped, never read past name/size.
- [x] A frontend scan panel takes a path, starts the scan, shows live progress + per-skip reasons + the final summary, then renders the accumulated 3D graph.
- [x] `pnpm --filter demo-backend typecheck` + `build` and `demo-frontend` typecheck + `build` pass; new unit tests pass.
