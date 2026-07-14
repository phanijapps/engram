# Spec: context-packet-contract-additions

- **Status:** Draft
- **Owner:** phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** RFC-0013, ADR-0025 (framework/content boundary), ADR-0009 (retrieval-composition seam), ADR-0022 (engine neutrality), ADR-0003 (implementation-stack gate)
- **Brief:** none
- **Contract:** `contracts/v1/schemas/engram-v1.schema.json` (the `RetrievalTargetType` enum is the only frozen-v1 surface touched) + `contracts/v1/compatibility.md` (the enum-add tolerance note); the three new structs are draft-extension, documented in `docs/domain-data-model.md`
- **Shape:** data

> **Spec contract:** this document defines what "done" means. The implementing
> PR must match this spec, or update it. Verification must be derivable from it.

## Objective

The context-graph packet layer (RFC-0013) rests on four framework contract types that do not yet exist: `ContextSubgraph` (a connected-subgraph packet shape), `ApplicabilityRule` (a condition→target governing rule), `DecisionTrace` (a candidate-only agent decision record), and an `ontologyClassRefs` link on `KnowledgeEntity` (entity→ontology-class typing). Phase 1 adds these four as **contract surface only** — the types compile, the v1 schema regenerates cleanly, the generated TypeScript stays in sync, and `RetrievalTargetType` gains the variants that make rules and traces first-class packet members. No behavior is wired: `ContextSubgraph` is not emitted by `compose_context`, no `ApplicabilityRule`/`DecisionTrace` writer is implemented, and no `DecisionTrace.promote` method ships (the never-auto-promote invariant is documented; the method lands in Phase 4). Success is a downstream spec being able to reference these types against a green workspace gate set (`cargo check`, `contracts:check-generated`, `check-contracts.sh`, `check-engine-neutrality.sh`).

## Boundaries

### Always do

- Add the three new structs as **draft-extension** types (Rust + `docs/domain-data-model.md`), mirroring `KnowledgeEntity`'s draft-extension status — they are **not** added to the frozen v1 schema.
- Follow the existing serde pattern for every new struct/field: `#[serde(skip_serializing_if = "Vec::is_empty", default)]` for optional Vecs, `skip_serializing_if = "Option::is_none"` for Options, camelCase at the contract boundary.
- Update every `RetrievalTargetType` match site when adding variants (1 exhaustive + 2 wildcards — see AC5).
- Run the full gate set before claiming done: `cargo fmt --all`, `cargo check --workspace`, `cargo clippy --workspace --all-targets`, `contracts:check-generated`, `check-contracts.sh`, `check-engine-neutrality.sh`, `check-docs.sh`.

### Ask first

- Any frozen-v1 schema change beyond the `RetrievalTargetType` enum (e.g. adding the new structs as v1 `$defs`) — that broadens the contract-freeze decision (RFC-0013 Q2).
- Renaming or removing an existing `EntityKind` or `RetrievalTargetType` variant (breaking) — out of scope; flag instead.
- Adding a writer/repository port, or the `DecisionTrace.promote` method, for `ApplicabilityRule` or `DecisionTrace` — that is Phase 2+ wiring, not Phase 1.

### Never do

- Wire `ContextSubgraph` into `compose_context` or emit it from any adapter (Phase 3).
- Implement the `ApplicabilityRule` writer surface or the `DecisionTrace.promote` method (Phase 2 / Phase 4).
- Add domain ontology **content** to `core/domain` (ADR-0025) — only mechanism types ship here.
- Add a model/LLM/embedding dependency to `engram-domain` (ADR-0022) — the agentic stage is a Phase 2 adapter, never core.
- Hand-edit `packages/contracts/src/generated/` — regenerate via `contracts:generate`.

## Testing Strategy

- **New domain types — TDD.** Serde round-trip tests (Rust ↔ JSON) for each new struct (`ContextSubgraph`, `ApplicabilityRule`, `DecisionTrace`) and for the new `KnowledgeEntity.ontology_class_refs` field: construct, serialize, deserialize, assert equality; assert optional fields `skip_serializing_if` empty/none. Mirrors the existing round-trip conformance pattern.
- **Frozen-enum + generated contract — goal-based check.** `contracts:check-generated` passes (schema regenerates, generated TS in sync) and the exhaustive-`match` site compiles with the new arms.
- **Engine neutrality — goal-based check.** `check-engine-neutrality.sh` passes (the new types in `core/domain`, including its `retrieval` module, carry no engine symbols/SQL).
- **No-wiring invariant — goal-based check.** `cargo check --workspace` green and a grep confirms `ContextSubgraph` is not constructed inside `compose_context` or any adapter; no `ApplicabilityRule`/`DecisionTrace` writer call sites and no `DecisionTrace.promote` method exist.

## Acceptance Criteria

