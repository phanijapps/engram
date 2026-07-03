# Plan: source-assertion-reconciliation

- **Spec:** [`spec.md`](spec.md)
- **Status:** Drafting

> **Plan contract:** this is the implementation strategy. Unlike the spec, this
> document is allowed to change as you learn. When it changes substantially
> (a different approach, not just a re-ordering), note why in the changelog
> at the bottom.

## Approach

Two layers, bottom-up. First a portable `SourceAssertion` type in `engram-domain`
(no infra deps), plus an optional `#[serde(default)]` `authority_level` on the
belief-source relation — additive and wire-compatible, so existing serialized
beliefs are untouched. Then an in-memory authority-aware survivorship synthesizer
in `core/belief`, built as pure functions that take an injected `AuthorityPolicy`
value (tier ordering + tie rule) and a slice of `SourceAssertion`s, and return
derived `Belief`s plus advisory `Contradiction`s. The synthesizer reuses the
existing bitemporal helpers (`temporal.rs::interval_contains`/`live_at`) for
point-in-time selection and follows the existing detect-don't-overwrite stance
of `ContradictionDetector`. The riskiest part is the reconciliation rule itself
— getting authority-beats-recency and the equal-authority-tie-produces-a-
contradiction (not a silent winner) exactly right — so those are pure,
table-driven TDD with no I/O. No SQLite, no adapter, no trigger wiring in this
slice.

## Constraints

- **ADR-0012** — `SourceAssertion` is a distinct domain type referenced via
  `BeliefSource { target_type: Assertion }`; `authority_level` is optional and
  defaults to `primary`; authority tiers share no token with `review_status`.
- **ADR-0013** — the authority policy is injected data, not a compiled-in
  profile; reconciliation runs inside a caller-invoked path (no scheduler).
- **RFC-0007** — federate, don't replicate (link via `source_record_id`);
  advisory reconciliation, promotion deferred.
- **reference.md** — `engram-domain` depends on nothing infra; behavior lives in
  `core/belief`; in-memory before SQLite; typed `CoreError`/`CoreResult`.

## Construction tests

Most construction tests live per-task below. Cross-cutting:

**Integration tests:** one `core/belief` test that drives the synthesizer end to
end over a fixture set of `SourceAssertion`s (mixed tiers, overlapping and
disjoint validity, one equal-authority conflict) and asserts the derived beliefs,
their `BeliefSource(Assertion)` provenance, and exactly one `Contradiction`.
**Manual verification:** none (no UI/runtime surface in this slice).

## Design (LLD)

Shape: `service`. Sub-sections: data & schema, interfaces & contracts, failure &
resilience.

### Data & schema
`SourceAssertion` (new, `engram-domain`): `id: AssertionId`, `scope: Scope`,
`subject: BeliefSubject`, `predicate: String`, `object: Scalar`, `source_system:
String`, `source_record_id: String`, `source_uri: Option<String>`,
`authority_level: AuthorityTier` (serde default `Primary`), `confidence:
Option<f32>`, `valid_from/valid_until: Option<Timestamp>`, `asserted_at:
Timestamp`, `review_status: AssertionReviewStatus`, `policy: Policy`, `provenance:
Provenance`. `AssertionId` is a new id alias in `engram-domain` (mirroring the
existing `BeliefId`/`MemoryId` aliases). `AuthorityTier` enum `{ Primary,
Secondary, Inferred }`; `AssertionReviewStatus` enum `{ Source, Candidate,
Reviewed, Authoritative, Disputed, Deprecated, Rejected }` (disjoint token sets).
`authority_level` also added to `BeliefSource` as `Option<AuthorityTier>` with
`#[serde(default)]`. Traces to: AC1, AC2, AC3 · contracts: none (domain-only).

