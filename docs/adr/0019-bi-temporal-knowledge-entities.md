# ADR-0019: Bi-temporal knowledge entities

- **Status:** Accepted
- **Date:** 2026-07-08
- **Decision-makers:** phanijapps
- **Supersedes:** none
- **Related:** RFC-0012 (code-structural graph layer), `docs/codegraph-parity-roadmap.md` (B6), ADR-0018 (knowledge-graph retraction/convergence), domain-data-model §Knowledge Model / §Timestamp (`validFrom`/`validUntil` semantics)

## Decision summary

- **Decision:** `KnowledgeEntity` gains optional `validFrom` / `validUntil` timestamps, stamping each entity version with the interval over which it is the authoritative representation of its subject — mirroring the existing `valid_from` / `valid_until` on `Belief`, `MemoryRecord`, and `MemoryAssertion`.
- **Because:** bi-temporal entity versioning is the foundation for symbol version timelines and the temporal scoring engine (codegraph-parity C6); without it, re-ingest / renames can't be reasoned about as "what was true when."
- **Applies to:** the knowledge-graph entity model. `KnowledgeEntity` is a **draft extension** type — it is not in the frozen v1 wire schema (`contracts/v1/schemas/engram-v1.schema.json`), so this is not a v1-breaking change and requires no v1 schema regeneration. Not memory/belief (already bi-temporal).
- **Tradeoff accepted:** adds two optional fields; ingest/retrieval opt into stamping them — there is no default-on bi-temporal semantic today.
- **Revisit if:** entity-level `as_of` retrieval (filtering to versions valid at an instant) needs a first-class `QueryFilter` / `RetrievalRequest` field — that touches the frozen v1 `QueryFilter` and is a separate v1 contract micro-spec.

## Context

`valid_from` / `valid_until` already model truth-intervals on `Belief`
(`core/domain/src/belief.rs`), `MemoryRecord`, and `MemoryAssertion`, but
`KnowledgeEntity` carries only `created_at` / `updated_at` (modification time,
not validity time). Code-structural entities (symbols) change across commits;
"what this function looked like at commit X" needs a validity interval, not a
modification timestamp. RFC-0012 item B6 / the codegraph-parity roadmap call this
out; the A1 audit (`docs/research/codegraph-parity-audit.md`) confirmed the
entity type lacks the fields while belief/memory/assertion have them.

## Decision

Add optional `validFrom` / `validUntil` (RFC 3339 `Timestamp`) to
`KnowledgeEntity`:

- Both optional (`None` = open interval / not bi-temporally tracked), so existing
  records and the frozen v1 wire contract are unaffected.
- Semantics follow the existing `Timestamp` meanings in `docs/domain-data-model.md`:
  `validFrom` = when this entity version became the authoritative representation;
  `validUntil` = when it stopped.
- Stamping the intervals on ingest / re-ingest (tying `validUntil` to retracted
  versions per ADR-0018, `validFrom` to replacements) is an implementation concern
  for the ingest adapter / temporal scoring, not part of this contract decision.
- A first-class `as_of` retrieval filter is **deferred** to a separate micro-spec:
  `QueryFilter` is in the frozen v1 contract, so adding a field there warrants its
  own compatibility review.

## Consequences

- `KnowledgeEntity` records may carry validity intervals; consumers that ignore
  them are unaffected (optional fields, camelCase via the struct's `rename_all`).
- Enables the temporal scoring engine (C6) and symbol version timelines with no
  v1 breaking change and no v1 schema regeneration.
- Follow-ups: (a) `as_of` retrieval filter (separate v1 contract micro-spec);
  (b) ingest stamping of `valid_from` / `valid_until` on re-index / rename;
  (c) surfacing validity in `RetrievalResult` / explanations.
