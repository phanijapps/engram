# RFC-0007: Federated assertion reconciliation

- **Status:** Accepted
- **Author:** phanijapps
- **Approver:** phanijapps
- **Date opened:** 2026-07-03
- **Date closed:** 2026-07-03
- **Decision weight:** standard <!-- The v1 domain contract is pre-1.0 and not frozen (no ADR marks it immutable), so this additive change is standard, not heavy. If the Approver rules the domain contract frozen, this becomes heavy — see Reviewer brief. -->
- **Related:** RFC-0002 (knowledge source extension — the corpus ingestion model this sits beside), RFC-0004 (enterprise knowledge platform demo — belief/contradiction/bitemporal surface), ADR-0011 (consolidation trigger policy — explicit-command baseline), ADR-0005 (storage adapter semantics), ADR-0010 (behavior port split). Follow-on ADRs on acceptance.

## Reviewer brief

- **Decision:** Add a second ingestion path — federated *source assertions* reconciled by typed authority over bitemporal validity — alongside the existing copy-into-`KnowledgeChunk` corpus path, with promotion policy behind a pluggable, config-selected seam.
- **Recommended outcome:** accept.
- **Change if accepted:** (1) new `SourceAssertion` domain type linked from `BeliefSource(Assertion)`; (2) typed `authorityLevel` + attribute-level survivorship on belief reconciliation; (3) a `Gate`/validation-event consolidation trigger *family* selecting promotion, configurable and conditional on scope/source-authority.
- **Affected surface:** `core/domain` (new type + fields), `core/knowledge` (federated ingestion port), `core/belief` + `core/consolidation` (survivorship synthesizer + trigger), a new assertion adapter, bindings.
- **Stakes:** costly-to-reverse (touches the domain contract) but additive and wire-compatible — the corpus path and existing belief lifecycle are unchanged. **Reviewer must rule** whether the v1 domain contract is frozen; if yes, decision weight becomes heavy.
- **Review focus:** (a) is `SourceAssertion` a distinct type or overloaded onto `BeliefSource`; (b) is advisory-by-default reconciliation the right stance vs auto-resolve; (c) do the corroboration/time-settling triggers stay caller-invoked per ADR-0011.
- **Not in scope:** `ApplicabilityRule` (conditional rule binding), write-back/coexistence sync, a scheduler runtime, a review-queue UI.

## The ask

- **Recommendation (BLUF):** Approve a federated assertion-and-reconciliation path so agents can build *trustworthy* knowledge — facts that stay in their sources, enter as candidates with typed authority, reconcile over bitemporal validity, and promote to trusted via a config-selected trigger — reusing the existing `Belief`/`Contradiction` machinery rather than a new engine.
- **Why now (SCQA):** *Situation* — Engram ingests knowledge by copying corpora into `KnowledgeChunk`/`Entity` records (RFC-0002), stamped `confidence: 1.0` and authoritative-by-default. *Complication* — every target use (enterprise SDLC review, a personal coding agent, an autonomous news-research agent) needs the opposite: facts that live in their sources, disagree with each other, get corrected over time, and only become "trusted" on evidence or approval. The belief layer already has the primitive (bitemporal, confidence, supersession, contradiction) but nothing feeds it federated assertions, and reconciliation is manual. *Question* — do we add a federated assertion path with pluggable authority + promotion, or keep copying-and-trusting?
- **Decisions requested:**

  | ID | Question | Recommendation | Why | Decide by | Reviewer action |
  | --- | --- | --- | --- | --- | --- |
  | D1 | How do enterprise/volatile facts enter? | Add a **Registry-style federated path** (link + `sourceRecordId`, never replicate) beside the existing Consolidation corpus path | Volatile facts rot when copied; the two paths serve different needs and should coexist | this review | confirm both paths coexist |
  | D2 | Where does the claim + its state live? | A **distinct `SourceAssertion`** type, separate from the canonical record | Separating claim from evidence is what lets conflicts be reviewed, not overwritten; `BeliefSource(Assertion)` already anticipates it | this review | confirm new type vs overload |
  | D3 | How is authority + conflict modeled? | Typed **`authorityLevel` + attribute-level survivorship**, resolution **advisory by default** (candidate winner + `Contradiction` on tie) | Preserves the existing detect-don't-overwrite stance; matches MDM per-attribute survivorship | this review | rule on advisory vs auto-resolve |
  | D4 | What promotes candidate → trusted? | A **trigger family** (`human-gate \| corroboration \| time-settling \| explicit-command`), config-selected and conditional on scope/source-authority | One core serves three domains by swapping the trigger; ADR-0011 already lists these strategies | this review | confirm config-driven promotion |
  | D5 | Is `ApplicabilityRule` in scope? | **Defer** to a follow-on | It's a rules engine orthogonal to the ingestion+authority core; not needed for the first brief | this review | confirm deferral |

