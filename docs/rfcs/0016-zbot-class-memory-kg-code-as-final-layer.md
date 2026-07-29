# RFC-0016: engram-mcp as a Zbot-class memory+KG layer, with code graph as the final layer
<!-- Evolve the unified engram-mcp (RFC-0015) from a code-centric surface into a full
Zbot-class memory+KG layer — surfacing the provider handles it already owns (beliefs,
hierarchy, bi-temporal, ontology/taxonomy writes, KG traversal) — with the code graph as the
topmost layer over one unified graph. Scoping is Org → Domain → Subdomain (Zbot's ward,
recast). Procedures/episodes/wiki deferred. Foundation (single-file store + subgraph
bridging) in docs/research/engram-mcp-connectivity-and-single-file-storage.md. -->

- **Status:** Accepted (2026-07-29)
- **Author:** phanijapps
- **Approver:** phanijapps *(solo project)*
- **Date opened:** 2026-07-29
- **Decision weight:** standard
- **Related:** RFC-0015 (unified MCP), RFC-0013 (context-graph packets), RFC-0014 (canonical KG identity), RFC-0008 (cross-repo linkage), ADR-0008 (OntologyRepository), ADR-0022 (engine neutrality / surface parity), [`docs/research/engram-mcp-connectivity-and-single-file-storage.md`](../research/engram-mcp-connectivity-and-single-file-storage.md) (foundation defects + fix plan), [`docs/research/agentzero-engram-memory-integration-comparison-matrix.md`](../research/agentzero-engram-memory-integration-comparison-matrix.md)

## Reviewer brief

- **Decision:** Evolve `engram-mcp` (RFC-0015) from its current *code-centric* shape (8 of 20 tools are code-graph; 0 cover beliefs / hierarchy / procedures / episodes / distillation) into a **Zbot-class memory+KG layer**, by surfacing the `EngramProvider` handles it already owns, and make the **code graph the topmost layer over one unified knowledge graph** rather than a parallel silo. Scope the first pass to the provider-owned layers Zbot has and the MCP lacks — **graph, hierarchy, belief** — plus distillation rigor; **ontology/taxonomy stay as-is** (read-only launch config); defer procedures / episodes / wiki / goals.
- **Recommended outcome:** accept.
- **Change if accepted:**
  - **Foundation (ship first):** default the MCP to a single-file `engram_data.db` matching the Zbot adapter invariant; bridge the code↔concept subgraphs so code entities are citizens of the unified KG (not a separate lane). Detail in the related research doc.
  - **Surface the Zbot-parity layers the provider already owns** — **graph** (KG traversal / subgraph / alias-resolution), **hierarchy** (LeanRAG clusters), and **belief** (write / list / retract / contradiction). Bi-temporal (`valid_from` / `as_of`) rides with belief + recall, not as a standalone surface.
  - **Ontology/taxonomy stay exactly as they are** — multi-layer, read-only, supplied as MCP launch config. No write or enforce-validate surface is ported from Zbot.
  - **Recast Zbot's `ward` scoping as Org → Domain → Subdomain** via engram's `Scope` (`tenant = Org`, `workspace = Domain[/Subdomain]`).
  - **Code composites traverse the unified graph** — `symbol_context` / `architecture` / `get_context` see concepts and code together.
  - **Distillation stays agent-side** (the `engram-distill` skill, strengthened to require cross-layer bridging edges), formalized as the primary KG-population path; the server never calls an LLM.
