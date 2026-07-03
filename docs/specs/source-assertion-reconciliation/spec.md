# Spec: source-assertion-reconciliation

- **Status:** Draft
- **Owner:** phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** ADR-0012, ADR-0013, RFC-0007
- **Brief:** none
- **Contract:** none
- **Shape:** service

> **Spec contract:** this document defines what "done" means. The implementing
> PR must match this spec, or update it. Verification must be derivable from it.

## Objective

Engram reconciles competing facts asserted by external sources into a single
trusted belief, weighted by how much each source is trusted rather than by
recency of write. A `SourceAssertion` records one claim (subject-predicate-object)
from a system of record, with the source's identity, an authority tier, its
confidence, and the interval over which the claim is valid. An in-memory
survivorship synthesizer takes an **authority policy as an injected parameter**,
selects the winning assertion per `(subject, predicate)` for a given point in
time, and derives a `Belief` that cites its winning `SourceAssertion`. When two
assertions of equal top authority disagree over an overlapping valid interval,
the synthesizer does not silently pick one — it derives no trusted winner and
emits an advisory `Contradiction` for review. The user is an agent (or the host
building on Engram) that needs to hold a defensible position on what is true
across sources that disagree and change over time; success is that the derived
belief reflects the highest-authority live assertion, carries provenance back to
its source, and never overwrites a competing claim in silence.

## Boundaries

The three-tier guard that keeps an implementing agent inside the lines.
*Always do* applies without asking; *Ask first* requires human sign-off
before proceeding; *Never do* is a hard rule, even under time pressure.

### Always do

- Keep `SourceAssertion` in `engram-domain` with no SQL, vector, async-runtime,
  Node, or embedding dependency (AGENTS.md; reference.md constraint).
- Take the authority policy (tier ordering + tie rule) as an injected parameter
  to the synthesizer, so named profiles are a future config value, not a code
  change (ADR-0013).
- Preserve provenance: every derived `Belief` cites the winning `SourceAssertion`
  via `BeliefSource { target_type: Assertion }`, and every reconciliation respects
  bitemporal validity using the existing `temporal.rs` helpers (`interval_contains`,
  `live_at`).
- Emit an advisory `Contradiction` (never overwrite) when equal-top-authority
  assertions disagree over an overlapping valid interval.
- Stamp each derived `Belief` (and `Contradiction`) with the winning assertion's
  `scope`; reconcile only over assertions the caller supplies for one scope, and
  never mix assertions across scopes in a single reconciliation.

### Ask first

- Introducing a named-profile switch, a config loader, or `enterprise-gate` /
  `autonomous-research` presets (this slice ships one default policy value only).
- Changing the public belief domain contract beyond adding `SourceAssertion` and
  the optional `authority_level` field.
- Adding model inference, entity resolution, or embedding-backed similarity to
  reconciliation.

### Never do

- Never copy volatile source field values into a durable canonical store —
  `SourceAssertion` links to the source (`source_record_id`, `source_uri`);
  federation, not replication (RFC-0007 D1).
- Never resolve an equal-authority conflict by silently choosing a winner or by
  mutating the losing assertion.
- Never add a new top-level workspace crate or a scheduler/timer dependency for
  this slice (promotion triggers are a deferred follow-on; reconciliation runs
  inside a caller-invoked path).
- Never share tokens between the `authority_level` tier set and the assertion
  `review_status` lifecycle.

## Testing Strategy

- **`SourceAssertion` shape + invariants: TDD.** Serde round-trip, optional
  `authority_level` defaults to `primary` when absent, and no infra dependency —
  a compressible invariant with clear inputs/outputs.
- **Authority-weighted survivorship selection: TDD.** Given competing assertions
  with different tiers/validity, the highest-authority live assertion wins; a
  lower-tier assertion never beats a higher one regardless of recency. Pure logic
  with an invariant — the core of the slice.
