# engram-code Phase-2 index benchmark

Recorded 2026-08-23 with `adapters/ingest/examples/benchmark_code_index`
(`cargo run -p engram-ingest --example benchmark_code_index -- <repo>`):
full Phase-2 pipeline — AST chunking (receiver-qualified identity, docstring
+ signature lexical text), typed structural edges, framework patterns,
cross-file resolution, unresolved-refs ledger, orphan sweep — against an
in-memory `SqlKnowledgeStore`, then a one-file-edit re-scan with the prior
manifest (convergent re-ingest of exactly the touched file).

Hardware: the dev workstation this repo is built on (debug build, rayon
parallel ingest). Numbers are indicative, not gate thresholds.

## TencentDB-Agent-Memory (TypeScript-heavy)

| Metric | Value |
|---|---|
| Files ingested | 863 |
| Full index time | 17.9 s |
| Entities | 10,531 |
| Relationships | 24,693 — calls 18,473 · imports 2,610 · contains 2,592 · extends 45 · implements 98 · routes_to 12 |
| Pending ledger rows | 11,344 |
| One-file-edit sync | 862 ms (re-ingested 1 file) |

## mem-alpha (Rust-heavy, this repository)

| Metric | Value |
|---|---|
| Files ingested | 930 |
| Full index time | 17.4 s |
| Entities | 8,614 |
| Relationships | 27,354 — calls 21,300 · imports 2,635 · contains 2,298 · extends 3 · implements 176 · routes_to 0 |
| Pending ledger rows | 15,810 |
| One-file-edit sync | 879 ms (re-ingested 1 file) |

## Notes

- Sub-second incremental sync on ~900-file repos: the manifest skip works —
  only the edited file re-ingests, and the orphan sweep runs over the
  pending ledger afterwards.
- Pending ledger volume is high in dependency-heavy repos: most pending rows
  are references into external crates/npm packages (`serde_json::…`,
  `express`, …) that no scanned file defines. A follow-up may classify
  clearly-external references (path roots that never resolve) as `failed`
  after N sweeps instead of keeping them `pending`.
- `routes_to` is non-zero only where framework patterns matched (Tencent's
  Express/NestJS routes); mem-alpha has no web-framework sources.
- Kernel-scale (70k files) not yet run — the codegraph comparison target
  quotes <12 min for the Linux kernel; engram-code has no hard repo-size cap
  but the RFC-0012 full-load analytics ceiling still applies to the query
  side, not ingestion.
