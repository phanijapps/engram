# Phase 03 Spec: Retrieve Context Slice

## Status

Done for the in-memory baseline.

## Scope

Retrieve exact and keyword matches from in-memory records while enforcing scope,
policy, status, budget, and explanations.

## Acceptance

- Retrieval never crosses tenant or narrowed scope boundaries.
- Policy-denied candidates are omitted, not returned.
- Budget-truncated candidates appear in `omitted`.
- Accepted and invalid v1 retrieval examples execute as fixtures.