## Problem & goals

Engram's knowledge ingestion is *replicative and trusting*: [adapters/ingest](../../adapters/ingest/src/ingestor.rs) reads files/git, chunks, and writes `KnowledgeSource → SourceDocument → KnowledgeChunk → Entity/Relationship` with `confidence: 1.0` and no fact-state. That is correct for a static corpus (a documentation set, a code snapshot) but wrong for knowledge that (a) lives in a system of record and changes, (b) is asserted by multiple sources that disagree, and (c) is only trustworthy after evidence or review. The belief layer ([core/domain/src/belief.rs](../../core/domain/src/belief.rs)) already models confidence, bitemporal validity, supersession, and contradiction — but nothing produces federated assertions for it to reconcile, and reconciliation today is manual (`resolve_contradiction`) plus advisory detection.

**Goals.**
- A federated ingestion path that records a claim + its source + authority + validity **without replicating** the source of record.
- Reuse `Belief`/`BeliefSource`/`Contradiction` as the reconciliation engine (additive changes only).
- Make promotion (candidate → trusted) a **config-selected, conditional policy**, so the same core serves an enterprise gate, a personal agent, and an autonomous research agent.

**Non-goals** (could-have-been goals, deliberately dropped).
- **Coexistence / write-back sync** to source systems — bidirectional sync is the operational-MDM heaviness we are explicitly avoiding; Engram reads and reconciles, it does not own or write the source.
- **`ApplicabilityRule`** (conditional "fact X binds target Y only when Z") — a rules engine deferred to a follow-on.
- **A scheduler runtime** — ADR-0011 keeps consolidation scheduler-free; triggers stay caller-invoked.
- **A review-queue / approval UI** — the enterprise profile may need one, but it is an add-on, not core.

## Proposal

### D1 — Two ingestion paths that coexist

Keep the existing **Consolidation** path (copy + chunk + embed for semantic retrieval). Add a **Registry** path: a source adapter emits `SourceAssertion`s that carry `sourceRecordId` and a link back to the authoritative system, and does *not* copy volatile field values into a durable canonical store. An autonomous research agent is the case that needs both at once — copy articles for retrieval *and* extract claims as reconciled assertions.

### D2 — `SourceAssertion` as a distinct domain type

```text
SourceAssertion {
  id, scope,
  subject (BeliefSubject key), predicate, object,   // the claim
  source_system, source_record_id, source_uri,      // federation: link, don't replicate
  authority_level,                                   // typed (see D3)
  confidence,
  valid_from, valid_until,                           // event/application time (bitemporal)
  asserted_at,                                        // knowledge/transaction time
  review_status,                                      // source | candidate | reviewed | authoritative | disputed | deprecated | rejected
  policy, provenance
}
```

A `Belief` is derived over the set of `SourceAssertion`s for a `(subject, predicate)` via `BeliefSource { target_type: Assertion, target_id, weight, confidence, valid_from, valid_until }` — the `Assertion` variant already exists in [`BeliefSourceTargetType`](../../core/domain/src/belief.rs) and is currently unused. Separating the claim (`SourceAssertion`) from the layer's stance (`Belief`) is what preserves the existing "expose tension without overwriting" contract.

### D3 — Typed authority + attribute-level survivorship, advisory by default

`authority_level` is a small typed, ordered set of **authority tiers** — `{ primary, secondary, inferred }` by default — deliberately sharing **no tokens** with the `review_status` lifecycle (D2), so "authority" (how much a source is trusted) and "state" (how far a claim has progressed) never collide. The `enterprise-gate` profile overlays its own tiers (`semantic/record/policy`). Reconciliation is a `BeliefSynthesizer` strategy (survivorship) that, per `(subject, predicate)`:
1. selects a candidate winner by the active authority profile (source-priority / recency / completeness / trust-score);
2. respects bitemporal validity (a claim only competes while `live_at` its interval — reusing [`core/belief/src/temporal.rs`](../../core/belief/src/temporal.rs));
3. when authoritative sources tie or contradict, emits a `Contradiction` (Logical/Temporal/Tension) and leaves promotion to the trigger — it does **not** silently overwrite.

