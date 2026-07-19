# Plan: Demo UI polish + durable shared state (demo Slice 4)

- **Spec:** [`spec.md`](spec.md)
- **Status:** Done

## Approach
(1) Optional `path: Option<String>` on the memory/knowledge/ingest engine
constructors (file-backed when set, in-memory when not — backward compatible).
(2) `@engram/node` transport factories pass an optional `dbPath`. (3) The demo
backend opens one shared file (`ENGRAM_DB`, default `demo-engram.db`) for those
three engines; retrieval stays in-memory. (4) README documents the full demo.

## Tasks
- **T1** Optional path constructors on the three engines + binding test fix. Done.
- **T2** `dbPath` through the TS transport factories + binding types. Done.
- **T3** Backend shared `ENGRAM_DB`; retrieval in-memory. Done.
- **T4** README rewrite (four panels + durability). Done.
- **T5** Gates + durability/cross-engine smoke. Done.

## Changelog
- 2026-06-30: initial plan (Slice 4).scoped to durable shared state + docs;
  bundle code-splitting and `engram-retrieval` fusion left as future polish.
