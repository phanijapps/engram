# AgentZero to Engram Belief and Bitemporal Cutover Research

- **Status:** Applied research
- **Date:** 2026-07-02
- **Related spec:** `zbot-engram-belief-bitemporal-cutover` (historical; capability covered in [docs/product/engram.md](../product/engram.md))
- **Question:** How should AgentZero's belief, contradiction, and bitemporal memory behavior map to Engram when AgentZero cuts over from its current stores to an Engram-backed Rust library?

## Short Verdict

The cutover should be adapter-first. AgentZero already has a stable memory API,
UI wire shape, sleep-cycle scheduler, and store traits. Engram already has
belief, contradiction, source, scope, provenance, and valid interval fields.
Engram now also has storage-neutral valid-time belief query/lifecycle ports and
SQLite-backed valid-time `as_of` reads. The missing piece is not a new Engram
ontology; it is a compatibility adapter that implements AgentZero's store traits
over Engram while preserving AgentZero's public API and UI contracts.

Current implementation note: AgentZero's `BeliefStore::get_belief` is still an
`as_of` valid-time query, and Engram's `BeliefRepository::get_belief` now
supports valid-time filters over `valid_from` / `valid_until`. The remaining
temporal gap is record-time history, not valid-time lookup. Engram's SQLite
belief adapter stores current rows, so it rejects record-time history queries
instead of pretending `created_at` / `updated_at` are a full bitemporal audit
log.

## Evidence From AgentZero

AgentZero's belief model is an aggregate over one or more `MemoryFact` rows. It
is partition-scoped, carries its own valid interval, retains source fact IDs,
and uses `stale` for multi-source invalidation. Sole-source invalidation retracts
the belief by closing `valid_until`; multi-source invalidation marks the belief
stale for the next synthesizer cycle.

Source evidence:

- `/home/videogamer/projects/agentzero/stores/zbot-stores-domain/src/belief.rs`
  defines `Belief` with `partition_id`, `subject`, `content`, `confidence`,
  `valid_from`, `valid_until`, `source_fact_ids`, `synthesizer_version`,
  `reasoning`, `created_at`, `updated_at`, `superseded_by`, `stale`, and
  optional little-endian f32 embedding bytes.
- `/home/videogamer/projects/agentzero/stores/zbot-stores-traits/src/belief.rs`
  is the compatibility contract: `get_belief(partition_id, subject, as_of)`,
  `list_beliefs`, `upsert_belief`, `supersede_belief`, `mark_stale`,
  `retract_belief`, `beliefs_referencing_fact`, `get_belief_by_id`,
  `list_stale`, `clear_stale`, and `search_beliefs`.
- `/home/videogamer/projects/agentzero/stores/zbot-stores-sqlite/src/belief_store.rs`
  shows the exact query semantics:
  `valid_from IS NULL OR valid_from <= as_of`, and
  `valid_until IS NULL OR valid_until > as_of`, ordered by latest valid start.
- `/home/videogamer/projects/agentzero/gateway/gateway-memory/src/sleep/belief_propagator.rs`
  makes invalidation event-driven and non-blocking: propagation errors are
  counted and logged, not bubbled to the source fact operation.
- `/home/videogamer/projects/agentzero/gateway/gateway-memory/src/sleep/belief_synthesizer.rs`
  reprocesses stale beliefs first, groups active facts by subject key, derives
  confidence as recency-weighted average confidence, and sets a belief's
  `valid_from` to the earliest constituent fact valid time.
- `/home/videogamer/projects/agentzero/gateway/gateway-memory/src/sleep/belief_contradiction_detector.rs`
  keeps contradiction scheduling in AgentZero, scans current beliefs, groups by
  subject prefix, applies a budgeted LLM judge, and delegates idempotency to the
  contradiction store.

AgentZero's contradiction model is pairwise and reviewable:

