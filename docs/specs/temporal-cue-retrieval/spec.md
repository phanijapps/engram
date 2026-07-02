# Spec: Temporal + Cue retrieval dispatch

- **Status:** Shipped
- **Mode:** light — feature scoped to one adapter; the contract already carries the variants/cues/score fields; policy + scope isolation are carried forward unchanged. Single adversarial pass (user preference).
- **Gap:** `docs/arch_divergence.md` "Retrieval mode completeness" — `RetrievalMode::Cue` and `::Temporal` are defined but no retrieval path dispatches them (two of the four research-mandated modes, `architecture-design-v2.md:802`).

## Objective

Dispatch `RetrievalMode::Temporal` and `RetrievalMode::Cue` in the in-memory adapter (`engram-store-memory`) so memories are retrievable by time-window/recency and by slot-value cues. The domain contract already carries these variants, `filters.since/until`, `cues: Vec<Cue>`, and the `cue_match` / `matched_cues` score+explanation fields — only the dispatch is missing.

## Acceptance Criteria

- [x] **AC1 — Temporal dispatch.** When `request.modes` contains `Temporal`, in-scope memories within `filters.since/until` (all in-scope if no window is set) become candidates scored by **recency** (newer → higher), even with no keyword match; `score.recency` is populated. Scope, policy, expired, and redacted checks are unchanged.
- [x] **AC2 — Cue dispatch.** When `request.modes` contains `Cue` and `request.cues` is non-empty, in-scope memories whose **links** match the cues become candidates, scored by a **weighted cue-match ratio**; `score.cue_match` is populated and `explanation.matched_cues` lists the matched cues. A cue matches a link where `link.rel == cue.slot` and `link.target_id` satisfies `cue.operator` against `cue.value` (`Equals`, `Contains`, `StartsWith`, `EndsWith`, `Exists`, `In`, `Range`); `cue.weight` defaults to 1.0. (Links are the writable slot/value source — `WriteMemoryRequest` carries no `metadata`; matching `metadata` directly is a future extension once writes can set it.)
- [x] **AC3 — Mode combination, no regression.** Keyword retrieval stays always-on (existing keyword/hierarchy/semantic paths unchanged). Temporal/Cue are additive: a memory is a candidate if keyword matches OR an active new mode matches; `score.total` = max over the matching mode scores; sub-scores (`relevance`/`recency`/`cue_match`) reflect which modes matched.
- [x] **AC4 — Gates + regression.** `cargo fmt`/`clippy (--workspace --all-targets -D warnings)`/`test` + `pnpm typecheck`/`build` green; existing retrieval tests unchanged; new tests cover the temporal window (in/out), each cue operator, and combined modes, plus a policy/scope denial case.

## Boundaries

- In-memory adapter (`engram-store-memory`) only. No contract change — variants, cues, and score fields already exist in `engram-domain`.
- Scope + policy isolation are carried forward unchanged (the new modes go through the same gates as keyword retrieval).

## Testing Strategy

- TDD: new tests under the retired memory in-memory adapter (see `docs/specs/retire-memory-inmem/spec.md`) — temporal recall (within window / outside window / no window), cue recall (each `CueOperator`, weighted), combined `Keyword + Temporal + Cue`, and a policy/scope denial still omits. The existing suite is the regression gate.
- Single adversarial pass (user preference); a Blocker earns one re-review.

## Slices

1. Temporal dispatch + recency scoring (AC1, part of AC3).
2. Cue dispatch + cue scoring across operators (AC2, rest of AC3).
3. Gate sweep + regression check (AC4).

## Changelog

- 2026-07-01 — spec opened.
