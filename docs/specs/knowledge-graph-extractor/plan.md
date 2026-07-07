# Plan: Deterministic knowledge-graph extractor (demo Slice 2)

- **Spec:** [`spec.md`](spec.md)
- **Status:** Done

## Approach
Bottom-up. (1) `GraphExtractor` in `engram-ingest` (pure `extract` + persisting
`extract_into`). (2) `serde` on the ingest request so the binding can decode it.
(3) `NativeIngestEngine.ingestExtractJson` + `@engram/node` ingest transport.
(4) Backend `/ingest/extract` route. (5) Frontend IngestPanel with Cytoscape.

## Tasks
- **T1** `GraphExtractor` + test (`alpha → beta` calls edge + neighbors). Done.
- **T2** `serde` (+ camelCase) on `DocumentIngestRequest`/`DocumentMetadata`. Done.
- **T3** `NativeIngestEngine` + `NativeIngestTransport` + backend route + test. Done.
- **T4** `IngestPanel` (Cytoscape) in `demo/frontend`. Done.
- **T5** Full gates green. Done.

## Changelog
- 2026-06-30: initial plan (Slice 2). Graph viz implemented with Cytoscape in-panel;
  shared durable state across engines deferred to Slice 4 polish.
