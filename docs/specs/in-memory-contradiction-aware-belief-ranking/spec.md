# Spec: In-Memory Contradiction-Aware Belief Ranking

- **Status:** Shipped
- **Owner:** phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** ADR-0003, ADR-0004
- **Brief:** none
- **Contract:** none
- **Shape:** service

> **Spec contract:** this document defines what "done" means. The implementing
> PR must match this spec, or update it. Verification must be derivable from it.

## Objective

Belief retrieval uses explicit open contradiction records as ranking evidence:
active beliefs that match the query still remain retrievable, but open scoped
contradictions targeting those beliefs reduce their deterministic retrieval
score and explain why the rank changed.

## Boundaries

### Always do

- Use only stored, scoped, open contradiction records as ranking evidence.
- Preserve belief retrieval as a distinct `RetrievalTargetType::Belief` result.
- Keep contradicted beliefs retrievable unless normal lifecycle or policy rules
  omit them.
- Explain contradiction-aware score reduction when explanations are requested.

### Ask first

- Automatically retract, supersede, or archive contradicted beliefs.
- Add model-assisted contradiction detection or semantic conflict scoring.
- Change public v1 retrieval schemas or domain data model fields.

### Never do

- Treat resolved or ignored contradictions as active ranking penalties.
- Penalize beliefs from contradictions outside the request scope.
- Mutate belief or contradiction records during retrieval.
- Add model, embedding, vector, SQL, scheduler, runtime, or TypeScript
  dependencies for this in-memory slice.

## Testing Strategy

- TDD: belief retrieval tests cover open-contradiction down-ranking, resolved
  contradiction recovery, scope isolation, and explanation text.
- Regression: existing belief repository, contradiction detection, and retrieval
  tests continue to pass.
- Goal-based: full repository gates and contract drift checks continue to pass
  without public schema changes.

## Acceptance Criteria

- [x] Open contradictions targeting a matching belief reduce that belief's
  retrieval score.
- [x] Matching contradicted beliefs remain retrievable as beliefs.
- [x] Resolved or ignored contradictions do not apply the ranking penalty.
- [x] Out-of-scope contradictions do not apply the ranking penalty.
- [x] Contradiction-aware ranking is explained when explanations are requested.
- [x] Retrieval does not mutate belief or contradiction records.
- [x] No public v1 schema changes are introduced.

## Assumptions

- Technical: beliefs and contradictions are stored separately in in-memory state
  (source: the retired memory in-memory adapter (see `docs/specs/retire-memory-inmem/spec.md`)).
- Technical: belief retrieval already creates `RetrievalTargetType::Belief`
  candidates through shared fusion (source:
  the retired memory in-memory adapter (see `docs/specs/retire-memory-inmem/spec.md`)).
- Technical: explicit contradiction resolution now updates review records
  without target mutation (source:
  `docs/specs/in-memory-contradiction-resolution/spec.md`).
- Process: semantic contradiction detection remains future work until a quality
  spec exists (source: `docs/implementation-roadmap.md`).