### Interfaces & contracts
`AuthorityPolicy { tiers_high_to_low: Vec<AuthorityTier>, tie: TieRule }` — a
value, not a trait; `TieRule` is `ContradictionOnTie` for this slice. Synthesizer
entry point (pure): `reconcile(assertions: &[SourceAssertion], at: Timestamp,
policy: &AuthorityPolicy) -> CoreResult<Reconciled>` where `Reconciled { beliefs:
Vec<Belief>, contradictions: Vec<Contradiction> }`. Scope is not a parameter: each
derived `Belief`/`Contradiction` inherits the winning assertion's `scope`, and the
caller is responsible for supplying single-scope assertion sets (the boundary
rule). `reconcile` returns `Err(CoreError::InvalidRequest)` when the policy's
`tiers_high_to_low` is empty (no tier order to rank by) or lists a tier twice.
No `BeliefSynthesizer` trait change required; this is an additive free function in
`core/belief` a future consolidation task can call. Traces to: AC4, AC5, AC7
· contracts: none.

### Behavior & rules
The reconciliation rule, per `(subject, predicate)`: (1) keep only assertions
`live_at(at)`; (2) find the highest authority tier present among them; (3) among
that top tier, if all agree on `object`, derive one `Belief` (ordering by
`confidence`, then `asserted_at`, only to pick among *agreeing* assertions for the
citation); (4) if any two top-tier assertions *disagree* on `object` over
overlapping validity, derive **no** belief and emit exactly one `Contradiction`.
Confidence never resolves a disagreement — it only orders agreeing assertions.
Lower-tier assertions never override or break a tie against a higher tier. Traces
to: AC4, AC5, AC7.

### Failure, edge cases & resilience
Empty input → empty `Reconciled`. Empty/duplicated policy tier list →
`Err(CoreError::InvalidRequest)`. All assertions for a subject outside their valid
interval at `at` → no belief for that subject (AC6). Single assertion → belief
with that source, no contradiction. Equal-top-authority overlapping disagreement →
no belief winner + one `Contradiction`, inputs unmutated (AC7). Never panic on
malformed intervals (`valid_until <= valid_from` treated as never-live). Traces
to: AC6, AC7.

## Tasks

### T1: `SourceAssertion` domain type + `authority_level` on `BeliefSource`

**Depends on:** none

**Touches:** core/domain/src/assertion.rs, core/domain/src/belief.rs, core/domain/src/types.rs, core/domain/src/lib.rs

**Tests:**
- Serde round-trip of `SourceAssertion` (all fields incl. `id`/`scope`; optional
  fields absent).
- `authority_level` absent on a belief-source JSON deserializes to default
  `Primary`; a pre-change `Belief` fixture deserializes unchanged (AC2).
- `AuthorityTier` token set ∩ `AssertionReviewStatus` token set = ∅ (AC3).
- Compile check: `engram-domain` has no infra dependency (AC8).

**Approach:**
- Add `SourceAssertion`, `AuthorityTier`, `AssertionReviewStatus` in a new focused
  module `core/domain/src/assertion.rs` (not a catch-all `lib.rs`); re-export
  narrowly from `lib.rs`.
- Add `AssertionId` alias in `core/domain/src/types.rs` beside `BeliefId`/`MemoryId`.
- Add `authority_level: Option<AuthorityTier>` with `#[serde(default)]` to
  `BeliefSource` in `core/domain/src/belief.rs`.

**Done when:** the round-trip, default, disjoint-token, and `cargo check` tests
are green (AC1, AC2, AC3, AC8).

### T2: authority policy value + point-in-time survivorship selection

**Depends on:** T1

**Touches:** core/belief/src/reconcile.rs, core/belief/src/lib.rs

**Tests:**
- Higher-tier live assertion wins over a lower-tier one asserted more recently
  (authority beats recency) (AC4, AC5).
- Swapping `AuthorityPolicy.tiers_high_to_low` flips the winner with no code
  change (AC4).
- An assertion outside `[valid_from, valid_until)` at `at` does not win; uses
  `temporal.rs::live_at` (AC6).
- Malformed interval (`valid_until <= valid_from`) treated as never-live, no panic.
- Empty or duplicated `tiers_high_to_low` → `Err(CoreError::InvalidRequest)`.

**Approach:**
- Add `AuthorityPolicy` + `TieRule` value types and the `reconcile` free function
  in a new `core/belief/src/reconcile.rs`; re-export from `lib.rs`.
- Selection: validate policy; group by `(subject, predicate)`; filter by
  `live_at(at)`; pick the highest tier present; among agreeing top-tier
  assertions order by `confidence` then `asserted_at` to choose the cited winner.
  Confidence is used **only** to order assertions that agree on `object` — it
  never adjudicates a disagreement (that path is T4).

