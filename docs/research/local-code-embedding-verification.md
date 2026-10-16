# Local code embeddings: live verification

Date: 2026-10-14. [Spec](../specs/local-code-embeddings/spec.md).

Local code embeddings use shared Rust backfill behavior. Rust SDK, native
TypeScript, stdio MCP scan/reindex and HTTP maintenance invoke the same
operation. FastEmbed explicitly declares in-process execution; external and
unknown providers continue to exclude raw code.

## Clean self-store rebuild

The retained `~/.engram/mem-alpha-self` store was backed up with SQLite's
online backup API. Existing Rust `SqliteVectorIndex::clear` removed **11,701
vectors**. All **28 canonical tables** had identical counts and SHA-256 row
content digests before and after reset. The shared `engram_data.db` and scan
manifests were preserved.

| Check | Result |
| --- | --- |
| Source scan | 35 ingested, 962 unchanged, zero errors/admission failures |
| Source generations | 622 active, zero updating/incomplete |
| Exact MCP source reads | 622/622 matched bytes and SHA-256; 4,743,980 bytes |
| Vector index | 21,341 vectors |
| Semantic code vectors | 9,316; every current nonempty semantic code chunk indexed |
| Code roles | 4,794 symbols; 1,782 container-context; 2,740 residual |
| Snapshot vectors | Zero, versus 628 physical source-snapshot records |
| Repeat backfill | Zero embedded, zero pending |
| Scan/backfill/source/query verification | 1,107.96 seconds |

Actual embedded target IDs came from `VectorIndex::embedded_ids`, not shadow
row-count assumptions. They were compared with canonical chunk roles. Exact
source was read through the real MCP server and compared with the checkout.

All four self-search probes returned vector-attributed source chunks:

- Nested-function preservation: `callable_with_nested_declaration_keeps_complete_body`
  in `core/code/src/parser/tree_sitter_chunker.rs`, tagged `graph+vector`.
- Parser discovery: the `TreeSitterChunker` implementation context in the same
  file, tagged `vector`.
- Source policy: `code_source_retrieval_authorized` in
  `core/knowledge/src/code_source.rs`, tagged `vector`.
- Local backfill: its module declaration in `core/integration/src/lib.rs`,
  tagged `vector`, alongside documentation.

This establishes real code-vector operation, not a relevance benchmark.

## Other surfaces and regression checks

A disposable code-only stdio store embedded three code units and returned the
full invoice late-payment function for a paraphrased query. Exact source
matched and repeated backfill was idle. The real native addon embedded one
Rust unit; HTTP MCP embedded two TypeScript units. Both returned actual code
with vector attribution and repeated with zero pending work.

All **1,070 Rust tests** and **206 TypeScript workspace tests** passed,
including explicitly enabled live-model smokes. Build, typecheck, strict
Clippy, contracts, docs, engine neutrality and surface parity passed. The 19
source-selection fixtures cover role/snapshot/hash reuse, source policy,
expiry/scope, inference races, external rejection, model/insertion failures,
retry, and 65 large Unicode passages with bounded inputs/batches and intact
canonical text. Independent adversarial, security and quality reviews are clean.

The optional spec-status linter reports 15 pre-existing legacy-spec metadata
violations, reproduced against HEAD with zero new violations. Both touched
specs pass its checks; the legacy cleanup is recorded in the backlog.

A fresh RustSec audit reproduced the existing 16-advisory baseline. No
dependency version or lockfile changed. This is not a clean security-audit
claim; remediation remains in the dependency-security backlog.

## Reproduction and limits

```bash
cargo build --release -p engram-mcp
python3 scripts/dev/code_embedding_smoke.py
python3 scripts/dev/mcp_driver.py --storage ~/.engram/mem-alpha-self \
  --project mem-alpha-self --wait 12 \
  call maintain '{"op":"reindex","limit":512}'
```

Native/HTTP live-model tests require `ENGRAM_LIVE_FASTEMBED=1`, with `HF_HOME`
and `FASTEMBED_CACHE_DIR` pointing at the existing cache. Default tests skip
only those live-model cases and retain model-free deterministic fixtures.
The live run used cached local BGE-small-en-v1.5.

The 8,192-character input bound and finite model token window do not promise
complete vector coverage of long methods. Token-aware AST segments, parser
registration, dependency hydration and generated summaries remain later slices.

Backup/raw evidence: `/tmp/engram-code-vector-rebuild-z3qf8cr0/` and
`/tmp/engram-code-self-rebuild.log`. Those are temporary machine-local files;
this report preserves the durable results.
