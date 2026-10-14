# Code source preservation: live self-verification

Date: 2026-10-14. Runtime checkout: `22013a1b048ab59c9ecc7c138a6b60122d9cfe3c`
(`codex/code-source-preservation`). Implements the first slice of
[RFC-0021](../rfcs/0021-structural-code-context.md).

## Result

The rebuilt server indexed this repository into the retained
`~/.engram/mem-alpha-self` store, project `mem-alpha-self`, without errors.
Every indexed code file was then read through the real MCP `code source_read`
operation and compared byte for byte with the checkout. All 620 matched,
including SHA-256 verification. There were no incomplete or updating source
generations after the scan.

| Check | Observed result |
| --- | --- |
| Initial source-format upgrade | 1,260 entries scanned; 992 ingested; 10 skipped; zero errors/admission failures |
| Source files | 620 active: 383 Rust, 163 TypeScript, 49 TSX, 15 Python, two MJS, eight shell |
| Exact source reads | 620/620 matched; 4,696,670 bytes; 1.918 seconds accumulated read time |
| Coverage descriptors | 9,243: 4,743 symbols, 1,769 container-context units, 2,731 residual units |
| Parse quality | 9,233 clean descriptors; 10 recovered descriptors, explicitly labeled |
| Source snapshots | 626 physical records; zero in the ordinary `knowledge_chunks` eligibility view |
| Nested executable body | Complete 1,067-byte `encode_chunk_projection` unit, including nested `strip`, reader-version selection and final encoding |
| Tool authorization | Caller-supplied scope override rejected |
| Symbol search | `encode_chunk_projection` returned by the real `code search` operation |
| Unchanged second scan | Zero ingested, 992 unchanged, zero errors; 6.19 seconds |

The nested-body proof is actual repository source at
[`bindings/node/src/knowledge_chunks.rs`](../../bindings/node/src/knowledge_chunks.rs),
not only a synthetic fixture. It verifies the failure class identified by
the earlier [AST chunking audit](ast-aware-code-chunking-audit.md): an outer
function no longer loses behavior surrounding a nested declaration. Exact
source preservation also covers files without an AST parser; extension counts
do not imply AST support for every extension.

## Build and transport checks

Passed `cargo build --release -p engram-mcp`, `pnpm run build`, all 18
`@engram/node` tests, and all 78 `@engram/runtime` tests against the rebuilt
native addon. The separate live
[`code_source_smoke.py`](../../scripts/dev/code_source_smoke.py) fixture passed
fresh/reopened-store reads, TypeScript/Rust/Python/empty-file preservation,
nested bodies, snapshot exclusion, repeat scans, invalid-UTF-8 preservation,
and removal without retracting a sibling file.

The self-store check used the real stdio server and SQLite read-only queries
to inspect physical versus eligible records. A consistent online SQLite
backup and the prior scan sidecars were saved before the upgrade. The store
was not wiped, preserving its temporal baseline.

Rebuild and repeat the durable fixture/self-scan with:

```bash
cargo build --release -p engram-mcp
pnpm run build
python3 scripts/dev/code_source_smoke.py
python3 scripts/dev/mcp_driver.py --storage ~/.engram/mem-alpha-self \
  --project mem-alpha-self --no-vector \
  call scan '{"path":"/home/videogamer/projects/mem-alpha","target":"repo"}'
```

Local raw evidence, the exact-file verification harness and pre-upgrade
backup were retained under `/tmp/engram-self-verification-oosgzl5y`,
`/tmp/engram_self_source_verify.py`, and `/tmp/engram-self-*.log`. These are
temporary machine-local artifacts; this report records the durable results.

## Code-intelligence findings and limits

Architecture, health, changed and two-hop impact queries completed. The
analytics graph contained 3,301 nodes and 7,907 edges. Its leading bridge
was `scan_repository` (betweenness 8,878.13), followed by `bootstrap_sqlite`
(3,678.81), a 2.41-fold difference. Source inspection confirms the scanner
orchestrator remains 1,336 lines; its existing modularization backlog item
still applies. Impact returned bounded callers without truncation, including
the native and MCP scan entry points; those callers were checked in source.

Health's 2,330 dead-code leads are not verified unused code. For example,
`ConformanceHarness::run_beliefs_fixture` has a real `self` caller at
`adapters/integration/src/harness.rs:119`, and the composite consolidation
executor is constructed and registered in
`core/integration/src/sqlite/bootstrap.rs`. Existing receiver/trait-dispatch
resolution limitations remain; this source-preservation slice does not fix
call resolution or establish the accuracy of temporal rankings.

The full legacy-format refresh took **861.75 seconds**, materially longer
than the unchanged **6.19-second** scan. During the long tail, observations
showed all source files had been activated while individual files were
subsequently refreshed for reference healing. The finished ledger contained
2,072 resolved and 16,291 pending references. Source inspection shows a full
manifest lookup and guarded file refresh per healed reference. This is a
profiling lead, not a measured attribution of Rust phase costs; no performance
optimization was made. Track the upgrade cost separately from correctness.

All scans used `--no-vector`: zero chunks were embedded. This run establishes
source coverage, source retrieval and graph-query operation. It does not
establish semantic vector recall, generated behavior summaries, hydrated
dependency context or a new parser-registration interface. Those remain later
RFC slices.

## Vector-enabled follow-up

On the same day, the self store was reopened with vectors enabled, using the
cached local BGE-small-en-v1.5 model. The real capability report returned
`vector: supported`. Reindexing covered **11,626 eligible document chunks**
and drained the backlog; a subsequent reindex returned **zero embedded,
zero pending**. These are the tool's own embedding counts, including reused
vectors, rather than counts inferred from SQLite shadow tables.

Four natural-language queries exercised complete nested function bodies,
adding language parsers, exact source/hash reads, and vector eligibility.
Diagnostic search output showed vector contributions, including
`[vector, ...]` and `[graph+vector, ...]`. Results included the source
preservation spec and RFC-0021's parser-provider proposal. General recall
also returned results. This establishes real vector retrieval of indexed
documentation; it is not a relevance benchmark or proof that proposed parser
extensions have been implemented.

At this stage, raw code chunks were excluded from automatic model feeds.
The vector-backend switch alone did not implement code embedding. The later
[local-code-embeddings](../specs/local-code-embeddings/spec.md) slice enables
policy-authorized in-process code embedding through the shared Rust operation.
External enrichment remains a separate authorization boundary. Source snapshots
remain excluded from embedding and ordinary retrieval.

Use vectors for subsequent self-store runs:

```bash
python3 scripts/dev/mcp_driver.py --storage ~/.engram/mem-alpha-self \
  --project mem-alpha-self --wait 12 \
  call maintain '{"op":"capabilities"}' \
  call maintain '{"op":"reindex","limit":512}'
```

Repeat reindex until `backlog drained`. The server defaults vectors on;
omit `--no-vector`, or pass `--vector` when launching the server directly.
The persisted vectors survive process restarts. Raw follow-up evidence is
in `/tmp/engram-self-verification-oosgzl5y/vector/` and
`/tmp/engram-self-vector-proof.log`.

The subsequent implementation and clean self-vector rebuild are recorded in
[local code embedding verification](local-code-embedding-verification.md):
9,316 current code vectors, zero snapshot vectors, 622 exact source reads.
