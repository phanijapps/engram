# ADR 0002: Language Selection Criteria

## Status

Draft

## Context

The memory layer may need strong API ergonomics, fast local iteration, durable
storage integrations, high-quality ML ecosystem access, and predictable
deployment behavior. The language choice should follow the intended first
vertical slice and user surface.

## Candidate Stacks

### TypeScript

Strengths:

- Strong fit for web APIs, SDKs, and agent app integrations.
- Good developer experience for composable packages.
- Natural fit for JSON schemas and OpenAPI contracts.

Risks:

- ML and embedding experimentation may depend on external services or Python
  sidecars.
- Long-running worker reliability requires disciplined runtime choices.

### Python

Strengths:

- Best access to ML, embeddings, notebooks, and agent framework experiments.
- Fastest research-to-prototype loop.
- Rich data tooling for evaluation.

Risks:

- SDK packaging and typed API boundaries need extra care.
- Service performance and concurrency choices should be explicit.

### Rust

Strengths:

- Strong correctness, performance, and embeddable library potential.
- Good fit for local-first indexes and durable storage primitives.
- Excellent for a stable core once contracts are clear.

Risks:

- Slower iteration for research-heavy memory behavior.
- Smaller agent-framework ecosystem than TypeScript or Python.

### Hybrid

Strengths:

- Allows a fast research layer and a hardened core.
- Can support multiple SDK surfaces.

Risks:

- Cross-language contracts, packaging, and testing overhead arrive early.

## Selection Criteria

- First user surface: local library, HTTP service, CLI, or framework plugin.
- Retrieval and storage dependencies needed for the first vertical slice.
- Evaluation workflow and data science needs.
- Deployment target and operational constraints.
- Contributor familiarity and maintenance cost.
- Contract stability and SDK ergonomics.

## Decision Needed

Choose the first implementation stack before adding runtime-specific package
manifests. A later ADR can approve a hybrid split if the first slice proves the
need.
