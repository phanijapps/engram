# ADR-0023: Evidence write-side — port-level rewrite on `ProvenanceQuery`

- **Status:** Proposed
- **Date:** 2026-07-10
- **Decision-makers:** phanijapps
- **Supersedes:** none
- **Related:** ADR-0022 (engine grid vs backend recipe), [`docs/specs/episode-evidence-api/spec.md`](../specs/episode-evidence-api/spec.md) (S2 read half + this write follow-up), `docs/backlog.md` (`episode-evidence-write-side`, resolved by this ADR)

## Decision summary

- **Decision:** Add a single write op — `ProvenanceQuery::attach_evidence` — implemented as a **port-level rewrite** (read the record → append the `EvidenceRef` to `Provenance.evidence` → write the record back via the knowledge store's existing `get_*`/`put_*`), keeping the trait on `ProvenanceQuery` (additive, not renamed).
- **Because:** the deferred write-side needed a scope-safe, idempotent-ish append contract and a storage model; a read-modify-write over the existing `record_json` needs no schema change, no new port, and no new storage, and stays engine-neutral (ADR-0022 rule 1).
- **Applies to:** `core/integration/src/provenance.rs` (port) and `adapters/integration/src/provenance.rs` (`SqlProvenanceQuery` impl). v1 backs the same knowledge-graph core as the reads — entity, relationship, source.
- **Tradeoff accepted:** the rewrite is not an atomic append (a concurrent writer between the get and the put can be overwritten); v1 is single-writer-per-scope in practice, and a journaled/atomic append is deferred until a second writer or bi-temporal evidence retrieval demands it.
- **Revisit if:** concurrent writers on the same record appear, or evidence must be journaled for bi-temporal retrieval — that triggers a real append/ledger storage model (and likely a new `EvidenceService` port or a schema change).

## Context

S2 shipped the **read half** of the episode/evidence API: the engine-neutral
`ProvenanceQuery` port + `SqlProvenanceQuery` read the `Provenance`/`EvidenceRef`
embedded in stored records. The backlog entry `episode-evidence-write-side`
blocked the write half on two open questions:

1. **Storage model** — append-vs-rewrite. A real append (a separate evidence
   ledger / `record_json` migration) is a schema change; a rewrite reuses the
   store's existing `get_*`/`put_*` over the lossless `record_json` with no
   migration.
2. **Where the op lives** — on `KnowledgeRepository`/`MemoryService`, on a new
   `EvidenceService` port, or on `ProvenanceQuery` itself.

The host-SDK brief needs *some* way to attach evidence to an already-stored
record (a record created without it, or evidence discovered later), but v1 has
one backend (SQLite) and one writer per scope.

## Decision

`attach_evidence` lives on `ProvenanceQuery` as an **additive write op** and is
implemented as a **port-level rewrite**:

> Read the record (`get_entity` / `get_relationship` / `list_sources`+find),
> append the `EvidenceRef` to `Provenance.evidence`, write the modified record
> back (`put_entity` / `put_relationship` / `put_source`). Return the updated
> `Provenance`.

Three rules govern it:

1. **Additive, same port.** The trait keeps the name `ProvenanceQuery`; the
   write is a natural extension of the same facade. No new port, no new crate.
2. **Engine-neutral port, engine-specific impl.** The port names no engine type
   (ADR-0022 rule 1, gated by `check-engine-neutrality.sh`); only the
   `SqlProvenanceQuery` impl composes the `SqlKnowledgeStore`.
3. **Same v1 target surface as the reads.** Entity, relationship, source are
   backed; a `KnowledgeRelationship` additionally carries its own
   `evidence: Vec<EvidenceRef>` slot, which the impl appends to as well (mirrors
   the read side's `evidence_for`, which surfaces both slots). Every other
   `EvidenceTargetType` (memory, belief, document, chunk, concept, event, url)
   returns `CoreError::CapabilityUnsupported`; a record absent in the caller's
   scope returns `CoreError::NotFound`.

## Decision drivers

- **No schema change / no migration** — the rewrite reuses the existing
  `record_json` round-trip; the winning driver for v1.
- **Engine neutrality preserved** — the port stays clean; the impl composes
  existing repository methods.
- **Smallest surface** — one op on one port beats a new `EvidenceService` port
  for a single-writer, single-backend v1.

## Consequences

**Positive:**

- Hosts can attach evidence to an existing record through the same handle they
  read provenance through — no second port, no new storage.
- No schema migration: the impl is a read-modify-write over `record_json`.
- The flexibility guarantee (ADR-0022) holds — a future engine implements the
  same port method its own way.

**Negative:**

- **Not an atomic append.** A concurrent writer between the get and the put can
  be silently overwritten (last-write-wins). Acceptable for v1's
  single-writer-per-scope; a journaled append is deferred.
- **No bi-temporal evidence journal.** Attaching evidence mutates the record's
  provenance in place; prior evidence state is not retained for `as_of`
  retrieval of *evidence history* (the record's own `valid_from`/`valid_until`
  bi-temporality is unaffected). Deferred until demanded.
- The unsupported-target short-circuit means memory/belief/document/chunk
  evidence cannot be attached through this op in v1 — each needs its own
  scope-safe get/put before it can be wired.

**Revisit if:** concurrent writers on one record, or bi-temporal evidence
retrieval, become real requirements — that triggers a real append/ledger model
(and possibly a dedicated `EvidenceService` port).

## Confirmation

- **Mode:** TDD (block_on integration tests in
  `adapters/integration/tests/provenance_query.rs`): attach to entity →
  persisted in `provenance.evidence`; attach to relationship → appears in *both*
  the relationship's own `evidence` vec and `provenance.evidence`; attach to an
  unsupported target (memory) → `CapabilityUnsupported`; attach to a missing
  record → `NotFound`. Plus `.codex/hooks/check-engine-neutrality.sh` for the
  port layer.
- **Owner:** engram core team.

## Alternatives considered

- **A new `EvidenceService` port.** Rejected for v1: a second port for one op on
  one backend duplicates the facade and forces every backend to implement two
  ports. Wins only if evidence storage diverges from the record (a ledger) —
  deferred until that divergence is real.
- **A real append ledger / `record_json` migration.** Rejected as premature: it
  is a schema change bought to solve a concurrency/bi-temporal problem v1 does
  not have (single writer per scope). Revisit when a second writer or
  evidence-history retrieval arrives.
- **Put the op on `KnowledgeRepository`/`MemoryService`.** Rejected: it would
  spread the evidence-attach contract across every record-kind port and lose the
  single backend-neutral entry point the host-SDK brief wants. `ProvenanceQuery`
  already owns the evidence read; the write belongs next to it.

## References

- ADR-0022 — engine neutrality this preserves (port clean, impl composes the
  engine).
- `docs/specs/episode-evidence-api/spec.md` — S2 read half + this write-side AC.
- `docs/backlog.md` — `episode-evidence-write-side` (resolved by this ADR).
