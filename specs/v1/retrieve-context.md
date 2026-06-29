# Spec: Retrieve Context

## Contract Types

- `RetrievalRequest`
- `ContextPayload`
- `RetrievalResult`
- `RetrievalScore`
- `RetrievalExplanation`
- `FusionTrace`
- `OmittedResult`
- `RetrievalSourceFailure`

## Preconditions

- Request body validates against `contracts/v1/schemas/retrieval-request.schema.json`.
- `scope.tenant` is present and non-empty.
- `requester.actor` is present.
- `limit`, `budget.maxItems`, `budget.maxTokens`, and `budget.maxBytes` are
  positive when supplied.

## Required Behavior

- Retrieval must apply scope eligibility before result composition.
- Retrieval must apply policy before returning result content.
- The in-memory baseline supports exact and keyword retrieval without requiring
  vector search, SQL, graph search, or model providers.
- Each returned item includes `targetType`, `targetId`, `content`, `score`,
  `provenance`, and `policy`.
- `ContextPayload.createdAt` records composition time.
- If `includeExplanations` is true, selected results should include
  `explanation` whenever the retrieval source can produce one.
- Non-fatal source failures are reported through `sourceFailures` instead of
  being hidden.
- Omitted known candidates are reported through `omitted` when the omission is
  due to policy, budget, low score, expiration, or redaction.

## Forbidden Behavior

- Do not return records from another tenant.
- Do not return forgotten records unless a future explicit audit API is added.
- Do not expose redacted content through result text, explanation, metadata, or
  omitted-result detail.
- Do not require vector search, SQL, graph search, or model providers for v1
  contract conformance.

## Acceptance Checks

- Valid request example: `contracts/v1/examples/retrieval-request.json`.
- Valid response example: `contracts/v1/examples/context-payload.json`.
- Cross-tenant fixture must return no leaked items.
- Policy-denied candidate must not appear in `items`.
- Budget-excluded candidate may appear in `omitted` with
  `reason: budget_exceeded`.
- Exact or keyword matches should include matched terms when explanations are
  requested.
