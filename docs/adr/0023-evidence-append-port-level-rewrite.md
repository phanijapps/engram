# ADR-0023: Evidence append — port-level rewrite over separate evidence table

- **Status:** Proposed
- **Date:** 2026-07-10
- **Decision-makers:** phanijapps
- **Supersedes:** none
- **Related:** ADR-0018 (knowledge-graph retraction/convergence — the rewrite model), [`episode-evidence-api`](../specs/episode-evidence-api/spec.md) (S2 write-side deferred), [`engram-host-sdk`](../product/briefs/engram-host-sdk.md) brief

## Decision summary

- **Decision:** Attaching evidence (`EvidenceRef`) to an already-stored record rewrites the record's `record_json` (read → append to `Provenance.evidence` → write back), presented as an append at the port level.
- **Because:** records are small lossless JSON blobs; a separate evidence table adds schema complexity + joins that v1 does not need.
- **Applies to:** the evidence write-side (S2's deferred `episode-evidence-write-side`).
- **Tradeoff accepted:** the entire record is rewritten for one evidence link (cheap for small blobs; a separate table would avoid this but at schema cost).
- **Revisit if:** evidence append frequency or record size makes the rewrite prohibitively expensive.

## Context

S2 shipped the **read half** of `episodes_evidence` (the `ProvenanceQuery` port). The **write half** — a dedicated *attach evidence to an existing record* operation — was deferred (logged in `docs/backlog.md` → `episode-evidence-write-side`). The deferred item's blocker was: "an ADR deciding the append-vs-rewrite storage model."

Records are stored losslessly as `record_json TEXT` blobs (ADR-0005). `Provenance.evidence` is a `Vec<EvidenceRef>` embedded inside the record JSON. Attaching evidence means modifying that array.

Two storage models were considered:
1. **Port-level rewrite** — read the record, append to `Provenance.evidence`, write the whole blob back. The port presents this as `attach_evidence(target, evidence_ref)`; the impl does the read-modify-write.
2. **Separate evidence table** — a new `record_evidence` table with `(record_id, evidence_ref_json)` rows, joined on read to reconstruct the full `Provenance.evidence` list.

## Decision

Attaching evidence to an existing record uses **port-level rewrite** (option 1). The port exposes an append-style API (`attach_evidence`); the implementation reads the record, appends the `EvidenceRef` to `Provenance.evidence`, and writes the updated `record_json` back. No new table, no schema change.

## Consequences

**Positive:**
- No schema migration — reuses the existing `record_json` storage model.
- Simple implementation (read-modify-write, one per-store operation).
- The append API is honest about what it does (the record is rewritten) but abstracts the mechanics from the caller.

**Negative:**
- The entire record is rewritten for one evidence link. For small JSON blobs (the common case) this is cheap; for very large records (e.g., with many existing evidence links) it is wasteful.
- Concurrent appends to the same record race (last-write-wins) unless the caller serializes.

**Revisit if:** evidence append frequency or record size makes the rewrite prohibitively expensive, or concurrent-append contention becomes a problem.

## Alternatives considered

- **Separate evidence table** (option 2). Rejected: adds schema complexity (a new table + foreign keys + a join on every read that reconstructs `Provenance.evidence`) for a v1 that doesn't need it. The read path already deserializes the full record JSON; an additional table + join is overhead for no gain at demo scale.

## References

- `docs/backlog.md` → `episode-evidence-write-side` (the deferred item this ADR unblocks).
- ADR-0018 (retraction/convergence — the same read-modify-write model for retraction).
