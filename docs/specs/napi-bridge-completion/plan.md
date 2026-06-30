# Plan: N-API bridge completion (demo Slice 0)

- **Spec:** [`spec.md`](spec.md)
- **Status:** Drafting

> **Plan contract:** implementation strategy. Allowed to change as we learn;
> substantial changes noted in the changelog at the bottom.

## Approach

Three layers, built bottom-up so each de-risks the next. (1) Add a `build:native` script to `@engram/node` that runs `cargo
build --release -p engram-node` and places the cdylib as a loadable
`engram_node.node`; replace the fake-only test with a real-load
smoke test that constructs `NativeMemoryEngine` from compiled Rust. (2) `demo/
backend`: a Hono server that constructs the native engine once and exposes three
memory endpoints over JSON. (3) `demo/frontend`: a Vite + React memory panel
that calls the backend. The riskiest step is T1 — the `build:native` script producing a
loadable `.node` for the `napi` 3.x crate — so it lands first and is gated by the
real-load test before any backend work begins.

## Constraints

- RFC-0003 Slice 0 (memory-only; binding extension is Slice 1 after ADR-0007).
- ADR-0003 (Rust core + TS bindings; Rust is a library).
- ADR-0006 (the memory engine is the SQLite adapter).
- `AGENTS.md` boundary rules; `workspace-responsibility-layout` (N-API/TS config
  is Ask-first — pre-authorized here by RFC-0003 D1).
- Lighter adversarial review (single pass).

## Construction tests

- **Integration:** real-load round-trip in `@engram/node` (write → retrieve →
  forget against compiled Rust, no fake).
- **Integration:** `demo/backend` HTTP round-trip (post a contract fixture to
  each `/memory/*` endpoint, assert Rust-backed response).
- **Manual verification:** `demo/frontend` — submit a memory, observe it in
  retrieve results.

## Design (LLD)

### Design decisions

- `build:native` = `cargo build --release -p engram-node` + place the cdylib as
  `engram_node.node` (probe-proven to load in Node); no `@napi-rs/cli` dependency
  for the local demo. Traces to: AC1, AC2.
- Backend is Hono (minimal, TS-native, Node runtime); one native engine instance
  held in module scope, shared across requests. Traces to: AC3.
- `demo/*` added to `pnpm-workspace.yaml` so `demo/backend` depends on
  `@engram/node: workspace:*` without publishing. Traces to: AC3, AC6.

### Interfaces & contracts

- Demo-local HTTP (not a versioned contract): `POST /memory/write`,
  `POST /memory/retrieve`, `POST /memory/forget`, each accepting the v1 request
  JSON and returning the v1 response JSON unchanged from Rust. CORS enabled for
  the Vite dev origin. Traces to: AC3.

### Component / module decomposition

- `packages/node`: add `build:native` script (`cargo build --release` + place
  artifact) + real-load test.
- `demo/backend`: `src/server.ts` (Hono app), `src/engram.ts` (engine
  construction + JSON transport), `package.json`, `tsconfig.json`.
- `demo/frontend`: Vite + React app; `src/App.tsx` memory panel; `vite.config.ts`
  dev proxy to the backend.
- `demo/README.md`: run steps.

### Dependencies & integration

- `cargo build --release -p engram-node` (via `build:native`) → produces `engram_node.node`.
- `demo/backend` → `@engram/node` (workspace:*), `hono`, `@hono/node-server`.
- `demo/frontend` → `react`, `react-dom`, `vite`, `@vitejs/plugin-react`.
- No new Rust dependencies; `engram-node` is unchanged source-wise.

## Tasks

### T1: Produce a loadable `engram_node.node` via a `build:native` script

**Depends on:** none

**Tests:**
- `pnpm --filter @engram/node build:native` exits 0 and writes `engram_node.node`
  into `packages/node` (AC1).
- The produced `.node` lands at a loader-candidate path
  (`packages/node/engram_node.node`) so `loadNativeBinding()` resolves it without
  throwing.

**Approach:**
- Add `packages/node/scripts/build-native.mjs`: `execFileSync('cargo',
  ['build','--release','-p','engram-node'])` then copy the platform cdylib
  (`libengram_node.so` / `.dylib` / `.dll`) to `packages/node/engram_node.node`.
- Add `build:native` script to `packages/node/package.json`.
- Add `*.node` to `.gitignore` (build artifact).

**Done when:** `pnpm --filter @engram/node build:native` produces
`packages/node/engram_node.node` and `node -e "require('./packages/node/engram_node.node')"`
loads without throwing.

