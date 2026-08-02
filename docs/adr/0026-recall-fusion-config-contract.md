# ADR-0026: `[recall_fusion]` config contract for externally tunable recall fusion

- **Status:** Accepted
- **Date:** 2026-08-02
- **Decision-makers:** phanijapps
- **Supersedes:** none
- **Related:** RFC-0019 (hybrid recall fusion — this ADR records the config contract it promised), [ADR-0009](0009-retrieval-composition-seam.md) (retrieval composition seam — the existing RRF + `compose_context` machinery this reuses), [ADR-0022](0022-engine-grid-vs-backend-recipe.md) (engine neutrality — the config carries no engine type), `docs/specs/recall-fusion-config/spec.md` (the spec this gates)

## Decision summary

- **Decision:** Engram's unified recall fusion (RRF `k`, per-lane `source_weights`, reranker strategy) is exposed as an **operator-facing, serde-serializable `[recall_fusion]` config** carried on `EngramConfig`, loaded from an explicit profile section or discovered at `<root>/.engram/recall.json`, and validated on load.
- **Because:** out-of-box recall was BM25+graph+PPR+facts+temporal+beliefs with no vector, no rerank, and equal-weight fusion, while the weighted-fusion API, the cross-encoder adapter, and the vector lane all existed but lay dormant; operators had no code-free knob to bias vector vs BM25 vs graph (zbot's `vector_weight`/`bm25_weight` pattern, engram-neutral).
- **Applies to:** the external config surface (`RecallFusionConfig` / `RerankConfig` in `engram-retrieval`), the `EngramConfig.recall_fusion` field + `.with_recall_fusion` builder + `discover_recall_fusion` / `resolve_recall_fusion` ladder in `engram-integration`, and the engine-specific wiring in `bootstrap_sqlite` / the MCP `open_provider` feeder.
- **Tradeoff accepted:** a new public config surface (one more thing to keep stable + documented); operators can now tune ranking without code changes.
- **Revisit if:** a second fusion algorithm is needed (weighted-sum is already in-tree as `WeightedFusionConfig`; weighted RRF was chosen as the default), or the lane-tag vocabulary grows and the vocab check (warn-only) should tighten to an error.

## Context

RFC-0019 retracted RFC-0018's "vector+reranking is out of scope" deferral and called for engram's unified recall to become a competent generic full-hybrid retriever. The genuinely new surface was small — weighted-RRF fusion (`ReciprocalFusionConfig`), the `RetrievalReranker` port, the cross-encoder adapter, and the vector lane all already existed — but none of it was reachable from configuration. `SqlUnifiedRecall` hard-coded `ReciprocalRankFusion::default()` (equal-weight) and `reranker = None`.

The open decision (RFC-0019 D1) was the *config contract*: how does an operator tune fusion without a code change, without dragging application-specific policy (category weights, contradiction penalty, KG decay, episodes — zbot's gateway-memory) into engram's neutral layers (ADR-0022, no-god-module)?

Forces:

- **Operator tunability.** A deployment must bias vector vs BM25 vs graph by editing config, not code (zbot's `vector_weight`/`bm25_weight` pattern). The knob has to reach the fusion machinery `SqlUnifiedRecall` already wraps.
- **Engine neutrality (ADR-0022).** The config may carry no engine type (`Sql*`, `pgvector`, …) and no SQL. It is keyed on lane *source tags*, not adapter identities, so it ports to any engine that stamps the same vocabulary.
- **Reuse over invention.** Weighted RRF (`ReciprocalFusionConfig { k, default_source_weight, source_weights }` + `ReciprocalRankFusion::new`) already existed; the contract reuses it rather than inventing a new fusion algorithm. The reranker reuses the existing `RetrievalReranker` port + `compose_context`'s rerank slot.
- **Backward compatibility.** Absent config ⇒ today's behavior (equal-weight RRF, no reranker). The config is additive; no existing host breaks.
- **Honest scope.** The contract ships *mechanism*, not *application policy* (ADR-0025's framing): zbot's category weights / contradiction penalty / KG decay / episodes stay in gateway-memory.

## Decision

### 1. The contract — `RecallFusionConfig`

```jsonc
// `[recall_fusion]` profile section OR `<root>/.engram/recall.json`
{
  "rrf_k": 60,                      // RRF constant (>= 1); default 60
  "default_source_weight": 1.0,     // weight for any lane not in source_weights; default 1.0 (pure RRF)
  "source_weights": {               // per-lane overrides; keys = normalized lane source tags
    "vector": 0.7,
    "lexical": 0.3
  },
  "rerank": {                       // optional reranker
    "strategy": "mmr",              // none | mmr | cross_encoder
    "lambda": 0.5                   // MMR relevance/diversity trade-off in [0, 1]
  }
}
```

- **Serde + snake_case** (`RecallFusionConfig` / `RerankConfig` in `engram-retrieval/src/config.rs`), so the same struct deserializes from a TOML profile section and a JSON discovery file.
- **Keys = lane source tags.** The weight keys are the *normalized* `fusion_trace.source` strings the lanes stamp (`vector`, `lexical`, `graph`, `associative_graph`, `community_summary`, `temporal`, `facts`, `belief`), surfaced as `engram_retrieval::KNOWN_LANE_TAGS`. This is why RFC-0019 D5 (lane-tag normalization) is load-bearing: a weight keyed on the pre-normalization `vector.semantic` would be silently inert.
- **Validation on load** (`RecallFusionConfig::to_reciprocal_config`): `rrf_k >= 1`; weights finite & ≥ 0; `rerank.lambda ∈ [0, 1]` (Nit 6 — symmetry with weight validation). An *explicit* profile section that fails validation surfaces as a load error; a *discovered* `.engram/recall.json` that fails surfaces as a boot error in the MCP feeder (Blocker 1) rather than silently degrading.

### 2. The loading ladder — explicit > `.engram/recall.json` > None

`EngramConfig::resolve_recall_fusion(explicit, discovery_root)`:

1. **Explicit** — an already-loaded `[recall_fusion]` profile section (eager-validated in `from_profile_file`). Wins.
2. **`.engram/recall.json`** — `EngramConfig::discover_recall_fusion(discovery_root)` reads `<discovery_root>/.engram/recall.json` directly (no `exists()` probe, mirroring the `scan.json` ladder in `engram-mcp::codegraph`, so a transient removal between probe and read cannot mislead). `NotFound` ⇒ `Ok(None)`; read/parse/validate failure ⇒ `Err`.
3. **None** — equal-weight RRF default (backward-compatible).

The MCP server (`engram_mcp::bootstrap::open_provider`) resolves from the storage path (rung 2; v1 `McpConfig` carries no profile path, so rung 1 is unreachable from the MCP — operators use `.engram/recall.json`). A present-but-invalid discovered file is a **boot error**, not a silent degrade: the operator wrote a config expecting weighted fusion; falling back silently would hide the malformed file until someone noticed recall "feels wrong".

### 3. Wiring — reuse, never reimplement

- **Weighted RRF reuse.** `SqlUnifiedRecall::with_reranker` takes the `ReciprocalFusionConfig` built by `to_reciprocal_config`; it never re-implements fusion (ADR-0009). `ReciprocalFusionConfig::default()` reproduces equal-weight behavior.
- **Reranker dispatch.** `bootstrap_sqlite::select_reranker` reads `recall_fusion.rerank` and dispatches `None`/`Mmr`/`CrossEncoder` by strategy. MMR **injects an `EmbeddingProvider`** (the `RetrievalReranker` port exposes no embeddings — RFC-0019 D4) and degrades to relevance-only with a warning when no embedder is wired; cross-encoder warns + falls back when no scorer is wired (deferred — see D2.1c).
- **Vector default-on for the MCP (RFC-0019 D3 reversed).** The `fastembed` cargo feature is in `engram-mcp`'s `default`, so a fresh `cargo build -p engram-mcp` ships the vector lane + embedding provider out-of-the-box. Two disable paths remain: **runtime** — `enable_vector = false` (MCP `--no-vector`) skips wiring the vector index + embedding model + vector recall lane at boot, leaving `vectors = None` even when fastembed is compiled in (lets a deployment avoid the model download/load without a rebuild); **build-time** — `--no-default-features` drops fastembed entirely. The config sets the lane's *weight*; the runtime/build switches set its *presence*. `capability_report` reflects the resolved state. This reverses the original D3 opt-in choice (operator preference: works out-of-the-box).
- **No application policy.** Category weights, contradiction penalty, KG decay, episodes, and intent boost (zbot's gateway-memory techniques) are **not** in this contract and do not belong in `engram-retrieval`, `engram-integration`, or any adapter (ADR-0022, no-god-module). They are consumer epistemics over a consumer's own store.

### 4. Vocab honesty — warn on unknown keys

`to_reciprocal_config` warns (stderr) on `source_weights` keys outside `KNOWN_LANE_TAGS`. A typo like `"vectors"` (vs `"vector"`) would otherwise be silently inert — the same defect class RFC-0019 fixed for the lane tags themselves. The warning is **non-fatal** (warn, not error) so a deployment running an older engram that later adds a lane does not fail to boot on a config naming that new lane. The pure helper `unknown_lane_keys(&BTreeMap)` is unit-testable independent of stderr capture.

## Decision drivers

- **Operator tunability without code change** — the primary force; the contract exists so fusion is config-driven.
- **Engine neutrality (ADR-0022)** — keyed on lane source tags, not engine types; ports to any engine that stamps the vocabulary.
- **Reuse over invention** — weighted RRF, the reranker port, and the composition seam already existed.
- **Backward compatibility** — absent config ⇒ today's behavior.
- **Honest scope (ADR-0025 framing)** — mechanism, not application policy.

## Consequences

**Positive:**

- Operators bias vector/BM25/graph + select a reranker by editing `.engram/recall.json` (or a profile section); no code change, no rebuild to activate vector (it is default-on for the MCP), and `--no-vector` skips the model load at boot without a rebuild.
- The dormant weighted-fusion + reranker machinery is now reachable; engram's recall is a competent generic full-hybrid retriever.
- The config ports to any future engine (pgvector's `PgUnifiedRecall` is the deferred follow-on — `docs/backlog.md` → `pgvector-recall-fusion`) because it is keyed on lane tags, not engine types.

**Negative:**

- One more public config surface to keep stable and documented (`rrf_k`, `default_source_weight`, `source_weights`, `rerank.{strategy,lambda}` + the lane vocabulary). Mitigation: the JSON schema at `contracts/v1/schemas/recall-fusion.schema.json` + `KNOWN_LANE_TAGS`.
- The lane-tag vocabulary is now a soft contract: a future lane that stamps a new tag must add it to `KNOWN_LANE_TAGS` or operators see a (non-fatal) warning. Mitigation: warn-not-error + a comment listing the stamping sites.
- Cross-encoder selection is honest-but-disappointing: it dispatch-recognizes `cross_encoder` but warns + falls back rather than re-scoring (no in-tree model). The deferral is explicit in the spec (D2.1c) and backlog (`cross-encoder-rerank`).

**Revisit if:** a second fusion algorithm is needed (weighted-sum is already in-tree as `WeightedFusionConfig` — a future selector could expose it), the lane vocabulary grows enough that the warn-only vocab check should tighten to an error, or an operator need cannot be met without application policy (which would then belong in a consumer layer, not here).

## Confirmation

- **Mode:** reviewer-checked + schema-checked.
- **Signal:** `EngramConfig` carries `recall_fusion: Option<RecallFusionConfig>` with `.with_recall_fusion` + `discover_recall_fusion` / `resolve_recall_fusion`; `SqlUnifiedRecall::with_reranker` takes a `ReciprocalFusionConfig`; `bootstrap_sqlite` reads `config.recall_fusion`; the MCP `open_provider` feeder resolves `.engram/recall.json` and surfaces a malformed file as a boot error; `to_reciprocal_config` validates `k`/weights/lambda and warns on unknown vocab keys. The JSON schema at `contracts/v1/schemas/recall-fusion.schema.json` is the machine-readable contract artifact.
- **Owner:** phanijapps (architecture).

## Alternatives considered

- **Weighted-sum fusion (`WeightedFusionConfig`) as the default.** Rejected: weighted-sum is score-scale-sensitive; weighted RRF is rank-based and more robust to heterogeneous lane scores. `WeightedFusionConfig` stays in-tree as a non-default option for a future selector.
- **A new fusion algorithm.** Rejected (YAGNI): weighted RRF + MMR are standard hybrid-IR techniques; the gap was wiring + config, not algorithm.
- **Extending `RetrievalReranker` / `RetrievalResult` to carry embeddings (for MMR).** Rejected: a domain-truth change for one adapter (RFC-0019 D4). MMR **injects an `EmbeddingProvider`** instead, leaving the port unchanged.
- **Vector opt-in (original RFC-0019 D3).** Superseded — the original "off by default" choice is reversed: vector is now default-on for the MCP. The runtime `--no-vector` disable preserves the "skip the model download/load" escape hatch without forcing a rebuild, and `--no-default-features` preserves the build-time disable. The config still sets the weight; the runtime/build switches set presence.
- **Silent soft-fail on a malformed discovered file (mirroring `scan.json`).** Rejected for the MCP feeder: an operator who wrote a `[recall_fusion]` config expecting weighted fusion should not have it silently ignored. An *absent* file is `Ok(None)` (equal-weight default); a *present-but-bad* file is a boot error. (`scan.json` soft-fails because a bad filter is a nicety; a bad fusion config is the operator's primary ranking knob.)
- **Hard-error on unknown vocab keys.** Rejected: would break older configs the day a lane is added. Warn instead; the pure `unknown_lane_keys` helper lets tooling fail closed if it wants to.

## References

- [RFC-0019](../rfcs/0019-hybrid-recall-fusion.md) — hybrid recall fusion; D1 (this contract), D2 (reranker selection), D3 (vector default-on for the MCP + runtime/build disable — reverses the original opt-in), D4 (MMR injected embedder), D5 (lane-tag vocabulary), D6 (app-policy fence).
- [ADR-0009](0009-retrieval-composition-seam.md) — retrieval composition seam; the RRF + `compose_context` machinery this reuses.
- [ADR-0022](0022-engine-grid-vs-backend-recipe.md) — engine neutrality; why the config carries no engine type.
- [ADR-0025](0025-framework-content-boundary.md) — framework/content boundary; the "mechanism, not application policy" framing this contract applies to recall fusion.
- `docs/specs/recall-fusion-config/spec.md` — the spec this ADR records the contract for.
- `contracts/v1/schemas/recall-fusion.schema.json` — machine-readable contract artifact.
