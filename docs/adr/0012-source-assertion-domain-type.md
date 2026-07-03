# ADR-0012: Federated facts as a SourceAssertion type over authority-tiered belief sources

- **Status:** Accepted
- **Date:** 2026-07-03
- **Decision-makers:** phanijapps
- **Supersedes:** none
- **Related:** RFC-0007 (federated assertion reconciliation — the accepted proposal this records), RFC-0002 (knowledge source extension — the corpus path this sits beside), ADR-0005 (storage adapter semantics), ADR-0010 (behavior port split)

## Decision summary

- **Decision:** We will model a federated fact as a new `SourceAssertion` domain type, referenced from `BeliefSource` via its existing `Assertion` target variant, and add an optional, `#[serde(default)]` authority tier to the belief-source relation.
- **Because:** separating the claim from the layer's stance is what lets competing facts be reconciled and reviewed instead of overwritten — and Engram must not become a replica of the systems that own the facts.
- **Applies to:** the `engram-domain` contract and the belief-source relation; not the corpus `KnowledgeChunk` path, which is unchanged.
- **Tradeoff accepted:** two overlapping "knowledge" types (`KnowledgeChunk` vs `SourceAssertion`) that authors must learn to distinguish.
- **Revisit if:** attribute-level survivorship proves to need a representation the `(subject, predicate)`-plus-`BeliefSource` shape cannot express.

## Context

Engram's knowledge ingestion is replicative and trusting: [`adapters/ingest`](../../adapters/ingest/src/ingestor.rs) copies corpus text into `KnowledgeSource → SourceDocument → KnowledgeChunk → Entity/Relationship` at `confidence: 1.0` with no fact-state. That is correct for a static corpus but wrong for enterprise/operational facts that live in a system of record, are asserted by multiple sources that disagree, evolve over time, and are only trustworthy after evidence or review.

The belief layer already models the *stance* over evidence: [`core/domain/src/belief.rs`](../../core/domain/src/belief.rs) has `Belief` with bitemporal `valid_from/valid_until`, a `BeliefStatus` lifecycle (`Active/Stale/Superseded/Retracted/Archived`), a `sources: Vec<BeliefSource>`, and `Contradiction` records; `BeliefSourceTargetType` already includes an `Assertion` variant that is currently unused. What is missing is the *input*: a first-class record of "source S asserts claim C, with this authority, valid over this interval," distinct from the canonical fact and from the belief. Without it, nothing feeds the belief layer federated facts, and there is no place to attach source-level authority for conflict resolution.

This is distinct from the existing [`MemoryAssertion`](../../core/domain/src/memory.rs) (subject/predicate/object + confidence + `valid_from/valid_until`), which is a claim the *agent* asserts from its own experience, embedded in `MemoryRecord.assertions` and synthesized into beliefs citing `BeliefSource(Memory)` (the Shipped `in-memory-belief-assertion-synthesis`). A `SourceAssertion` is a claim from an *external system of record*: it stands alone (not embedded in a memory), carries federation and authority metadata `MemoryAssertion` lacks, and is cited via the `BeliefSource(Assertion)` variant. The memory/knowledge boundary (AGENTS.md) is why these are two types, not one: an agent's memory is not a federated fact. They share a subject-predicate-object core, which the implementation may factor.

Constraints: `engram-domain` must not depend on SQL, vector stores, async runtimes, or Node (AGENTS.md); public contract changes must be classified compatible or breaking; the change must be additive over serialized beliefs already written by the belief adapter.

## Decision

We will introduce a `SourceAssertion` domain type in `engram-domain` and reference it from beliefs through the existing `BeliefSource { target_type: Assertion }` variant, and we will add an optional authority tier to the belief-source relation.

`SourceAssertion` carries:

- identity and scope: `id` (a new `AssertionId` alias) and `scope`, so a derived belief can cite it and inherit its scope;
- the claim: `subject` (a `BeliefSubject` key), `predicate`, `object`;
- federation, not replication: `source_system`, `source_record_id`, `source_uri` (a link back to the authoritative system — volatile field values are not copied into a durable canonical store);
- `authority_level` — an authority *tier*;
- `confidence`;
- bitemporal time: `valid_from`/`valid_until` (event/application time) and `asserted_at` (knowledge/transaction time);
- `review_status` — the promotion lifecycle (`source | candidate | reviewed | authoritative | disputed | deprecated | rejected`);
- `policy`, `provenance`.

