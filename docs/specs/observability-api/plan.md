# Plan: Observability API (S6)

- **Spec:** [`spec.md`](spec.md)
- **Status:** Done

## Approach

S6 exposes existing diagnostics + record counts behind one `Observability` provider handle. Four moves:

1. **Port + DTO.** `Observability` + `DiagnosticsSnapshot` in `core/integration/src/observability.rs`. The snapshot carries `CapabilityReport`, `RecordCounts`, `EmbeddingProviderConfig`, versions, slow-query (`None` v1).
2. **SQLite impl.** `SqlObservability` in `adapters/integration/src/observability.rs` — aggregates the provider's existing fields + counts records by listing the wired concrete stores.
3. **Provider + capability.** `EngramProvider` gains an `observability: Option<Arc<dyn Observability>>` handle; `bootstrap_provider` constructs `SqlObservability`, runs the fixture, flips `observability` to `Supported` on pass.
4. **Conformance fixture + gate.** Snapshot from a seeded scope → all fields populated.

## Constraints
- ADR-0022: port in `core/integration/src/observability.rs` (engine-neutral); impl in `adapters/integration`.
- Delegate to the existing `CapabilityReport` + config fields — don't recompute.
- Record counts via listing (no new SQL, no schema change).
- Slow-query/retrieval diagnostics deferred v1.

## Tasks

### T1: Observability port + DTO + gate
**Depends on:** none · **Mode:** goal-based
Port + `DiagnosticsSnapshot` compile, re-exported; add `$ROOT/core/integration/src/observability.rs` to gate `GATED_PATHS`.

### T2: SqlObservability impl
**Depends on:** T1 · **Mode:** TDD
Tests: snapshot from a seeded scope → correct record counts (knowledge + memory); capability report + embedding config + versions present; slow-query None. Impl: aggregate + list+count.

### T3: Provider handle + capability flip + conformance fixture
**Depends on:** T2 · **Mode:** TDD
Handle + flip gated on fixture (seeded scope → snapshot → all fields). Self-sufficient.

### T4: Gate verify
**Depends on:** T3 · **Mode:** goal-based
Gate green across `core/integration/src/{...,observability}.rs`.

## Changelog
- 2026-07-10: initial plan (S6 of engram-host-sdk brief; aggregates existing diagnostics + record counts; slow-query deferred v1).
