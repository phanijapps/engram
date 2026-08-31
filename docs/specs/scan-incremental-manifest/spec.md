# Scan Incremental Manifest (MCP)

- **Status:** Shipped <!-- Draft | Implementing | Shipped | Deferred -->
- **Spec:** scan-incremental-manifest
- **Area:** `mcp/engram-mcp` (scan_repo)
- **Depends on:** scan-reliability (Shipped)

## Problem

The MCP `scan_repo` handler passed an empty manifest into `scan_repository` on
every call, so every scan re-ingested every file — even with zero changes
(~18s of pure waste on this repository, and the difference between fitting
inside and blowing the 60s MCP request ceiling on populated stores). The
N-API binding already solved this with a caller-supplied `manifestPath`
JSON file; the MCP path dropped the returned manifest on the floor.

## Acceptance Criteria

- [x] AC1 — `scan_repo` persists each root's manifest under
  `<storage>/scan-manifests/<hash>.json` (hash of the canonicalized root path)
  and loads it as the prior manifest on the next scan of the same root, so
  unchanged files are skipped (`summary.unchanged > 0`, `ingested == 0`).
- [x] AC2 — A missing or corrupt manifest file degrades to a full scan, never
  an error; manifest writes are atomic (tmp + rename) and non-fatal on failure.
- [x] AC3 — `force: true` bypasses the prior manifest (full re-ingest) while
  still persisting the fresh manifest; the tool description documents it.
- [x] AC4 — Live verification: consecutive scans of this repository drop from
  ~44–53s (full) to 18.1s (`unchanged: 933, ingested: 0`).

## Non-goals

- Moving the manifest into the store behind a port (needed only when multiple
  hosts scan the same store; today every host with scan access owns a local
  storage dir).
- Draining the embed backlog (the 256/call cap still applies; a first-class
  reindex op is the follow-up — see backlog).

## Assumptions

- One manifest per canonical root is the right identity: `/repo` and `/repo/`
  and symlinked spellings that canonicalize to the same target share it.
- A branch/commit switch changes file hashes only for changed files; the
  manifest naturally re-ingests just those (scanner contract).
