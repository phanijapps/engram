# EmbeddingGemma 2: frozen code retrieval comparison

EmbeddingGemma 2 was evaluated at native 768 dimensions against the previously measured BGE Small/384 and BGE Base/768 results, using the same 45 source-labeled questions, 7,001 chunks and 50 required-unit judgments across Engram, Flask and Vite. All questions completed, every nonempty chunk had a vector, and copied knowledge records remained identical to the original vector-free seeds.

## Overall results

| Lane / metric | BGE Small | BGE Base | EmbeddingGemma 2 |
| --- | ---: | ---: | ---: |
| vector / hitAt10 | 80.0% | 80.0% | 86.7% |
| vector / recallAt10 | 78.9% | 80.0% | 84.4% |
| vector / mrrAt10 | 0.485 | 0.516 | 0.516 |
| vector / ndcgAt10 | 0.549 | 0.573 | 0.577 |
| fused / hitAt10 | 64.4% | 71.1% | 73.3% |
| fused / recallAt10 | 62.2% | 67.8% | 68.9% |
| fused / mrrAt10 | 0.419 | 0.417 | 0.447 |
| fused / ndcgAt10 | 0.460 | 0.471 | 0.493 |

Gemma improved vector Recall@10 by 5.6 percentage points and fused Recall@10 by 6.7 points versus BGE Small. Versus BGE Base the gains were 4.4 and 1.1 points. These are retrieval metrics, not generated-answer accuracy.

## Per-repository recall

| Corpus | Small vector | Base vector | Gemma vector | Small fused | Base fused | Gemma fused |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| flask | 93.3% | 93.3% | 80.0% | 90.0% | 93.3% | 86.7% |
| vite | 60.0% | 66.7% | 93.3% | 46.7% | 53.3% | 53.3% |
| engram | 83.3% | 80.0% | 80.0% | 50.0% | 56.7% | 66.7% |

The strongest vector improvement was Vite (60.0% to 93.3% versus BGE Small). Flask fell from 93.3% to 80.0%; Engram fell from 83.3% to 80.0%. This is a workload-dependent gain, not a universal replacement verdict.

## Paired uncertainty

Gemma minus BGE Small; 5,000 paired question-bootstrap samples. The questions are reused from the earlier source-authored evaluation, not a new independent blind test.

| Lane / metric | Delta | Bootstrap 95% interval | Wins / losses / ties |
| --- | ---: | --- | --- |
| vector / hitAt10 | +0.0667 | [-0.0667, +0.2000] | 7 / 4 / 34 |
| vector / recallAt10 | +0.0556 | [-0.1000, +0.2000] | 8 / 6 / 31 |
| vector / mrrAt10 | +0.0314 | [-0.0817, +0.1457] | 15 / 14 / 16 |
| vector / ndcgAt10 | +0.0284 | [-0.0790, +0.1329] | 16 / 16 / 13 |
| fused / hitAt10 | +0.0889 | [+0.0000, +0.2000] | 5 / 1 / 39 |
| fused / recallAt10 | +0.0667 | [-0.0333, +0.1778] | 5 / 2 / 38 |
| fused / mrrAt10 | +0.0281 | [-0.0243, +0.0784] | 9 / 5 / 31 |
| fused / ndcgAt10 | +0.0334 | [-0.0225, +0.0881] | 9 / 6 / 30 |

## Inference and comparison controls

- Exact checkpoint: [`google/embeddinggemma-2`](https://huggingface.co/google/embeddinggemma-2), revision `914f7f89142e33e77833254d9c9b90c3cef7303b`. All downloaded checkpoint files were hashed.
- SentenceTransformers 6.1.0, Transformers 5.20.1 and PyTorch 2.14.1. The text encoder alone ran in FP32 on the RTX 3090. The full Hugging Face checkpoint supplied the weights; no Ollama quantized substitute was used.
- Query prefix: `task: code retrieval | query: `. Passage prefix: `title: none | text: `. Prefixes were applied once by the Engram HTTP adapter; the server added none. Model-side mean pooling and normalization used the published SentenceTransformers modules. Context limit was 8,192 tokens; the existing MCP passage cap remained 8,192 characters for all models.
- Smoke checks confirmed finite, normalized 768-dimensional vectors and batch/single agreement (maximum absolute error about 4.1e-8).
- The same MCP release binary and frozen vector-free seed stores were reused. The Rust SDK probe was rebuilt from the same sources with the HTTP embedding feature enabled. The real Engram served-model adapter handled construction, dimension verification, query/passages and batch requests.
- Chunk, entity and relationship records were verified identical across each original seed and Gemma store after evaluation. Source and question hashes were verified; the BGE receipt remained unchanged. The same limit-10 vector and SDK fused-recall endpoints, grading and fixed labels were used. No fusion/reranker thresholds or questions were tuned.
- The Gemma runtime used a GPU while BGE used CPU FastEmbed. Their timings cannot establish relative model speed. Differences in model context capacity and prescribed prompt formats are part of the evaluated configurations.
- Labels are agent-authored source judgments without independent human review. Required-unit recall and duplicate suppression follow the [BGE comparison methodology](embedding-model-comparison-2026-10-16.md). Unlabeled helpful results receive no relevance credit; absent-feature/abstention accuracy was not evaluated.

## Can it run inside Engram like BGE Small?

An [EmbeddingGemma 2 ONNX export](https://huggingface.co/onnx-community/embeddinggemma-2-ONNX) is available. The installed FastEmbed 5.17.2 catalogue supports the original EmbeddingGemma 300M, but does not contain Gemma 2. FastEmbed exposes a custom ONNX loader with external-initializer support, so a native implementation has a plausible route using Engram's existing Rust embedding-provider boundary.

This run validates the Hugging Face reference through a separate loopback Python process. It does not implement or validate in-process Gemma 2. Native work still needs ONNX input/output and runtime compatibility checks, model/tokenizer loading, task prefixes, pooling/normalization, model-space identity, and parity against reference embeddings and retrieval results.

## Reproduction and artifacts

Run `scripts/dev/embeddinggemma2_server.py` in an isolated environment containing its documented dependencies, against the pinned local checkpoint. It binds only to loopback and writes a readiness file with its endpoint. Then run `scripts/dev/embeddinggemma2_eval.py --baseline <BGE-output> --output <new-directory> --endpoint <endpoint>`. The evaluator copies only vector-free seed stores and asserts baseline/corpus/question consistency.

Full local stores, raw query payloads and stderr: `/tmp/engram-embeddinggemma2-comparison-20261016`. Persistent compressed raw receipt, checkpoint hashes, dependency lock and server metadata: `/home/videogamer/.engram/evaluations/embeddinggemma2-comparison-2026-10-16`. Compact results: [JSON](embeddinggemma2-comparison-2026-10-16.json).
