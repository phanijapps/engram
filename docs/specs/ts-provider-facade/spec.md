# Spec: TypeScript provider facade over NativeProvider

- **Status:** Shipped <!-- Draft | Implementing | Shipped | Deferred -->
- **Owner:** phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** RFC-0017 (Phase A), ADR-0022 (engine neutrality + surface parity), RFC-0015 D4 (LLM stays agent-side)
- **Brief:** none
- **Contract:** none — the facade is typed from `bindings/node/src/provider.rs`; no `contracts/` artifact
- **Shape:** integration

> **Spec contract:** this document defines what "done" means. The implementing
> PR must match this spec, or update it. Verification must be derivable from it.

## Objective

TypeScript consumers — the future ingest, HTTP-MCP, and maintenance modules —
drive the full Engram provider pattern (recall, write, scan, consolidate, graph,
beliefs) over the Rust core through one held, engine-routed provider. The held
`NativeProvider` already exists in Rust and passes the ADR-0022 surface-parity
gate (20 capabilities, 0 acknowledged debt), but no TypeScript reaches it: the
hand-written `@engram/node` binding surface exposes only the flat per-family
engines (each re-opens storage), and `scan` is composed inside the MCP rather
than reachable as a first-class operation. This spec closes that gap so each
operational module composes one held provider instead of re-opening storage per
family — the keystone adoption step of RFC-0017 Phase A.

## Boundaries

The three-tier guard that keeps an implementing agent inside the lines.

### Always do

- Route every TS operation through `NativeProvider` (the held provider), never
  the flat per-family engines.
- Mirror the existing hand-written `binding.ts` pattern (`NativeProviderBinding`
  interface + entry on `NativeBinding`) for any new binding surface.
- Promote a shared Rust helper to its owning crate (`engram-ingest` owns
  `scan_repository` and its trait bound — the scan fan-in lives there) rather
  than duplicating it in the binding.
- Keep the Rust core and this facade free of scheduler, network, queue, and LLM
  code — those live in the TS operational modules (Phases C–E), not here.

### Ask first

- Introducing a new `packages/` package (default: extend `@engram/node`).
- Changing generated contracts, domain types, or a port trait.

### Never do

- Re-implement Rust behavior in TypeScript — the binding is transport over Rust,
  not a second implementation.
- Put a scheduler, queue consumer, HTTP server, or LLM call in the Rust core or
  this facade.
- Widen `EngramTransport` (memory-narrow: write/retrieve/forget) to force the
  provider pattern through it — the provider facade is a richer, separate surface.
- Introduce a second `NativeProvider`-shaped surface — one provider pattern,
  consumed everywhere.

## Testing Strategy

- **TDD** — the facade's operation dispatch (scan request → Rust call shape;
  consolidate `dry_run`; recall/write/putEntity/batch routing) is pure dispatch
  over a mock `NativeProviderBinding`, verified by `vitest` unit tests.
- **Goal-based check** — `pnpm run typecheck` clean; the `binding.ts` interface
  mirrors the Rust `#[napi]` surface (grep parity); `pnpm run build:native`
  succeeds; `check-surface-parity.sh` still passes (no parity regression).
- **Visual / manual QA** — a smoke script constructs `NativeProvider` from a
  config, writes a memory, scans a tiny fixture directory, runs a consolidation
  (**execution**, non-dry-run), and recalls, recording the observed JSON at each
  step (the real built artifact exercised end-to-end).

## Acceptance Criteria

- [x] `@engram/node` exports a typed `NativeProvider` surface covering the
  operations Phase A delivers — constructor from config JSON, `fromProfileFile`,
  `capabilitiesJson`, the two direct methods (`consolidateJson`,
  `scanRepositoryJson`), and the five handle proxies the facade dispatches
  (`requireMemoryApi`, `requireRecallApi`, `requireGraphApi`, `requireBatchApi`,
  `requireBeliefsApi`) with their sub-interfaces; `pnpm run typecheck` passes.
  The remaining 13 `require*Api` proxies are out of scope here — typed on demand
  when a module in Phases C–E needs one.
- [x] A TS consumer can scan a fixture repository over the held provider and the
  scanned entities land in the knowledge store (manual QA smoke).
- [x] A TS consumer can run consolidation execution over the held provider
  (`consolidateJson`) and observe a `ConsolidationRun` with task-level outcomes
  (manual QA smoke).
- [x] The TS facade dispatches `recall`, `write`, `scan`, `consolidate`,
  `putEntity`, and `batchIngest` to `NativeProvider` (TDD: mock-binding unit
  tests green).
- [x] `pnpm run typecheck` and `pnpm run test` are green; the scan fan-in
  (`KnowledgeRepoGraph`) lives in `engram-ingest`, not duplicated in the binding
  or MCP.

## Assumptions

- Technical: the TS binding surface is hand-maintained in
  `packages/node/src/binding.ts` (8 flat engines); no napi-generated `.d.ts` is
  consumed — the build is a custom `scripts/build-native.mjs`. (source:
  packages/node/src/binding.ts, packages/node/package.json)
- Technical: `NativeProvider` (Rust) holds an `EngramProvider` and reaches all 20
  capability handles through typed proxies, including consolidation execution
  (`consolidateJson` → `ConsolidationService::consolidate`); the parity gate
  reports 0 acknowledged debt. (source: bindings/node/src/provider.rs,
  .codex/hooks/check-surface-parity.sh)
- Technical: `EngramTransport` (`packages/client`) is memory-narrow
  (writeMemory/retrieve/forget) — too narrow for the provider pattern; the
  facade is a new richer surface, not an `EngramTransport` implementation.
  (source: packages/client/src/transport.ts, packages/client/src/native.ts)
- Technical: `scan_repository<R: KnowledgeRepository + KnowledgeGraphRepository>`
  requires a fan-in; `KnowledgeRepoGraph` is currently MCP-local, not shared.
  (source: adapters/ingest/src/scanner.rs, mcp/engram-mcp/src/codegraph.rs)
- Technical: build/test commands are `pnpm run build:native`, `pnpm run
  typecheck` (recursive tsc), `pnpm run test` (recursive vitest); `@engram/node`
  uses tsup + tsc + vitest. (source: package.json)
- Process: this runs in full-mode `work-loop` (structural change + dependent
  tasks); RFC-0017 Phase A is the governing record. (source: docs/rfcs/0017,
  work-loop skill)
