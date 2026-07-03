# Plan: AgentZero Engram Adapter Integration

- **Spec:** [`spec.md`](spec.md)
- **Status:** Executing

## Approach

Build the integration as an AgentZero-side compatibility provider. The first
implementation captures current AgentZero behavior as fixtures, then adds a
focused `zbot-engram-adapter` crate/package that implements the existing store
and provider traits over Engram. The gateway composition root selects the
provider behind a feature/config flag; routes, UI transports, sleep workers, and
settings continue to speak AgentZero contracts. Engram core changes stay out of
scope unless a fixture proves the adapter cannot preserve a required behavior.

## Constraints

- Engram's `docs/architecture/reference.md` is the normative architecture:
  domain contracts first, Rust deterministic behavior, infrastructure behind
  adapters, narrow package entry points, no god modules.
- AgentZero remains the host: it owns timers, manual triggers, settings storage,
  model/provider selection, gateway routes, UI DTOs, and observability panels.
- The belief and valid-time slice is governed by
  `docs/specs/zbot-engram-belief-bitemporal-cutover/spec.md`.
- Direct AgentZero SQLite table compatibility is not the target contract; typed
  AgentZero traits and HTTP/UI snapshots are.
- Destructive migration, live dual-write, and delete/forget propagation need a
  separate rollout/migration spec.

## Construction tests

**Integration tests:** a provider parity harness runs AgentZero trait and route
fixtures against the current provider and the Engram adapter provider, normalizes
allowed timestamp/ID translation differences, and compares JSON snapshots.

**Manual verification:** memory tab, ward content, unified recall, belief detail
and contradiction resolver, belief-network observatory, hierarchy observatory,
settings read/write, and manual consolidation trigger are exercised with the
Engram provider selected.

## Design (LLD)

### Design decisions

- The adapter lives in AgentZero, not Engram, because the compatibility contract
  is AgentZero's store/API/UI shape.
- Provider selection happens at one gateway composition root so route handlers
  and UI code do not branch on Engram.
- Scope mapping is explicit configuration, with a default mapping from AgentZero
  `ward_id` / `partition_id` to Engram `Scope.workspace` and a configured tenant.
- Capabilities are reported at startup. Unsupported optional stores disable only
  the dependent jobs or UI panels rather than failing unrelated memory features.
- Migration begins with dry-run import and diagnostics. Apply mode is a later
  explicit action, not a side effect of provider construction.

### Interfaces & contracts

AgentZero surfaces consumed by the adapter:

- `MemoryFactStore`, `WikiStore`, `KgEpisodeStore`, `EpisodeStore`,
  `ProcedureStore`, `CompactionStore`, `OutboxStore`, `GoalStore`,
  `RecallLogStore`, `DistillationStore`, `BeliefStore`, and
  `BeliefContradictionStore`.
- `KnowledgeGraphStore` for entity/relationship graph behavior.
- `MemorySettings`, `RecallConfig`, `BeliefNetworkConfig`, `HierarchySettings`,
  `MmrConfig`, `QueryGateConfig`, and sleep worker `SleepOps` wiring.
- HTTP route projections for memory, wards, beliefs, contradictions,
  belief-network activity/stats, hierarchy stats, and execution settings.

Engram surfaces consumed behind the adapter:

- `engram-domain` memory, knowledge, ontology, belief, hierarchy, retrieval,
  policy, scope, provenance, and metadata types.
- Rust behavior/repository ports where they satisfy AgentZero semantics.
- Adapter-local sidecars only where exact AgentZero compatibility has no Engram
  equivalent yet, such as raw belief embedding byte preservation.

### Component / module decomposition

Proposed `zbot-engram-adapter` structure:

- `config.rs`: provider mode, Engram paths/handles, tenant, scope mapping,
  feature flags, embedding mode, migration mode.
- `capabilities.rs`: startup capability report and dependent-job disablement.
- `scope.rs`: ward/global/session/partition to Engram `Scope` mapping.
- `errors.rs`: typed adapter errors and AgentZero trait string conversion.
- `mapping/memory.rs`: memory fact, lifecycle, provenance, epistemic class,
  source episode/ref, valid interval, and pinned metadata mapping.
- `mapping/knowledge.rs`: wiki/source/chunk, KG entity/relationship, ontology
  class/property, and procedure mapping.
- `mapping/belief.rs`: delegates to the belief/bitemporal specialist mapping.
- `stores/*.rs`: focused AgentZero trait implementations.
- `recall.rs`: AgentZero recall result projection and ranking policy mapping.
- `maintenance.rs`: operation adapters called by AgentZero sleep workers, not a
  scheduler.
