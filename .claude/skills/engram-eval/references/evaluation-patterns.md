# Evaluation Patterns

Use these patterns to design engram quality checks.

## Fixture Shape

Each fixture should declare:

- Scope and requester.
- Records, source documents, chunks, beliefs, hierarchy nodes, or policies to seed.
- Retrieval or operation request.
- Expected returned IDs.
- Forbidden returned IDs.
- Required explanation or omission reasons.
- Deterministic assertions and model-dependent assertions separately.

## Baseline Fixtures

- `tenant_isolation`: same content in two tenants never crosses retrieval boundaries.
- `workspace_filtering`: workspace-scoped records do not leak into another workspace.
- `forget_tombstone`: forgotten memory is omitted and reports the policy reason when observable.
- `document_grounding`: returned knowledge chunks include document and source references.
- `code_symbol_lookup`: code ingestion can retrieve symbol, file, and semantic references.
- `belief_contradiction`: new evidence can create a contradiction without overwriting the prior belief.
- `hierarchy_expansion`: query can expand across parent and child hierarchy nodes with traceable paths.
- `fusion_trace`: hybrid retrieval exposes score components and fusion decisions.

## Quality Gates

- No feature is complete without at least one positive and one forbidden retrieval case.
- Do not assert exact vector scores unless the embedding provider is stubbed.
- Prefer stable IDs and explanation categories over brittle text snapshots.
- Keep provider-backed tests opt-in until deterministic stubs exist.
