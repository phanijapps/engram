# Brief: engram derivation layer — deterministic derivations over the knowledge graph

- **Slug:** `engram-derivation-layer`
- **Received:** 2026-10-27
- **Owner:** engram core team
- **Source:** session analysis 2026-10-27 — comparison with [fabio-rovai/open-ontologies](https://github.com/fabio-rovai/open-ontologies) (concepts adapted, not adopted wholesale); dataflow diagram at `.archify/dataflow-engram-knowledge-20261027-111141/engram-ingestion-ontology.html`
- **Shape:** A — no stories; spec boundaries derived from Outcome + Scope.

> Engram's knowledge graph is written by agent reasoning (engram-distill) and
> read by fused retrieval. Nothing is ever **derived** deterministically, no
> fact can say **what supports it** beyond a `sourceRefs` list, and retrieval
> correctness is measured by coverage-shaped numbers that cannot see the one
> claim an answer needed. This brief imports the *architecture* proven by
> open-ontologies — untrusted assertions, recorded derivations, verdict
> vocabulary, per-claim preservation — without importing its theorem-prover
> machinery. The keystone is a **closure pass**: a small fixed rule table over
> engram's own typed edges, run as post-write maintenance (never inside
> ingestion), with every derivation recorded and kept strictly separate from
> asserted truth.

## Outcome

Every fact an agent can query from engram's knowledge layer carries an honest
verdict word — `asserted_by_agent`, `grounded_in_source`, or `derived` — where
`derived` is only ever earned by a deterministic closure pass over typed edges
(subclass/broader transitivity, instance-of inheritance, part-of/hierarchy
transitivity, evidence links), each derivation recorded as
`rule → conclusion → premises` and never re-asserted as input. On that
substrate: a reviewer can ask **which asserted edges a derived fact rests on**
(justification), an ontology edit gets an **advisory conservativity report**
(does this change any consequence over names already in use?), retrieval
evaluation asks **per claim** whether a recall slice preserves it (four-cell
verdict separating retriever loss from ungrounded generation), and ontology
lint reports declaration-level defects. All of it reachable through
`EngramProvider`, the N-API binding, `CapabilityReport`, and engram-mcp — with
the asserted write path (chunks → agent → graph + ontology) untouched.

## Success metrics

> Not given. Proposed defaults — correct me:

- **Rebuildability:** wiping all derived state and re-running the closure over
  asserted records reproduces an identical derived set (determinism fixture in
  `core/eval`; no clock, no embedding, no LLM in the rule table).
- **Separation enforced, not promised:** a derived record cannot enter the
  closure as a premise — proven by type/table separation and a negative
  fixture, not by convention (open-ontologies decision 0001: *an inference is
  not an assertion*).
- **Verdict vocabulary is law:** every fact-carrying surface (justify tool,
  belief queries, eval reports) emits one of the fixed verdict words;
  agent-written facts can never be labeled `derived`; a test fails if the
  vocabulary drifts (their decision 0003: *a rule is data, and an assumption
  is not a fact*).
- **Preservation beats coverage:** eval reports the four cells per claim —
  `preserved` / `lost_by_projection` / `ungrounded_in_source` / engine-side —
  on the existing fixture corpus; `coverage_ratio` is demoted, not deleted.
- **Surface parity in one unit:** closure status + justify reachable from the
  Rust facade, N-API, `CapabilityReport`, and engram-mcp in the same change
  (`scripts/ci/check-surface-parity.sh` green).
- **Bounded cost:** closure pass over this repository's own knowledge graph
  completes under a stated budget (benchmark before optimization; correctness
  fixtures first per repo standards).

## Scope / Non-goals

**In scope:**

- A pure, storage-neutral closure behavior crate (the `engram-graph-analytics`
  pattern: no I/O, deterministic, fixture-tested) owning the rule table and
  derivation records.
- Persistence for derived edges + derivation records in a **separate** store
  surface from asserted graph records.
- A maintenance-pass executor (consolidation/decay pattern) with stale
  invalidation on graph writes, ontology edits, and source reindex — the same
  slot reindexing occupies today.
- Verdict vocabulary as a generated contract type.
- Per-claim preservation evaluation in `core/eval`.
- Declaration-defect lint as new `MaintenanceCandidate` kinds.
- Advisory conservativity report for ontology/taxonomy edits (the
  `validate_graph` stance: reports, never blocks).