### T2: Real-load smoke test in `@engram/node`

**Depends on:** T1

**Tests:**
- A new test constructs `NativeMemoryEngine` from the compiled addon (no injected
  fake), round-trips `writeMemoryJson` → `retrieveJson` → `forgetJson` using the
  `contracts/v1/examples` fixtures, and asserts the Rust-backed response shapes
  (AC2).

**Approach:**
- Add `test/real-load.test.ts` guarded to run only when the `.node` is present
  (skip with a clear message if absent, so default `pnpm test` still passes
  without a build).
- Reuse the contract fixtures used by the Rust binding test.

**Done when:** `pnpm --filter @engram/node test` runs the real-load test green
when the `.node` is built, and skips cleanly otherwise.

### T3: `demo/backend` Hono server with `/memory/*` endpoints

**Depends on:** T2

**Tests:**
- Integration test starts the server (or imports the app) and posts the write/
  retrieve/forget fixtures to `/memory/*`, asserting the Rust-backed JSON
  responses (AC3).

**Approach:**
- `demo/backend/src/engram.ts`: construct the native engine via
  `createNativeMemoryTransport`, expose JSON pass-through helpers.
- `demo/backend/src/server.ts`: Hono app with three POST routes + CORS; serve on
  a configurable port.
- `demo/backend/package.json` (`@engram/node: workspace:*`, `hono`,
  `@hono/node-server`), `tsconfig.json`, scripts (`dev`, `build`).
- Add `demo/*` to `pnpm-workspace.yaml`.

**Done when:** `pnpm --filter demo-backend dev` starts the server and the
integration test round-trips all three endpoints against real Rust.

### T4: `demo/frontend` Vite + React memory panel

**Depends on:** T3

**Tests:**
- Manual QA: run the frontend, submit a memory, observe it in the retrieve
  results — browser → Node → Rust (AC4). Optional Playwright smoke.

**Approach:**
- `demo/frontend`: Vite + React + `@vitejs/plugin-react`; `src/App.tsx` with a
  write form, a retrieve list, and a forget action.
- `vite.config.ts` dev proxy `/memory` → `http://localhost:<backend>`.
- `package.json`, `tsconfig.json`, scripts (`dev`, `build`).

**Done when:** the app loads, a submitted memory appears in retrieve results
sourced from Rust, and `pnpm --filter demo-frontend build` succeeds.

### T5: `demo/README`, workspace wiring, and full gates

**Depends on:** T4

**Tests:**
- `demo/README.md` run steps reproduce the demo from a clean checkout (AC6).
- Repo gates pass with no contract drift (AC5).

**Approach:**
- Write `demo/README.md` (prereqs, build the `.node`, run backend + frontend,
  seed a memory).
- Run `cargo check --workspace`, `cargo test -p engram-node`, `pnpm run
  typecheck`, `pnpm run test`, `.codex/hooks/check-contracts.sh`,
  `.codex/hooks/check-docs.sh`.
- Commit per task; final commit updates `phases.json` PHASE52 → DONE.

**Done when:** all gates green; README steps verified; PHASE52 marked DONE.

## Rollout

- **Delivery:** local demo only; big-bang on the `demo/engram-ui` branch.
  Reversible — all changes are additive files; rollback is deleting `demo/` and
  the `build:native` script.
- **Infrastructure:** none beyond the local Node/Rust toolchain.
- **External-system integration:** none.
- **Deployment sequencing:** T1 (`.node`) → T2 (real-load) → T3 (backend) → T4
  (frontend) → T5 (README + gates).

## Risks

- The cdylib fails to load in Node (symbol/registration mismatch).
  Mitigation: T1's real-load test catches it before T3 (probe already passed).
- `.node` is host-specific; a reviewer on another OS must rebuild. Mitigation:
  README documents the rebuild; T2 skips cleanly when absent.
- `pnpm` workspace glob for `demo/*` may not resolve `@engram/node`'s built
  `.node`. Mitigation: T3 verifies resolution in its integration test.

## Changelog

- 2026-06-30: initial plan (Slice 0 of RFC-0003 demo program).
- 2026-06-30: switched T1 from `@napi-rs/cli` to a plain `cargo build --release`
  + place-artifact script after a probe proved the cdylib loads in Node when
  placed as `engram_node.node`; simpler, no new CLI dependency. `@napi-rs/cli`
  reserved for future cross-platform packaging.
