# RFC-0018: Code-intelligence retrieval quality — easy high-impact fixes
<!-- short, identifying title; the fuller explanation lives in "The ask" -->

- **Status:** Accepted <!-- Draft | Open | Final Comment Period | Accepted | Rejected | Withdrawn | Experimental -->
- **Author:** phanijapps
- **Approver:** phanijapps
- **Date opened:** 2026-08-01
- **Date closed:**
- **Decision weight:** light <!-- reversible, narrow, query-layer-only; no frozen-v1 contract change or ADR reversal -->
- **Related:** RFC-0012 (code-structural-graph-layer), RFC-0013 (context-graph-packets), RFC-0014 (canonical identity), RFC-0015 (unified engram MCP), ADR-0022 (engine grid). Evidence base: the zbot evaluation at `/home/videogamer/projects/agentzero/docs/engram-debugging-evaluation.md` (external repo) and the in-session four-agent root-cause analysis summarized in project memory `codegraph-retrieval-defects-root-cause`.

## Reviewer brief

- **Decision:** Approve three low-risk fixes that remove the three worst user-visible defects from the zbot code-debugging evaluation, scoping deliberately away from the larger architecture work.
- **Recommended outcome:** accept.
- **Change if accepted:**
  - `engram-graph-analytics` gains **additive** bounded-traversal variants; `symbol_context`/`change_impact` depth defaults are lowered. (§6.1 flood)
  - `engram-integration` (the facade) gains an **additive** `LexicalSearch` trait; MCP `search` routes through it with entity-id resolution instead of whole-string `.contains()`. (§6.3 search)
  - `fetch_rels` becomes fallible; the five pure graph tools propagate errors, `get_context` degrades with a note. (§6.6 inconsistency)
- **Affected surface:** `core/graph-analytics` (additive bounded variants), `engram-integration` (additive `LexicalSearch` trait + SQLite impl), `codegraph/queries` (additive bounded siblings), `mcp/engram-mcp/src/codegraph.rs`, and tests. `bindings/node` unchanged (bounded variants deferred). No frozen-v1 contract change, no storage schema change, no migration.
- **Stakes:** reversible — additive surface + behavior-only changes behind existing tools.
- **Review focus:** (1) the visited-cap must not silently truncate meaningful neighborhoods (mitigated by a `truncated` flag surfaced on BOTH `symbol_context` and `change_impact`, + a calibrated cap); (2) the new `LexicalSearch` trait is the one genuinely additive surface change — confirm it belongs on the facade.
- **Not in scope:** qualified/per-definition identity, killing the full-load model, edge provenance (§6.5), field-level analysis (§6.4), a typed code lane in recall (§6.2), and closing the pre-existing composite-parity gap (symbol_context/change_impact are MCP-only; their graph primitives are on the N-API binding but the composites are not). All deferred and linked in Non-goals.

## The ask

- **Recommendation (BLUF):** Approve three low-risk fixes that remove the three worst user-visible defects from the zbot evaluation — the depth-3 neighborhood flood (§6.1), the multi-term search "no results" (§6.3), and the inconsistent empty-graph results (§6.6) — deferring the larger architecture work those defects ultimately point to. Two of the three are behavior-only; D2 adds one additive read handle to the SDK facade.

- **Why now (SCQA):**
  - *Situation:* The zbot evaluation used engram's code-intel tools to debug a real React reducer bug and rated them 7/10, with reproducible failures: `symbol_context` floods with hundreds of unrelated symbols, multi-term `search` returns nothing, and graph results are intermittently empty.
  - *Complication:* A four-agent root-cause investigation traced all six catalogue defects to two deep causes — a name-collapsed, unprovenanced, full-loaded code graph, and no first-class "code" concept in the retrieval stack. The deepest fixes (qualified identity, an indexed no-full-load backend, field-level extraction) are large and partly architectural.
  - *Question:* What is the smallest set of changes that removes the worst user-visible defects *now*, deferring the architecture work until justified by scale?

