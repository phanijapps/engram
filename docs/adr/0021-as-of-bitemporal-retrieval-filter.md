# ADR-0021: `as_of` bi-temporal retrieval filter

- **Status:** Accepted
- **Date:** 2026-07-09
- **Decision-makers:** phanijapps
- **Supersedes:** none
- **Related:** ADR-0019 (bi-temporal knowledge entities), RFC-0012 (codegraph layer),
  `docs/domain-data-model.md` §Retrieval Model / §Timestamp

## Decision summary

- **Decision:** `QueryFilter` gains an optional `asOf` timestamp field that
  filters retrieval to entity versions valid at a given instant
  (`valid_from <= as_of < valid_until`).
- **Because:** ADR-0019 added `validFrom`/`validUntil` to `KnowledgeEntity`, but
  retrieval had no way to query by validity interval — only by observed time
  (`since`/`until`). The `as_of` filter lets an agent ask "what did this symbol
  look like at commit X?" (the C6 temporal scoring engine's foundation).
- **Applies to:** `QueryFilter` in the accepted v1 contract. This is a
  **compatible addition** — `asOf` is optional (`None` = no bi-temporal filtering).
- **Tradeoff accepted:** callers that don't set `asOf` see no change; callers that
  do get bi-temporal filtering but must understand the distinction from
  observed-time filters.
- **Revisit if:** a full bi-temporal model (multiple overlapping validity
  intervals per entity) needs more than a point-in-time filter.

## Context

ADR-0019 added `validFrom`/`validUntil` to `KnowledgeEntity` for bi-temporal
versioning. The retrieval model already had `since`/`until` on `QueryFilter`,
but those filter on **observed time** (when the record was created/observed),
not on **validity time** (when the entity was the authoritative representation of
its subject). Without `asOf`, the temporal scoring engine (C6) cannot answer
point-in-time questions like "what was valid at the time of this incident?"

## Decision

Add `asOf: Option<Timestamp>` to `QueryFilter`:

- **Optional** — `None` means no bi-temporal filtering (existing behavior).
- **Semantics:** `valid_from <= as_of < valid_until`. Entities without
  `valid_from` are excluded when `asOf` is set (they have no validity interval).
  Entities without `valid_until` are included (open interval — still valid).
- **Storage-neutral:** the filter is a contract-level field; how adapters
  implement the interval check is their business (SQL `BETWEEN`, in-memory scan,
  or graph query).
- **Must not overclaim:** per the domain model's compatibility rules, `as_of`
  provides **point-in-time validity filtering**, not full bitemporality (which
  would require system-time + valid-time axes). The domain invariant
  "valid-time `as_of` support must not be described as full bitemporality" holds.

## Consequences

- `QueryFilter` gains one optional field — compatible under the freeze policy
  ("Add optional fields").
- Adapters that support `as_of` (currently: none; the MCP server's temporal
  tools compute scores from entity timestamps but don't filter retrieval by
  `as_of` yet) implement the interval check; adapters that don't simply ignore it.
- Enables C6 temporal scoring to operate on point-in-time views.
- ADR-0019's `validFrom`/`validUntil` fields now have a first consumer in the
  retrieval contract.
