# Spec: Zbot Engram Belief Bitemporal Cutover

- **Status:** Draft
- **Owner:** phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** ADR-0003, ADR-0004, ADR-0007, `docs/research/zbot-engram-belief-bitemporal-cutover.md`
- **Brief:** none
- **Contract:** AgentZero store traits and HTTP wire shapes; no accepted Engram v1 contract change
- **Shape:** integration

> **Spec contract:** this document defines what "done" means. The implementing
> PR must match this spec, or update it. Verification must be derivable from it.

## Objective

AgentZero can cut over belief, contradiction, and bitemporal memory behavior to
an Engram-backed Rust library without changing AgentZero's public APIs, memory
tab UI shapes, sleep-cycle scheduler, or operator settings. The adapter
implements AgentZero store traits over Engram records and preserves valid-time
queries, stale/retract/supersede behavior, source fact provenance, contradiction
idempotency, and semantic belief search compatibility.

## Boundaries

### Always do

- Implement AgentZero-facing store traits as the compatibility boundary.
- Keep AgentZero responsible for sleep-cycle scheduling, RRF/MMR/recall settings,
  belief-network interval config, hierarchy config, and UI route ownership.
- Preserve AgentZero `Belief`, `BeliefContradiction`, `MemoryFact`, and HTTP
  response wire shapes at the API boundary.
- Preserve valid-time `as_of` semantics for beliefs and facts.
- Preserve `source_fact_ids`, stale state, supersession links, retraction
  intervals, contradiction pair canonicalization, and resolution fields.
- Keep Engram-specific IDs, scopes, provenance, and embedding refs behind
  adapter mapping functions.
- Split adapter code by responsibility: config, mapping, temporal filtering,
  store trait implementations, embedding conversion, errors, and fixtures.

### Ask first

- Add or change Engram domain fields, accepted schemas, generated contracts, or
  public repository ports.
- Change AgentZero public routes, TypeScript UI wire types, or settings shape.
- Regenerate embeddings instead of migrating/scoring AgentZero's existing
  little-endian f32 belief embedding bytes.
- Add record-time audit query APIs beyond AgentZero's current valid-time
  `as_of` behavior.
- Move belief synthesis, contradiction detection scheduling, decay, or hierarchy
  worker ownership from AgentZero into Engram.

### Never do

- Treat Engram's current belief SQLite adapter as sufficient when it only stores
  valid intervals for display and does not implement `as_of` queries.
- Collapse beliefs into memory facts, knowledge chunks, hierarchy nodes, or
  source truth.
- Drop or rewrite `source_fact_ids` during mapping.
- Return stale, superseded, or retracted beliefs as active recall results unless
  the AgentZero trait explicitly asks for them.
- Resolve contradictions by mutating beliefs or facts automatically.
- Create a god class that owns construction, mapping, state, scheduling,
  scoring, persistence, and error translation.

## Testing Strategy

- **TDD:** adapter unit tests cover field mapping, valid-time interval matching,
  stale/retract/supersede transitions, source fact provenance, contradiction
  canonicalization, resolution mapping, embedding byte conversion, and error
  translation.
- **Goal-based integration:** a cutover fixture runs the same AgentZero store
  trait calls against current SQLite stores and the Engram adapter, then compares
  normalized JSON snapshots.
- **Goal-based API:** existing `/api/beliefs/*`, `/api/contradictions/*`, and
  `/api/belief-network/*` route tests pass without UI wire shape changes.
- **Manual QA:** memory tab and observatory belief-network views render the same
  belief, source fact, stale, contradiction, and resolution states before and
  after provider switch.

## Acceptance Criteria

- [ ] `zbot-engram-adapter` implements `BeliefStore` with exact AgentZero
  valid-time `get_belief(partition_id, subject, as_of)` semantics.
- [ ] `zbot-engram-adapter` implements `upsert_belief` idempotently by
  `(partition_id, subject, valid_from)` while preserving original AgentZero IDs
  at the trait/API boundary.
- [ ] `supersede_belief`, `retract_belief`, `mark_stale`, `clear_stale`,
  `list_stale`, `beliefs_referencing_fact`, and `get_belief_by_id` match the
  current SQLite store snapshots.
- [ ] `search_beliefs` preserves AgentZero's live-belief filters and cosine
  ranking for existing little-endian f32 embedding bytes, or a documented
  accepted migration test proves the new ranking contract.
- [ ] `zbot-engram-adapter` implements `BeliefContradictionStore` with
  canonical pair ordering, idempotent insert, `for_belief`, `list_recent`,
  `pair_exists`, and `resolve` behavior matching current snapshots.
- [ ] Memory fact mapping preserves `valid_from`, `valid_until`,
  `superseded_by`, `epistemic_class`, `source_episode_id`, `source_ref`,
  `ward_id`, and global-versus-local/session scope information.
- [ ] Existing AgentZero belief HTTP responses and UI TypeScript wire types do
  not change during cutover.
- [ ] AgentZero remains the owner of sleep-cycle scheduling and memory settings;
  Engram receives parameterized operations and persisted records only.
- [ ] A fixture demonstrates pre-cutover SQLite and Engram-backed adapter parity
  for active, historical, stale, superseded, retracted, contradicted, and
  semantically searched beliefs.
- [ ] Adapter modules remain focused; no single public struct owns mapping,
  scheduling, query filtering, scoring, persistence, and API translation.

## Assumptions

- Technical: AgentZero store traits are the stable compatibility contract
  (source: `/home/videogamer/projects/agentzero/stores/zbot-stores-traits/src/belief.rs`).
- Technical: AgentZero's current bitemporal behavior is valid-time behavior;
  record-time audit queries are not currently exposed (source:
  `/home/videogamer/projects/agentzero/stores/zbot-stores-sqlite/src/belief_store.rs`).
- Technical: Engram belief domain fields can represent AgentZero belief and
  contradiction records, but the current SQLite belief adapter does not provide
  AgentZero's as-of query semantics (source:
  `adapters/orchestration/belief-sqlite/src/service.rs`).
- Product: AgentZero API and UI continuity matters more than exposing Engram DTOs
  directly during cutover (source: user direction 2026-07-02).
- Process: Engram core changes require a separate accepted spec or ADR; this
  cutover starts with an adapter-first implementation (source: repository
  boundary rules).