- **Bitemporal selection at a point in time: TDD.** An assertion outside its
  `[valid_from, valid_until)` at the query instant does not compete — exercised
  against `temporal.rs` helpers.
- **Advisory contradiction on equal-authority tie: TDD.** Equal-top-authority
  disagreement over overlapping validity yields no trusted belief winner and one
  `Contradiction`; it does not mutate either assertion. Confidence never resolves
  a same-tier *disagreement* — it only orders assertions that agree on `object`.
- **Provenance trace: TDD.** Each derived belief's `sources` cites the winning
  `SourceAssertion` id with `target_type: Assertion`, and the belief's scope is
  the winning assertion's scope — asserted inside the T5 integration test.
- **Boundary conformance: goal-based check.** `cargo check` on `engram-domain`
  proves it pulls in no infra crate; a `grep` proves `authority_level` tokens and
  `review_status` tokens are disjoint.

## Acceptance Criteria

- [ ] `SourceAssertion` exists in `engram-domain` with fields: id (`AssertionId`),
  scope, subject, predicate, object, source_system, source_record_id, source_uri,
  authority_level (optional, `#[serde(default)]`), confidence, valid_from,
  valid_until, asserted_at, review_status, policy, provenance — and serde
  round-trips.
- [ ] A pre-existing serialized belief (no `authority_level`) deserializes
  unchanged; absent `authority_level` defaults to `primary` (compatible/additive).
- [ ] The authority-tier set (`{ primary, secondary, inferred }` default) shares
  no token with the `review_status` lifecycle (`source | candidate | reviewed |
  authoritative | disputed | deprecated | rejected`).
- [ ] The survivorship synthesizer accepts an authority policy as a parameter;
  swapping the policy value changes the winner with no code change.
- [ ] Given competing live `SourceAssertion`s for one `(subject, predicate)`, the
  derived `Belief` reflects the highest-authority assertion and cites it via
  `BeliefSource { target_type: Assertion }`.
- [ ] An assertion outside its valid interval at the query instant does not win.
- [ ] Equal-top-authority disagreement over overlapping validity produces no
  trusted belief winner and exactly one advisory `Contradiction`; neither
  assertion is mutated.
- [ ] `engram-domain` gains no SQL/vector/runtime/Node/embedding dependency
  (`cargo check` clean; boundary preserved).

## Assumptions

- Technical: `SourceAssertion` does not exist yet — new type, no conflict (source: grep `SourceAssertion --include=*.rs` → empty, 2026-07-03).
- Technical: `MemoryAssertion` (subject/predicate/object + confidence + valid_from/valid_until) exists and is distinct — it is agent-asserted and embedded in `MemoryRecord`; `SourceAssertion` is source-of-record-asserted and stands alone (source: core/domain/src/memory.rs:124).
- Technical: belief synthesis from assertions is Shipped but cites `BeliefSource(Memory)`; the `BeliefSource(Assertion)` variant is present-but-unused and is what this slice wires (source: core/belief/src/lifecycle.rs:101; docs/specs/in-memory-belief-assertion-synthesis/spec.md).
- Technical: bitemporal helpers `interval_contains`/`live_at` and an advisory `ContradictionDetector` already exist and are reused (source: core/belief/src/temporal.rs:15,27; adapters/orchestration/belief-sqlite/src/detector.rs).
- Technical: design conforms to the normative reference architecture — domain type in `engram-domain`, synthesizer behavior in `core/belief`, in-memory before SQLite (source: docs/architecture/reference.md).
- Process: spec is a granular in-memory-first slice per project convention; owner/boundary sign-off is phanijapps (source: docs/CONVENTIONS.md §4; dozens of `in-memory-*` specs).
- Product: scope is the reconciliation core; the Registry source adapter, promotion trigger family, and named-profile config are deferred to follow-on specs (source: user confirmation 2026-07-03).
- Product: the slice ships one default authority policy value (the `personal-default` ordering) injected as a parameter, not a named-profile switch (source: user confirmation 2026-07-03).
