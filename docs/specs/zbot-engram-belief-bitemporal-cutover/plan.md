# Plan: Zbot Engram Belief Bitemporal Cutover

- **Spec:** [`spec.md`](spec.md)
- **Status:** Executing

## Approach

Build the cutover as an AgentZero-side `zbot-engram-adapter` crate that
implements existing AgentZero store traits over Engram. Start with parity
fixtures, not production wiring: capture current SQLite behavior as JSON
snapshots, implement focused mapping and temporal filtering modules, then switch
the gateway store handles behind a config flag. Engram core remains unchanged
unless the fixture proves an adapter-only implementation cannot preserve the
contract.

## Constraints

- Engram domain truth remains in `docs/domain-data-model.md` and
  `core/domain`; TypeScript and adapters must not redefine it.
- AgentZero owns scheduler settings: `MemorySettings`, `BeliefNetworkConfig`,
  recall/RRF/MMR, hierarchy, decay, and sleep-cycle intervals.
- Engram's current belief SQLite adapter stores valid intervals but does not
  implement AgentZero-compatible as-of queries.
- The adapter must avoid god modules: mapping, config, store behavior, scoring,
  and error translation are separate modules.

## Construction tests

**Integration tests:** a parity fixture runs the same AgentZero trait calls
against `SqliteBeliefStore` / `SqliteBeliefContradictionStore` and the
Engram-backed adapter, normalizes timestamps where the contract permits, and
compares JSON outputs.

**Manual verification:** memory tab belief list/detail, contradiction resolver,
and observatory belief-network activity render the same states after the
provider flag switches to Engram-backed storage.

## Design (LLD)

### Design decisions

- Adapter-first. `zbot-engram-adapter` implements AgentZero traits; Engram does
  not import AgentZero routes, settings, wards, or sleep workers.
- Valid-time first. `as_of` maps to valid interval filtering. Record-time is
  stored as provenance/metadata, not exposed as a fake bitemporal API.
- Fixture-gated. No provider switch lands until current SQLite behavior and
  Engram-backed adapter behavior match for the cutover cases.
- Scope mapping is config-driven. Default maps `partition_id` to
  `Scope.workspace`, with a configured tenant, but deployments may choose
  `workspace`, `session`, or `environment`.
- Embedding parity is explicit. Either migrate raw little-endian f32 bytes into
  an adapter sidecar or accept a documented ranking-contract migration.

### Interfaces & contracts

Consumed AgentZero contracts:

- `BeliefStore`
- `BeliefContradictionStore`
- the memory fact store methods that carry valid intervals and source metadata
- existing belief, contradiction, memory, and observatory HTTP routes

Consumed Engram contracts:

- `engram_domain::Belief`, `BeliefSource`, `BeliefStatus`
- `engram_domain::Contradiction`, `ContradictionKind`, `ContradictionResolution`
- `engram_domain::Scope`, `Provenance`, `Metadata`
- `BeliefRepository` only where it is behaviorally sufficient

### Component / module decomposition

- `config.rs`: provider path, tenant, scope mapping, embedding mode, feature
  flag defaults.
- `error.rs`: typed internal errors and final trait-boundary string conversion.
- `mapping/belief.rs`: AgentZero belief to/from Engram belief.
- `mapping/contradiction.rs`: contradiction targets, canonical pairs, and
  resolution mapping.
- `mapping/fact.rs`: memory fact valid interval, source, ward/session/global,
  and epistemic-class mapping.
- `temporal.rs`: interval matching, `as_of` semantics, open interval helpers.
- `belief_store.rs`: AgentZero `BeliefStore` implementation.
- `contradiction_store.rs`: AgentZero `BeliefContradictionStore`
  implementation.
- `embedding.rs`: little-endian f32 byte conversion and scoring helpers.
- `fixtures.rs`: seeded rows and normalized snapshot comparison helpers.

### Behavior & rules

- `get_belief(partition_id, subject, as_of)` returns the latest belief whose
  valid interval contains `as_of`, using `Utc::now()` when `as_of` is absent.