- `/home/videogamer/projects/agentzero/stores/zbot-stores-domain/src/belief_contradiction.rs`
  defines `logical`, `tension`, and reserved `temporal` contradiction types,
  plus `a_won`, `b_won`, `compatible`, and `unresolved` resolutions.
- `/home/videogamer/projects/agentzero/stores/zbot-stores-sqlite/src/belief_contradiction_store.rs`
  canonicalizes pair ordering and uses uniqueness on `(belief_a_id,
  belief_b_id)` for idempotency.

AgentZero's API and UI are already abstracted over store traits:

- `/home/videogamer/projects/agentzero/gateway/src/http/beliefs.rs` routes
  through `state.belief_store` and `state.belief_contradiction_store`, returns
  flattened timestamp strings, source fact IDs, stale state, and contradiction
  resolution fields, and skips raw embedding bytes.
- `/home/videogamer/projects/agentzero/apps/ui/src/features/memory/command-deck/types.beliefs.ts`
  mirrors those response shapes. The UI does not need Engram-native DTOs.

## Evidence From Engram

Engram has the right domain vocabulary for the cutover:

- [`core/domain/src/belief.rs`](../../core/domain/src/belief.rs) defines
  `Belief`, `BeliefSource`, `BeliefStatus`, `Contradiction`,
  `ContradictionKind`, `ContradictionStatus`, and explicit contradiction
  resolutions.
- [`core/domain/src/identity.rs`](../../core/domain/src/identity.rs) defines
  `Scope` as `tenant`, optional `subject`, `workspace`, `session`, and
  `environment`.
- [`docs/domain-data-model.md`](../domain-data-model.md) describes beliefs as
  derived stances over evidence, not source truth, and includes valid intervals,
  stale/supersession semantics, provenance, and source evidence.

The current Engram SQLite belief adapter is durable and supports the
valid-time subset AgentZero needs, but it is still not a complete AgentZero
provider by itself:

- [`adapters/orchestration/belief-sqlite/src/service.rs`](../../adapters/orchestration/belief-sqlite/src/service.rs)
  persists belief and contradiction payloads as JSON with scope columns and
  implements valid-time `as_of` reads over `valid_from` / `valid_until`.
- That same file rejects record-time history queries because it stores current
  rows, not historical versions.
- Detection is advisory and simple: active beliefs on the same subject with
  differing content produce a logical contradiction. AgentZero's detector is
  budgeted, LLM-judged, and scheduled by the sleep worker.

This means Engram should remain the durable/domain-backed library, while
AgentZero keeps scheduling and implements its existing traits through a minimal
adapter.

## External Research Extract

Bitemporality has two axes:

- Valid or actual time: when a fact is true in the modeled world.
- Transaction, system, processing, or record time: when the system knew or
  recorded that version.

Martin Fowler's bitemporal history article frames this as actual history versus
record history, and notes that bitemporal queries need both an actual date and a
record date. XTDB's docs describe transaction-time as arrival into the database
and valid-time as the user/domain timestamp used for historical queries; XTDB's
current time docs say "`as of`" / "`with effect from`" requirements usually
indicate valid time. Datomic's history view is a useful caution: history queries
see present and past assertions/retractions, so they cannot be treated as a
single point-in-time entity view.

Implication for this cutover:

- AgentZero currently exposes valid-time behavior through `as_of`.
- AgentZero does not expose a full two-parameter API like
  `(valid_as_of, recorded_as_of)`.
- Engram should not pretend `created_at` / `updated_at` alone are sufficient for
  full bitemporal audit queries.
- The adapter should preserve valid-time now and record system-time metadata in
  provenance/events so later audit-time APIs can be added without data loss.

Belief revision research also matters. The Stanford Encyclopedia of Philosophy
entry distinguishes revision from update: revision handles new information about
the world, while update handles the world changing. AgentZero's split mirrors
that distinction:

