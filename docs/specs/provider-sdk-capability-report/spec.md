# Spec: Provider SDK + capability report (S1)

- **Status:** Shipped
- **Owner:** phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** ADR-0022 (engine grid vs backend recipe; Proposed, PR #12), [`rust-crate-integration`](../rust-crate-integration/spec.md) (the **Implementing** spec that owns the provider facade + `CapabilityReport` + `CoreError` extension + conformance harness — S1 *extends* it against the brief + ADR-0022; it does **not** re-establish the facade), ADR-0007 (N-API binding surface — reference only; not modified here)
- **Brief:** [`docs/product/briefs/engram-host-sdk.md`](../../product/briefs/engram-host-sdk.md) (slice S1)
- **Contract:** none — this completes an internal Rust model/trait surface (`CapabilityReport`, `CoreError`); it does not define a new `contracts/<type>` interface.
- **Shape:** data

> **Spec contract:** this document defines what "done" means. The implementing
> PR must match this spec, or update it. Verification must be derivable from it.

## Objective

The provider facade, `CapabilityReport`, `CoreError` extension, and conformance
harness already exist and are in flight under the `rust-crate-integration` spec.
This slice (S1) **completes that contract against the host-SDK brief and
ADR-0022**, in four narrow ways, without re-establishing the facade:

1. **`CapabilityReport` 10 → 18 keys.** The report today covers 10 families; the
   brief's capability sections map to 18 keys. S1 adds the 8 missing keys
   (`hybrid_search`, `episodes_evidence`, `contradiction`, `atomic_batch`,
   `unified_recall`, `export_import`, `maintenance`, `observability`) and has the
   unwired provider mark every not-yet-built area `Unsupported { FeatureDisabled }`
   — present, never silently absent. (`export_import` is distinct from the existing
   `migration` key: schema-version migration vs semantic-state export/import +
   backend-to-backend movement.)
2. **`CoreError` → the brief's 10 categories.** `rust-crate-integration` already
   added `CapabilityUnsupported`, `EmbeddingSpaceMismatch`,
   `MigrationManifestStale`; five brief categories still lack a dedicated
   variant (migration failed, validation failed, transaction unsupported,
   transient backend, permanent backend). S1 adds them as structured variants.
3. **ADR-0022 rule-1 neutrality gate.** New (the owning spec predates ADR-0022).
   A gate fails if an engine symbol or SQL appears in the port-trait crates or
   `core/integration`'s provider/capability modules.
4. **SDK documentation.** Rustdoc `EngramProvider` as the canonical entry and
   add a host-usage guide.

A host opening `EngramProvider` reads a report that accounts for all 18
`CapabilityReport` keys; unsupported operations fail with typed errors across
all 10 categories; and the neutrality gate keeps the contracts the host
programs against engine-free.

## Boundaries

The three-tier guard that keeps an implementing agent inside the lines.
*Always do* applies without asking; *Ask first* requires human sign-off
before proceeding; *Never do* is a hard rule, even under time pressure.

### Always do

- Mark every capability area the brief names with an explicit `CapabilityState` — `Supported` or a typed non-`Supported` state. Never omit a capability silently.
- Keep the neutrality gate green on the gated surface (the 8 clean port-trait crates + `core/integration/src/{provider,capability}.rs`).
- Add new `CoreError` variants as structured errors (typed fields), each mapped to one of the brief's 10 error categories.
- Document `EngramProvider` as the canonical Rust SDK entry via rustdoc plus a host-usage guide under `docs/guides/how-to/`.

### Ask first

- Move `SqliteStorageLayout` out of `EngramConfig`, retire `engram-runtime`'s home-grown `Sql*` types, touch `bindings/node`'s direct `Sql*` construction, or `core/eval`'s fixture runner — these are **deferred neutrality debt** (logged in `docs/backlog.md`); coordinate before changing.
- Introduce a new `CapabilityState` variant or `CapabilityReason` beyond what the brief's 18 areas require.
- Change the stable string form of an existing `CapabilityReason` (compat-breaking under the capability-report stability contract).

### Never do

- Import an engine crate (`engram-store-*`, `rusqlite`, `sqlx`, `tantivy`, `sqlite-vec`) into the port-trait crates or `core/integration`'s provider/capability modules. *(structural)*
- Construct an engine type (`Sql*`, `pgvector`, …) or hold raw SQL in those layers.
- Rename existing adapter crates or create a `backends/` crate — ADR-0022 defers that until a second engine is adopted (YAGNI). *(structural)*
- Build a new storage engine (SurrealDB / pg / pglite / lance) — out of scope for this slice.
- Silently fall back when a capability is unsupported (the brief's fail-explicit principle).

## Testing Strategy

- **Capability-report completeness — TDD.** A test asserts the report carries an explicit `CapabilityState` for each of its 18 keys (per the §plan T1 mapping table), and that a provider bootstrapped with no backend wired reports the not-yet-built areas as `Unsupported { FeatureDisabled }` (present, not absent). Logic has a compressible invariant: "every key has a state."
- **Error categories — TDD.** Tests assert each new `CoreError` variant is constructible, carries structured fields, and maps to its brief category — with an explicit assertion that no category is represented by a raw string match.
- **Neutrality gate — TDD (inject-then-revert invariant) with a goal-based baseline.** A gate fails if an engine symbol, engine-crate import, or raw-SQL literal appears in the gated surface (the 8 clean port-trait crates + `core/integration/src/{provider,capability}.rs`). It is green today and must stay green; a parametric injection test proves it fires on all three forbidden pattern classes. This is the ADR-0022 rule-1 enforcement.
- **SDK documentation — goal-based check.** `cargo doc` builds clean for `engram-integration` and the host-usage guide exists under `docs/guides/how-to/`.
- **No regression — goal-based check.** Existing workspace tests stay green; SQLite behavior is unchanged.

## Acceptance Criteria

- [x] `CapabilityReport` exposes an explicit `CapabilityState` for each of its **18 keys** — the 10 existing (memory, knowledge, graph, ontology, taxonomy, beliefs, hierarchy, retrieval, vectors, migration) plus 8 new (hybrid_search, episodes_evidence, contradiction, atomic_batch, unified_recall, export_import, maintenance, observability). The brief-area → key mapping is fixed in [`plan.md`](plan.md) §T1 Design. The brief's meta rows (#1 provider facade, #2 backend abstraction, #3 capability discovery, #15 stable data model, #16 error model, #17 conformance) are infrastructure, not capability keys, and carry no state. No key is silently absent.
- [x] A provider bootstrapped with no backend wired reports every not-yet-built capability area as `Unsupported` (or another typed non-`Supported` state) carrying a stable reason code.
- [x] `CoreError` covers the brief's 10 error categories as structured variants — unsupported capability, backend unavailable, migration required, migration failed, embedding dimension mismatch, validation failed, conflict, transaction unsupported, transient backend failure, permanent backend failure — none requiring string matching to discriminate.
- [ ] The neutrality gate passes: zero engine symbols (`Sql*` type names; `rusqlite`/`sqlx`/`tantivy`/`sqlite-vec`/`pgvector` imports; raw-SQL literals) in the gated surface — the 8 clean port-trait crates (`domain`, `memory`, `knowledge`, `retrieval`, `belief`, `hierarchy`, `consolidation`, `orchestration`) plus exactly `core/integration/src/provider.rs` and `core/integration/src/capability.rs` (file list, not `pub mod`). The gate's three pattern classes are each covered by an injection case. Full ADR-0022 rule-1 coverage is deferred: `engram-runtime` (home-grown `SqliteOpenOptions`/`SqliteJournalMode`/`SqlitePath`), `core/integration/src/config.rs` (`SqliteStorageLayout`), `core/eval`, and `bindings/node` are documented exceptions. (deferred: `provider-sdk-capability-report`)
- [x] `EngramProvider` is documented as the canonical Rust SDK entry — rustdoc on the type/`bootstrap`, plus a host-usage guide under `docs/guides/how-to/`.
- [x] SQLite behavior is unchanged: existing workspace tests green; no adapter crate renamed; no `backends/` crate created.

## Assumptions

- Technical: The provider facade, `CapabilityReport`, `CoreError` extension (`CapabilityUnsupported`, `EmbeddingSpaceMismatch`, `MigrationManifestStale`), `EmbeddingProvider`/`VectorIndex` traits, migration/import API, and conformance harness are owned by the **Implementing** `rust-crate-integration` spec. S1 extends that surface; it does not re-spec or rebuild the facade. (source: `docs/specs/rust-crate-integration/spec.md` — Status: Implementing; `core/integration/*` is its implementation.)
- Technical: Eight port-trait crates (`core/{domain,memory,knowledge,retrieval,belief,hierarchy,consolidation,orchestration}`) have zero engine dependencies and no home-grown `Sql*` types, so ADR-0022 rule 1 is enforceable there on landing. (source: `Cargo.toml` grep — 0 engine-dep lines; `grep pub (struct|enum|fn) Sql[A-Z]` over each `src/` — no hits; the single `core/knowledge/src/graph.rs` `Sql*` mention is a doc comment.)
- Technical: `engram-runtime` is the exception — it re-exports home-grown `SqliteJournalMode`/`SqliteOpenOptions`/`SqlitePath` (`core/runtime/src/options.rs`, via `core/runtime/src/lib.rs:15`) and holds a SQL redaction regex (`core/runtime/src/error.rs`). It is therefore **excluded** from the S1 gate as pre-existing ADR-0022 debt. (source: repo grep; deferral logged in `docs/backlog.md`.)
- Technical: `core/integration` `provider.rs` + `capability.rs` are already engine-neutral (import only port traits); `CapabilityReport` has 10 builder families (memory, knowledge, graph, ontology, taxonomy, beliefs, hierarchy, retrieval, vectors, migration) that this spec extends to 18. (source: `core/integration/src/{provider,capability}.rs`)
- Technical: `CapabilityState` already carries `Supported` / `Unsupported` / `Degraded` / `RequiresMigration` / `RequiresReindex` / `Misconfigured` plus 9 stable reason codes; no new variant is needed for S1. (source: `core/domain/src/capability.rs`)
- Technical: `CoreError` has 11 variants today; the brief's 10 categories require ~5 more. (source: `core/runtime/src/error.rs`)
- Technical: ADR-0022 rule 1 is currently violated in `core/integration/config.rs` (`SqliteStorageLayout`), `core/eval/tests/fixture_runner.rs` (`SqlMemoryService`), and `bindings/node` (pervasive `Sql*`) — these are deferred neutrality debt and are out of scope for the gate. (source: repo grep; scope confirmed below.)
- Product: S1's neutrality gate covers only the clean port-trait crates plus `core/integration`'s provider/capability modules; `SqliteStorageLayout`, `core/eval`, and `bindings/node` are documented deferred debt so SQLite stays untouched. (source: user confirmation 2026-07-09)
- Product: The ~8 capability areas whose implementations arrive in later slices (episodes/evidence, contradiction, atomic batch, unified recall, export/import, maintenance, hybrid search) are forward-declared as `CapabilityReport` keys marked `Unsupported { FeatureDisabled }`, not omitted. (source: user confirmation 2026-07-09)
- Product: SDK docs target rustdoc on `EngramProvider` plus a host-usage guide under `docs/guides/how-to/`. (source: user confirmation 2026-07-09)
- Process: ADR-0022 is Proposed (PR #12); S1 implements its rule-1 gate against the Proposed decision. (source: `gh pr view 12` — OPEN)
