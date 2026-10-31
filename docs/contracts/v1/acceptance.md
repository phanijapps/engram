# v1 Contract Acceptance

## Status

Accepted on 2026-06-29.

## Accepted Surfaces

- `Actor`
- `Requester`
- `Scope`
- `Policy`
- `EvidenceRef`
- `DerivationRef`
- `Provenance`
- `MemoryRecord`
- `MemoryContent`
- `MemoryLink`
- `MemoryEvent`
- `KnowledgeSource`
- `SourceDocument`
- `KnowledgeChunk`
- `RetrievalRequest`
- `QueryFilter`
- `Cue`
- `RetrievalResult`
- `RetrievalScore`
- `RetrievalExplanation`
- `FusionTrace`
- `RetrievalSourceFailure`
- `ContextPayload`
- `OmittedResult`
- `WriteMemoryRequest`
- `WriteMemoryResponse`
- `ForgetRequest`
- `ForgetResult`
- `EvaluationFixture`

## Deferred Surfaces

- Belief network behavior
- Contradiction detection and resolution behavior
- Hierarchy build and navigation behavior
- Taxonomy evolution behavior
- Consolidation and sleep-cycle behavior
- Ingestion execution behavior
- Training export governance

## Required Validation

Before changing accepted v1 artifacts, run:

```bash
python3 tools/contracts/validate_contracts.py
.codex/hooks/check_contracts.py
.codex/hooks/check_docs.py
```

## Breaking Change Process

Breaking changes require a future `contracts/v2/` package and an ADR. Do not
rename, remove, or change the meaning of accepted v1 fields in place.