- `list_beliefs` preserves AgentZero's updated-at descending order and limit.
- `upsert_belief` is idempotent by `(partition_id, subject, valid_from)`.
- `supersede_belief` closes the old belief interval and sets replacement ID.
- `retract_belief` closes the old belief interval without replacement.
- `mark_stale` and `clear_stale` mutate only stale state and update metadata.
- `beliefs_referencing_fact` returns active dependent belief IDs only.
- `search_beliefs` filters non-live beliefs and null embeddings before cosine
  scoring.
- Contradiction insert canonicalizes pairs before idempotency checks.
- Contradiction resolution updates the contradiction row only.

### Failure, edge cases & resilience

- Missing Engram records map to `Ok(None)` for reads and clear trait errors for
  mutation targets.
- Malformed timestamps, source JSON, embeddings, or enum strings fail loud in
  tests and return trait errors in production.
- Precision loss from f64/f32 confidence conversion is bounded by tests.
- Adapter write failures do not change AgentZero scheduler semantics; callers
  retain current error-handling behavior.
- If Engram cannot preserve AgentZero IDs directly, metadata stores original IDs
  and all trait/API responses translate back to original IDs.

### Dependencies & integration

- AgentZero gateway wiring adds a provider selection config that chooses current
  SQLite stores or Engram-backed adapter stores.
- Existing UI code continues to call current HTTP endpoints.
- Existing sleep workers continue to receive trait objects and settings.
- Engram crate dependencies stay inside the adapter crate, not across AgentZero
  UI or gateway route modules.

## Tasks

### T0: Engram belief behavior ports expose valid-time compatibility primitives

**Depends on:** none

**Tests:**
- TDD: interval helpers prove start-inclusive/end-exclusive matching, open
  intervals, and `as_of = now` defaults.
- TDD: repository query helpers filter by scope, subject, active/stale status,
  valid interval, source fact reference, and cosine search eligibility.
- TDD: lifecycle helpers mark stale, clear stale, retract, and supersede without
  mutating unrelated fields.
- TDD: contradiction helpers canonicalize target pairs and produce stable keys.

**Approach:**
- Add focused modules to `engram-belief` for temporal filtering, query request
  types, lifecycle transitions, embedding byte/cosine helpers, and
  contradiction canonicalization.
- Extend `BeliefRepository` with explicit read/lifecycle/search methods needed
  by AgentZero adapters while keeping AgentZero field names out of Engram.
- Implement the expanded port in `engram-store-belief-sqlite` by filtering
  stored contract JSON and returning an explicit invalid request for
  record-time history queries the adapter cannot preserve yet.

**Done when:** an AgentZero-side adapter can call Engram belief ports for
valid-time reads and lifecycle transitions without re-implementing those rules,
while record-time history remains honest and unsupported until a versioned
store can answer it.

### T1: Current SQLite parity snapshots exist

**Depends on:** none

**Tests:**
- Snapshot fixture covers active, historical, stale, superseded, retracted,
  contradicted, and semantically searchable beliefs.
- Snapshot fixture covers `as_of` before, during, and after valid intervals.

**Approach:**
- Seed current AgentZero SQLite stores with deterministic facts, beliefs,
  embeddings, and contradictions.
- Call store traits and API projection helpers, then write normalized JSON
  snapshots.

**Done when:** snapshots fail if current AgentZero valid-time, stale,
supersede/retract, contradiction, or search behavior changes.

### T2: Adapter mapping modules round-trip AgentZero records

**Depends on:** T1

**Tests:**
- Unit tests round-trip belief, contradiction, and memory fact records through
  Engram domain shapes without losing AgentZero API fields.
- Edge tests cover null intervals, global ward, session-local scope, stale
  beliefs, superseded beliefs, temporal contradictions, and missing embeddings.

**Approach:**
- Implement `mapping/*`, `config.rs`, `error.rs`, and `temporal.rs`.
- Keep original IDs and source fact IDs visible at the trait boundary.