- `observability.rs`: stats/activity read models for existing UI panels.
- `migration.rs`: dry-run/apply importer with deterministic diagnostics.
- `fixtures.rs`: current-provider and Engram-provider parity harness.

### Behavior & rules

- Provider construction validates capabilities and returns a report before any
  background worker starts.
- Store trait implementations return AgentZero domain types and errors at the
  trait boundary; Engram DTOs do not leak into route handlers or UI transports.
- AgentZero settings convert to operation policy values only when an operation
  runs. Settings persistence remains in AgentZero.
- Sleep workers retain their current intervals, order, partial-failure handling,
  and observability updates.
- Recall preserves AgentZero result kinds and fusion/reranking behavior unless a
  deliberate ranking migration fixture is accepted.
- Ward/global/session scope is never inferred from string defaults in Engram; it
  is translated by `scope.rs` and tested.
- Unsupported capability paths fail closed and are visible in health/status
  output.

### Failure, edge cases & resilience

- A missing optional capability disables the dependent job/panel and records the
  capability in health output; it does not disable unrelated memory features.
- Scope translation failures are hard errors, not fallback-to-global behavior.
- Malformed timestamps, embeddings, JSON payloads, ontology labels, or
  relationship types fail loud during migration and tests.
- Adapter write failures preserve AgentZero's existing partial-failure behavior
  in sleep workers.
- Provider rollback is config-only until an accepted migration spec declares
  irreversible writes.

### Dependencies & integration

- AgentZero depends on Engram crates through the adapter crate only.
- Existing AgentZero route handlers depend on trait objects, not Engram types.
- Existing AgentZero UI code continues through current transport APIs.
- Engram does not depend on AgentZero crates, route modules, settings files, or
  UI packages.

## Tasks

### T1: Provider parity fixture corpus exists

**Depends on:** none

**Tests:**
- Goal-based: fixture generation captures current-provider JSON snapshots for
  memory facts, wards, wiki, graph, procedures, episodes, beliefs,
  contradictions, hierarchy stats, recall, settings, and health output.
- Goal-based: snapshots include local/session/global scope cases and unsupported
  optional capability cases.

**Approach:**
- Add deterministic AgentZero fixture seed data and route/trait call scripts.
- Normalize timestamps and nondeterministic IDs only where the contract permits.
- Reuse the belief/bitemporal fixture cases from the specialist spec.

**Done when:** changing current AgentZero behavior changes at least one parity
snapshot.

### T2: Adapter crate boundary and configuration exist

**Depends on:** T1

**Tests:**
- TDD: config parsing validates provider mode, Engram path/handle, tenant,
  scope mapping, embedding mode, and migration mode.
- Goal-based: gateway startup reports adapter capabilities before workers run.

**Approach:**
- Add the `zbot-engram-adapter` crate/package in AgentZero.
- Implement `config.rs`, `capabilities.rs`, `scope.rs`, and `errors.rs`.
- Wire provider selection only at the gateway composition root.

**Done when:** AgentZero can start with current provider or Engram adapter
selected and route handlers remain unchanged.

### T3: Memory fact and ward jobs match current behavior

**Depends on:** T2

**Tests:**
- Trait parity tests cover list/search/create/delete facts, valid intervals,
  supersession, decay inputs, provenance, `ward_id`, `__global__`, session-local
  facts, pinned facts, epistemic class, source episode ID, and source ref.
- Route parity tests cover `/api/memory*` and `/api/wards*`.

**Approach:**
- Implement `mapping/memory.rs` and memory fact store methods required by
  gateway routes, distillation, consolidation, decay, and belief propagation.
- Keep unsupported broad memory methods capability-gated until implemented.

**Done when:** memory tab and ward content snapshots match for both providers.

### T4: Knowledge, wiki, graph, procedure, and episode jobs match or report unsupported

**Depends on:** T2

**Tests:**
- Trait parity tests cover wiki article rows, KG episodes, entities,
  relationships, procedures, compaction state, outbox records, recall logs, and
  distillation records where implemented.
- Startup capability tests prove unsupported optional stores disable only their
  dependent jobs/panels.

**Approach:**
- Implement `mapping/knowledge.rs` and focused store adapters in priority order:
  wiki/source/chunks, graph entities/relationships, episodes, procedures,
  compaction/outbox/auxiliary records.
- Use Engram ontology validation for graph type mapping diagnostics without
  enforcing new type rules during compatibility mode.