- A superseding fact or new valid interval is an update to current world state.
- A contradiction review is a revision workflow over competing beliefs.
- The adapter must not collapse these into one destructive overwrite path.

Sources:

- Martin Fowler, "Bitemporal History":
  <https://martinfowler.com/articles/bitemporal-history.html>
- XTDB v1 bitemporality docs:
  <https://v1-docs.xtdb.com/concepts/bitemporality/>
- XTDB "Time in XTDB":
  <https://docs.xtdb.com/about/time-in-xtdb.html>
- Datomic database filters:
  <https://docs.datomic.com/reference/filters.html>
- Stanford Encyclopedia of Philosophy, "Logic of Belief Revision":
  <https://plato.stanford.edu/entries/logic-belief-revision/>

## Cutover Contract

### Stable AgentZero Surface

These surfaces remain stable during cutover:

- `zbot_stores_traits::BeliefStore`
- `zbot_stores_traits::BeliefContradictionStore`
- the existing memory fact store methods that write/read valid intervals,
  `superseded_by`, `epistemic_class`, source episode IDs, and source refs
- `/api/beliefs/*`, `/api/contradictions/*`, `/api/belief-network/*`
- memory tab and observatory UI wire types
- `MemorySettings`, including recall, MMR/RRF, hierarchy, and belief network
  scheduling config

Engram becomes a backing provider. It does not become the AgentZero scheduler,
API route namespace, or UI contract.

### Temporal Semantics

The adapter must preserve these semantics:

- `as_of == None` means valid-time now.
- Active at `as_of` means:
  `valid_from IS NULL OR valid_from <= as_of`, and
  `valid_until IS NULL OR valid_until > as_of`.
- `supersede_belief(old_id, new_id, t)` closes the old belief's valid interval
  at `t`, sets `superseded_by = new_id`, and updates record metadata.
- `retract_belief(id, t)` closes the valid interval at `t` without setting a
  replacement.
- `mark_stale(id)` sets `stale = true` without changing valid interval.
- `clear_stale(id)` clears the stale flag after successful re-synthesis.
- Semantic belief search only scores live beliefs with embeddings.

Record-time handling:

- The adapter records write time through Engram `created_at`, `updated_at`,
  provenance, and adapter metadata.
- Full audit-time reads are out of scope until AgentZero exposes a record-time
  parameter or Engram accepts a bitemporal repository contract.
- The adapter must not label current `created_at` / `updated_at` support as
  full bitemporality.

### Field Mapping

| AgentZero field | Engram mapping | Notes |
| --- | --- | --- |
| `Belief.id` | `Belief.id` | Preserve the same string ID where Engram ID validation allows it; otherwise store `zbot.originalBeliefId` metadata and return original ID at the trait boundary. |
| `partition_id` | `Scope.workspace` or adapter-configured scope field | Default should be `workspace = partition_id`, `tenant = adapter tenant`; do not hard-code `root`. |
| `subject` | `BeliefSubject.key` and optionally `Scope.subject` | Subject remains the aggregation key. |
| `content` | `Belief.content` | No rewrite at adapter boundary. |
| `confidence` | `Belief.confidence` | Convert f64 to f32 at Engram boundary and back carefully; tests should allow only loss expected from f32. |
| `valid_from` / `valid_until` | `Belief.valid_from` / `valid_until` | Must be queryable by the adapter even if Engram repo lacks native query. |
| `source_fact_ids` | `Belief.sources[]` target type memory/assertion + metadata copy | Preserve order and original IDs for UI detail resolution. |
| `synthesizer_version` | `Belief.synthesizer` or metadata | Preserve as compatibility metadata if no exact Engram field is accepted. |
| `reasoning` | `Belief.reasoning` | Direct mapping exists. |
| `created_at` / `updated_at` | `Belief.created_at` / `updated_at` | Use as record metadata, not as valid-time substitute. |
| `superseded_by` | `Belief.superseded_by` and `status = superseded` | Keep AgentZero response field exact. |
| `stale` | `Belief.stale` and `status = stale` when true | Do not return stale beliefs as active unless the AgentZero trait asks for stale. |
| `embedding` | adapter vector sidecar or Engram embedding refs | AgentZero stores raw little-endian f32 bytes; Engram has embedding refs. Preserve bytes in an adapter sidecar unless a vector adapter migration is accepted. |

