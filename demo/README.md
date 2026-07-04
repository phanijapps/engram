# Engram Demo

A local, end-to-end demo of Engram memory: a browser UI (Vite + React) talks to a
Node backend (Hono), which loads the Rust core through the `engram-node` N-API
binding. **Browser → Node → Rust** — real behavior, no mocks.

The UI is five views, all backed by real Rust over the N-API bridge:

- **Dashboard** — index a repository and see indexed repos with git metadata.
- **Graph** — the hero view: a 2D force-directed knowledge graph, community-clustered
  and readable at the class level (module/class hubs labeled, methods as small
  satellites, colored by cluster).
- **Chat** — grounded + agentic Q&A over knowledge and memory, plus a semantic
  context composer.
- **Memory** — write / retrieve / forget observations.
- **Belief** — record beliefs with confidence, detect and resolve contradictions.

State is **durable and shared**: the running backend opens one SQLite file
(`demo-engram.db`, set via `ENGRAM_DB`) for memory, knowledge, and ingest, so
writes persist across restarts and a graph extracted by ingest is visible to the
knowledge engine. (Vectors for semantic search are in-memory, re-indexed each
session; the FastEmbed model downloads on first retrieval use, then is cached.)

## Prerequisites

- Rust toolchain (rustc 1.85+; the workspace is Rust 2024).
- Node 22+ and pnpm 10+.
- The native addon is **host-specific** (OS + arch) — rebuild it on each machine.

## Build & run

From the repository root:

```bash
# 1. Install workspace dependencies (links demo/backend and demo/frontend).
pnpm install

# 2. Build the native addon: cargo build --release -> packages/node/engram_node.node
pnpm --filter @engram/node build:native

# 3. Build the TypeScript wrapper (@engram/node dist/).
pnpm --filter @engram/node build

# 4. Run the backend (Hono on :8787).
pnpm --filter demo-backend dev

# 5. In another terminal, run the frontend (Vite on :5173).
pnpm --filter demo-frontend dev
```

Open <http://localhost:5173>.

## Try it

1. Type a memory in the left panel and **Write memory**.
2. The right panel retrieves matching memories (keyword) from Rust.
3. **Forget** removes a memory (hard delete in this demo).

## How it works

```text
Browser (demo/frontend, Vite + React)
  │  fetch /memory/* (proxied to the backend by Vite in dev)
  ▼
demo/backend (Hono)  ──loads──▶  @engram/node (engram_node.node)
                                      │  N-API JSON round-trip
                                      ▼
                                engram-store-sql (SqlMemoryService, Rust)
```

The backend is a thin JSON transport: v1 JSON in, v1 JSON out, unchanged by Rust.
Rust stays a library; the Node layer is the only place HTTP lives.

## Notes

- The native addon is a build artifact (`*.node` is gitignored); rebuild it after
  Rust changes via `pnpm --filter @engram/node build:native`.
- Default `pnpm test` skips native-dependent tests when the addon is absent, so
  CI stays green without a Rust build. Build the addon to run the real-load and
  backend integration tests.
