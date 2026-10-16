# ADR-0030: Code explanations remain derived knowledge

- **Status:** Accepted
- **Date:** 2026-10-14
- **Decision-makers:** phanijapps
- **Supersedes:** none
- **Related:** [RFC-0021](../rfcs/0021-structural-code-context.md), [RFC-0020](../rfcs/0020-unified-knowledge-indexing.md), [ADR-0029](0029-normalized-code-parser-port.md), [ADR-0009](0009-retrieval-composition-seam.md)

## Context

Nontechnical consumers need explanations of code behavior with enough source
context to inspect the evidence. Current code scan retains deterministic
chunks/graph facts, and TypeScript maintenance has an injected LLM seam.
The accepted RFC-0021 calls these logical stages Bronze, Silver and Gold.
Generated explanations and inferred requirements must not replace source or
become authoritative merely because they are stored alongside a code graph.

## Decision

Code explanations are separate, versioned, evidence-linked knowledge derived
from retained source and deterministic context; PM-facing requirements views
consume that knowledge without silently promoting inference into requirements.

Bronze comprises revision-grounded source snapshots and deterministic units /
graph facts. Silver comprises behavior summaries with evidence, model/prompt
and context provenance, freshness and source-derived scope/policy. Gold is a
view over supporting source, summaries, code/docs and process links that
identifies observed behavior and inferred intent separately. These are logical
stages over existing knowledge/retrieval ports, not three persistence engines
or a merger of memory, belief and knowledge domains.

TypeScript performs optional non-agentic generation through the injected LLM
seam. Deterministic Rust services prepare authorized inputs and validate/commit
outputs through knowledge ports. Rust has no LLM client. The enrichment grant
binds requester, job, destination, input policies/versions and finite usage
budgets; readable source alone does not authorize external processing.
Summaries have their own identity/lifecycle, lose current visibility when any
supporting input becomes stale or restricted, and remain marked as derived.

## Consequences

Consumers can retrieve explanations with source evidence and rebuild derived
artifacts. Deterministic validation and policy are shared across transports.
The cost is extra records, context/provenance tracking, dependency invalidation,
provider authorization and quality evaluation. A valid schema/evidence ID is
not proof that a natural-language claim is correct. Model/provider retention
must be authorized independently of Engram's local retraction guarantees.

**Revisit if:** measured polyglot summary quality fails the agreed pilot bar,
or derived records cannot follow their input visibility/freshness lifecycle.

## Confirmation

- **Mode:** architecture fitness test
- **Signal:** independent source/summary identities, strict prepare/commit rejection fixtures, stale/deleted-input exclusion, transport parity and the RFC's evaluated polyglot pilot.
- **Owner:** phanijapps

Generation and pilot gates are follow-on work; this record does not claim
Silver/Gold is implemented.

## Alternatives considered

- Raw code only: avoids generation cost but leaves the approved explanation
  workflow unmet.
- Transient explanations: simpler storage, but cannot support durable repeated
  retrieval with an inspectable derivation/version lifecycle.
- Replace original chunks with summaries: reduces detail but destroys source
  authority and makes generated claims indistinguishable from implementation.
- Validate/persist only in TS or backend adapters: duplicates deterministic
  invariants or strands them on one transport/engine.

## References

- [RFC-0021](../rfcs/0021-structural-code-context.md), D2–D5.
- [RFC-0020](../rfcs/0020-unified-knowledge-indexing.md), deterministic ingest and TS maintenance separation.
- Acceptance records the user's explicit approval on 2026-10-14. Exact contracts and implementation gates are owned by the follow-on specs.