- MCP tools: `justify` (minimal support set), closure status/refresh.

**Non-goals (confirmed stance — challenge if wrong):**

- **No formal verification.** No Lean checkers, no OWL/DL tableaux, no SHACL,
  no TPTP/SMT export. We borrow the pattern (untrusted engine + checkable
  derivation artifact), not the machinery. Deterministic replay of derivation
  records is the checking story.
- **No ingestion changes.** The closure is post-write only; `SourceReader →
  Chunker` is untouched.
- **No distill changes.** Agent extraction/classification/bridging stays as-is;
  only the recording contract on the server side upgrades.
- **No blocking gates.** Lint and conservativity are advisory; no write path
  grows a hard reject in v1.
- **No ranking changes.** Derived edges feeding an extra recall lane is a
  later, separate spec (v1 proves the substrate; fusion stays untouched).
- **No belief-model rewrite.** Justification applies to graph derivations
  first; extending MinA-style minimal support to reflection beliefs is a
  follow-up, not this brief.

## Appetite

Two cycles. Cycle 1: keystone (crate + persistence + maintenance executor +
surface parity) — shippable and useful on its own (justify + closure status).
Cycle 2: consumers (preservation eval, lint kinds, conservativity report).
Each slice is independently landable; nothing ships half-wired per the
surface-parity rule.

## Current state (gap analysis, 2026-10-27)

- **Ontology:** `OntologyRepository` port exists with advisory
  `validate_graph` (`core/knowledge/src/ontology.rs`); multi-layer
  (technical/domain/business) as MCP launch config.
- **Graph:** `KnowledgeGraphRepository`; entities/relations/facts written by
  the agent through engram-mcp. No derivations of any kind exist today.
- **Beliefs:** `sourceRefs` + confidence numbers the domain model itself calls
  "diagnostic, NOT calibrated" — no minimal-support sets, no named derivation
  rules.
- **Maintenance:** `MaintenanceCandidate` kinds are
  orphan/low_confidence/unsupported/duplicate; decay executor exists. No
  declaration-level lint, no feedback learning, no drift report.
- **Eval:** deterministic fixtures and regression harness exist
  (`core/eval`); retrieval correctness is coverage-shaped, no per-claim
  preservation.
- **Adjacent precedent:** `engram-graph-analytics` proves the pure-algorithm
  crate pattern; `adapters/consolidation/decay` proves the executor pattern;
  the `lexical-keyword-retrieval` + `lexical-wiring` split proves adapter-
  unit-then-wiring sequencing.

## Proposed slices (the cut — awaiting confirmation)

1. **`closure-core`** — pure rule table + `DerivationRecord` + determinism and
   separation fixtures. No storage, no MCP. The keystone everything else sits
   on.
2. **`closure-persistence-parity`** — derived-record persistence (separate
   surface), maintenance executor + invalidation triggers, facade + N-API +
   `CapabilityReport` + MCP tools (`justify`, closure status) in one
   surface-parity unit.
3. **`preservation-eval`** — four-cell per-claim verdicts over existing
   fixtures; `coverage_ratio` demoted; outcome-reporting tie-in.
4. **`ontology-lint-conservativity`** — declaration-defect `CandidateKind`s
   (capped listings with `truncated` totals) + advisory conservativity report
   on ontology edit plans.

## Spec map

To be created under `docs/specs/` per slice once this brief is confirmed;
domain-type additions (`DerivedEdge`, `DerivationRecord`, verdict enum,
`CandidateKind` extensions) land in `docs/domain-data-model.md` **before**
contract generation, per the contract-first rule.

## References

- open-ontologies decisions 0001, 0002, 0003, 0007, 0016 and
  `docs/decisions/README.md` (failure-driven ADR discipline);
  `docs/explanation.md` (justification vs certificate substrate — the DAG
  distinction); `docs/modules-and-conservativity.md`; `docs/lifecycle.md`.
- engram: `docs/domain-data-model.md`, `docs/product/engram.md`,
  `AGENTS.md` boundary rules (surface parity, memory-knowledge boundaries,
  no-god-modules).
- Session artifacts: comparison analysis and dataflow diagram (`.archify/`),
  2026-10-27.
