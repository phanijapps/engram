# Expanded embedding comparison: 105 questions across six repositories

This follow-up adds 60 fixed answerable questions on Starlette, Axum and Vitest to the original 45 on Flask, Engram and Vite. Each model was measured on all questions. The combined set has 35 questions per language (Python, Rust and TypeScript). It tests retrieval of source-labeled implementation units, not generated-answer accuracy.

Gemma improves fused required-unit Recall@10 on the additional set by **7.5 percentage points over BGE Small** and **5.8 points over BGE Base**. Both paired-question uncertainty intervals include zero, so this follow-up supports a promising configurable option rather than a settled model winner. BGE Small remains the default.

The tradeoff is retrieval coverage versus ranking: Gemma finds more required units in fused top-10 results, but its additional-set MRR is below BGE Base (0.298 versus 0.315). BGE Base also leads vector recall on that set (70.0% versus 65.8%). Across all 105 questions, Gemma has the strongest TypeScript vector recall; BGE Base leads Python. Model selection should therefore follow the target corpus and retrieval lane.

The next useful experiments are improving fusion where it loses vector hits, adding absent-feature questions and independent label review, and validating native Gemma runtime parity before a controlled latency/memory comparison. This experiment changes neither retrieval tuning nor the default model.

## Additional 60 questions

This is the primary follow-up set. The earlier 45 influenced the decision to continue testing and are shown separately. The labels are reused historical source judgments, not a new independent human-labeled blind test.

### Additional set

| Metric | BGE Small | BGE Base | EmbeddingGemma 2 |
| --- | ---: | ---: | ---: |
| vector / recallAt10 | 56.7% | 70.0% | 65.8% |
| vector / mrrAt10 | 0.440 | 0.447 | 0.476 |
| vector / ndcgAt10 | 0.453 | 0.490 | 0.502 |
| fused / recallAt10 | 50.8% | 52.5% | 58.3% |
| fused / mrrAt10 | 0.277 | 0.315 | 0.298 |
| fused / ndcgAt10 | 0.324 | 0.352 | 0.358 |

### Combined descriptive set

| Metric | BGE Small | BGE Base | EmbeddingGemma 2 |
| --- | ---: | ---: | ---: |
| vector / recallAt10 | 66.2% | 74.3% | 73.8% |
| vector / mrrAt10 | 0.459 | 0.477 | 0.493 |
| vector / ndcgAt10 | 0.494 | 0.525 | 0.534 |
| fused / recallAt10 | 55.7% | 59.0% | 62.9% |
| fused / mrrAt10 | 0.338 | 0.359 | 0.362 |
| fused / ndcgAt10 | 0.382 | 0.403 | 0.416 |

## Vector Recall@10 by repository

| Repository | Questions | BGE Small | BGE Base | Gemma 2 |
| --- | ---: | ---: | ---: | ---: |
| flask | 15 | 93.3% | 93.3% | 80.0% |
| vite | 15 | 60.0% | 66.7% | 93.3% |
| engram | 15 | 83.3% | 80.0% | 80.0% |
| starlette | 20 | 77.5% | 92.5% | 77.5% |
| axum | 20 | 47.5% | 57.5% | 52.5% |
| vitest | 20 | 45.0% | 60.0% | 67.5% |

## Fused Recall@10 by repository

| Repository | Questions | BGE Small | BGE Base | Gemma 2 |
| --- | ---: | ---: | ---: | ---: |
| flask | 15 | 90.0% | 93.3% | 86.7% |
| vite | 15 | 46.7% | 53.3% | 53.3% |
| engram | 15 | 50.0% | 56.7% | 66.7% |
| starlette | 20 | 70.0% | 70.0% | 72.5% |
| axum | 20 | 40.0% | 37.5% | 45.0% |
| vitest | 20 | 42.5% | 50.0% | 57.5% |

## Language breakdown

| Language / lane | Questions | Small Recall@10 | Base Recall@10 | Gemma Recall@10 |
| --- | ---: | ---: | ---: | ---: |
| Python / vector | 35 | 84.3% | 92.9% | 78.6% |
| Python / fused | 35 | 78.6% | 80.0% | 78.6% |
| Rust / vector | 35 | 62.9% | 67.1% | 64.3% |
| Rust / fused | 35 | 44.3% | 45.7% | 54.3% |
| TypeScript / vector | 35 | 51.4% | 62.9% | 78.6% |
| TypeScript / fused | 35 | 44.3% | 51.4% | 55.7% |

## Primary measure uncertainty

Gemma minus comparator for fused required-unit Recall@10. Question bootstrap keeps model outputs paired. Repository-cluster bootstrap resamples whole repositories with their questions together. These are descriptive sensitivity intervals; three new or six total repositories are too few to establish broad population superiority.

| Set / comparator | Delta | Question bootstrap 95% | Repository-cluster bootstrap 95% | Wins / losses / ties |
| --- | ---: | --- | --- | --- |
| additional60 / BGE Small | +0.0750 | [-0.0083, +0.1667] | [+0.0250, +0.1500] | 8 / 3 / 49 |
| additional60 / BGE Base | +0.0583 | [-0.0250, +0.1417] | [+0.0250, +0.0750] | 7 / 3 / 50 |
| combined105 / BGE Small | +0.0714 | [+0.0095, +0.1381] | [+0.0190, +0.1238] | 13 / 5 / 87 |
| combined105 / BGE Base | +0.0381 | [-0.0238, +0.1000] | [-0.0100, +0.0750] | 10 / 5 / 90 |

## Controls and limitations

- 14,217 chunks per model across six repositories; 127/127 required-unit judgments represented in the indexes. No question was dropped because retrieval failed or a labeled unit was unindexed.
- Production sources, questions and model settings were frozen before the additional queries. Model settings, chunker, fusion and ranking were unchanged. The [analysis protocol](embedding-expanded-protocol-2026-10-16.md) was documented while the BGE baseline was running and before the new Gemma run, not as a preregistered blind experiment.
- Each corpus was scanned once, then its vector-free seed knowledge/graph data and lexical sidecars were copied for each model. Model/corpus query IDs and texts were checked for exact equality, every nonempty chunk had a vector and all measured queries completed without retrieval-source failure.
- BGE Small/384 and BGE Base/768 used the same cached FastEmbed models. Gemma used the same pinned `google/embeddinggemma-2` revision `914f7f89142e33e77833254d9c9b90c3cef7303b`, text-only FP32 inference, 768 dimensions and task/document prefixes as the first comparison. No dimensions, prefixes, questions or fusion thresholds were tuned.
- Gemma ran on GPU through the temporary loopback reference server; BGE ran on CPU. Some work overlapped across repositories. Timing is not a comparative model-speed benchmark. Native in-process Gemma 2 remains unimplemented.
- Labels are agent-authored and may omit useful alternative implementations. These measurements do not evaluate generated answers, absent-feature rejection or production user satisfaction.
- Original methods and model provenance: [BGE comparison](embedding-model-comparison-2026-10-16.md), [Gemma reference comparison](embeddinggemma2-comparison-2026-10-16.md). Complete input receipt paths and hashes, per-question metrics and secondary uncertainty analyses are in the accompanying JSON.

Persistent raw receipts, per-corpus manifests, frozen labels and identical-knowledge audit: `/home/videogamer/.engram/evaluations/embedding-expanded-comparison-2026-10-16`. Frozen label bundle: [JSON](embedding-expanded-comparison-2026-10-16.labels.json).