Attribute-level survivorship ("trust the API catalog for endpoints, architecture repo for ownership") maps to *one `Belief` per `(subject, attribute)`*, which is already how `BeliefSubject.key` is shaped.

### D4 — Promotion as a config-selected, conditional trigger family

Promotion from candidate → trusted always needs *a* trigger; the config chooses which. The trigger family: `human-gate | corroboration(min_sources, min_authority) | time-settling(window) | explicit-command`. **These are trigger *reasons* evaluated inside a caller-invoked `consolidate()` cycle — never a background auto-fire.** ADR-0011 keeps `engram-consolidation` scheduler-free (no timers/loops) and defers any automatic scheduler to its own ADR; this RFC honors that by making corroboration and time-settling *conditions checked when a cycle runs*, not self-firing triggers. ADR-0011's existing `ConsolidationStrategy` values (`Manual`, `EventCount`, `TimeWindow`, `RetrievalFailure`, `Hybrid`) already carry these reasons; this adds a validation-event trigger and wires it to a gated promotion through the existing `GatedConsolidationService` / `evaluation_gate.rs`. A future scheduler that invokes `consolidate()` on a policy is out of scope and would need its own ADR. Promotion policy is **per-profile and conditional on scope/source-authority** — e.g. auto-promote wire-service claims on corroboration while gating social-media claims.

Three reference profiles ship as presets of the one policy knob:

| Profile | Authority | Promotion trigger | Human gate |
| --- | --- | --- | --- |
| `enterprise-gate` | semantic / record / policy | human-gate | on |
| `personal-default` | user-word-wins; code/tests auto-trusted | user-confirmation, commit, test-pass | mostly implicit |
| `autonomous-research` | source trust-score | corroboration + time-settling | off |

### D5 — `ApplicabilityRule` deferred

Out of scope; recorded as a follow-on so the ingestion + authority core stays shippable and can produce the first "do not approve yet" brief without it.

### Migration path

Additive and **wire-compatible** (per AGENTS.md contract classification). Existing `KnowledgeChunk`/`Entity` records and the corpus ingestor are unchanged. `SourceAssertion` is a new type. `authority_level` is added to the belief-source relation as an **optional, `#[serde(default)]` field**; when absent it defaults to the tier that reproduces today's behavior (single-source ⇒ `primary`), so existing serialized beliefs deserialize unchanged. No existing belief is rewritten. This is a **compatible** public-contract change, not breaking.

## Options considered

Axis for D1/D2 (the load-bearing choice): **where the reconciled fact physically lives and who owns updates.** The four MDM implementation styles name the points on that axis (source-owned → hub-owned); the claim that they *partition* the axis exhaustively — every design either links, copies-one-way, copies-both-ways, or originates — is this RFC's argument, not the vendor's. Profisee is cited below only for the style *definitions*.

| Option | What it is | Trade-off vs goals | |
| --- | --- | --- | --- |
| **Registry (link) + keep Consolidation** | Link to source, keep cross-reference; corpus copy stays for retrieval | Matches federate-don't-replicate; two paths to maintain | ★ recommended |
| Consolidation only (= current state / do-nothing) | Keep copying everything into the hub | Zero work, but facts rot and stay trusted-by-default — the problem itself | |
| Coexistence | Copy + bidirectional sync back to sources | Adds write-back and conflict-on-write; operational heaviness we reject | |
| Centralized | Hub owns and originates the fact | Palantir-scale; Engram becomes a system of record | |

Do-nothing here coincides with the Consolidation-only row (it is the status quo). Its cost of delay: agents keep acting confidently on stale/contradicted knowledge — the exact failure mode this proposal exists to remove; the belief layer stays a demo surface with nothing feeding it.

Axis for D3 (conflict resolution): **automatic vs advisory**. Auto-resolve (silently pick a winner) is faster but destroys the audit trail and contradicts the existing `Contradiction`-as-review-record design; advisory-by-default keeps the human/gate in the loop and degrades to automatic under the `autonomous-research` profile. Recommended: advisory-by-default, auto under an explicit profile.

## Risks & what would make this wrong