**Done when:** mapper tests prove round-trip compatibility for every field in
the current UI/API wire shapes.

### T3: Engram-backed `BeliefStore` matches snapshots

**Depends on:** T2

**Tests:**
- Trait parity tests compare `SqliteBeliefStore` and Engram-backed adapter JSON
  outputs for all T1 belief cases.
- Unit tests cover interval matching and idempotent upsert by
  `(partition_id, subject, valid_from)`.

**Approach:**
- Implement `belief_store.rs` with explicit valid-time filtering and lifecycle
  mutation methods.
- Use Engram repository calls only where they satisfy the contract; keep
  compatibility filtering in the adapter when needed.

**Done when:** AgentZero belief store parity snapshots pass against the adapter.

### T4: Engram-backed `BeliefContradictionStore` matches snapshots

**Depends on:** T2

**Tests:**
- Reversed-pair insert remains idempotent.
- `for_belief`, `list_recent`, `pair_exists`, and `resolve` match current
  SQLite snapshots.
- `a_won`, `b_won`, `compatible`, and `unresolved` mapping is exact.

**Approach:**
- Implement `contradiction_store.rs` with canonical pair ordering and Engram
  contradiction target mapping.
- Keep resolution as a contradiction-row update only.

**Done when:** contradiction parity snapshots pass against the adapter.

### T5: Memory fact temporal subset maps cleanly

**Depends on:** T2

**Tests:**
- Valid interval, supersession, epistemic class, source episode/ref, ward, and
  global/session-local scope mapping tests pass.
- Fact invalidation inputs still drive belief propagation through current
  AgentZero workers.

**Approach:**
- Implement the memory fact mapping required by belief synthesis and
  propagation before broad memory-store cutover.
- Keep non-belief memory behavior behind existing AgentZero store traits.

**Done when:** belief synthesis and propagation fixture runs against
Engram-backed fact reads/writes without scheduler changes.

### T6: Provider switch preserves API and UI shapes

**Depends on:** T3, T4, T5

**Tests:**
- Gateway route tests for `/api/beliefs/*`, `/api/contradictions/*`, and
  `/api/belief-network/*` pass with both providers.
- UI typecheck passes without changing belief wire types.

**Approach:**
- Add a provider selection config that chooses current SQLite stores or the
  Engram adapter.
- Wire store trait objects in one composition point.

**Done when:** provider switch does not require API route or UI type changes.

### T7: Cutover validation and adversarial review complete

**Depends on:** T6

**Tests:**
- Full parity fixture passes.
- Cargo, TypeScript, and route test gates pass in AgentZero.
- Engram docs/contract gates pass if any Engram files change.

**Approach:**
- Review for god classes, hidden schedule migration, source/provenance loss,
  stale leakage, fake bitemporality, and embedding ranking drift.
- Document any accepted incompatibility as an explicit migration note.

**Done when:** the provider can be switched to Engram-backed storage with
documented parity and a rollback to current SQLite stores.

## Rollout

- **Delivery:** ship behind an AgentZero provider flag. Default remains current
  SQLite until parity fixtures and manual UI checks pass.
- **Rollback:** switch the provider flag back to current SQLite stores. Data
  written only to Engram during canary must either be dual-written or declared
  non-rollbackable before production use.
- **Deployment sequencing:** mapping and fixtures first, adapter stores second,
  gateway composition third, provider canary last.

## Risks

- Engram's existing belief repository may be too narrow for efficient valid-time
  reads; the adapter may need an Engram-side list/query extension later.
- Exact embedding search parity requires preserving raw bytes or accepting a
  ranking migration.
- Scope mapping can leak data if `partition_id`, ward, session, and global
  semantics are collapsed into one field.
- Record-time audit semantics can be overstated; current cutover preserves
  valid-time and records write metadata only.

## Changelog

- 2026-07-02: initial plan from AgentZero code research, Engram belief adapter
  review, and bitemporal/belief-revision research.