- **Affected surface:** `mcp/engram-mcp` (new tools, config flags, scoping model); `adapters/ingest` (subgraph bridging, wire `MarkdownChunker`). **No `core/domain` contract break required** — every surfaced layer corresponds to an existing provider handle. Procedures (no handle) is the one deferred exception.
- **Stakes:** additive tool wiring + one scanner change + a scoping-model recast; low contract risk, moderately costly-to-reverse on the scoping model and the subgraph-bridge predicate set (those shape the graph's semantics).
- **Review focus:** (1) the **surface-not-build** framing (D1); (2) the **Org → Domain → Subdomain** scoping model recasting Zbot's ward (D2); (3) **subgraph bridging** strategy — deterministic floor + agentic enrichment (D3); (4) **single-file shared store** as the cross-consumer contract (D4); (5) the **narrowed parity scope** — graph + hierarchy + belief come in from Zbot; ontology/taxonomy stay as-is; the MCP's code-graph strengths are preserved and not overridden by Zbot (D5/D6).
- **Not in scope:** procedures / episodes / wiki / goals (Layer 6 + companions — procedures has no provider handle; the rest are Zbot-specific and lower-value for a code-centric MCP); **ontology/taxonomy write or enforce-validate surface** (the read-only launch-config approach is approved as-is — D6); server-side LLM extraction (rejected, as in RFC-0015 D4); SurrealDB backend parity (deferred); enforced write-rejecting ontology validation (stays advisory, ADR-0008); a reusable `engram-mcp-core` library (YAGNI until a second consumer); changing engram core's `MultiFileDirectory` default (the override is MCP-boundary only).

## The ask

**Recommendation (BLUF):** Approve evolving `engram-mcp` into a Zbot-class memory+KG layer whose **code graph is the final layer of one unified knowledge graph**, scoped Org → Domain → Subdomain, delivered in stages (foundation first, then surfacing provider-owned layers, then code-over-unified-graph). Record six ratified-by-dialogue decisions (D1–D6).

**Why now (SCQA):**
- *Situation:* `engram-mcp` (RFC-0015) ships 20 tools, but routes through only **8 of 20** `EngramProvider` handles. 8 of the 20 tools are code-graph; **zero** cover beliefs, hierarchy, procedures, episodes, distillation, or ontology writes. The `EngramProvider` is **already a superset of Zbot's memory+KG** (it owns beliefs, hierarchy, bi-temporal, ontology, taxonomy — all wired, all unrouted). Zbot itself has **no code-graph layer** (its only code hook is one ward-scoped `upsert_primitive` fact).
- *Complication:* The MCP is code-centric **by accident, not design** — the generic memory+KG layers the provider owns are unreachable from the agent surface, and the code graph is a **silo, not a layer** (dogfooding confirmed: code and concept subgraphs are disconnected; storage is 5 files instead of the 1 the Zbot adapter expects). The substrate for the goal already exists; it is just not exposed or unified.
- *Question:* Do we evolve the MCP to surface the full Zbot-class memory+KG (what the provider already owns) and make the code graph the topmost layer of one unified graph — recasting Zbot's `ward` as Org → Domain → Subdomain?

**Decisions requested** (ratified in design dialogue; recorded here for the record):

| ID | Question | Decision | Why | Decide by |
| --- | --- | --- | --- | --- |
| D1 | Build Zbot's model, or surface what the provider owns? | **Surface, not build** — wire the 12 unrouted handles into MCP tools; add no new domain modeling | The provider already owns beliefs/hierarchy/bi-temporal/ontology-writes/KG-traversal. The work is additive tool wiring, not domain design — except procedures (no handle), which is deferred (D5). Keeps engram contract-first. | RFC acceptance |
| D2 | What replaces Zbot's `ward` scoping unit? | **Org → Domain → Subdomain**, via `Scope { tenant: Org, workspace: Domain[/Subdomain] }` | "Ward" is Zbot's operational boundary; the MCP's analogue is an org/domain hierarchy. `tenant` already means the ownership boundary; the workspace path gives hierarchical isolation. No `core/domain` change for v1. | RFC acceptance |
| D3 | How does the code graph become a *layer* of the KG, not a silo? | **Deterministic floor + agentic enrichment** — generalize the scanner's cross-file resolver past `calls` and emit concept→code `describes` edges; strengthen `engram-distill` to require across-predicate bridging | A bare `scan_repo` must yield a connected graph (deterministic floor); the agent adds semantic `describes`/`realized_by` edges (enrichment). Additive only — never drops existing `calls`/`belongs_to`/`mentions`. | RFC acceptance |
| D4 | One store or many? | **One single-file `engram_data.db`**, shared shape with the Zbot adapter | The MCP and the agentzero gateway must consume the *same* DB; the adapter already defaults to `SingleFile { "engram_data.db" }`. The MCP's current multi-file default violates `agentzero/docs/specs/engram-provider-adoption/spec.md`. | RFC acceptance |
| D5 | Which Zbot layers come in, and what stays? | **Graph + hierarchy + belief come in** (all provider-owned, surfaced); **ontology/taxonomy stay as-is** (D6); the **code-graph layer is preserved** (Zbot has no analogue and does not override it). Procedures/episodes/wiki/goals deferred. | Parity without loss: bring in what Zbot has and the MCP lacks; keep what the MCP does better. | RFC acceptance |
| D6 | Does the MCP gain ontology/taxonomy *write* + validation (Zbot has it)? | **No** — ontology/taxonomy stay multi-layer, read-only, MCP launch config | The current approach is approved as-is; adding writes/enforcement is scope creep and risks the framework/content boundary (RFC-0013). | RFC acceptance |

## Problem & goals

**Problem.** Today the `engram-mcp` server exposes a code-graph surface with a thin generic underlay: agents can scan and query code, write loose facts/entities, and recall — but they cannot manage **beliefs** (the synthesized "current position"), traverse the **knowledge graph**, use **bi-temporal** semantics (valid-time, supersession), build/query **hierarchy** (LeanRAG clusters), or **write/validate** the ontology/taxonomy. Worse, the code graph and the distilled concept graph are **disconnected subgraphs**, so "code as a layer of the KG" is not yet true — and the server writes **five** SQLite files where the Zbot adapter expects one. The capability substrate for a Zbot-class layer already exists in `EngramProvider`; it is simply not exposed.

**Goals.**
1. **Parity with Zbot's memory+KG, additive only** — bring in the Zbot layers the provider owns but the MCP doesn't surface: **graph** (KG traversal), **hierarchy**, and **belief**. The MCP loses nothing it has today; areas where the MCP is stronger (the code-graph layer) stay, and Zbot does not override them.
2. **Code graph as the final layer** — code entities are citizens of one unified knowledge graph; the code-intel composites traverse concepts + code together.
3. **One shared store** — a single-file `engram_data.db` consumable by both the MCP and the Zbot gateway.
4. **Org → Domain → Subdomain scoping** — Zbot's ward, recast as an org/domain hierarchy on `Scope`.
5. **Distillation as the primary ingestion path** — agent-side extraction (the `engram-distill` skill), strengthened to require cross-layer bridging; the server stays deterministic and LLM-free.
6. **Staged delivery** — foundation ships working value first; each subsequent layer is its own spec + work-loop slice.

**Non-goals** (deliberately dropped).
- **No procedures / episodes / wiki / goals in v1.** Procedures (Layer 6) is the one layer with no provider handle — it needs a new `ProcedureRepository` port + adapter and is deferred. Episodes/wiki/goals are Zbot-specific and lower-value for a code-centric MCP.
- **No server-side LLM extraction.** Distillation is the agent's job (skill); the server only stores what it is given (RFC-0015 D4 holds).
- **No `core/domain` contract break.** Every surfaced layer maps to an existing handle. The scoping model uses existing `Scope` fields.
- **No enforced ontology validation** — stays advisory (ADR-0008).
- **No ontology/taxonomy write surface.** The current approach — multi-layer ontology + taxonomy supplied as read-only MCP launch config — is approved as-is (D6). Zbot's write/validate path is not ported.
- **No SurrealDB parity in v1** — new exposure lands on the SQLite adapter; Surreal deltas reconciled later.
- **No reusable `engram-mcp-core` library** — YAGNI until a second consumer (Zbot consumes engram as a *library*, not via MCP).

## Proposal

### The layered target

The MCP becomes a layered stack. Each lower layer is the substrate the layer above queries; **code is the topmost layer** — the differentiator Zbot never had.

```
Layer 7  CODE GRAPH            scan_repo + composites over the UNIFIED graph
Layer 6  reflective synthesis  consolidate (belief synthesis + decay)        [PARTIAL today]
Layer 5  beliefs + contrad.    provider handle UNROUTED                      [surface]
Layer 4  hierarchy (LeanRAG)   provider handle UNROUTED                      [surface]
Layer 3  bi-temporal           valid_from / as_of / supersede params         [surface]
Layer 2  knowledge graph       entities/rels + traverse/subgraph/alias       [finish]
Layer 1  facts                 write_memory + store_knowledge + distill      [finish]
Layer 0  ONE shared DB         single-file engram_data.db                    [foundation]
```

Zbot's own stack is layers 1–6 (facts/episodes → KG → bi-temporal → hierarchy → beliefs → procedures → reflective synthesis). The MCP targets **1–5** (all provider-owned) and adds **7** (code) — the layer Zbot lacks. Code sits *above* the Zbot mimic because it is the most structured, precise source, layered onto the same entities/relationships the lower layers own.

### The capability-parity finding (why this is tractable)

A capability-parity audit against the Zbot store traits and the agentzero 7-layer memory spec found that the path to "mimic Zbot memory+KG" runs through **wiring the unrouted provider handles into MCP tools, not through changing the engram domain.** Today the MCP routes only `memory`, `knowledge`, `graph`, `knowledge_query`, `lexical_feed`, `recall`, `consolidation`, `batch`; among the others, `beliefs`, `hierarchy`, and the richer KG-read operations are wired but unreachable. The layers to surface for parity are: **beliefs** (Layer 5), **hierarchy** (Layer 4), and **KG traversal/subgraph/alias** (Layer 2) — all surfaced, not built. **Ontology/taxonomy writes are explicitly *not* ported** (D6); the read-only launch-config approach is approved as-is. Bi-temporal (`as_of`/`valid_from`) rides with belief + recall. The single true build gap is **procedures** (no handle), deferred by D5.

### Scoping model (D2): ward → Org → Domain → Subdomain

Zbot scopes every write/recall by `ward_id` (a bounded operational context). The MCP's analogue is an **organizational hierarchy**:

- `Scope.tenant` = **Org** — the ownership/billing boundary (the literal meaning of "tenant").
- `Scope.workspace` = **Domain** with an optional **/Subdomain** path component (e.g. `checkout/payment-ui`).

Under `ScopeMappingStrategy::Strict` (the MCP's current strategy), the full workspace string isolates subdomains exactly — sibling subdomains cannot blend. Cross-subdomain recall within a domain (e.g. all of `checkout/*`) is a later concern addressable via a hierarchical matching strategy or an explicit query; **isolation is the v1 priority**, matching the fused-per-project invariant from RFC-0015.

Launch config gains `--org <Org>` and `--domain <Domain> [--subdomain <Subdomain>]` (the existing `--project` becomes the workspace path for backward compatibility). This is a tooling change in the MCP, not a `core/domain` change — `Scope` already carries these fields.

### Subgraph bridging (D3): deterministic floor + agentic enrichment

The disconnected-subgraph defect (full root-cause in the related research doc) is the visible proof that code is not yet a layer of the KG. The fix has two complementary mechanisms, shipped together:

1. **Deterministic floor** (in `adapters/ingest`): generalize the scanner's cross-file resolver (`scanner.rs:428-442`, currently gated on `predicate == "calls"`) to also resolve `mentions`, and emit **concept→code `describes`** edges when a concept (or its doc-chunk heading) matches a code-symbol name in the global `name_index`. Wire the existing-but-unused `MarkdownChunker` into the scanner so heading anchors are first-class. A bare `scan_repo` then yields a connected graph with no LLM.
2. **Agentic enrichment** (the `engram-distill` skill): strengthen the skill to *require* (not just encourage) across-predicate bridging — after `scan_repo`, list code-entity names (via `search`/`KnowledgeQuery`) and emit at least one `describes`/`realized_by`/`governs` edge per concept that maps to a scanned artifact. This is the semantically rich layer the deterministic pass cannot produce alone.

**Boundaries:** bridging is **additive** — new edges only; never drops `calls`/`belongs_to`/`mentions`; no v1 contract-field or generated-type change. The deterministic bridge is lexical and best-effort; its precision limits are documented, not hidden. Predicate strings stay free-form (`KnowledgeRelationship.predicate: String`); no predicate enum constrains the agentic layer.

### Single-file shared store (D4)

The MCP defaults to `SqliteStorageLayout::SingleFile { file_name: "engram_data.db" }` (via `.with_sqlite_storage_layout(...)` in `open_provider`), matching the Zbot adapter's invariant exactly so the gateway can reopen the same DB. A `--layout single|multi` flag keeps the multi-file path available for tests/advanced use without making it the default. The existing 5-file deployment is migrated by re-indexing into a fresh single-file DB (the data is regenerable; no in-place cross-store merge). Engram core's default stays `MultiFileDirectory` (backward compatible); the override is MCP-boundary only.

### Ontology & taxonomy defaults (lightweight, expandable)

The baked-in defaults are deliberately **lightweight but extensible** — a 3-layer *generic technology ontology* (a rich `technical` layer + light `domain`/`business` layers) plus a SKOS-aligned taxonomy (`label`↔`skos:prefLabel`, `broader`↔`skos:broader`), so the MCP runs with a meaningful vocabulary zero-config. Both stay overridable per-project via `--ontology`/`--taxonomy` (D6 — read-only launch config). The expansion path is guaranteed by design, not deferred: every config field is JSON-driven and additive (`#[serde(default)]`), so growing the model later — full SKOS (`altLabel`/`related`/`ConceptScheme`), richer classes/predicates, project-specific content — is a backward-compatible addition, never a migration. Enriching the current placeholder defaults (a single `generic` layer `[Concept, Entity, Relation]` and a single-concept taxonomy) is a **P0-adjacent task**: the subgraph bridge's `describes`/`realized_by` predicates resolve against a real ontology, so the default must be real before the bridge is meaningful zero-config.

## Phased plan

Each phase becomes its own spec under `docs/specs/` and runs through `work-loop`. Phases are ordered so the cheapest, highest-certainty work lands first and unblocks shared-DB consumption.

| Phase | Scope | Decisions | Risk | Depends on |
| --- | --- | --- | --- | --- |
| **P0 — Foundation** | single-file store + subgraph bridge (deterministic floor) | D3, D4 | low (storage) / medium (scanner) | — |
| **P1 — Facts + KG completion** | bi-temporal params on writes/recall; KG `traverse`/`subgraph`/`alias` tools | D1 | low (surfacing) | P0 |
| **P2 — Beliefs + hierarchy** | belief write/list/retract/contradiction tools; hierarchy summary/cluster tools | D1 | low (surfacing) | P1 |
| **P3 — Distillation rigor** | formalize `engram-distill` as the primary ingestion path (require cross-layer bridging edges); bi-temporal `as_of` on recall + `valid_from` on writes | D1, D3 | medium | P2 |
| **P4 — Code over unified graph** | code composites traverse concepts+code; scoping model (`--org`/`--domain`/`--subdomain`) | D2, D3 | medium (scope migration) | P3 |
| *(deferred)* | procedures / episodes / wiki / goals | D5 | high (new port) | — |

**Sequencing rationale.** P0 ships immediately useful value (one DB, connected graph) and is the smallest change. P1–P3 surface provider-owned layers additively. P4 elevates the code layer onto the now-complete KG and lands the scoping model. The scoping recast is last among the surfaced work so it migrates a *finished* surface, not a moving target.

## Risks

- **Scoping-model migration.** Recasting `--project` → Org/Domain/Subdomain changes how existing deployments scope data. Mitigation: keep `--project` as a workspace-path alias for backward compatibility; migrate `~/.engram/agentzero` by re-indexing.
- **Deterministic-bridge precision.** Lexical concept→code matching produces false positives. Mitigation: start with heading-anchor + exact symbol-name matches only; keep prose co-occurrence behind a flag; document precision limits.
- **Re-index cost.** `knowledge.db` is ~131 MB; a full re-index takes minutes. Acceptable as a one-time migration; confirm scan idempotency before automating.
- **Version skew (MCP ↔ adapter).** The MCP is built from the workspace `core/integration`; the Zbot adapter from a git checkout of engram. The `SqliteStorageLayout` + surfaced-handle APIs must stay compatible across both. Mitigation: pin the adapter's checkout to a known-compatible ref, or add a cross-consumer conformance test.
- **Surface-parity lint (ADR-0022).** Each new tool that reaches a provider handle must also be reachable via N-API `bindings/node` and reflected in `CapabilityReport`; a parity lint is the intended enforcement.

## Open questions

- **Cross-subdomain recall.** Is strict subdomain isolation sufficient for v1, or do we need parent-domain recall (a hierarchical `ScopeMappingStrategy`) early? (Default: isolation first.)
- **Belief auto-promotion.** Should `consolidate` optionally promote high-confidence, low-contradiction facts to beliefs automatically, or stay explicit (RFC-0015 non-goal)? (Default: explicit.)
- **Procedure handle.** If procedures become in-scope later, does the MCP add a `ProcedureRepository` port to engram-core, or model procedures as a typed `KnowledgeEntity` kind within the existing KG? (Deferred by D5.)

## What "done" looks like (for the accepted scope)

- The MCP writes exactly **one** `engram_data.db`, reopenable by the Zbot gateway.
- A `scan_repo` over a mixed code + markdown fixture yields **one connected component** (concept ↔ function reachable via `describes`).
- Agents can **write, list, retract, and query beliefs**; traverse the KG (`subgraph`/`traverse`/`alias`); use **bi-temporal** `as_of`; build/query **hierarchy**; **write** the ontology/taxonomy — all over the provider handles.
- The code-intel composites return concept nodes in a symbol's neighborhood.
- Launch config expresses **Org → Domain → Subdomain**; subdomains are isolated under Strict matching.
- The `engram-distill` skill emits cross-layer bridging edges as a required step.
- Every new tool is reflected in `CapabilityReport` and reachable via N-API (ADR-0022 parity).