- **Decisions requested:**

  | ID | Question | Recommendation | Why | Decide by | Reviewer action |
  | --- | --- | --- | --- | --- | --- |
  | D1 | How do we stop the neighborhood flood (§6.1)? | Add **additive** bounded-traversal variants to `engram-graph-analytics` + lower tool depth defaults (2→1, 3→2) + surface a `truncated` flag. | Cheapest, keeps the public API stable, kills the flood without touching identity or storage. | This review | Confirm the additive-variant approach; confirm default cap (proposed 64, calibrated). |
  | D2 | How do we make multi-term search work (§6.3)? | Add an **additive** `LexicalSearch` trait to `engram-integration`; route MCP `search` through it, resolving target_ids→entities. | The BM25 lane is sound; the facade already builds the `Arc<LexicalIndex>` in bootstrap but exposes no read path. One additive trait unlocks it. | This review | Confirm the trait belongs on the facade (vs. resolver branch, option ii). |
  | D3 | How do we stop inconsistent empty results (§6.6)? | Make `fetch_rels` fallible; propagate in the five pure graph tools; degrade with a note in `get_context`. | A transient store error or unwired capability currently masquerades as "no relationships" across six tools. | This review | Confirm the propagate-vs-degrade split for get_context. |

## Problem & goals

Diagnosis (from the root-cause investigation, reproduced live against engram's own code):

- **§6.1 flood** — `symbol_context "recall" depth 3` returns ~44 "callers" and ~140 "callees" (`add`, `all`, `get`, `from`, `lock`, `now`…). Generic names (`new`, `open`, `get`) are super-hubs (`new` betweenness 430,017); unbounded-fanout BFS through them explodes at depth ≥ 2.
- **§6.3 search** — `mcp search "reciprocal rank fusion"` returns "No results." `search()` (`mcp/engram-mcp/src/codegraph.rs:175-200`) lowercases the *entire* query and tests `"{name} {kind}".contains(whole_query)`; a multi-word phrase can never be a single entity's contiguous substring. The BM25 lane is not used.
- **§6.6 inconsistency** — `fetch_rels` (`codegraph.rs:162-168`) does `.unwrap_or_default()`, so a transient store error **or an unwired knowledge-query capability** yields an empty edge list (`[Graph] (none)`) that later calls contradict. `fetch_rels` is shared by six tools: `symbol_context`, `change_impact`, `code_health`, `architecture`, `whats_changed`, and `get_context`.

**Goals.**
1. A `symbol_context`/`change_impact` call returns a bounded, meaningful neighborhood — not a transitive explosion through generic hubs.
2. Multi-term and natural-language `search` queries return ranked symbol hits.
3. Graph tools fail loudly when the store is unavailable or the capability is unwired; never silently empty.

**Non-goals** (deliberately deferred — could-have-been-goals dropped for this RFC):
- **Qualified/per-definition identity** — stop collapsing `NativeProvider::recall` / `MemoryService::recall` into one node (`codegraph/queries/src/queries.rs:13-18`, `adapters/ingest/src/tree_sitter_chunker.rs:338-367`). The root cause of §6.1; medium effort, breaks the name-query contract. Follow-on, aligns with RFC-0014.
- **Kill the full-load model** — `fetch_rels` deserializes the whole relationship table per call; the hard scalability ceiling for kernel-sized repos. Needs indexed lookups / a CSR or graph-DB backend. Follow-on, aligns with ADR-0022.
- **Edge provenance (§6.5)** — stop discarding the call-site line; add file/line/edge-kind. Additive but touches extraction + storage schema. Follow-on.
- **Field-level analysis (§6.4)** — `EntityKind` has no field kind; tree-sitter matches defs + calls only. TS/JS incremental; C/C++ needs a compiler/data-flow backend (architectural limit). Follow-on.
- **Typed code lane in recall (§6.2)** — code vs docs indistinguishable after fusion; no path/subsystem scope; no per-lane budget. Follow-on, aligns with RFC-0013.
- **Composite surface parity** — surface `symbol_context`/`change_impact` on the N-API binding (their graph primitives already are). Pre-existing gap; deferred.

## Proposal

The three fixes are independent and ship as three small slices.

### D1 — Bounded, hub-aware traversal (§6.1)

All changes here are **additive siblings** — no existing public function or type is modified, so the N-API binding (which calls `cgq::blast_radius`, which delegates to `ancestors`, today) is unaffected; adopting the bounded variants there is deferred (composite parity Non-goal).

- `core/graph-analytics/src/reachability.rs`: add **additive** `ancestors_bounded`/`descendants_bounded`, each taking a `max_visited: usize` cap and returning `(set, truncated)`; keep the existing unbounded `ancestors`/`descendants` untouched (Rust has no default args, and `engram-graph-analytics` is a published pure-algorithms crate — its public API must stay stable). The cap is **per-direction** (ancestors and descendants are capped independently).
- `codegraph/queries/src/queries.rs`: add **additive** `symbol_context_bounded(...) -> SymbolContextBounded { ctx: SymbolContext, truncated: bool }` and `blast_radius_bounded(...) -> BlastRadiusBounded { callers, truncated }`; keep `symbol_context` and `blast_radius` (which returns `HashSet<String>`) unchanged. Leaving the originals untouched is what makes this additive — changing `blast_radius`'s return type would break its N-API JSON consumer (`bindings/node/src/codegraph.rs`).
- `mcp/engram-mcp/src/codegraph.rs`: the `symbol_context`/`change_impact` tools call the bounded variants; lower defaults `symbol_context` depth 2→1 and `change_impact` 3→2; surface the cap as an explicit arg; print `truncated` in both tools' output.
- The **depth reductions (2→1, 3→2) are the primary flood bound** — they cut `recall` from 184 nodes (depth-3) to 38 (depth-1) for `symbol_context` and to 28 callers (depth-2) for `change_impact`. The visited-cap is a **safety net** for raised-depth or super-hub queries, calibrated per-direction and measured live on `recall` (a bridge/hub whose *callees* explode but whose *callers* stay moderate: depth-1 = 20 callers / 18 callees; depth-2 = 28 / 84; depth-3 = 44 / 140). With cap = 64 per direction: at the new defaults neither direction truncates (38 and 28 < 64); if a caller raises depth, the callees direction (84 for `recall` at depth-2) exceeds 64 → bounded with `truncated` signaled. `change_impact` (callers only) truncates only for symbols with >64 transitive callers, with the signal; a caller widens the cap or lowers depth to see more.
- Optional follow-on (documented, not in this RFC): hub-degree pruning — skip traversal *through* high-degree nodes.

This is a bandage over the name-collapse root cause — it does **not** change identity or storage — but it removes the user-visible flood immediately and is fully compatible with the later qualified-identity fix.

### D2 — Route MCP `search` through BM25 (§6.3)

- Add an **additive** read capability as a new `LexicalSearch` trait in `engram-integration` (mirroring the write-only `LexicalFeed`), plus an inherent delegating method on `EngramProvider`. The read surface is a *trait* (so the change stays engine-neutral — the SQLite impl satisfies it, backed by the shared `Arc<LexicalIndex>` the bootstrap already constructs, `core/integration/src/sqlite/bootstrap.rs`), and the accessor is an inherent method on `EngramProvider` (a struct, so an inherent method can be added without touching a trait). Extending `LexicalFeed` itself would break every downstream impl unless given a default method; a new trait avoids that. Today there is **no read getter** anywhere on the facade; this adds one.
- `mcp/engram-mcp/src/codegraph.rs::search`: replace the whole-string `.contains()` loop with `provider.lexical_search(...)`, then resolve returned `target_id`s back to entities via the existing `KnowledgeQuery::list_entities`. This deliberately does **not** route through recall's `KnowledgeLexicalResolver`, which is chunk-only: an entity-id hit resolves to `Ok(None)` and is silently skipped by `LexicalRetrievalIndex::retrieve_candidates` (`adapters/retrieval/tantivy-lexical/src/retrieval.rs`), never surfaced.
- Accuracy note (correcting a prior draft): the N-API binding's BM25 path and the MCP lexical feed are **separate physical Tantivy indexes** in separate processes (the binding holds its own `LexicalIndex`, `bindings/node/src/knowledge.rs:175`). D2 does not "tap a shared index with the binding"; it gives the MCP tool BM25 semantics *like* the binding's, via the provider's own index. Parity net effect: the MCP gains a read capability it lacked; full composite-tool parity remains a deferred Non-goal.
- **Why BM25 (lexical) here, and not vector + reranking?** engram *has* both — a fastembed vector lane and a cross-encoder reranker trait — but neither is active for this tool today: the vector lane is feature-gated behind `fastembed` (**off by default**, `mcp/engram-mcp/Cargo.toml`; the running instance reports no vectors capability) and indexes code *chunks* (semantic bodies), whereas `search` returns symbol *entities* (names); the reranker is wired to `None` (`recall.rs:74`). BM25 is always-on, indexes exactly what `search` returns (entity names), and directly fixes §6.3. Semantic vector search + reranking is the stronger path for *meaning* queries, but it belongs in the deferred §6.2 code-lane work — gated on enabling `fastembed`, wiring the reranker, and adding chunk→entity resolution. That is a larger, separate initiative, not an "easy fix," so it is out of scope here.

### D3 — Make `fetch_rels` honest (§6.6)

- `mcp/engram-mcp/src/codegraph.rs::fetch_rels`: change from `Vec<KnowledgeRelationship>` (with two silent `.ok()`/`unwrap_or_default()` swallow points) to `Result<Vec<KnowledgeRelationship>, ToolError>`. Distinguish the two failure modes: unwired `knowledge_query` capability → "knowledge query capability not configured"; store error → propagated.
- The five pure-graph tools (`symbol_context`, `change_impact`, `code_health`, `architecture`, `whats_changed`) propagate the error — for them an empty graph *is* the result, so silent emptiness is wrong.
- `get_context` degrades: its primary value is recall; it computes `recall_text` first, then calls `fetch_rels` for the graph/code sections. On `fetch_rels` failure it returns recall plus a `(graph unavailable: …)` note rather than failing the whole call.

## Options considered

**D1 axis — *what* to bound** (MECE: visited-node count / fanout-per-node / node identity / edge pre-filter; "traversal depth" is the existing depth control the tool defaults already tune, folded into (i) rather than a separate option):
- *(i) visited-count cap, plus lowered tool depth defaults* — **recommended.** Generic, additive (new variants), calibrated; the depth reduction rides along on the same change.
- *(ii) fanout-per-node (hub-degree) pruning* — more precise (keeps meaningful neighbors, drops hub fanout) but needs degree precomputation + a tunable threshold. Documented follow-on.
- *(iii) identity-uniqueness (qualified identity)* — the true root-cause fix; medium effort, breaks the name-query contract. Non-goal.
- *(iv) edge pre-filter in `call_edges`* (e.g. drop test symbols / filter by path before traversal) — cheaper partial bound, overlaps with the full-load Non-goal. Follow-on.
- *do-nothing* — the flood stays; the tool remains unusable for real debugging (the zbot 7/10 cap).

**D2 axis — *how to get BM25 to the MCP tool*** (MECE: read the provider's index / extend the recall resolver / build a local index / do nothing):
- *(i) add a `lexical_search` read handle to the facade* — **recommended.** Additive; reuses the single shared provider index; natural home for a provider read capability.
- *(ii) extend `KnowledgeLexicalResolver` with an entity-id branch + route through recall `Keyword`* — also valid, benefits recall generally, but touches the shared resolver + recall lane (larger blast radius). Alternative.
- *(iii) build + hold a local `LexicalIndex` in the MCP, mirroring the binding* — works but duplicates the index in RAM and must rebuild after `scan_repo`. Not recommended.
- *do-nothing* — multi-term search stays broken.

**D3 axis — *how to handle store unavailability / unwired capability*** (MECE: propagate vs swallow, split by tool):
- *propagate in graph tools, degrade in get_context* — **recommended.** Honest; matches each tool's semantics.
- *swallow to empty* (current) — hides outages behind wrong-empty results.

## Risks & what would make this wrong

- **Pre-mortem:**
  - *The visited-cap hides real neighbors.* Mitigation: `truncated` surfaced via the bounded wrappers; cap is an explicit arg; the depth reduction (not the cap) is the primary flood bound, with the cap a safety net calibrated well above the depth-1 default.
  - *The `lexical_search` handle bloats the facade / is engine-specific.* Mitigation: it is a trait method returning neutral hits; the SQLite impl wraps the already-built index. Confirm it generalizes (Open Question #1).
  - *Error propagation breaks callers that relied on infallible empty graphs.* Mitigation: only `get_context` had a non-graph primary value and it degrades; the five graph tools failing loudly is the intended behavior. Document in capability behavior.
- **Key assumptions (falsifiable):**
  - The graph-analytics unit tests use ≤4-node chains, so a cap of 64 never triggers in tests (verified: `reachability.rs:149-225`).
  - The SQLite bootstrap's shared `Arc<LexicalIndex>` is reachable to wrap in a read adapter (verified in `bootstrap.rs`; the read handle is the additive wiring).
- **Drawbacks:** D1 is a bandage — the graph stays name-collapsed, so neighborhoods remain imperfect until qualified identity lands. D2 adds one facade method (small ongoing surface cost). D3 makes graph tools noisier on outages (intended). We accept this: the goal is to remove the worst user-visible failures now, not to fix everything.

## Evidence & prior art

- **Spike / de-risk:** (a) The traversal is pure, generic, dependency-free Rust with a `HashSet` dedup already present — the cap is a clean early-exit (`core/graph-analytics/src/reachability.rs:29-99`); bounded variants keep the originals stable. (b) The BM25 lane is sound (OR semantics, identifier tokenizer) and the facade already builds the index but exposes no read path — the additive handle is the missing wiring. (c) Cap calibrated by live measurement on `recall`: depth-1=38, depth-2=112, depth-3=184. (d) Parity not worsened: D2 adds an MCP read capability; composite parity is a pre-existing deferred gap.
- **Repo precedent:** RFC-0012 (the code graph layer), RFC-0014 (canonical identity — the deferred qualified-identity fix aligns here), RFC-0013 (context packets — the deferred recall/code-lane work aligns here), ADR-0022 (engine grid — the deferred no-full-load backend aligns here).
- **External prior art:** Code-graph hub explosion is the standard reason production code intelligence (SCIP/LS-based indexes) uses definition-site identity and bounded neighborhoods; BM25 over identifier-split tokens is the standard code-symbol keyword search. Stated as known practice; no unfetched citations.
- **Promoted research:** The four-agent root-cause investigation is the evidence base (in-session; summarized in project memory `codegraph-retrieval-defects-root-cause`). The original zbot evaluation lives in the agentzero repo at the path in *Related*.

## Open questions

1. **`LexicalSearch` trait shape** — new trait in `engram-integration` returning `{target_id, score}`, satisfied by the SQLite impl over the shared index, with an inherent delegating method on `EngramProvider`; entity resolution stays in the MCP tool. · owner: implementer · decide-by: spec.
2. **Cap value** — default 64 visited nodes, surfaced as `truncated`. · owner: implementer · decide-by: spec.
3. **Hub-degree pruning in this RFC or as a follow-on?** — recommended default: follow-on (documented in Proposal), to keep this RFC minimal. · owner: phanijapps · decide-by: this review.

## Follow-on artifacts

(To be filled on acceptance.)
- Spec: `docs/specs/codegraph-retrieval-fixes/` — three slices (bounded traversal, BM25 search, honest fetch_rels).
- No ADR required (no frozen-v1 contract change; one additive facade method). The deferred items (qualified identity, no-full-load backend, provenance, field-level, code lane, composite parity) each warrant their own RFC when taken up.
