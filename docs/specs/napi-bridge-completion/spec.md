# Spec: N-API bridge completion (demo Slice 0)

- **Status:** Draft
- **Owner:** phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** RFC-0003, ADR-0003, ADR-0006, `docs/specs/workspace-responsibility-layout`, `docs/specs/typescript-native-surface`
- **Brief:** none
- **Contract:** none (the demo's HTTP surface is local to `demo/`, not a versioned `contracts/<type>/` artifact)
- **Shape:** mixed

> **Spec contract:** this document defines what "done" means. The implementing
> PR must match this spec, or update it. Verification must be derivable from it.

## Objective

A developer running the Engram demo builds the native binding, starts a Node
backend and a Vite frontend, and in the browser writes a memory, retrieves it,
and forgets it — each operation backed by real Rust behavior through the
`engram-node` N-API bridge. Today that bridge compiles but cannot be loaded
from Node (no `.node` is produced and the `@engram/node` test runs against an
injected fake engine), so there is no real browser → Node → Rust path at all.
Success for this slice: the `@engram/node` test exercises real Rust, and a
browser round-trips memory operations through a Hono backend to the Rust
`SqlMemoryService` and back — proving the foundation every later demo slice
depends on. This slice is memory-only; extending the binding to knowledge,
ingest, retrieval, or taxonomy is Slice 1 (after ADR-0007).

## Boundaries

### Always do

- Build the `.node` via `@napi-rs/cli` and load it for real in `@engram/node`'s
  test (a real-load smoke test against compiled Rust, not only the fake).
- Keep the binding memory-only in this slice — the existing
  `writeMemoryJson`/`retrieveJson`/`forgetJson` over `SqlMemoryService`; no new
  N-API methods.
- Keep `demo/` out of `packages/` (a separate `demo/*` workspace glob is
  allowed so the demo consumes `@engram/node` via `workspace:*`).
- Run the repository's Rust and TypeScript gates; keep v1 contracts drift-free.

### Ask first

- Adding `@napi-rs/cli` as a workspace devDependency / build step (pre-authorized
  by RFC-0003 D1; recorded here because `workspace-responsibility-layout` flags
  N-API/TS configuration surfaces as Ask-first).
- Making the demo's memory durable (file-backed SQLite). Default for this slice:
  keep the existing in-memory engine; defer file-backed construction to Slice 1.

### Never do

- Add knowledge, ingest, retrieval, or taxonomy operations to the binding in
  this slice (Slice 1, after ADR-0007).
- Introduce a Rust HTTP server or an async runtime into the workspace — Rust
  stays a library; HTTP lives only in the Node demo backend.
- Change v1 contract fields or generated TypeScript types.
- Move `demo/` into `packages/` or make any demo package publishable.
- Add any new Rust crate, workspace dependency, or module boundary in this slice
  beyond `@napi-rs/cli` and the demo's own runtime deps (`hono`,
  `@hono/node-server`, `react`, `react-dom`, `vite`, `@vitejs/plugin-react`).

## Testing Strategy

- **Binding load (goal-based, integration):** `@napi-rs/cli` builds a loadable
  `engram_node.node` for the host triple, verified by a real-load test. Why
  goal-based: the outcome ("a `.node` loads and round-trips") is best proved by
  a build + a load assertion, not a unit invariant.
- **Real-load round-trip (TDD-shaped integration):** `@engram/node` constructs
  `NativeMemoryEngine` from the compiled addon — no injected fake — and
  round-trips write → retrieve → forget against real Rust. Integration surface
  because it crosses the Node↔Rust boundary.
- **Backend HTTP (goal-based, integration):** `demo/backend` exposes
  `/memory/write`, `/memory/retrieve`, `/memory/forget`; an integration test
  posts a contract fixture and asserts the Rust-backed JSON response. Why
  goal-based: each endpoint's contract is "v1 JSON in → v1 JSON out, unchanged by
  Rust", best proved by posting a fixture and asserting the response, not by a
  unit invariant.
- **Frontend shell (visual / manual QA):** the Vite app renders a memory panel;
  a recorded gesture submits a memory and observes it in retrieve results.
  Optionally backed by a Playwright smoke.

## Acceptance Criteria

- [ ] `@napi-rs/cli` `build` produces a loadable `engram_node.node` for the host
  triple (linux x64-gnu for local runs) — verifiable by `pnpm --filter
  @engram/node build:native && node -e "require('./packages/node/engram_node.node')"`.
- [ ] A real-load test in `@engram/node` constructs `NativeMemoryEngine` from the
  compiled addon (no injected fake) and successfully round-trips
  `writeMemoryJson` → `retrieveJson` → `forgetJson` against real Rust.
- [ ] `demo/backend` (Hono on Node) starts and exposes `/memory/write`,
  `/memory/retrieve`, `/memory/forget` that delegate to the native engine and
  return Rust-backed JSON.
- [ ] `demo/frontend` (Vite + React) renders a memory panel; a user submits a
  memory and sees it returned by retrieve — end-to-end browser → Node → Rust.
- [ ] Default repo gates pass with no v1 contract drift: `cargo check
  --workspace`, `cargo test -p engram-node` (real-load), `pnpm run typecheck`,
  `pnpm run test`.
- [ ] `demo/README.md` documents how to build the `.node` and run the backend and
  frontend.

## Assumptions

- Technical: `engram-node` compiles as a cdylib and emits `libengram_node.so`
  (source: `cargo build -p engram-node` probe, 2026-06-30).
- Technical: N-API crates `napi` 3.9.4 / `napi-derive` 3.5.7 / `napi-build`
  2.3.2 (source: `bindings/node/Cargo.toml`).
- Technical: `@napi-rs/cli` latest is 3.7.2 and matches the `napi` 3.x Rust
  crates (source: `npm view @napi-rs/cli version` probe, 2026-06-30). Resolves
  RFC-0003 OQ1.
- Technical: toolchain Node 22.14.0, pnpm 10.0.0, rustc 1.94.1 (source: version
  probes, 2026-06-30); satisfies `engines: node>=22`.
- Technical: `pnpm-workspace.yaml` globs `packages/*` only; a `demo/*` glob is
  added so the demo consumes `@engram/node` via `workspace:*` while remaining
  outside `packages/` (source: `pnpm-workspace.yaml`; RFC-0003 "standalone").
- Technical: the binding is memory-only — `writeMemoryJson`/`retrieveJson`/
  `forgetJson` over `SqlMemoryService::open_in_memory()` (source:
  `bindings/node/src/lib.rs`).
- Technical: the loader requires `engram_node.node` at package-adjacent paths
  (source: `packages/node/src/binding.ts`).
- Product: the demo is local-first on the host triple (linux x64-gnu); the
  `.node` is host-specific and rebuilt per OS/arch (source: RFC-0003 decision,
  user confirmation 2026-06-30).
- Product: backend = Hono on Node; frontend = Vite + React (source: RFC-0003 OQ3
  default, user confirmation 2026-06-30).
- Process: lighter adversarial review — single pass, not multi-pass (source: user
  confirmation 2026-06-30).
- Process: no public contract change in this slice; ADR-0007 (binding surface
  extension) is deferred to Slice 1 (source: RFC-0003 D3 / Slice 0).
