# Spec: codegraph-temporal

- **Status:** Draft
- **Owner:** phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** RFC-0012, `docs/codegraph-parity-roadmap.md` (C6), ADR-0019 (bi-temporal entities)
- **Brief:** none
- **Contract:** none — pure scoring math over versioned symbols; no public contract change
- **Shape:** service

> **Spec contract:** this document defines what "done" means.

**Scope.** The temporal scoring engine for the on-top codegraph layer (RFC-0012):
ranks versioned symbols by recency, blast-radius-weighted impact, and a compound
blend — the first three of memtrace's six scoring modes. Pure math over a
`VersionedSymbol` input (key, `valid_from`/`valid_until` from ADR-0019,
in/out-degree from the call graph); the `novel` / `directional` / `overview`
modes (which need a change-diff / baseline model) are deferred.

## Objective

A coding agent asks "what changed recently?", "what has the highest blast
radius?", or "what matters most right now?" over the versioned call graph. This
crate scores versioned symbols by recency (exponential decay from `valid_from`),
impact (`in_degree^0.7 × (1+out_degree)^0.3`), and a compound blend, returning
ranked lists — mirroring memtrace's temporal engine.

## Boundaries

### Always do
- Score a caller-built `VersionedSymbol` input (the caller supplies validity +
  degrees); stay decoupled from storage/AST.
- Keep the crate dependency-light (chrono only for time math).

### Ask first
- The `novel` / `directional` / `overview` modes — they need a change-diff /
  baseline distribution model not yet present.

### Never do
- Depend on storage, bindings, or `engram-domain` (the input is generic); or
  duplicate the graph algorithms (degrees are passed in).

## Testing Strategy

- **TDD** — `recent` ranks a just-introduced symbol above an old one; `impact`
  ranks a high-in-degree symbol highest; `compound` blends + ranks. Deterministic
  on fixed timestamps.

## Acceptance Criteria

- [ ] `recent(versions, now, half_life)` ranks by `2^(-elapsed/half_life)` over
  currently-valid versions; an older version scores lower.
- [ ] `impact(versions)` ranks by `in_degree^0.7 × (1+out_degree)^0.3`.
- [ ] `compound(versions, now, half_life)` returns a normalized blend of recent +
  impact, ranked.
- [ ] Versions with no `valid_from` score 0 under `recent` (sort last), not NaN.
- [ ] Depends only on chrono; per-crate gates green.

## Assumptions

- Technical: bi-temporal `validFrom`/`validUntil` on `KnowledgeEntity` (ADR-0019)
  + in/out-degree from `engram-graph-analytics` supply the input at the wiring
  layer; this crate does the scoring math. (source: B6 + reachability)
- Process: light mode. (source: user confirmation 2026-07-08)
