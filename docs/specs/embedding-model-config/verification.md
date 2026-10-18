# Embedding configuration verification

Branch: `feat/embedding-model-config`, based on requested main `fbab016`.
Verified 2026-10-16. Shared stores were not wiped or reindexed.

## Functional evidence

- Configuration: eight tests cover defaults, legacy/partial JSON, automatic-dimension serialization, presets and precedence, invalid settings, semantic identity, disabled inference and fencing historical nondefault FastEmbed labels.
- HTTP adapter: ten loopback integration tests cover both protocols, authentication redaction, ordering, malformed output, bounds and timeouts. These verify transport behavior, not model quality.
- SQLite: three integration scenarios verify inferred dimensions and normalization, explicit dimension failure before index creation, and model/instruction/dimension changes preserving incompatible vectors.
- Pgvector: six live tests in disposable schemas verify identity fencing, provable legacy adoption, concurrent initialization, 2048-dimensional exact search and disabled-provider no-egress. Receipt: `/tmp/embedding-pg-tests.log`.
- Native Node: actual BGE Small/384 and BGE Base/768 query, passage and batch inference, with finite/nonzero outputs and batch/single agreement; 25 tests pass with the opt-in real-model suite.
- MCP: `python3 scripts/dev/embedding_config_smoke.py` passed against the release binary and disposable stores. BGE Small indexed one chunk in 28 ms and retrieved it through the vector lane in 16 ms; BGE Base indexed one chunk in 44 ms and retrieved it in 27 ms. These are smoke timings, not comparative benchmarks. Repeat reindex embedded zero chunks. A model switch fenced the lane without changing vector metadata or row count; restoring the original model recovered the lane. A served fixture inferred three dimensions and retrieved its indexed document; `--no-vector` made no HTTP calls or credential lookup. Receipt: `/tmp/engram-embedding-smoke-lm84tj3f/receipt.json`.

## Build and contract gates

Workspace check and 1,018 passing tests (13 ignored), package build/typecheck, formatting, generated contracts, documentation, engine neutrality and surface parity were exercised. Surreal optional-feature compilation also passed; it explicitly rejects unsupported inferred/extended embedding settings.

Independent final review found no runtime, security, data-loss, model-space or surface-parity blocker. Its compatibility wording finding was fixed: legacy `cosine` normalization is rejected, and FastEmbed cannot provide unnormalized vectors.

## Existing main limitations

The full TypeScript test run has five BFF live-store failures against the shared agentzero store. They reproduce with an independently rebuilt, unmodified main `fbab016` addon: its older knowledge adapter attempts to index newer `knowledge_*` views and fails with “views may not be indexed”; knowledge/community capabilities consequently remain unavailable. Receipt: `/tmp/engram-embedding-main-baseline-receipt.json`. These tests were not weakened or skipped to hide the failure. Disposable-store paths and the embedding suites pass.

Workspace strict Clippy stops on existing main code (`core/runtime/src/redaction.rs:124`, redundant borrow). Scoped changed-crate Clippy completes with existing warnings; strict HTTP-adapter Clippy passes. No unrelated compiler/CI repairs were included.

BGE-M3 and Qwen presets were checked as configuration candidates; their real-model retrieval quality was not evaluated. The subsequent [paired Small/Base retrieval study](../../perf/embedding-model-comparison-2026-10-16.md) records measured retrieval changes; it does not measure generated-answer accuracy. Served model IDs must be immutable or explicitly reindexed after retagging.

The subsequent [EmbeddingGemma 2 reference evaluation](../../perf/embeddinggemma2-comparison-2026-10-16.md) completed the same 45 questions through the real HTTP adapter and Rust/MCP indexing path. It validates reference inference, not native in-process Gemma 2 support.
