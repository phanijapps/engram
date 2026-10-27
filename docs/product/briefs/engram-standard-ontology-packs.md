<!--
  Factory brief — authored by engram-frame with the owner, 2026-10-27.
  Registered at factory create (--brief-file); REQUIRED analysis input.
-->

# Brief: make use of the 33 ontologies from open-ontologies — without losing the capabilities of engram

> **Functional nature**: this brief describes what the system must do, for
> whom, and why it matters — in language a functional reader can verify.
> How it is built belongs to analysis and plan. Owner-fixed technical
> constraints are allowed but labeled; owner suggestions are non-binding.

| Field | Value |
|---|---|
| Reader / question | Analyzer asking "what does the human want, and what may I decide?" · owner ratifying their own intent |
| Status | drafted 2026-10-27 by engram-frame with the repository owner (pi session) · supersedes: `docs/product/briefs/engram-derivation-layer.md` (unratified draft; its concepts survive only in the Backlog) |
| Budget | ≤ 300 narrative words. Quoted owner text and ledger rows are exempt. |

## The ask

> "I want to make use of the 33 ontologies that are part of open ontologies
> and get whatever is possible without losing the capabilities of engram"
>
> Refined in interview: "My goal is if I am ingesting banking docs and
> codebase those respective ontologies or taxonomy is setup like wise
> healthcare, travel etc"
>
> And: "I have a knowledge engine but not correctly used by coding agents.
> I need a skill or a subagent for claudecode/codex etc to make sense of the
> ingestion or distillation."

## Problem

