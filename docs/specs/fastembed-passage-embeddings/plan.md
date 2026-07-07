# Plan: FastEmbed passage embeddings + semantic retrieval (demo Slice 3)

- **Spec:** [`spec.md`](spec.md)
- **Status:** Done

## Approach
(1) Add `embed_passage` to `FastEmbedBgeSmallQueryProvider`. (2) Add a `fastembed`
feature on `engram-node` + a cfg-gated `NativeRetrievalEngine` (BGE-small passage
index + query search over `SqliteVectorIndex`). (3) `build:native` enables the
feature; `@engram/node` gets a `NativeRetrievalTransport`. (4) Backend
`/retrieval/{index,search}`. (5) Frontend `SearchPanel`.

## Tasks
- **T1** `embed_passage` on the FastEmbed provider. Done.
- **T2** `engram-node` `fastembed` feature + cfg-gated `NativeRetrievalEngine`. Done.
- **T3** `build:native --features fastembed` + `NativeRetrievalTransport` + backend routes. Done.
- **T4** `SearchPanel` in `demo/frontend`. Done.
- **T5** Gates: default + `--features fastembed` clippy, workspace tests, TS, hooks. Done.

## Risks
- Model download blocked → spike confirmed it works in this environment; if a
  future environment blocks it, the `#[ignore]` test + feature gate keep default
  builds unaffected (the demo simply cannot run retrieval there).
- Tiny corpora discriminate poorly (BGE-small) → documented; real docs work well.

## Changelog
- 2026-06-30: initial plan (Slice 3). Full `engram-retrieval` multi-source fusion
  deferred — slice delivers direct semantic retrieval over sqlite-vec.