Contradiction mapping:

| AgentZero field | Engram mapping | Notes |
| --- | --- | --- |
| `id` | `Contradiction.id` | Preserve ID at boundary. |
| `belief_a_id`, `belief_b_id` | two `ContradictionTarget`s | Maintain canonical lexicographic pair ordering for idempotency. |
| `logical` / `tension` / `temporal` | `ContradictionKind` | Engram already has temporal; AgentZero reserves it for future. |
| `severity` | `Contradiction.severity` | Direct mapping, f64/f32 precision tested. |
| `judge_reasoning` | `Contradiction.reasoning` | Direct mapping. |
| `detected_at` | `Contradiction.detected_at` | Direct mapping. |
| `resolved_at` + `resolution` | `Contradiction.resolution` and status | `a_won` / `b_won` require target IDs to resolve `winning_target_id`. |

### Adapter Modules

A clean `zbot-engram-adapter` crate should be split by reason to change:

- `config.rs`: Engram path/provider, tenant, partition-to-scope mapping,
  embedding mode, compatibility flags.
- `error.rs`: typed adapter errors translated to AgentZero trait `String`
  errors only at the trait boundary.
- `mapping/belief.rs`: `zbot::Belief` <-> `engram_domain::Belief`.
- `mapping/contradiction.rs`: pair mapping and resolution mapping.
- `mapping/fact.rs`: memory fact valid interval/source/provenance mapping.
- `belief_store.rs`: implements `BeliefStore`, including valid-time filtering.
- `contradiction_store.rs`: implements `BeliefContradictionStore`, including
  canonical pair idempotency.
- `embedding.rs`: little-endian f32 byte conversion and Engram embedding
  reference/sidecar behavior.
- `fixtures.rs`: cutover fixtures and snapshot helpers.

Do not put construction, mapping, temporal filtering, vector scoring, and error
translation into one `EngramMemoryService` god class.

## Fixture-First Acceptance Probe

The cutover is acceptable only when these JSON snapshots match the current
AgentZero SQLite behavior:

1. A current belief with `valid_from <= now` and `valid_until = NULL` is returned
   by `get_belief(None)`.
2. A historical belief closed at `T2` is returned for `as_of = T1` and omitted
   for `as_of = T3`.
3. `supersede_belief` closes the old interval and records the replacement.
4. `retract_belief` closes the interval without a replacement.
5. `mark_stale` excludes the belief from active recall but includes it in
   `list_stale`.
6. `beliefs_referencing_fact` returns only currently active dependent beliefs.
7. `search_beliefs` omits superseded/retracted/no-embedding beliefs and preserves
   cosine ranking for byte-encoded embeddings.
8. Contradiction insertion is idempotent for reversed pairs.
9. Contradiction resolution maps `a_won`, `b_won`, and `compatible` without
   mutating source beliefs.
10. `/api/beliefs/*` and the memory tab receive the same wire shapes before and
    after provider switch.

## Open Questions

- Does Engram want a native valid-time belief query port, or should
  `zbot-engram-adapter` own compatibility filtering until another consumer
  needs it?
- Should belief embedding bytes be migrated as a sidecar for exact AgentZero
  search parity, or regenerated into Engram embedding refs with an explicit
  ranking-change acceptance test?
- Should `partition_id` map to `Scope.workspace` universally, or should the
  adapter support per-deployment mapping to `workspace`, `session`, or
  `environment`?
- Does AgentZero need future record-time audit queries, or is valid-time
  sufficient for the cutover?
