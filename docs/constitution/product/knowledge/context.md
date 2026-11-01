# Architecture context + roadmap + maintenance

<!-- prerequisite: identity -->

## Architecture (in one paragraph)

A two-row memory pipeline: a **build/store path** (one-way writes — agent
ingests/writes → memory & knowledge construction → durable store, with
consolidation maintaining the store) and a **retrieve path** (bidirectional —
agent ↔ context path ↔ retrieve ↔ store), with **governance** spanning every
read and write. See the [architecture overview](../architecture/overview.md)
for the full diagram and layer responsibilities.

## Roadmap highlights

- **Now:** codegraph parity, retrieval wiring polish, demo polish.
- **Next:** SurrealDB (2nd engine), lexical live-wiring, cross-encoder rerank,
  the agent-host (AgentZero) adapter.
- **Not in scope:** being an LLM host; eager index-time embeddings; distributed
  cross-store write consistency; a second TypeScript implementation of the core.

Full direction in [`roadmap.md`](roadmap.md); user-visible changes in
[`changelog.md`](changelog.md).

## How this doc is maintained

This is a **living product view** of the shipped implementation. It rolls
features up by **capability** and tracks product-level status. When a capability
ships or its status changes, update the relevant line here in the same PR — drift
between this doc and reality is a bug. New feature work is specced in
[`docs/specs/`](../specs/); open items live in [`docs/history/backlog.md`](../backlog.md).
(See [`docs/guides/CONVENTIONS.md`](../CONVENTIONS.md) document-lifecycle.)

## See also

- [README](../../README.md) — project overview, use cases, quick start.
- [Architecture overview](../architecture/overview.md) — the pipeline + layer responsibilities.
- [Roadmap](roadmap.md) · [Changelog](changelog.md).
- [Backlog](../backlog.md) — open / deferred items.
- [ADRs](../adr/README.md) · [RFCs](../rfcs/README.md) — durable decisions + proposals.
- [Research synthesis](../research/README.md) — prior-art + concept grounding.
- Charter retired 2026-10-31 (content drifted); mission + scope live in this document.
