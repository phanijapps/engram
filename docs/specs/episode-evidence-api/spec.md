# Spec: Episode / evidence (provenance) query API (S2)

- **Status:** Shipped
- **Owner:** phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** ADR-0022 (engine neutrality), [`rust-crate-integration`](../rust-crate-integration/spec.md) (the Implementing facade S2 extends), [`provider-sdk-capability-report`](../provider-sdk-capability-report/spec.md) (S1 — owns the `episodes_evidence` capability key S2 flips)
- **Brief:** [`docs/product/briefs/engram-host-sdk.md`](../../product/briefs/engram-host-sdk.md) (slice S2, capability #6)
- **Contract:** none — a Rust port trait (`ProvenanceQuery`) + provider handle, not a `contracts/<type>` interface.
- **Shape:** service

> **Spec contract:** this document defines what "done" means. The implementing
> PR must match this spec, or update it. Verification must be derivable from it.

## Objective

A host reads durable provenance and evidence for a record through one
backend-neutral query surface on `EngramProvider`. The port accepts any target
type as a typed input; in v1 it returns the supporting `Provenance` and
`EvidenceRef` already embedded in stored records for the **knowledge-graph core**
— entity, relationship, and source — optionally narrowed by scope
(tenant/workspace/session/environment) and a time window over
`Provenance.observed_at`. This explains *why* an entity, relationship, or source
exists. The `episodes_evidence` capability, reported `Unsupported { FeatureDisabled }`
today, becomes `Supported` behind a `ProvenanceQuery` provider handle wired through
the SQLite backend.

S2 is **read-only**. Recording of episodes and evidence continues to flow through
the existing knowledge/memory/belief repository writes, which already carry
`Provenance`/`EvidenceRef` at write time; a dedicated *attach-evidence-to-an-
existing-record* write operation is documented as a future enhancement and is
**out of scope** (logged in `docs/backlog.md`).

## Boundaries

The three-tier guard that keeps an implementing agent inside the lines.
*Always do* applies without asking; *Ask first* requires human sign-off
before proceeding; *Never do* is a hard rule, even under time pressure.

### Always do

- Accept every `EvidenceTargetType` as a typed query input; return embedded `Provenance`/`EvidenceRef` for the v1-supported targets (entity, relationship, source) and `CoreError::CapabilityUnsupported` for the rest (memory, belief, document, chunk, concept, event, url) until their scope-safe listing is wired.
- Filter the time window on `Provenance.observed_at` for all targets (`valid_from` is read-only metadata on the types that carry it, not a v1 filter field).
- Read provenance/evidence from the records' existing `Provenance`/`EvidenceRef` fields — no new storage.
- Keep the `ProvenanceQuery` port in `core/integration` (engine-neutral); put the SQLite impl in `adapters/integration`.
- Flip `episodes_evidence` to `Supported` only when the conformance fixture passes; attach the handle only then.

### Ask first

- Add a dedicated write op to attach evidence to an *existing* record (deferred — documented, not built in S2).
- Wire the v1-unsupported targets (memory, belief, document, chunk) into the query — each needs a scope-safe listing path.
- Promote `ProvenanceQuery` out of `core/integration` into its own behavior crate.
- Add indexed columns or a materialized evidence table (schema change) to accelerate queries beyond Rust-side filtering.

### Never do

- Name an engine type (`Sql*`, `pgvector`, …) or hold SQL in the `ProvenanceQuery` port or `core/integration`. *(structural, ADR-0022)*
- Duplicate provenance/evidence storage — read from existing `record_json`; do not create a parallel evidence store. *(structural)*
- Mutate records in a query path — S2 is read-only.
- Build a new backend or storage engine.

## Testing Strategy

- **ProvenanceQuery port + SQLite impl — TDD.** Given entity/relationship/source records carrying `Provenance`/`EvidenceRef`, `provenance_for`/`evidence_for` recover them; `provenance_by_source` filters by source; a disjoint `observed_at` window returns empty. An unsupported target type (e.g. memory, belief) returns `CoreError::CapabilityUnsupported`. Scope isolation: a query in tenant A does not return tenant B's evidence. The invariant is compressible: "the query returns exactly the evidence embedded in the matching records."
- **Provider handle + capability flip — TDD.** A provider bootstrapped with the SQLite backend exposes the `provenance()` handle and reports `episodes_evidence` `Supported`; an unwired provider reports it `Unsupported` with no handle.
- **Conformance fixture — goal-based check.** A fixture writes records carrying `Provenance`/`EvidenceRef` and queries them through the handle; the capability flips to `Supported` only when the fixture passes.
- **Engine neutrality — goal-based check.** `core/integration/src/{provider,capability,provenance}.rs` stay green under `.codex/hooks/check-engine-neutrality.sh`.
- **No regression — goal-based check.** Existing workspace tests stay green; no schema change.

## Acceptance Criteria

- [x] The `ProvenanceQuery` port in `core/integration/src/provenance.rs` accepts any `EvidenceTargetType` as a typed query input. v1 returns the embedded `Provenance`/`EvidenceRef` for **entity, relationship, source** (by id, scope, `stable_source_key`, and `observed_at` time window — `stable_source_key` is the source-grouping key, not the `KnowledgeSource.id`); it returns `CoreError::CapabilityUnsupported` for memory, belief, document, chunk, concept, event, and url targets until their scope-safe listing is wired. (`EvidenceTargetType` gains `Relationship` and `Belief` variants as an additive domain extension so the typed input can name them.)
- [x] A SQLite `ProvenanceQuery` implementation reads from existing `record_json` (+ scope columns on `knowledge_entities`/`knowledge_relationships`/`knowledge_sources`) via Rust-side filtering; no schema migration, no new table.
- [x] `EngramProvider` exposes a `provenance()` handle; the `episodes_evidence` capability flips to `Supported` (from S1's `Unsupported { FeatureDisabled }`) only when the conformance fixture passes (a fixture failure reports `ConformanceFailed`, like the other implemented families).
- [x] A conformance fixture writes entity/relationship/source records carrying `Provenance`/`EvidenceRef` and recovers them through the handle; the capability is `Supported` only on pass.
- [x] `.codex/hooks/check-engine-neutrality.sh` covers `core/integration/src/provenance.rs` (added to `GATED_PATHS`); the port layer is engine-symbol-free.
- [ ] The deferred write-side — a dedicated *attach-evidence-to-existing-record* operation — is documented here and in `docs/backlog.md`, not built. (deferred: `episode-evidence-write-side`)
- [x] SQLite behavior for existing operations is unchanged; existing workspace tests green. Each task's Done-when re-asserts this regression bar.

## Assumptions

- Technical: `Provenance` + `EvidenceRef` + `DerivationRef` domain types exist and are rich (source: `core/domain/src/provenance.rs`). `EvidenceTargetType` today has Memory/Event/Source/Document/Chunk/Entity/Concept/Url — it lacks `Relationship` and `Belief`, which S2 adds as an additive extension so the query port can name those record kinds as typed inputs.
- Technical: `Provenance` is embedded in stored records — `KnowledgeEntity`/`KnowledgeRelationship` (the latter also has `evidence: Vec<EvidenceRef>`), `MemoryRecord` (`MemoryContent.provenance` is `Option<Provenance>`), `Belief` (source: `core/domain/src/{knowledge,memory,belief}.rs`).
- Technical: scope columns (tenant/subject/workspace/session/environment) exist on `knowledge_sources`, `knowledge_entities`, `knowledge_relationships`, `knowledge_graphs` — NOT on `knowledge_documents`/`knowledge_chunks` (they inherit scope via parent), so document/chunk targets are deferred from v1 (source: `adapters/knowledge/sqlite/src/schema.rs`).
- Technical: the knowledge SQLite adapter stores each record losslessly as `record_json TEXT`, so provenance/evidence query = Rust-side filter over the scope-column listings with no schema change (source: `adapters/knowledge/sqlite/src/schema.rs`).
- Technical: existing scope-filtered listings exist for entities/relationships/graphs by source (`list_entities_by_source` / `list_relationships_by_source` / `list_graphs_by_source`); `list_memories` has no scope parameter and there is no queryable belief listing wired into the query path (source: `adapters/knowledge/sqlite/src/service.rs`, `adapters/memory/sqlite/src/service.rs`).
- Technical: `EngramProvider` has the `episodes_evidence` capability key (default `Unsupported { FeatureDisabled }`) but no handle (source: `core/integration/src/provider.rs`).
- Product: S2 is **read-only** — the query surface + provider handle + capability flip; the dedicated write-side (attach evidence to an existing record) is deferred. v1 query targets are entity/relationship/source; memory/belief/document/chunk return `CapabilityUnsupported` until wired. (source: user confirmation 2026-07-10)
- Design: the `ProvenanceQuery` port lives in `core/integration` as a facade-level trait; the SQLite impl lives in `adapters/integration`. (source: user confirmation 2026-07-10; my recommendation accepted)
- Process: SQLite is the only backend; the port must stay engine-neutral (ADR-0022; CONVENTIONS boundary rules).
