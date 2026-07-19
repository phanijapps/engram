# Spec: belief-contradiction-bitemporal (RFC 0004 Slice 5 / PHASE62)

- **Status:** Shipped
- **Shape:** mixed (data + service + ui)
- **Constrained by:** RFC-0004 D5 + the boundary rule (belief/contradiction are **engram-orchestration** ports → durable storage in a NEW `engram-store-belief-sqlite` adapter, NOT folded into `engram-store-knowledge-sqlite`); ADR-0007 (focused `NativeBeliefEngine`, no god-struct)
- **Contract:** none (the `BeliefRepository`/`ContradictionDetector` ports + `core/domain/src/belief.rs` types already exist; transport stays `unknown`-typed)

## Objective

Beliefs (derived stances) and contradictions (reviewable tension) persist durably and are visible in the demo. A new `engram-store-belief-sqlite` adapter implements `BeliefRepository` (put_belief, put_contradiction, get_contradiction, resolve_contradiction) and `ContradictionDetector` (detect over a slice of beliefs — flags active beliefs on the same subject with differing content). A focused `NativeBeliefEngine` exposes it over N-API; `/belief/*` routes + a `BeliefPanel` let the user add beliefs (with `valid_from`/`valid_until`), list them, run detection, and resolve contradictions. Bi-temporal is display-only: `valid_from`/`valid_until` are shown; there is no `transaction_time` (grep-confirmed) and no time-travel/as-of queries.

## Decision (aligns with RFC D5)

Durable belief/contradiction storage lives in a distinct new adapter (`engram-store-belief-sqlite`) that depends on `engram-core` (the orchestration crate owning the ports), mirroring the `engram-store-knowledge-sqlite` table-per-record pattern. `NativeBeliefEngine` is focused (belief + contradiction ops only) per ADR-0007. Full memory-assertion → belief consolidation-driver synthesis is documented future work (the `DryRunConsolidationService` already exists for planning); this slice delivers durable belief storage + contradiction detection + resolution + bi-temporal display.

## Assumptions

- Technical: `BeliefRepository`/`ContradictionDetector` ports + `Belief`/`Contradiction` types already exist in `engram-core`/`engram-domain`; the in-memory impl (the retired memory in-memory adapter (see `docs/specs/retire-memory-inmem/spec.md`)) is the template. (verified)
- Technical: NO `transaction_time` field exists; `valid_from`/`valid_until` are display-only. (grep-verified)
- Technical: new adapter mirrors `engram-store-knowledge-sqlite` (table-per-record + `record_json`, scope on parent, idempotent schema, `ON CONFLICT` upsert). (verified)
- Process: lighter single-pass adversarial review. (user standing preference)

## Boundaries

**Always do**
- Put belief/contradiction persistence in `engram-store-belief-sqlite` only; depend on `engram-core` (orchestration), not `engram-store-knowledge-sqlite`.
- Keep `NativeBeliefEngine` focused (no consolidation/hierarchy leakage); mirror the taxonomy/knowledge binding pattern.
- Keep bi-temporal display-only (no transaction_time, no temporal queries).
- `resolve_contradiction` enforces scope (NotFound when not visible).
- `ContradictionDetector` is advisory (it surfaces tension; resolution is a human/action choice).

**Ask first**
- Full memory→belief consolidation driver (synthesis from assertions); enforced belief policy.
- Temporal / as-of queries.

**Never do**
- Fold belief storage into `engram-store-knowledge-sqlite` or the memory adapter; create a god-struct engine; add `transaction_time`; change ports/domain types.

## Testing Strategy

- **TDD (unit, Rust):** `engram-store-belief-sqlite` — put/get/resolve round-trips with scope filtering; `detect_contradictions` flags same-subject differing-content beliefs + ignores single-content groups.
- **Goal-based (build):** `cargo fmt/check --workspace && cargo test -p engram-store-belief-sqlite`; rebuild native binding; backend + frontend typecheck/build.
- **Goal-based (plumbing):** `/belief/*` routes round-trip via curl; detection + resolution work.
- **Manual QA:** add two opposing beliefs, detect the contradiction, resolve it; confirm valid_time display.

## Acceptance Criteria

- [x] `engram-store-belief-sqlite` implements `BeliefRepository` + `ContradictionDetector` durably (idempotent schema, scope filtering) with Rust unit tests.
- [x] `NativeBeliefEngine` exposes put_belief/put_contradiction/get_contradiction/resolve_contradiction/detect; TS transport wraps them; native module rebuilds.
- [x] `/belief/{put,contradiction,get,resolve,detect}` routes work via curl.
- [x] Bi-temporal is display-only (valid_from/valid_until shown; no transaction_time).
- [x] A BeliefPanel lists beliefs (subject, content, status, confidence, valid_time), adds beliefs, runs detection, and resolves contradictions.
- [x] No god-struct; no fold into knowledge/memory adapter; cargo fmt/check/test + typecheck/build pass.