Coding agents (Claude Code, Codex) do not use engram's knowledge engine
correctly during ingestion and distillation — owner's words above. When a
domain's docs and codebase are ingested (banking, healthcare, travel), no
domain-appropriate vocabulary stands ready, so distilled facts get ad-hoc
classification instead of a real standard taxonomy. Facts also cannot say
what supports them (owner-ranked pain #1) and ontology edits can silently
change what existing knowledge means (pain #2).

## Outcome measures

- Fresh banking repo + docs → agent runs the bundle → distill classifies
  facts under FIBO vocabulary with **zero hand-built taxonomy**.
- Two same-domain projects queried → facts return under **shared standard
  terms** (banking and travel are the named test repos).
- All 33 standard packs installable; multiple domains testable by the owner.
- Existing 3-layer defaults, distill mechanics, and retrieval fusion results
  behave exactly as today (additive only).

## Non-goals

- Formal OWL semantics — packs are classification vocabulary + hierarchy
  only; no entailment, disjointness, or SHACL machinery.
- Any behavior change to existing defaults, distill mechanics, or retrieval
  fusion results.
- License-restricted content (SNOMED et al.).
- Blocking gates on pack install (advisory only).
- Memory layer, belief synthesis, codegraph, viz, cross-project federation,
  host-embedder features, ontology editor UX (see Backlog for continuations).

## Priorities (MoSCoW)

| Bucket | Items |
|---|---|
| Must | Installable skill/subagent bundle (Claude Code / Codex) that guides interactively AND runs autonomously: scan → domain pick → pack install → ingest → distill · All 33 standard packs importable into engram's structural ontology model · Domain-appropriate taxonomy standing at distill time · Proof on banking + travel repos (fresh-repo classification under standard terms; two same-domain projects under shared vocabulary) |
| Should | Verdict words on every fact surface (`asserted_by_agent` / `grounded_in_source` / `derived`) — owner's #1 pain · Multi-domain verification runs across the catalogue |
| Could | Justification queries · Pack-install advisory change report · Per-claim retrieval preservation eval · Healthcare pack beyond the 33 — all Backlog (below), excluded from this job |
| Won't | As Non-goals above |

## Backlog (next phases — owner-requested, LED-15)

- **Phase 2** — justification queries ("what supports this fact", minimal
  support sets); pack-install advisory change report (what a new pack changes
  about existing knowledge — pain #2).
- **Phase 3** — derivation layer (deterministic closure over typed edges;
  activates the reserved `derived` verdict — see superseded brief for the
  researched arc); per-claim retrieval preservation eval (four-cell verdicts).
- **Phase 3/4** — healthcare/clinical pack sourced beyond the 33
  (licensing-flagged); drift report on reindex; maintenance feedback learning.
- **Later** — cross-project federation under shared standard vocabulary (the
  v2 payoff of the shared-terms outcome).

## Known constraints and standards

- owner-fixed: deliverable is an installable agent skill/subagent bundle for
  Claude Code / Codex hosts (imposed by owner pick LED-3; necessary because
  the adoption gap lives in those hosts).
- owner-fixed: additive-only, no retrieval regression (LED-5).
- House (AGENTS.md): contract-first — `docs/domain-data-model.md` before
  contract changes; surface parity (facade + N-API + CapabilityReport + MCP,
  parity lint); engine neutrality; per-project scope; server never calls an
  LLM — domain pick and classification stay agent-side; CI gates
  (contracts/docs/neutrality/parity).

## Decision ledger

| ID | Choice | Authority | Statement / boundary | Revisit trigger |
|---|---|---|---|---|
| LED-1 | Primary pain ranking | HUMAN-DECIDED | "A then C. I have a knowledge engine but not correctly used by coding agents. I need a skill or a subagent for claudecode/codex etc to make sense of the ingestion or distillation." (R1-Q1) | — |
| LED-2 | Primary beneficiary | HUMAN-DECIDED | Owner picked A: the agent itself (R1-Q2) | — |
| LED-3 | Deliverable shape | HUMAN-DECIDED | Owner picked A: fixed constraint — installable skill/subagent bundle (R2-Q4) | — |
| LED-4 | Domain setup behavior | HUMAN-DECIDED | "My goal is if I am ingesting banking docs and codebase those respective ontologies or taxonomy is setup like wise healthcare, travel etc" (R2-Q5) | — |
| LED-5 | Compatibility | HUMAN-DECIDED | Owner picked C: existing flow unchanged AND no retrieval regression (R2-Q6) | — |
| LED-6 | Outcome proof | HUMAN-DECIDED | "A or C when I give banking or travel repos.." (R3-Q7) | — |
| LED-7 | Catalogue scope | HUMAN-DECIDED | "I want all 33 so that I can test it against multiple" (R3-Q8) | — |
| LED-8 | Bundle mode | HUMAN-DECIDED | Owner picked C: guide when interactive, autonomous when delegated (R3-Q9) | — |
| LED-9 | MoSCoW buckets | HUMAN-DECIDED | "accepted" — as proposed, verdict words at Should (R4) | — |
| LED-10 | Boundaries + constraints | HUMAN-DECIDED | "go for it" — 9 OUT items + house constraints as proposed (R5) | — |
| LED-11 | Pack sourcing | HUMAN-DECIDED | Owner accepted recommendation C: hybrid — catalogue pinned, pack fetched once then cached; repeats offline and identical (R6-D1, "go recommended") | — |
| LED-12 | Pack granularity (FIBO-scale) | DELEGATED | Analysis may decide slicing, within: no classification coverage silently lost; agent can widen vocabulary on demand (R6-D2 via "go recommended") | A domain test shows missing terms |
| LED-13 | Domain pick authority | HUMAN-DECIDED | Owner accepted recommendation B: propose-then-confirm; autonomous runs unconfirmed but logged (R6-D3) | — |
| LED-14 | Verdict vocabulary | HUMAN-DECIDED | Owner accepted recommendation A: closed set of three, generated contract type; `derived` is reserved in v1 (nothing emits it until the derivation Backlog phase) (R6-D4) | Derivation phase lands |
| LED-15 | Backlog required | HUMAN-DECIDED | "go recommended. but have a backlog of the next phases as well" (R6) | — |

*(All references: pi coding session with the owner, 2026-10-27, interview
rounds R1–R6 of the engram-frame run.)*

## Delegated to analysis (explicit only)

- `analysis may decide` how much of a large pack installs (full vs curated
  slice), `within` no silent classification-coverage loss and
  agent-widening-on-demand, `because` the owner delegated granularity while
  protecting the classification outcome (LED-12).

## Open — owner must decide

- None. All consequential choices are HUMAN-DECIDED or explicitly DELEGATED.

## Owner ratification

| Actor | Channel | Date | Confirmation (verbatim) |
|---|---|---|---|
| repository owner | pi coding session, engram-frame run | 2026-10-27 | owner picked A: "I ratify this brief." (whole-brief ratification, R7) |

## Owner suggestions (non-binding)

- None recorded.

## Handoff

- Feature text: the ask above, verbatim → `factory.py create --feature-file <ask> --brief-file docs/product/briefs/engram-standard-ontology-packs.md`.
- The controller registers the brief, requires it as an analysis input, and
  requires every LED row to be addressed by the analysis body.
- **Consumption contract**: Must → outcome core; Should → scope; Could →
  Backlog only (excluded from this job); Won't → non-goals; suggestions → no
  inherited authority; material OPEN rows → analysis blocks (none exist).
- **Rework supersession**: a later human rework note is the newer owner
  authority; ledger rows it explicitly reverses are superseded; analysis
  records both, never silently drops either.