**Done when:** implemented stores match snapshots and unsupported stores are
explicitly reported without route panics.

### T5: Belief, contradiction, and valid-time jobs satisfy specialist spec

**Depends on:** T2

**Tests:**
- All construction tests from
  `docs/specs/zbot-engram-belief-bitemporal-cutover/plan.md` pass.
- Route parity tests cover `/api/beliefs*`, `/api/contradictions*`, and
  `/api/belief-network/*`.

**Approach:**
- Implement or import the specialist belief/bitemporal adapter modules.
- Keep AgentZero belief synthesis, contradiction detection, and propagation
  scheduling in existing sleep workers.

**Done when:** the specialist spec's acceptance criteria pass under this
provider integration.

### T6: Recall and hierarchy jobs preserve AgentZero behavior

**Depends on:** T3, T4, T5

**Tests:**
- Route parity tests cover unified `/api/memory/search` result kinds, scores,
  provenance fields, and ordering.
- Goal-based: recall fixtures cover RRF, MMR, category weights, ward affinity,
  graph traversal, temporal decay, contradiction penalty, predictive recall, and
  score thresholds.
- Route parity tests cover `/api/hierarchy/stats`.

**Approach:**
- Implement `recall.rs` and hierarchy read model mapping.
- Keep hierarchy build scheduling in AgentZero and call Engram behavior only as
  an operation dependency.

**Done when:** search and hierarchy snapshots match or any ranking delta is
  documented as an accepted migration.

### T7: Migration dry-run and diagnostics are deterministic

**Depends on:** T3, T4, T5, T6

**Tests:**
- Dry-run importer reports deterministic counts and unsupported mappings for all
  major AgentZero memory surfaces.
- Dry-run does not write to Engram storage.
- Apply mode is disabled unless explicitly configured.

**Approach:**
- Implement `migration.rs` with read-only default behavior.
- Record diagnostics for ID translation, scope translation, ontology mapping,
  embedding handling, and unsupported rows.

**Done when:** a user can run a dry-run and know exactly what would migrate,
  what would be skipped, and why.

### T8: Provider switch passes API, UI, and worker smoke checks

**Depends on:** T3, T4, T5, T6, T7

**Tests:**
- Gateway route suite passes under both providers.
- UI typecheck and relevant memory/observatory tests pass without transport type
  edits.
- Manual QA exercises memory tab, belief panel, hierarchy observatory, settings,
  manual consolidation trigger, and at least one sleep-cycle run.

**Approach:**
- Add the final provider flag wiring and health/status display.
- Run current-provider and Engram-provider smoke checks back to back.

**Done when:** AgentZero can run its memory jobs through the Engram adapter with
  a config-only rollback path.

### T9: Adversarial integration review is clean

**Depends on:** T8

**Tests:**
- Review checks no god adapter class, hidden scheduler migration, route/DTO
  drift, scope leakage, unsupported silent fallback, fake bitemporality, or
  ranking drift.

**Approach:**
- Run adversarial review against the final diff and this spec.
- Fix blockers in scope; defer only explicit non-blocking follow-ups to the
  durable backlog.

**Done when:** the provider integration is review-clean and ready for a guarded
  AgentZero rollout.

## Rollout

- **Delivery:** ship behind an AgentZero provider flag. Default stays current
  provider until parity fixtures, route tests, and manual QA pass.
- **Rollback:** switch the provider flag back. Before irreversible migration,
  dual-write or export/import recovery needs a separate accepted spec.
- **Canary:** run one AgentZero profile with Engram provider selected, dry-run
  migration diagnostics reviewed, then API/UI/sleep smoke completed.
- **Irreversible steps:** data deletion, forget propagation, live dual-write, or
  replacing current storage defaults are out of this spec.

## Risks

- AgentZero behavior may depend on SQLite-specific quirks that are absent from
  trait comments; parity snapshots are the guard.
- Scope mapping is the largest leakage risk because AgentZero has agent, ward,
  session, and global concepts while Engram has tenant/workspace/session/
  environment.
- Exact recall parity may require preserving raw embedding bytes or accepting a
  ranking migration.
- Optional stores can be forgotten if capability reporting is too coarse.
- An adapter composition root can become a god class unless module boundaries
  are enforced in review.

## Changelog

- 2026-07-02: initial integration spec from AgentZero code scan, Engram
  reference architecture, and adapter-first research.
- 2026-07-02: implementation started with an AgentZero-side
  `stores/zbot-engram-adapter` workspace crate for config, capability reporting,
  scope mapping, and typed adapter errors; provider cutover remains gated on
  parity fixtures.
