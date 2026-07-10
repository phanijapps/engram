# Spec: Observability API (S6)

- **Status:** Shipped
- **Owner:** phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** ADR-0022 (engine neutrality), [`rust-crate-integration`](../rust-crate-integration/spec.md), [`provider-sdk-capability-report`](../provider-sdk-capability-report/spec.md) (S1 — owns the `observability` capability key S6 flips)
- **Brief:** [`docs/product/briefs/engram-host-sdk.md`](../../product/briefs/engram-host-sdk.md) (slice S6, capability #14)
- **Contract:** none — a Rust port trait (`Observability`) + provider handle.
- **Shape:** service

> **Spec contract:** this document defines what "done" means.

## Objective

A host queries the operational state of an Engram provider through one backend-neutral `Observability` handle that returns a `DiagnosticsSnapshot` — the `CapabilityReport` (backend type/status, migration status, per-capability states), record counts by semantic type (memory, knowledge entities/relationships/sources/documents/chunks, beliefs), the embedding configuration, schema/adapter versions, and — where supported — index status. Slow-query and retrieval diagnostics are reported as unsupported in v1 (honest degraded mode, not absent). The `observability` capability, reported `Unsupported { FeatureDisabled }` today, becomes `Supported`.

## Boundaries

### Always do
- Aggregate diagnostics the provider already holds (`CapabilityReport`, `schema_version`, `adapter_version`, `EmbeddingProviderConfig`) — do not recompute them.
- Count records by listing the wired concrete stores (`list_entities`, `list_sources`, `list_memories`, etc.) — no new SQL, no schema change.
- Report slow-query/retrieval diagnostics as unsupported in v1 (not silently absent).
- Keep the `Observability` port in `core/integration` (engine-neutral); put the SQLite impl in `adapters/integration`.
- Flip `observability` to `Supported` only when the conformance fixture passes.

### Ask first
- Add real slow-query timing/diagnostics (requires instrumentation).
- Add index-health probes (requires adapter-level introspection).

### Never do
- Name an engine type or hold SQL in the `Observability` port. *(structural, ADR-0022)*
- Reimplement capability reporting — delegate to the existing `CapabilityReport`.
- Block on a store that is unavailable — report counts for available stores, degrade for others.

## Testing Strategy
- **Port — goal-based.** Compiles, re-exported, gate green.
- **Impl — TDD.** A snapshot from a seeded provider has correct record counts for knowledge + memory; capability report + embedding config + versions present; slow-query field is None/unsupported.
- **Handle + flip — TDD.** `EngramProvider.observability()` handle; capability flips Supported on fixture pass.
- **Conformance fixture — goal-based.** Snapshot from a seeded scope has all fields populated.
- **Engine neutrality — goal-based.** Gate covers `observability.rs`.
- **No regression — goal-based.**

## Acceptance Criteria
- [x] The `Observability` port in `core/integration/src/observability.rs` exposes `async fn diagnostics(&self) -> CoreResult<DiagnosticsSnapshot>` where `DiagnosticsSnapshot` carries: the `CapabilityReport`, record counts by semantic type (memories, entities, relationships, sources, documents, chunks, beliefs), the `EmbeddingProviderConfig`, schema/adapter versions, and slow-query/retrieval diagnostics (`None` in v1).
- [x] A SQLite `Observability` implementation aggregates from the existing provider fields + counts records by listing the wired concrete stores; unavailable stores degrade (count reported as 0 or absent, not an error).
- [x] `EngramProvider` exposes an `observability()` handle; the `observability` capability flips to `Supported` only when the conformance fixture passes.
- [x] A conformance fixture seeds a scope + queries a snapshot → all fields populated with correct counts.
- [x] `.codex/hooks/check-engine-neutrality.sh` covers `core/integration/src/observability.rs`; the port is engine-symbol-free.
- [x] SQLite behavior for existing operations is unchanged; existing workspace tests green.

## Assumptions
- Technical: `CapabilityReport` (18 keys, S1), `schema_version`/`adapter_version`, `EmbeddingProviderConfig` already exist on `EngramProvider` (source: `core/integration/src/provider.rs`).
- Technical: no record-count methods exist on the stores; counts are derived by listing (`list_entities`/`list_sources`/`list_memories`/etc.) (source: grep).
- Technical: `observability` capability key is `Unsupported { FeatureDisabled }` (S1) with no handle.
- Design: `Observability` port aggregates existing diagnostics + counts records by listing; slow-query/retrieval diagnostics deferred v1; port in `core/integration`, impl in `adapters/integration`. (source: user confirmation 2026-07-10)
- Process: SQLite only; engine-neutral port (ADR-0022); additive only.
