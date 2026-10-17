# BGE Small versus BGE Base: paired code retrieval evaluation

BGE Small EN v1.5 (384 dimensions) and BGE Base EN v1.5 (768 dimensions) were compared on the same frozen production code and 45 previously authored natural-language questions: 15 each for Engram, Flask and Vite. This evaluates retrieval quality; it does not grade generated answers.

This run shows modest, mixed gains: Base increased vector Recall@10 by 1.1 percentage points and fused Recall@10 by 5.6 points. Fused MRR was effectively unchanged (0.419 to 0.417). Vector recall improved on Vite, stayed flat on Flask and fell on Engram. The paired recall intervals include zero, so this small set does not establish a consistent improvement. Base used about 2.05 times the indexing time. The configured default remains BGE Small.

## Method

- Runtime: the configurable-model branch based on requested main `fbab016`. These results are not directly comparable to earlier code-evidence evaluations on newer unmerged runtime branches.
- Frozen corpora come from the earlier reliable-code-intelligence evaluation. Engram uses its fresh 15 answerable questions; Flask and Vite use their 15 held-out answerable questions each. No questions or expected units were changed after retrieval. The split names are historical: these questions have been used in earlier evaluations. This is a paired regression comparison, not a new independent blind holdout. Labels are source-authored by agents, not independent human judgments.
- Production source files only: Flask `src/flask/**/*.py`; Vite `packages/vite/src/**/*.ts`, excluding test/fixture directories; Engram Rust/TypeScript `src` trees under core, adapters, backends, bindings, codegraph, MCP and packages, excluding separate test/fixture directories. Inline Rust tests remain part of their original source. A preliminary broad scan was stopped before scoring to exclude documentation and separate tests.
- One real MCP scan per corpus, then SQLite backup plus identical lexical/manifest sidecars into two disposable stores. Chunk records were compared for exact equality before model indexing. Every nonempty chunk was embedded; vector-row counts were checked.
- Default query/passage instructions, default batch settings and default fusion; no reranker or score threshold was tuned. Real cached FastEmbed ONNX models were used. Each measured query ran once after one unscored warm-up per model/corpus.
- SQLite raw vector values in the receipt are distances (lower is better), despite the older vector-port scalar being labeled `score`. Metrics use the returned nearest-neighbor order and never compare raw values across models.
- Vector results come from the actual provider vector index; fused results come from Rust SDK unified recall with limit 10. The MCP search tool has additional presentation/filtering behavior and is not the fused endpoint measured here.
- A relevant result must identify the source-labeled declaration: matching file, callable name and declaration start line. Both its chunk and associated symbol entity count as the same required unit; duplicate results receive no additional relevance gain. Required source hashes, corpus manifests and frozen-question hashes were checked. Missing index coverage remains a failure, not a dropped question.
- Hit@10 means any required unit appears. Recall@10 is per-question required-unit recall, averaged over questions. MRR@10 measures first relevant rank. nDCG@10 uses equal relevance for the labeled required declarations; additional potentially useful but unlabeled declarations are not credited. No absent-feature/abstention questions were scored.

## Vector-only retrieval

| Corpus | Small Recall@10 | Base Recall@10 | Small MRR@10 | Base MRR@10 | Small nDCG@10 | Base nDCG@10 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| flask | 93.3% | 93.3% | 0.619 | 0.661 | 0.682 | 0.703 |
| vite | 60.0% | 66.7% | 0.378 | 0.419 | 0.433 | 0.481 |
| engram | 83.3% | 80.0% | 0.457 | 0.468 | 0.532 | 0.535 |
| **Overall** | **78.9%** | **80.0%** | **0.485** | **0.516** | **0.549** | **0.573** |

## Fused SDK recall