Authority tiers are a small ordered set — `{ primary, secondary, inferred }` by default — sharing **no tokens** with `review_status`, so "how trusted a source is" (authority) never collides with "how far a claim has progressed" (state). Profiles may overlay their own tiers (e.g. `enterprise-gate` uses `semantic/record/policy`).

The two authority fields default differently, by design:

- On `SourceAssertion`, `authority_level` is a non-optional `AuthorityTier` with `#[serde(default)]` = `Primary`: a source that declares no tier is treated as `Primary`, reproducing today's single-source-is-authoritative behavior.
- On the belief-source relation, `authority_level` is `Option<AuthorityTier>` with `#[serde(default)]` = `None`: the field is meaningful only for assertion-backed sources, so a source pointing at a memory/event/chunk carries `None` rather than a spurious tier.

Either way the change is **compatible/additive** — existing serialized beliefs deserialize unchanged (their belief-sources get `None`), and no existing belief is rewritten.

Boundary: `SourceAssertion` is a domain type only. How it is produced (a Registry-style source adapter) and how it is reconciled (a survivorship `BeliefSynthesizer`) are separate concerns recorded elsewhere — this ADR fixes the contract, not the behavior.

## Decision drivers

- **Federate, don't replicate** — Engram must not become a stale copy of the systems of record.
- **Reconcile, don't overwrite** — competing facts must be reviewable, which requires the claim to be a distinct record from the stance.
- **Additive over the existing belief layer** — reuse `Belief`/`BeliefSource`/`Contradiction`; no remodel, no breaking change.
- **Vocabulary hygiene** — authority and lifecycle must not share tokens, or D2/D3 reconciliation logic becomes ambiguous.

## Consequences

**Positive:**
- The belief layer gets its missing input: federated facts it can reconcile over bitemporal validity.
- Source-level authority becomes representable, enabling attribute-level survivorship (one `Belief` per `(subject, attribute)`, matching how `BeliefSubject.key` already works).
- No breaking change; existing beliefs and the corpus path are untouched.
- Federation keeps `source_record_id`/`source_uri`, so retrieval can cite and re-fetch rather than trust a stale copy.

**Negative:**
- Two overlapping knowledge types (`KnowledgeChunk` = retrievable copied text; `SourceAssertion` = a claim linked to a source of record) that authors must be taught to distinguish.
- A new authority-tier vocabulary is one more thing to get wrong; the default must be sane.
- Facts may sit as `candidate` until a promotion trigger fires (acceptable — that is the point).

**Revisit if:** attribute-level survivorship proves to need a representation the `(subject, predicate)`-plus-`BeliefSource` shape cannot express.

## Confirmation

- **Mode:** reviewer-checked
- **Signal:** `engram-domain` gains no SQL/vector/runtime/Node dependency (AGENTS.md boundary); the `authority_level` field is optional/defaulted and a round-trip test deserializes a pre-change belief unchanged; `SourceAssertion` and `authority_level` tokens share nothing with `review_status`.
- **Owner:** phanijapps

## Alternatives considered

- **Fields on `KnowledgeChunk`/`Entity`** (rejected against *reconcile-don't-overwrite*): overloading the corpus record conflates copied text with a source-of-record claim and gives no place for competing assertions to coexist for review.
- **Overload `Belief`/`BeliefSource` only, no distinct type** (rejected against *reconcile-don't-overwrite*): collapsing the claim into the stance means a correction overwrites rather than supersedes, destroying the audit trail the `Contradiction` design exists to preserve.
- **Copy the fact into a canonical store (Consolidation/Centralized style)** (rejected against *federate-don't-replicate*): makes Engram a replica that rots when the source changes; the reduced-build research and RFC-0007 both reject this for volatile facts.

## References

- RFC-0007 `docs/rfcs/0007-federated-assertion-reconciliation.md` (D1, D2, D3, and the migration/compatibility framing).
- [Profisee — MDM Survivorship](https://profisee.com/blog/mdm-survivorship/) for per-attribute (field-level) authority prior art.
