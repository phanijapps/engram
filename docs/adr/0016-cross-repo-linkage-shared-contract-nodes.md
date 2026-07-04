# ADR-0016: Cross-repo linkage: shared contract nodes over symbol matching

- **Status:** Accepted
- **Date:** 2026-07-04
- **Decision-makers:** phanijapps
- **Supersedes:** none
- **Related:** RFC-0008 (cross-repo linkage), ADR-0017 (repository model), RFC-0007/ADR-0012/ADR-0013 (belief reconciliation), `docs/research/cross-repo-linkage.md`

## Decision summary

- **Decision:** A cross-repo integration link is a **shared contract node**, keyed by a normalized contract identifier (REST `method+path`, event channel/topic, gRPC `service/method`), with typed `exposes`/`consumes`/`publishes`/`subscribes` edges from each participating repo.
- **Because:** the two sides of a real cross-service dependency share a *contract*, not a code symbol, and are usually in different languages.
- **Applies to:** cross-repository knowledge-graph linkage of service integrations. Direct linkage via shared published libraries (SCIP-style symbol identity) is a separate track, not covered here.
- **Tradeoff accepted:** requires per-protocol/per-framework extraction and a contract-key normalization layer; symbol-name matching is demoted to a weak hint.
- **Revisit if:** contract-key normalization proves intractable across the target frameworks, or runtime-observed topology becomes the primary linkage need over static contracts.

## Context

Engram builds a per-file code knowledge graph. Today extraction is deterministic and **code-symbol-only** — `Function`/`Class`/`Concept` entities with `calls`/`mentions` edges (`adapters/ingest/src/extractor.rs`); there is no awareness of routes, topics, channels, or gRPC methods, and no OpenAPI/AsyncAPI/`.proto` ingestion. `EntityKind::Api` is defined (`core/domain/src/knowledge.rs:185`) but never produced.

The valuable cross-repo relationships in a real system are service integrations: repo A exposes a REST endpoint repo B calls; repo C publishes a Kafka topic repo D subscribes to. Constraints that shape the choice:

- The producer and consumer **share no code symbol** and are typically in different languages/frameworks, so symbol-name matching (and the pruned `graph-explorer` bare-name view) cannot connect them.
- The generic substrate is already expressive enough to carry typed edges without a contract change: `KnowledgeRelationship` has a free-string `predicate` and `confidence: Option<f32>` (`knowledge.rs:218-236`); entities can accrue evidence from multiple sources via `source_refs`.
- Prior internal design intent (`docs/research/more/it-sdlc-ontology-*.md`) already specifies `produces`/`subscribesTo` predicates, an `InterfaceContract`/`EventStream` concept, and an unbuilt OpenAPI/AsyncAPI "Contract scanner."

## Decision

**We will model a cross-repo integration link as a shared contract node keyed by a normalized contract identifier, with typed provides/consumes edges — not as symbol-name matching.**

- The contract node (e.g. an `EntityKind::Api` entity for a REST/gRPC operation, or a channel entity for events) is keyed by a normalized, language-independent identifier: REST `METHOD` + path template (params folded to placeholders); event channel/topic literal; gRPC `package.Service/Method`.
- Each participating repo attaches with a typed edge (`exposes`/`publishes` for producers, `consumes`/`subscribes` for consumers). Producers and consumers across repos are joined by matching the normalized key; one contract node accrues `source_refs` from every participating repo.
- The participation edge is carried as a `KnowledgeRelationship` (predicate + confidence). Declared-vs-inferred *authority* on the edge is deferred to the implementing spec (RFC-0008 OQ1).
- **Boundary:** this governs cross-*service* (indirect) linkage. Direct linkage — where repo B imports a symbol repo A publishes — is keyed by a shared symbol (SCIP-style `package + version + qualified name`) and is a distinct, later track. Bare entity-name matching is demoted to a weak within-tenant hint.

## Decision drivers

- **Cross-language independence** — the join must work when producer and consumer share no symbol and differ in language.
- **Richness** — the link should carry contract detail (schemas, methods, message types) where available.
- **Substrate reuse** — prefer representing edges with existing domain types over a contract change.
- **Prior-art alignment** — match how service catalogs and contract standards model this.

## Consequences

**Positive:**
- Produces *real* cross-service links across languages, not name-based guesses.
- Contract-first sources (OpenAPI/AsyncAPI/`.proto`) yield rich detail for free and the highest-authority nodes.
- Reuses the generic relationship type; no contract change required for the edge itself.
- Gives `EntityKind::Api` a first producer and realizes the `it-sdlc-ontology` interface model incrementally.

**Negative:**
- Requires per-protocol extractors and a contract-key normalization layer to build and maintain.
- Language/framework independence is not free — it needs a per-framework rule library (parsing is supplied by the existing tree-sitter layer).
- Inferred edges (from dynamically-constructed identifiers) are incomplete and must be surfaced with confidence, never as ground truth.
- Committing to this model is costly to reverse once later phases depend on it.

**Revisit if:** contract-key normalization proves intractable across the target frameworks, or runtime-observed topology (e.g. OpenTelemetry service maps) becomes the primary linkage need over static contract extraction.

## Confirmation

- **Mode:** reviewer-checked
- **Signal:** ingestion emits contract nodes keyed by the normalized identifier with typed `exposes`/`consumes`/`publishes`/`subscribes` edges, and two repos sharing a contract produce a cross-repo link on the shared key.
- **Owner:** maintainer (phanijapps).

## Alternatives considered

- **Bare entity-name matching** (the pruned `graph-explorer` heuristic). Rejected against *cross-language independence* and *richness*: it is language-blind, semantically empty, and over-links common names; demoted to a weak hint.
- **Shared code-symbol identity (SCIP: package + version + qualified symbol).** Correct for genuinely shared published libraries, but useless for cross-language services that share no symbol. Rejected as the *primary* mechanism against *cross-language independence*; retained as a separate direct-linkage track.
- **Runtime/observability topology (OpenTelemetry service graph).** Ground truth of who actually calls whom, but requires a running system and sits outside a static code graph. Rejected as the *base* against *substrate reuse*/scope; named as the future complement for the dynamic case.
- **Do-nothing.** Rejected: cross-service links stay invisible, `EntityKind::Api` stays unused, and the org-wide ambition (RFC-0004) stalls.

## References

- RFC-0008 (cross-repo linkage) and `docs/research/cross-repo-linkage.md`.
- Backstage `providesApi`/`consumesApi`; OpenAPI; AsyncAPI v3 (channels + `send`/`receive`); gRPC/proto IDL; Sourcegraph SCIP (direct-linkage contrast). Full citations in RFC-0008.
