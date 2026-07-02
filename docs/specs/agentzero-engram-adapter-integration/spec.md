# Spec: AgentZero Engram Adapter Integration

- **Status:** Draft
- **Owner:** phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** ADR-0003, ADR-0004, ADR-0007, `docs/architecture/reference.md`, `docs/research/agentzero-engram-memory-integration-comparison-matrix.md`, `docs/specs/zbot-engram-belief-bitemporal-cutover/spec.md`
- **Brief:** none
- **Contract:** AgentZero store traits, gateway HTTP wire shapes, UI transport wire shapes; no accepted Engram v1 contract change
- **Shape:** integration

> **Spec contract:** this document defines what "done" means. The implementing
> PR must match this spec, or update it. Verification must be derivable from it.

## Objective

AgentZero can build and select a `zbot-engram-adapter` provider that lets its
existing memory, knowledge, wiki, belief, hierarchy, recall, observability, and
settings jobs run against Engram-backed storage and operations. AgentZero's
gateway routes, UI transport types, store traits, sleep-cycle scheduler, and
operator settings remain the host contract; Engram remains a contract-first Rust
library behind the adapter.

## Boundaries

### Always do

- Implement AgentZero-facing store/provider traits as the adapter boundary.
- Keep AgentZero's HTTP routes, UI wire types, settings persistence, and sleep
  worker ownership stable during cutover.
- Preserve memory fact, wiki, graph, belief, contradiction, hierarchy, procedure,
  episode, ward/global/session scope, and recall result semantics at the
  AgentZero API boundary.
- Translate AgentZero settings into explicit Engram policy/config structs at the
  call boundary; do not make Engram read AgentZero settings files.
- Use fixture-first parity tests before switching any production provider flag.
- Keep adapter modules focused by reason to change: configuration, mapping,
  scope, temporal behavior, stores, recall, hierarchy, observability, errors,
  migration, and fixtures.
- Treat `docs/specs/zbot-engram-belief-bitemporal-cutover/spec.md` as the
  specialist contract for belief, contradiction, and valid-time behavior.

### Ask first

- Change AgentZero public HTTP routes, request/response DTOs, TypeScript
  transport types, settings JSON, or UI behavior.
- Add or change accepted Engram domain fields, schemas, generated contracts, or
  public repository ports.
- Move scheduling, worker intervals, manual triggers, provider/model selection,
  or UI feature enablement from AgentZero into Engram.
- Replace AgentZero's current recall ranking contract, RRF/MMR behavior,
  category weights, or embedding byte semantics.
- Add live bidirectional sync, dual-write, destructive migration, or automatic
  delete/forget propagation.

### Never do

- Import AgentZero product vocabulary such as wards, gateway routes, or UI DTOs
  into Engram domain truth.
- Build a god adapter struct that owns construction, mapping, scheduling,
  persistence, retrieval scoring, hierarchy synthesis, observability, and error
  translation.
- Collapse memory, knowledge, wiki, graph, hierarchy, belief, contradiction,
  policy, provenance, and evaluation concepts into one store or table shape.
- Treat AgentZero's SQLite schema as Engram's public contract.
- Bypass scope/tenant/ward/session/global checks during parity fixtures or
  migration.
- Claim full bitemporality unless both valid-time and record-time query
  semantics are specified and tested.

## Testing Strategy

- **TDD:** mapping modules, scope translation, temporal filtering, error
  translation, and embedding byte conversion use focused unit tests.
- **Goal-based integration:** parity fixtures run the same AgentZero trait calls
  against current stores and the Engram adapter, then compare normalized JSON
  snapshots for all covered jobs.
- **Goal-based API:** AgentZero gateway route tests pass with the current SQLite
  provider and with the Engram adapter selected.
- **Manual QA:** the memory tab, belief network panel, hierarchy observatory,
  settings surfaces, manual consolidation trigger, and recall traces behave the
  same under the adapter provider.
- **Migration rehearsal:** a dry-run importer reports row counts, unsupported
  mappings, scope translations, ID translations, and validation errors without
  writing until explicitly run in apply mode.

## Acceptance Criteria

- [ ] `zbot-engram-adapter` has an AgentZero-side crate/package boundary and is
  selectable from the AgentZero gateway composition root without changing
  existing route handlers or UI transport types.
- [ ] Adapter configuration covers Engram provider location, tenant, scope
  mapping, embedding mode, migration mode, read/write mode, and feature flags
  without making Engram read AgentZero settings files.
- [ ] `MemoryFactStore` behavior used by list, search, create, delete,
  consolidation, decay, supersession, provenance, ward/global/session scope, and
  belief propagation matches current AgentZero snapshots.
- [ ] Knowledge graph, wiki, episodes, procedures, compaction, outbox, recall
  logs, and auxiliary store behavior either match current snapshots through the
  adapter or are explicitly marked unsupported by a startup capability report
  that disables only the dependent UI/job.
- [ ] Belief, contradiction, and bitemporal behavior satisfies
  `docs/specs/zbot-engram-belief-bitemporal-cutover/spec.md`.
- [ ] Unified recall preserves AgentZero's result kinds, category weights,
  vector/BM25/RRF/MMR behavior, graph traversal knobs, ward affinity, temporal
  decay, contradiction penalties, predictive recall, and score thresholds, or
  documents an accepted ranking migration with before/after fixtures.
- [ ] AgentZero sleep-cycle jobs continue to own scheduling and call the adapter
  through store/provider traits for compaction, synthesis, conflict resolution,
  decay, pruning, belief synthesis/detection/propagation, hierarchy build, and
  observability updates.
- [ ] Existing `/api/memory*`, `/api/wards*`, `/api/beliefs*`,
  `/api/contradictions*`, `/api/belief-network/*`, `/api/hierarchy/stats`, and
  `/api/settings/execution` UI journeys pass against both providers.
- [ ] Migration dry-run produces deterministic counts and mapping diagnostics
  for memory facts, wiki articles, graph entities/relationships, beliefs,
  contradictions, hierarchy aggregates, procedures, episodes, and settings.
- [ ] Provider rollout is reversible by configuration until a separately
  accepted dual-write or migration spec declares an irreversible step.
- [ ] Adapter code remains split into focused modules; no public adapter type
  owns more than one of mapping, store behavior, scheduling integration, recall
  scoring, hierarchy synthesis, migration, or API translation.

## Assumptions

- Technical: AgentZero store traits exist for memory facts, wiki, episodes,
  compaction, procedures, outbox, belief, contradiction, and auxiliary stores
  (source: `/home/videogamer/projects/agentzero/stores/zbot-stores-traits/src/*`).
- Technical: AgentZero's gateway route table exposes the API surfaces the UI
  already calls for memory, wards, beliefs, contradictions, hierarchy,
  observability, and settings (source:
  `/home/videogamer/projects/agentzero/gateway/src/http/mod.rs`).
- Technical: AgentZero memory settings own recall, MMR, hierarchy, query gate,
  belief network, and worker interval knobs (source:
  `/home/videogamer/projects/agentzero/gateway/gateway-memory/src/lib.rs`).
- Technical: Engram's reference architecture requires contract-first domain
  boundaries, Rust deterministic core behavior, replaceable adapters, and no
  god modules (source: `docs/architecture/reference.md`).
- Product: AgentZero continuity is the compatibility target; Engram-native DTOs
  remain hidden behind the adapter during cutover (source: user direction
  2026-07-02).
- Process: Engram core/API changes require a separate accepted spec or ADR; this
  spec starts with AgentZero-side adapter integration (source: repository
  boundary rules and AGENTS.md).