- **Pre-mortem — ceremony kills adoption.** If federated ingestion forces manual fact promotion, personal and autonomous agents won't use it. *Mitigation:* invisible-by-default promotion — the machinery surfaces only on genuine contradiction; `personal-default` and `autonomous-research` promote without a human.
- **Pre-mortem — `SourceAssertion` and `KnowledgeChunk` blur.** Two overlapping "knowledge" types confuse authors. *Mitigation:* sharp rule — `KnowledgeChunk` = retrievable text copied for search; `SourceAssertion` = a claim linked to a source of record for reconciliation. Document it beside RFC-0002.
- **Key assumptions (falsifiable):**
  - *The existing `Belief`/`BeliefSource`/`Contradiction` model can reconcile federated assertions with additive changes only.* Wrong if attribute-level survivorship needs a per-attribute belief model Engram can't express. (Spike below says it can.)
  - *One promotion-policy seam covers all three domains.* Wrong if a domain needs a trigger outside `{gate, corroboration, settling, command}`.
- **Drawbacks:** a second ingestion path and a new domain type raise surface area and author cognitive load; the authority-profile config is a new thing to get wrong; advisory reconciliation means some facts sit as `candidate` until a trigger fires (acceptable — that is the point).

## Evidence & prior art

- **Spike / de-risk result (traced against code).** Riskiest assumption: the belief layer can serve as the reconciliation engine additively. Confirmed: [`BeliefSource`](../../core/domain/src/belief.rs) already carries `target_type: Assertion`, per-source `confidence`, and bitemporal `valid_from/valid_until`; `Belief` has bitemporal validity, a status lifecycle (`Active/Stale/Superseded/Retracted/Archived`), a sources vec, and contradiction records; [`temporal.rs`](../../core/belief/src/temporal.rs) has `interval_contains`/`live_at`; attribute-level survivorship maps to one belief per `(subject, attribute)` via `BeliefSubject.key`. No fundamental remodel needed — the path is new `SourceAssertion` → `BeliefSource(Assertion)` → add `authority_level` → survivorship as a `BeliefSynthesizer` → advisory `Contradiction` on tie.
- **Repo precedent.** RFC-0002 (corpus model, Draft — extendable); RFC-0004 (enterprise demo — no federation/authority concept, this fills the gap); ADR-0011 (explicit-command trigger baseline, invites follow-on triggers, forbids only schedulers); ADR-0005 (write-as-one-transaction + idempotency any assertion adapter must honor); the advisory `ContradictionDetector` in [belief-sqlite](../../adapters/orchestration/belief-sqlite/src/detector.rs) (detects, never mutates).
- **External prior art.** The claim–reconciliation shape is solved in master-data management. [Profisee — MDM Implementation Styles](https://profisee.com/blog/master-data-management-implementation-styles/) confirms the four style *definitions* (Registry/Consolidation/Coexistence/Centralized): Registry "links to sources without copying", Consolidation "not synced back … ideal for BI", Coexistence bidirectional, Centralized "only system accepting updates". (That these four *exhaustively partition* the who-owns-updates axis is the RFC's own argument — see Options — not a claim attributed to the vendor.) [Profisee — MDM Survivorship](https://profisee.com/blog/mdm-survivorship/) confirms per-attribute authority ("build logic based on each attribute … not full records from a single source") and the strategy set (source-priority, recency, completeness, quality-score) grounding D3. Engram's addition beyond MDM: bitemporal validity on the *assertions* (MDM golden records are typically current-state) and the memory↔knowledge provenance link.

## Open questions

- **Authority-tier vocabulary.** Recommended default: the ordered tiers `{ primary, secondary, inferred }` plus a per-attribute source-priority list, with `enterprise-gate` overlaying `semantic/record/policy`. (Tokens chosen to not collide with the `review_status` lifecycle.) Owner: phanijapps. Decide-by: before implementation ADR.
- **Survivorship trait boundary.** The strategy runs in `core/belief` (decided — see Proposal). Open sub-question: does the profile (authority ordering + trigger) enter as a trait parameter resolved in `core/belief`, or as config data loaded by the orchestration layer and passed in? Recommended default: config data passed in, so profiles are not compiled-in. Owner: phanijapps. Decide-by: implementation ADR.
- **Corroboration independence.** How to count "independent" sources for the `autonomous-research` trigger (wire syndication makes many outlets one source). Recommended default: dedupe by source-domain + originating-agency for v1; refine later. Owner: phanijapps. Decide-by: when the autonomous profile is built.

## Follow-on artifacts

<!-- Filled in on acceptance. -->
- ADR: `SourceAssertion` domain type + `authority_level` on the belief-source relation.
- ADR: consolidation validation-event trigger family (extends ADR-0011).
- Spec: `docs/specs/federated-assertion-ingestion/` (Registry adapter + survivorship synthesizer).
- Follow-on RFC: `ApplicabilityRule` (deferred from D5).
