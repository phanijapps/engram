# Engram Contract v1

## Status

Accepted for the core memory contract.

## Accepted Surface

V1 accepts these portable shapes:

- Identity and scope: `Actor`, `Requester`, `Scope`
- Policy and provenance: `Policy`, `EvidenceRef`, `DerivationRef`,
  `Provenance`
- Memory: `MemoryRecord`, `MemoryContent`, `MemoryLink`, `MemoryEvent`
- Knowledge setup/retrieval targets: `KnowledgeSource`, `SourceDocument`,
  `KnowledgeChunk`
- Retrieval: `RetrievalRequest`, `QueryFilter`, `Cue`, `RetrievalResult`,
  `RetrievalScore`, `RetrievalExplanation`, `FusionTrace`,
  `RetrievalSourceFailure`, `ContextPayload`, `OmittedResult`
- Operations: `WriteMemoryRequest`, `WriteMemoryResponse`, `ForgetRequest`,
  `ForgetResult`
- Evaluation: `EvaluationFixture`, `EvaluationSetup`, `EvaluationCase`,
  `EvaluationExpectation`

## Deferred Surface

These remain draft extension contracts and are not accepted v1 behavior:

- Belief network write/retrieval behavior
- Contradiction detection and resolution behavior
- Hierarchy build and navigation behavior
- Taxonomy evolution behavior
- Consolidation/sleep-cycle behavior
- Ingestion execution behavior

Knowledge source, document, and chunk records are included in v1 because
evaluation fixtures and retrieval targets need them. The actual ingestion engine
is deferred.

## Schemas

- `schemas/engram-v1.schema.json`: shared definitions.
- `schemas/memory-record.schema.json`
- `schemas/retrieval-request.schema.json`
- `schemas/context-payload.schema.json`
- `schemas/write-memory-request.schema.json`
- `schemas/write-memory-response.schema.json`
- `schemas/forget-request.schema.json`
- `schemas/forget-result.schema.json`
- `schemas/evaluation-fixture.schema.json`

## Examples

Examples under `examples/` are normative shape examples. They are not exhaustive
behavior tests; the specs under `docs/specs/` define acceptance behavior.

Accepted retrieval evaluation fixtures cover positive recall, forbidden recall,
budget-constrained retrieval, and no-result behavior. They are executable
through `engram-eval`.

Accepted forget examples cover delete, redact, tombstone, and archive mode
payloads. The unsuffixed `forget-request.json` and `forget-result.json` files
remain the canonical tombstone examples.

Invalid examples under `examples/invalid/` are negative contract fixtures. They
must fail schema validation or explicit semantic checks in
`tools/scripts/validate_contracts.py`.

## Validation

```bash
python3 tools/scripts/validate_contracts.py
.codex/hooks/check-contracts.sh
```