- [ ] `ContextSubgraph` struct exists in `core/domain/src/retrieval.rs`, carrying `nodes` (`RetrievalResult[]` — the included set) + `edges` (`KnowledgeRelationship[]`) + `omitted` (`OmittedResult[]` — the excluded set) + `budget` (`Option<ContextBudget>`), mirroring `ContextPayload`'s `items`/`omitted` split; serde round-trip test green.
- [ ] `ApplicabilityRule` struct exists in a new `core/domain/src/rule.rs` module, with `{ id, condition, target (EntityRef|ConceptRef), binding, scope, policy, provenance, validFrom, validUntil, createdAt }`; serde round-trip green; re-exported from `core/domain/src/lib.rs`.
- [ ] `DecisionTrace` struct exists in a new `core/domain/src/trace.rs` module, with `{ id, scope, agent (Actor), items_consulted, traversal_path, policy_applied, precedent, output, provenance, createdAt }`; serde round-trip green; re-exported. **No `promote` method ships in Phase 1** — the never-auto-promote invariant is documented in `domain-data-model.md`; the `promote(actor)` method is deferred to Phase 4.
- [ ] `KnowledgeEntity.ontology_class_refs: Vec<OntologyClassId>` (optional, `skip_serializing_if = "Vec::is_empty"`) added; existing `KnowledgeEntity` tests still pass; round-trip green.
- [ ] `RetrievalTargetType` gains `Rule`, `Policy`, `Axiom`, `DecisionTrace` variants; `contracts/v1/schemas/engram-v1.schema.json` enum updated; `contracts:check-generated` passes.
- [ ] The one exhaustive match on `RetrievalTargetType` (`core/eval/src/lib.rs:194`) gains arms for the new variants (compile-required); the two wildcard sites on `RetrievalTargetType` — `adapters/knowledge/sqlite/src/retrieval.rs:166` (slugs unknown → `"item"`) and `adapters/integration/src/fixtures/recall.rs:560` (parses unknown → `Memory`) — gain explicit arms for correctness. (`core/belief/src/contradiction.rs:50` matches `ContradictionTargetType` and `adapters/retrieval/sqlite-vec/src/index.rs:417` matches `EmbeddingTargetType`; both are unaffected.)
- [ ] `docs/domain-data-model.md` documents the four additions + the `RetrievalTargetType` extension + the `DecisionTrace` never-auto-promote invariant (ApplicabilityRule under the Policy and Provenance area).
- [ ] `contracts/v1/compatibility.md` carries the enum-add tolerance note (consumers must tolerate unknown `RetrievalTargetType` values).
- [ ] Gates green: `cargo fmt --all`, `cargo check --workspace`, `cargo clippy --workspace --all-targets`, `contracts:check-generated`, `check-contracts.sh`, `check-engine-neutrality.sh`, `check-docs.sh`.
- [ ] No wiring introduced: `ContextSubgraph` is not emitted by `compose_context`; no `ApplicabilityRule`/`DecisionTrace` writer and no `DecisionTrace.promote` method; grep confirms zero new call sites wiring the types into behavior.

## Assumptions

- Technical: target types live in `core/domain/src/{retrieval.rs,knowledge.rs}` — `ContextPayload`:297, `RetrievalTargetType`:109 (11 variants), `KnowledgeEntity`:203. (source: `core/domain/src/retrieval.rs`, `core/domain/src/knowledge.rs`)
- Technical: `KnowledgeEntity` already has `concept_refs` (`Vec<ConceptRef>`, `skip_serializing_if = "Vec::is_empty"`) + `valid_from`/`valid_until` + `metadata`; `ontology_class_refs` follows the identical optional-Vec pattern. (source: `core/domain/src/knowledge.rs:215`)
- Technical: `contracts/v1/schemas/engram-v1.schema.json` is the hand-authored source-of-truth schema; `contracts:generate` produces `packages/contracts/src/generated/` TS, gated by `contracts:check-generated` (`git diff --exit-code`). (source: `package.json:16-17`)
- Technical: `RetrievalTargetType` is a closed 11-variant enum; adding variants requires arms at 1 exhaustive match (`core/eval/src/lib.rs:194` — compile-required) and explicit handling at 2 wildcard sites (`adapters/knowledge/sqlite/src/retrieval.rs:166`, `adapters/integration/src/fixtures/recall.rs:560`) for correctness; 2 other `target_type` matches (`core/belief/src/contradiction.rs:50` on `ContradictionTargetType`, `adapters/retrieval/sqlite-vec/src/index.rs:417` on `EmbeddingTargetType`) are unaffected. (source: repo grep + arm inspection 2026-07-13)
- Technical: `KnowledgeEntity` is draft-extension (absent from frozen v1 schema + First Contract Slice) → the field addition needs no v1 schema regen; by precedent the three new structs are also draft-extension. (source: `contracts/v1/schemas/engram-v1.schema.json`, `docs/domain-data-model.md`)
- Technical: `contracts/v1/compatibility.md:7-8` requires enum-add changes to state that consumers must tolerate unknown values for that enum — the tolerance note is mandatory, not conditional. (source: `contracts/v1/compatibility.md`)
- Process: contract changes gate via the `engram-contract` skill + `check-contracts.sh` + `check-engine-neutrality.sh`; spec lifecycle Draft→Implementing→Shipped. (source: `AGENTS.md`, `docs/CONVENTIONS.md §4`)
- Process: spec-mode adversarial review, single pass. (source: user preference — lighter-adversarial-review-in-loops)
- Product: Phase 1 ships contract surface only — no composition/population/writer/promote wiring. (source: user confirmation 2026-07-13)
- Product: the new `RetrievalTargetType` variants are `Rule`, `Policy`, `Axiom`, `DecisionTrace`. (source: user confirmation 2026-07-13)
- Product: the three new structs are draft-extension (Rust + `docs/domain-data-model.md`); the only frozen-v1 schema edit is the `RetrievalTargetType` enum. (source: user confirmation 2026-07-13)