**Done when:** selection tests green, including policy-swap, bitemporal, and
invalid-policy cases (AC4, AC5, AC6).

### T3: belief derivation with `BeliefSource(Assertion)` provenance

**Depends on:** T2

**Touches:** core/belief/src/reconcile.rs

**Tests:**
- Winning assertion yields a `Belief` whose `sources` contains `BeliefSource {
  target_type: Assertion, target_id: <assertion id>, authority_level: Some(tier),
  confidence, valid_from, valid_until }` (AC5).
- Derived belief carries the winning assertion's `scope` and `policy`, and a
  `valid_from/valid_until` window equal to the winner's.
- Derived `content` equals the canonical rendering of the winner's
  `(predicate, object)`.

**Approach:**
- Map the winning `SourceAssertion` to a `Belief`: `subject` ← assertion subject;
  `content` ← a deterministic `"{predicate} {object}"` rendering; `scope`/`policy`
  ← winner's; `status` ← `Active`; `confidence` ← winner's confidence (default
  1.0 when absent); `valid_from/valid_until` ← winner's; `provenance` ← derived
  from the winner with `DerivationKind::Consolidation` (reconciliation runs as a
  consolidation activity — reuse the existing variant; no belief-contract change).
- Cite the assertion through the previously-unused `BeliefSourceTargetType::Assertion`.

**Done when:** provenance, scope/policy, content, and validity-mirroring tests are
green (AC5).

### T4: advisory contradiction on equal-authority tie

**Depends on:** T2

**Touches:** core/belief/src/reconcile.rs

**Tests:**
- Two equal-top-authority assertions disagreeing over overlapping validity →
  no trusted belief winner + exactly one `Contradiction` (AC7).
- Neither input assertion is mutated (inputs compared before/after) (AC7).
- Equal-authority assertions with *disjoint* validity do not contradict (they
  are sequential truth, not conflict).
- Higher confidence on one side does **not** resolve a same-tier disagreement —
  a `Contradiction` is still emitted (guards Blocker-2 semantics).

**Approach:**
- In `reconcile`, when the top tier has >1 live assertion with conflicting
  `object` over overlapping intervals, emit one `Contradiction` with
  `kind: Logical`, `status: Open`, and `severity` set the same way the existing
  `ContradictionDetector` sets it (reuse that `f32` severity derivation, do not
  invent a new value; treat an assertion's absent `confidence` as `1.0` — the
  same default T3 uses — so the derivation is total), citing both assertions as
  `ContradictionTargetType::Assertion`; produce no belief for that
  `(subject, predicate)`.

**Done when:** the tie, no-mutation, disjoint-validity, and confidence-does-not-
break-tie tests are green (AC7).

### T5: end-to-end synthesizer integration test

**Depends on:** T3, T4

**Tests:**
- The cross-cutting integration test (see Construction tests) over a mixed
  fixture: asserts derived beliefs, `BeliefSource(Assertion)` provenance, and
  exactly one contradiction.

**Approach:**
- Assemble a fixture set exercising tier precedence, bitemporal exclusion, and
  one equal-authority conflict; drive `reconcile` once.

**Done when:** the integration test is green and `cargo fmt --all` + `cargo check
--workspace` pass.

## Rollout

Pure-logic, in-memory change with no delivery, infra, or sequencing concerns:
domain type + a free function in `core/belief`. No feature flag, no migration
(the one contract change is additive and serde-default-compatible), no external
integration. Reversible by removing the new type/function.

## Risks

- **Tie semantics over-fit.** Defining "conflict over overlapping validity" too
  narrowly (or too broadly) mis-fires contradictions. Mitigation: table-driven
  tests including the disjoint-validity (no-conflict) case.
- **Authority/recency inversion.** A subtle rank bug could let recency beat
  authority. Mitigation: an explicit AC4/AC5 test asserting authority dominates a
  more recent lower-tier assertion.
- **Scope creep into promotion/adapters.** The trigger family and Registry
  adapter are tempting to pull in. Mitigation: `Never do` boundary + deferred to
  follow-on specs.

## Changelog

- 2026-07-03: initial plan.
