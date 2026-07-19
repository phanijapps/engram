# Spec: Forget Mode Contract Examples

- **Status:** Shipped
- **Owner:** phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** ADR-0002, ADR-0003
- **Brief:** none
- **Contract:** `contracts/v1/schemas/forget-request.schema.json`,
  `contracts/v1/schemas/forget-result.schema.json`
- **Shape:** contract fixture set

> **Spec contract:** this document defines what "done" means. The implementing
> PR must match this spec, or update it. Verification must be derivable from it.

## Objective

Engram has accepted v1 forget request/result examples for every accepted
`DeleteMode`: delete, redact, tombstone, and archive. The existing tombstone
example remains the canonical default example; additional mode-specific
examples make lifecycle outcomes explicit for future adapters and bindings.

## Boundaries

### Always do

- Keep examples under `contracts/v1/examples/`.
- Validate each example against the existing forget request/result schemas.
- Keep runtime adapter behavior unchanged in this slice.
- Keep examples focused on portable contract payloads, not repository state.

### Ask first

- Change `DeleteMode`, `ForgetStatus`, or `MemoryEventKind`.
- Add a new forget target type beyond memory.
- Add TypeScript helper APIs for forget examples.
- Change deletion semantics in in-memory or SQL adapters.

### Never do

- Encode SQL row state, in-memory maps, or vector records in examples.
- Treat archive, tombstone, redact, and delete as interchangeable outcomes.
- Hide forget mode in event payloads.
- Make physical deletion the required behavior for every adapter.

## Testing Strategy

- Schema: `tools/scripts/validate_contracts.py` validates all accepted forget
  request/result examples.
- Rust serde: domain tests deserialize every accepted forget example.
- Regression: existing in-memory and SQL forget tests remain unchanged.

## Acceptance Criteria

- [x] Delete request/result examples validate and deserialize.
- [x] Redact request/result examples validate and deserialize.
- [x] Archive request/result examples validate and deserialize.
- [x] Existing tombstone request/result examples continue to validate.
- [x] Contract validation knows about all accepted forget mode examples.
- [x] No schema, Rust domain, or generated TypeScript contract changes.

## Assumptions

- Technical: `forget-request.json` and `forget-result.json` remain the canonical
  tombstone examples for v1.
- Technical: mode-specific examples can reuse stable `memory-001` and
  `event-002` identifiers because examples are independent payloads.
- Process: adapter conformance fixtures for forget execution can build on these
  examples later.
