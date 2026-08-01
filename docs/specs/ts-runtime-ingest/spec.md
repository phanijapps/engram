# Spec: TS runtime layer + ingest module

- **Status:** Draft <!-- Draft | Implementing | Shipped | Deferred -->
- **Owner:** phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** RFC-0017 (Phase C / Module 1, Accepted), ADR-0022, the Phase A facade (`ts-provider-facade`, shipped)
- **Brief:** none
- **Contract:** none — composes the Phase A `NativeProviderTransport`; no new Rust surface
- **Shape:** service

> **Spec contract:** this document defines what "done" means. The implementing
> PR must match this spec, or update it. Verification must be derivable from it.

## Objective

A TypeScript operational layer (`@engram/runtime`) whose first module is
**`engram-ingest`** — a CLI that scans a repository into the held provider over
the Phase A facade, on a schedule (one-shot by default; periodic via an
`--every` interval). This is the first deployable TS module of RFC-0017's
three-mode target (ingest / HTTP-MCP / maintenance), proving the "TS entry point
over one held N-API provider" pattern and giving the user the ingestion path
they framed this work around ("consolidate ingestion and streaming"). Cron-first;
queue/webhook transport adapters are a later slice.

## Boundaries

The three-tier guard that keeps an implementing agent inside the lines.

### Always do

- Route every operation through `createNativeProviderTransport` (the held
  provider) from `@engram/node` — never the flat engines.
- Keep scheduling in node built-ins (`setInterval`) for this slice — no scheduler
  dependency.
- Share config + scope construction across modules (the package is the home for
  all three TS modules; each lives in its own focused sub-directory).

### Ask first

- Adding a runtime dependency (a cron library, a queue client, an HTTP framework).
- Adding a second module (maintenance / HTTP-MCP) to the package.

### Never do

- Bypass the facade (call the flat `NativeKnowledgeEngine` or open the provider
  outside the facade).
- Put a scheduler, queue consumer, HTTP server, or LLM call in the Rust core.
- Make `@engram/runtime` a god-package — each module in its own sub-directory
  (`src/ingest/`, later `src/maintenance/`, `src/mcp/`); shared helpers in
  `src/shared/`.
- Re-implement domain logic in TypeScript — it is orchestration over Rust.

## Testing Strategy

- **TDD** — the ingest scheduling + dispatch logic (one-shot vs periodic;
  argv → config → facade.scan) is pure logic over an injected mock transport,
  verified by `vitest`.
- **Goal-based check** — `pnpm --filter @engram/runtime typecheck` + `build`;
  the `engram-ingest` bin resolves.
- **Visual / manual QA** — run `engram-ingest` against a fixture repo + the real
  built addon (sqlite), recording the observed scan summary (entities written).

## Acceptance Criteria

- [ ] A `packages/runtime` package (`@engram/runtime`) exists, workspace-wired,
  with an `engram-ingest` bin + `typecheck`/`build`/`test` scripts.
- [ ] `engram-ingest --config <json|path> --path <repo>` scans the repo over the
  facade (one-shot) and the scanned entities land in the knowledge store (manual
  QA smoke).
- [ ] `engram-ingest ... --every <duration>` runs the scan on a `setInterval`
  schedule (TDD: the scheduling logic is unit-tested with a mock transport).
- [ ] Config + scope construction is shared (`src/shared/`), not inlined in the
  ingest module.
- [ ] `pnpm run typecheck` (recursive) + `pnpm --filter @engram/runtime test` are
  green.

## Assumptions

- Technical: the Phase A facade `createNativeProviderTransport` (+ `scan`,
  `capabilities`) is on `main` / shipped in `@engram/node` (PR #85 merged).
  (source: packages/node/src/provider.ts on main)
- Technical: `pnpm-workspace.yaml` globs `packages/*`, so a new package is
  auto-included; packages are ESM, tsup + vitest + tsc, deps via `workspace:*`.
  (source: pnpm-workspace.yaml, packages/client/package.json)
- Technical: scan takes `{ path, scope, scanFilter? }` and returns a `ScanSummary`
  JSON (`scanned`/`entities`/…). (source: packages/node/src/provider.ts:42)
- Process: full-mode work-loop (new package = structural change); RFC-0017 Phase C
  + the Phase A facade govern. (source: docs/rfcs/0017, AGENTS.md)
