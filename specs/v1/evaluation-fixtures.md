# Spec: Evaluation Fixtures

## Contract Types

- `EvaluationFixture`
- `EvaluationSetup`
- `EvaluationCase`
- `EvaluationExpectation`
- `WriteMemoryRequest`
- `KnowledgeSource`
- `SourceDocument`
- `KnowledgeChunk`
- `RetrievalRequest`

## Required Behavior

- Fixtures are portable contract data, not executable scripts.
- `setup.memories` uses `WriteMemoryRequest` so memory seeding follows normal
  policy and provenance rules.
- `setup.sources`, `setup.documents`, and `setup.chunks` may seed
  source-grounded knowledge without requiring an ingestion engine.
- Each case contains one retrieval request and one expectation.
- `mustInclude` defines required target IDs.
- `mustExclude` defines forbidden target IDs.
- `requiresExplanation` asserts that returned matching results need an
  explanation.

## Forbidden Behavior

- Do not make fixture correctness depend on model-provider randomness.
- Do not require exact vector scores unless the provider is deterministic and
  explicitly stubbed.
- Do not hide policy leakage by marking a forbidden target as merely low score.

## Acceptance Checks

- Valid fixture example: `contracts/v1/examples/evaluation-fixture.json`.
- Fixture with duplicate case IDs is invalid for runner behavior even if schema
  validation accepts it.
- Fixture that requires an explanation fails if a matching result has no
  explanation.
- Fixture fails if any `mustExclude` target appears in returned context.