| Corpus | Small Recall@10 | Base Recall@10 | Small MRR@10 | Base MRR@10 | Small nDCG@10 | Base nDCG@10 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| flask | 90.0% | 93.3% | 0.534 | 0.515 | 0.618 | 0.606 |
| vite | 46.7% | 53.3% | 0.267 | 0.252 | 0.317 | 0.322 |
| engram | 50.0% | 56.7% | 0.456 | 0.485 | 0.444 | 0.485 |
| **Overall** | **62.2%** | **67.8%** | **0.419** | **0.417** | **0.460** | **0.471** |

## Concrete Vite changes (vector-only)

| Required implementation | Small rank | Base rank |
| --- | ---: | ---: |
| Filesystem access checks: `isFileLoadingAllowed` | outside top 10 | 5 |
| Unexposed package subpaths: `resolveDeepImport` | outside top 10 | 4 |
| Proxy rule matching: `createProxyContextMatcher` | outside top 10 | 2 |
| In-flight transform reuse: `transformRequest` | 4 | outside top 10 |
| Asset inlining decisions: `shouldInline` | 4 | outside top 10 |

## Paired uncertainty

Base minus Small; 5,000 paired question-bootstrap samples with fixed seed 20261016. These intervals describe this small labeled set; they are not independent certification or evidence for every repository.

| Lane / metric | Mean delta | Bootstrap 95% interval | Wins / losses / ties |
| --- | ---: | --- | --- |
| vector / hitAt10 | +0.0000 | [-0.1111, +0.1111] | 3 / 3 / 39 |
| vector / recallAt10 | +0.0111 | [-0.1000, +0.1222] | 4 / 3 / 38 |
| vector / mrrAt10 | +0.0316 | [-0.0464, +0.1109] | 12 / 10 / 23 |
| vector / ndcgAt10 | +0.0240 | [-0.0506, +0.0985] | 14 / 12 / 19 |
| fused / hitAt10 | +0.0667 | [+0.0000, +0.1556] | 3 / 0 / 42 |
| fused / recallAt10 | +0.0556 | [-0.0111, +0.1333] | 3 / 1 / 41 |
| fused / mrrAt10 | -0.0013 | [-0.0447, +0.0400] | 6 / 6 / 33 |
| fused / ndcgAt10 | +0.0112 | [-0.0291, +0.0508] | 7 / 7 / 31 |

## Coverage and observed cost

Index times sum MCP reindex tool durations; initialization/download is excluded. Query timings are warm single runs on a shared workstation, not controlled performance benchmarks.

| Corpus | Chunks / required units indexed | Small index seconds | Base index seconds | Small DB MiB | Base DB MiB |
| --- | --- | ---: | ---: | ---: | ---: |
| flask | 446 / 17/17 | 26.7 | 54.5 | 6.5 | 8.0 |
| vite | 1815 / 15/15 | 104.5 | 214.1 | 28.4 | 31.4 |
| engram | 4740 / 18/18 | 264.9 | 542.1 | 74.8 | 82.3 |

## Reproduction and receipts

Copy the paths listed in each compact-result `sourceManifest` from its recorded repository revision, preserving paths and bytes. The included labels JSON can be shared by all three corpus entries (the harness selects the corpus key). Build `engram-mcp` and the `embedding_retrieval_probe` example with FastEmbed, then run `scripts/dev/embedding_model_eval.py --inputs <inputs.json> --output <new-directory>`. The harness documents the input shape. `--resume` verifies existing corpus/question/binary hashes before continuing an interrupted run. Stores are disposable and isolated from agentzero and the persistent self-index.

Full local receipts, raw query payloads, frozen labels, source manifests, copied stores and stderr: `/tmp/engram-bge-production-comparison-20261016`. Frozen questions/labels: [labels](embedding-model-comparison-2026-10-16.labels.json). Compact results and model asset hashes: [embedding-model-comparison-2026-10-16.json](embedding-model-comparison-2026-10-16.json).

A compressed full raw-query receipt is retained at `/home/videogamer/.engram/evaluations/embedding-model-comparison-2026-10-16/run.json.gz`.
