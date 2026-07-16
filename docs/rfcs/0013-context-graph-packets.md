# RFC-0013: Context-graph packets for AI agents
<!-- Engram as a framework: packet/rules/trace primitives + composition machinery
are product; the domain taxonomy/ontology content is consumer-loaded. -->

- **Status:** Accepted <!-- accepted by the approver 2026-07-13 (solo project; no circulation period) -->
- **Author:** phanijapps
- **Approver:** phanijapps <!-- solo project; a third-party approver line would strengthen sign-off (review nit #10) -->
- **Date opened:** 2026-07-13
- **Date closed:** 2026-07-13
- **Decision weight:** standard
- **Related:** ADR-0009 (retrieval-composition seam), ADR-0022 (engine neutrality), RFC-0008 (cross-repo linkage), RFC-0012 (codegraph on-top layer), `docs/research/engram-framing-synthesis.md`, `docs/research/more/it-sdlc-ontology-build-reduction-options.md`, `docs/domain-data-model.md`

## Reviewer brief

- **Decision:** Ratify Engram's direction as a **context-graph packet framework** and commit to six additive deltas that turn the modeled-but-unpopulated semantic layer into task-typed, governed packets for AI agents.
- **Recommended outcome:** accept.
- **Change if accepted:**
  - New framework primitives: `ContextSubgraph` packet shape, `ApplicabilityRule`, `DecisionTrace`, and an entity→ontology-class link.
  - One frozen-v1 enum touch (`RetrievalTargetType` additions, under the tolerance clause) + additive draft-extension types.
  - A new agentic-population **adapter** behind existing ports (the deterministic ingestor stays pure), and a thin packet-recipe layer.
- **Affected surface:** `core/domain` (new types), `core/retrieval` (composition output), `core/knowledge` (existing ingest ports implemented by the new adapter — no new port), a new `adapters/` crate, `contracts/v1/` (one schema regen), `docs/domain-data-model.md`.
- **Stakes:** costly-to-reverse (contract surface grows; one frozen-enum edit), but additive and v1-compatible — no data migration, no rewrite.
- **Review focus:** (1) the **framework/content boundary** — Engram ships types + machinery, never domain ontology content; (2) the frozen-`RetrievalTargetType` touch in D2; (3) the "decision traces are candidate, never authoritative" rule in D4.
- **Not in scope:** banking (or any domain) ontology **content** — consumer-loaded; authority-precedence conflict resolution (a sibling primitive of the reconciliation layer); a federated query engine over external warehouses (Engram owns its governed store); replacing the deterministic ingestor; auto-promoting trace-derived facts.

## The ask

**Recommendation (BLUF):** Approve Engram as a **context-graph packet framework** — ship the packet, rules, and trace primitives plus the composition machinery as the framework; ship the *mechanism* for defining a semantic spine, but **no spine content**; let consumers (e.g. banking) load their own taxonomy + ontology. Ratify six additive, v1-compatible contract decisions (one frozen-enum touch) and a four-spec sequence (this RFC is the gating Phase 0).

**Why now (SCQA):**
- *Situation:* Engram already **models** a context graph — knowledge graph + bi-temporal validity + provenance on every record + enforced `Policy` + belief/contradiction + ontology/taxonomy. The retrieval-composition seam (ADR-0009) already outputs a `ContextPayload`. The market has crystallized exactly this shape — "KG + time + provenance + governance + decision traces, optimized for AI" — as the "context graph" (PuppyGraph; Foundation Capital's "trillion-dollar" thesis).
- *Complication:* The semantic layer is **modeled but unpopulated** — the deterministic ingestor leaves `conceptRefs`, ontology links, and beliefs empty by design. The packet is a **flat item list**, not the connected subgraph agents need. Governing **rules** and agent **decision traces** are not first-class. So the substrate exists; the population stage, the subgraph payload, the rules primitive, and the trace primitive do not.
- *Question:* Do we ratify Engram as a context-graph packet framework and commit to the six additive deltas in dependency order?

**Framing reconciliation.** The framing synthesis positions Engram's moat as the **belief/reconciliation engine** (authority × bitemporal × provenance) and treats "context graph" as a market term Engram sits beside, not inside. This RFC aligns Engram's *delivery surface* with the context-graph framing the author has explicitly chosen (`docs/about.md`: "Knowledge Graph, Context Graph, anything memory"): context-graph packets are **how the reconciliation engine's value reaches AI agents** — the read path, not a replacement thesis. The synthesis is cited here for the primitives it names (`ApplicabilityRule`) and the boundary it draws (product/content), not adopted as Engram's whole strategic identity. The synthesis's *second* missing primitive — attribute-level authority-precedence conflict resolution — is a sibling concern of the reconciliation layer and is explicitly out of scope (see Non-goals).

**Decisions requested:**

| ID | Question | Recommendation | Why | Decide by | Reviewer action |
| --- | --- | --- | --- | --- | --- |
| D1 | Packet shape: flat items vs connected subgraph? | **`ContextSubgraph`** — extend `ContextPayload` to carry nodes + typed edges + included/excluded + provenance | A "context packet" *is* a connected subgraph (PuppyGraph; the research's "compiled context packet"). Edges-as-items forces the agent to reassemble. | RFC acceptance | Confirm subgraph as the packet shape |
| D2 | How do governing rules enter packets? | **New `ApplicabilityRule` primitive** ("fact X binds target Y when condition Z"), consumer-declared; + add `rule`/`policy`/`axiom` to `RetrievalTargetType` | The framing synthesis names `ApplicabilityRule` as a missing core primitive (confirmed absent). Rules are declared like ontology content, not extracted. **One frozen-enum touch.** | RFC acceptance | Rule on the new primitive **and** the frozen-enum edit |
| D3 | How does an entity declare its ontology class? | **Optional `ontologyClassRefs` on `KnowledgeEntity` + an `instance_of` relationship** | Spine typing needs an explicit class link; `KnowledgeEntity` is draft-extension (no v1 regen), so this is low-bar additive. | RFC acceptance | Confirm the field + relationship |
| D4 | How is agent decision provenance represented? | **New `DecisionTrace` type that produces CANDIDATE proposals only** + `decision_trace` target | Matches "trace → proposal governance (never auto-promote)." Atoms (events/beliefs/runs) exist; a cohesive, retrievable trace does not. | RFC acceptance | Confirm the **never-authoritative** rule + its writer-surface enforcement |
| D5 | Where does the model-backed extractor live? | **New adapter crate behind `IngestionService`/`Chunker`/`GraphExtractor` ports** (extract at ingest); consolidate later | Keeps `engram-ingest` deterministic; adapters aren't engine-neutrality-gated, so models/embeddings are allowed there. | RFC acceptance | Confirm adapter-behind-ports vs consolidation-only |
| D6 | Where do the types + composition + recipes live? | **Types in `core/domain`; composition extends `core/retrieval` (ADR-0009); recipes a thin on-top layer** | Mirrors the RFC-0012 codegraph pattern; keeps engine-neutral layers clean. | RFC acceptance | Confirm the layering |

Plus the **framework/content boundary** (Q1, confirmed): Engram ships framework *mechanism* (types + machinery); domain taxonomy/ontology *content* is consumer-loaded. This becomes its own ADR (the synthesis flags it as the boundary most likely to be violated under delivery pressure), and that ADR **gates** Q1.

## Problem & goals

**Problem.** Engram's semantic layer is richly modeled but operationally inert for agents: ingestion is deterministic-by-fiat (a conformance check grep-proves no model dependency in `engram-ingest`), so `conceptRefs`, ontology-class links, and beliefs are never populated. The packet (`ContextPayload.items[]`) is a flat list with no edges binding items into a graph. Governing rules live only as graph-governing metadata (`OntologyAxiom`, `Policy`), not as packet members. Agent decision traces have atoms (`MemoryEvent`, `Belief`, `ConsolidationRun`) but no cohesive, retrievable record. Net: an agent gets either flat RAG or an under-linked graph — never the task-typed, explainable, governed context packet the use cases need.

**Goals.**
1. Deliver **task-typed context-graph packets** (coding / root-cause / rules / policy) assembled by traversing a populated semantic layer.
2. Make the **semantic layer** (ontology + taxonomy + belief + bi-temporal) the packet's join and relevance structure — not similarity alone.
3. Stay **engine-neutral and v1-compatible** — additive only; one frozen-enum touch under the tolerance clause; no data migration.
4. Keep Engram a **framework** — ship mechanism + machinery + belief/reconciliation; domain content is loaded by consumers.

**Non-goals** (could-have-been-goals, deliberately dropped).
- **No domain ontology content in core.** Banking capabilities, BIAN mappings, the IT-SDLC spine, healthcare vocabularies are consumer-loaded `ConceptScheme`/`Ontology` sets — Engram ships the *mechanism* for defining a spine (the existing `Ontology`/`OntologyClass`/`Property`/`Axiom` + `ConceptScheme`/`Concept` types), never the content. A starter spine may ship as a consumer example under `examples/`, not in `core/domain`.
- **No authority-precedence conflict resolution.** The synthesis names attribute-level cross-source authority-precedence ("telemetry proposes, reviewed architecture confirms") as a second missing core primitive. It belongs to the belief/reconciliation layer's own roadmap, not the packet framework. Cost of delay: packets will *surface* contradictions (via the existing `Contradiction` model) but won't *resolve* them by precedence until that sibling work lands. Anchor: `docs/research/engram-framing-synthesis.md:88-97`.
- **No federation-over-warehouse.** Engram owns its governed store (policy/provenance/belief cannot be enforced on data you don't own). The federate-don't-replicate model applies to *sources* via adapters, not to Engram becoming a PuppyGraph-style zero-ETL layer over an external warehouse.
- **No replacement of the deterministic ingestor.** The agentic stage sits *behind the same ports*; `engram-ingest` stays pure.
- **No auto-promotion of trace-derived facts.** Traces produce candidates; promotion requires an explicit actor (mirrors `TaxonomyProposal`).
- **No new vector store, no general policy language.** `ApplicabilityRule` is a small condition→target binding, not an ODRL-scale engine.

## Proposal

**Framework/content split (the governing principle).** Engram ships the **mechanism** — the domain *types* (`ApplicabilityRule`, `ContextSubgraph`, `DecisionTrace`), the composition machinery, the belief/reconciliation engine, and the existing `Ontology`/`OntologyClass`/`Property`/`Axiom` + `ConceptScheme`/`Concept` types that let any vocabulary be defined. Engram ships **no domain ontology content**: no reference spine vocabulary, no banking/IT/healthcare classes. A consumer registers its own `Ontology` + `ConceptScheme` sets and its entity→class/concept mappings; a starter spine may ship as a consumer example under `examples/`, never in `core/domain`. The framework/content boundary is recorded as its own ADR, and **that ADR gates Q1** (it must land before any spine-vocabulary decision resolves).

**D1 — `ContextSubgraph` packet.** Extend `ContextPayload` (or add a sibling payload view) to carry `nodes` (mixed-type items, each with provenance/policy/explanation as today) + `edges` (the `KnowledgeRelationship`s binding them) + `included`/`omitted` + the binding budget. Emitted by `compose_context` through the ADR-0009 seam; the existing `relationship` target type already lets edges be retrieved, but the subgraph shape makes the packet a connected graph literally. Token cost is bounded by `ContextBudget` + the degree-cap/data-diet pattern already proven in `engram-viz`.

**D2 — `ApplicabilityRule`.** A new draft-extension domain type: `{ condition, target (EntityRef/ConceptRef), binding, provenance, validFrom/validUntil }` — "fact X binds target Y when condition Z." Validated like ontology axioms (`OntologyValidationFinding`-style). **Entry path:** `ApplicabilityRule` records are **consumer-declared** (like taxonomy/ontology content), written via a knowledge writer surface — *not* extractor-derived; the agentic extractor (D5) does not produce rules. To make rules packet members, add `rule`/`policy`/`axiom` to `RetrievalTargetType` (the frozen-enum touch — see Risks). **Naming:** the existing `Capability`/`CapabilityReport` types are *engine*-capability and live in `core/domain::capability`; if a domain `EntityKind::capability` is added (decided by the framework/content ADR), it is disambiguated by module path or renamed `business_capability` — the collision is not assumed away.

**D3 — entity→ontology-class link.** Optional `ontologyClassRefs: OntologyClassId[]` on `KnowledgeEntity` (draft-extension, low-bar) plus an `instance_of` `KnowledgeRelationship` for traversal cases. This is what lets axioms type and gate entities precisely (rules and spine queries depend on it).

**D4 — `DecisionTrace`.** A new draft-extension type capturing an agent run: items consulted, traversal path, policy applied, precedent cited, output, provenance. It **produces candidate proposals that feed `ConsolidationRun`** (via the existing `memory_to_belief` / `fact_extraction` task kinds); it feeds `TaxonomyProposal` *only* when the trace is specifically about taxonomy evolution, not as a general path. A **documented invariant** — traces are evidence, never authoritative facts; promotion requires an explicit `Actor` — is **enforced at the writer surface** in the Phase 4 spec (a `DecisionTrace::promote`-style path requiring an `Actor`), mirroring (and no stronger than) the existing `TaxonomyProposal` merge-requires-actor rule.

**D5 — agentic population adapter.** A new `adapters/` crate implementing the existing `IngestionService`/`Chunker`/`GraphExtractor` ports with a model-backed extractor/classifier that populates `conceptRefs`, `ontologyClassRefs` (D3), edges, and beliefs from source. It does **not** produce `ApplicabilityRule` records (those are consumer-declared, D2). Every write carries `Scope`+`Policy`+`Provenance`. The deterministic `engram-ingest` is untouched (the seam the architecture already left open — `extractor.rs:7`: "a later model-backed extractor can sit behind the same ports"). Later, a consolidation task refines/evolves (the end-state is ingest-extract + consolidate-evolve).

**D6 — layering.** New types land in `core/domain`; subgraph composition extends `core/retrieval` (store-free, engine-neutral per ADR-0009); packet recipes (coding/RCA/rules/policy presets over `RetrievalRequest`) live in a thin on-top layer mirroring the codegraph pattern. The agentic stage is an `adapters/` crate (not neutrality-gated). This keeps the god-module anti-pattern out (AGENTS.md) and preserves swap-by-config (ADR-0022).

**Migration.** Additive only. New fields are optional; new types are net-new; no existing record is converted. The one schema regen (`RetrievalTargetType`) is forward-compatible (older data still validates).

## Options considered

*D1 — packet structure (axis: where edges + included/excluded live in the output).*
- **(a) `ContextSubgraph`** ✅ — nodes + edges + included/excluded. Prior art: PuppyGraph (subgraph serialized to prompt); research "compiled context packet" (it-sdlc:187); v2 `traverse()→Subgraph` (architecture-design-v2:422).
- (b) edges-as-items — flat `items[]`, retrieve `relationship` items; agent reassembles. Cheaper, but loses the connected structure that is the whole point.
- (c) do-nothing — cost of delay: agents keep getting flat lists; the context-graph thesis goes unproven on Engram.

*D2 — rule representation (axis: how a governing rule is modeled).*
- **(a) `ApplicabilityRule` primitive + enum targets** ✅ — prior art: framing-synthesis "ApplicabilityRule" gap; SHACL (rules-as-validation reading fact-state); ODRL (policy-as-data).
- (b) reify rules as `concept`/`entity` nodes — lighter, no new type, but loses condition→target binding semantics.
- (c) out-of-band rules endpoint — keeps packet pure but a rule the agent can't see in-band is a rule it ignores.
- (d) do-nothing — Rules/Policy packets can't carry the rule itself.

*D3 — class membership (axis: where the link lives).*
- **(a) `ontologyClassRefs` field + `instance_of` edge** ✅ — prior art: `rdf:type`/SHACL `targetClass`; v2 `link_to_concept` (architecture-design-v2:422); Neo4j label+ontology mapping.
- (b) `instance_of` edge only — works for traversal, awkward for declarative typing/validation.
- (c) convention via `kind`+`conceptRefs` — no contract change, but imprecise (many classes per kind).
- (d) do-nothing — rules/spine queries stay imprecise.

*D4 — trace representation (axis: record type × authority level).*
- **(a) `DecisionTrace`, candidate-only** ✅ — prior art: framing-synthesis "trace→proposal, never auto-promote"; it-sdlc:152,215; AWS AgentCore observability; W3C PROV.
- (b) generalize `ConsolidationRun` — reuses a type, but conflates consolidation cycles with arbitrary agent actions.
- (c) `MemoryEvent`+`Belief` only — no new type, but no cohesive retrievable trace.
- (d) defer — the article's hero capability (agent decision tracing) stays unbuilt.

*D5 — enrichment stage (axis: lifecycle stage).*
- **(a) adapter behind ports (ingest-time)** ✅ — prior art: GraphRAG indexing (extraction + community summaries, it-sdlc:267); LlamaIndex KG; memtrace.
- (b) consolidation task (post-ingest) — fits existing `taxonomy_evolution`/`graph_evolution`/`belief_synthesis` kinds, but defers all enrichment to a sleep cycle.
- (c) both — ingest-extract now + consolidate-evolve later (the likely end-state).
- (d) do-nothing — the graph stays unlinked; packets stay empty.

*D6 — placement (axis: crate location).*
- **(a) types in domain, composition in retrieval, recipes on-top** ✅ — prior art: RFC-0012 codegraph pattern; ADR-0009 (composition in `core/retrieval`); ADR-0022 (neutral layers stay clean).
- (b) all in a new on-top layer — cleanest separation, but contract types belong in `core/domain` (they're portable truth, not on-top intelligence).
- (c) all in `core/integration` — risks the SDK facade owning behavior it shouldn't.
- (d) do-nothing — consequent on D1–D5: if those aren't pursued, no placement is needed; cost of delay is the modeled-but-unpopulated substrate staying inert (see Problem).

## Risks & what would make this wrong

**Pre-mortem (assume it shipped and failed).**
- *Agentic extraction is noisy → packets worse than flat RAG.* Mitigation: candidate-until-approved governance, confidence thresholds, and deterministic recall/forbidden-recall fixtures per packet type; the population spec owns this.
- *Frozen-enum regen breaks a consumer.* Mitigation: the tolerance-clause compatibility note + a versioned schema; the addition is forward-compatible (older data validates). Falsifiable: if a consumer does exhaustive `targetType` matching, it must tolerate unknown values — the freeze policy already requires this.
- *Subgraph payloads blow token budgets.* Mitigation: `ContextBudget{maxTokens}` + the degree-cap/data-diet pattern proven in `engram-viz` (4.57 MB → 358 KB).
- *Rules engine scope-creeps into a general policy language.* Mitigation: `ApplicabilityRule` stays a condition→target binding, explicitly not ODRL-scale; a later RFC would have to propose expanding it.
- *The framework/content line blurs under delivery pressure.* Mitigation: make it an ADR and **gate Q1 on it** (the synthesis flags this as the most-violated boundary).

**Key assumptions (falsifiable).**
- "Consumers tolerate unknown `RetrievalTargetType` values" — false if an exhaustive-matching consumer exists.
- "Subgraph composition stays engine-neutral" — false if `compose_context` must call a store directly (it must not; it operates on `RetrievalResult`s + relationships).
- "Domain ontology is loadable without core changes" — false if a domain needs an `EntityKind` not in the enum (then it's an additive enum addition, gated by the framework/content ADR).

**Drawbacks (what it costs).**
- Four specs of work (this RFC gates as Phase 0); one frozen-enum touch; three new domain types (contract surface grows).
- The agentic stage adds a model dependency behind a port (cost/latency at ingest).
- Rules + trace primitives are new governed contract surface — they need lifecycle/evolution discipline (they get it via the existing taxonomy-governance + consolidation models).

## Evidence & prior art

**Spike / de-risk (riskiest assumption: the additions fit the seams and stay v1-compatible).** Verified: (1) `RetrievalTargetType` is a closed frozen-v1 enum (`engram-v1.schema.json:450-453`) → D2 needs a v1 regen + compat note (permitted under the tolerance clause; not a v2 force); (2) `KnowledgeEntity` is **not** in frozen v1 (absent from the schema and the "First Contract Slice" list) → D3 freely additive; (3) `compose_context` lives in engine-neutral `core/retrieval` (gated by `check-engine-neutrality.sh`) and a domain `ContextSubgraph` trips no neutrality lint; (4) `ApplicabilityRule` returns zero hits across `core/` and `contracts/` → confirmed absent; (5) the agentic stage is an adapter (not gated) → models/embeddings are allowed there.

**Repo precedent.**
- `docs/about.md:26` — author intent: "Knowledge Graph, Context Graph, anything memory."
- `docs/research/engram-framing-synthesis.md` — the strategic framing; names `ApplicabilityRule` (+ authority-precedence, deferred here) as missing core primitives; the product/content boundary; "compiled context packet + trace" as product; "never auto-promote traces."
- `docs/research/more/it-sdlc-ontology-build-reduction-options.md` — compiled context packet (L187); agent traces as candidate (L152, L215); context-selector; federate-don't-replicate.
- `docs/research/architecture-design-v2.md:419-422` — already designs `traverse(start, path_pattern) → Subgraph`, `link_to_concept`, `traverse_cross_scheme`.
- `ADR-0009` — `ContextPayload` is the seam output; target-type/mechanism-agnostic.
- `RFC-0012` + `ADR-0022` — on-top layering + engine-neutrality pattern.

**External prior art** (surveyed in the cited repo corpus, verified by reading it; the corpus carries the URLs; the PuppyGraph article was read in full this session).
- **PuppyGraph "Context Graph"** — CG = KG + time + provenance + governance + decision traces; packet = serialized subgraph traceable to nodes/edges.
- **Microsoft GraphRAG** (it-sdlc:267) — graph extraction + community summaries + subgraph retrieval.
- **W3C SHACL** (it-sdlc:191) — rules-as-validation reading fact-state (grounding `ApplicabilityRule`).
- **Palantir Ontology / Glean Enterprise Graph / AWS AgentCore / Backstage / OpenMetadata / MCP resources+elicitation / PromptQL / Wikibase** (it-sdlc research-anchors L259-268) — operational-ontology, agent-trace, federated-catalog prior art.
- **W3C PROV / OWL `rdf:type` / ODRL** — decision-provenance, class-typing, policy-as-data standards.

## Open questions

- **Q1 — Spine scope.** Does Engram ship any spine vocabulary in `core/domain`, or only the mechanism for defining one? **Default (confirmed by author): mechanism only** — Engram ships the `Ontology`/`Taxonomy` types (already present) and no reference spine content in core; a starter spine is a consumer-loaded example. Adding domain-shaped `EntityKind` values (e.g. `capability`/`domain`) is itself a contract decision resolved by the framework/content-boundary ADR (see D2 for the naming collision). Owner: Approver. Decide-by: **the framework/content-boundary ADR** (gates this question, not merely RFC acceptance).
- **Q2 — Frozen-enum touch.** Is an in-version additive `RetrievalTargetType` change (regen + compat note) acceptable for D2, or should rules enter packets via draft-extension types only? **Default: in-version additive under the tolerance clause.** Owner: Approver (contract owner). Decide-by: RFC acceptance.
- **Q3 — Trace authority.** Confirm the "never auto-promote, always candidate" rule is **enforced at the writer surface** (a promote path requiring an explicit `Actor`), not merely documented? **Default (confirmed by author): yes — enforced at the writer surface in the Phase 4 spec.** Owner: Approver. Decide-by: RFC acceptance.

## Follow-on artifacts

- **ADRs (to file on acceptance):**
  - ADR-NNNN — Context-graph packet framework (the **framework/content boundary**; product vs bespoke). **This ADR gates Q1.**
  - ADR-NNNN — `ApplicabilityRule` + `RetrievalTargetType` additions (D2).
  - ADR-NNNN — `ContextSubgraph` packet shape (D1).
  - ADR-NNNN — `DecisionTrace` candidate-only invariant (D4).
- **Specs (four-spec sequence; this RFC is the gating Phase 0):**

  ```
  Phase 0 (this RFC, gate) ──▶ Phase 1 (contract additions: D1/D2/D3/D4 types)
                                │   Phase 1 unblocks 2, 3, and 4
                                ├──▶ Phase 2 — agentic-knowledge-population (D5)
                                ├──▶ Phase 3 — context-subgraph-packet-assembly (D1 wiring + D6 recipes)
                                └──▶ Phase 4 — agent-decision-traces (D4 wiring)
                                    Phase 2 ∥ Phase 3 after Phase 1;
                                    Phase 4 needs Phase 1 (better with Phase 2; independent of Phase 3)
  ```

  One at a time through `new-spec` → `work-loop`.
- **Convention change:** none expected to `docs/CONVENTIONS.md` (the framework/content split is recorded as an ADR, not a convention edit).
