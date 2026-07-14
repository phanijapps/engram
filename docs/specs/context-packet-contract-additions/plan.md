# Plan: context-packet-contract-additions

- **Spec:** [`spec.md`](spec.md)
- **Status:** Drafting <!-- plan vocabulary: Drafting | Executing | Done (distinct from the spec's Draft|Implementing|Shipped) -->

> **Plan contract:** this is the implementation strategy. Unlike the spec, this
> document is allowed to change as you learn. When it changes substantially
> (a different approach, not just a re-ordering), note why in the changelog
> at the bottom.

## Approach

Additive domain-type additions plus one frozen-enum edit, in dependency order: first the three new draft-extension structs and the `KnowledgeEntity` field (low-risk, no v1 regen), then the `RetrievalTargetType` variants and the three `RetrievalTargetType` match-site updates (the one frozen-v1 touch), then the schema regen + compatibility note, then the domain-model documentation, then the full gate pass. The riskiest part is the frozen-enum touch — it must keep generated TypeScript in sync (`contracts:check-generated`) and update the one exhaustive match (`core/eval/src/lib.rs:194`, compile-required) plus the two wildcard sites for correctness. No behavior is wired: the types are inert contract surface until Phases 2–4.

## Constraints

- **RFC-0013** — the six deltas; this spec implements Phase 1 (the contract types for D1/D2/D3/D4).
- **ADR-0025** — framework/content boundary: only mechanism types in `core/domain`; no domain ontology content.
- **ADR-0009** — retrieval-composition seam is read-path only; `ContextSubgraph` is a payload shape, not a new store call.
- **ADR-0022** — engine neutrality: no engine symbols/SQL in `engram-domain`.
- **ADR-0003** — run `.codex/hooks/pre-implementation-check.sh` before implementation.
- `docs/domain-data-model.md` is the contract source of truth until Rust domain types are accepted as the generation source.

## Construction tests

**Integration tests:** none beyond per-task — Phase 1 is additive types with no cross-component behavior.
**Manual verification:** none — all checks are mechanical gates.

## Design (LLD)

### Design decisions

- `ContextSubgraph` is a **sibling payload view**, not a replacement of `ContextPayload.items` — `nodes` IS the included set and `omitted` is the excluded set, mirroring `ContextPayload.items`/`omitted`; Phase 3 wires emission. Traces to: AC1 · implements `core/domain/src/retrieval.rs`.
- `ApplicabilityRule` lives in a **new `core/domain/src/rule.rs` module** (not folded into `policy.rs` or `operations.rs`) — one responsibility per module (AGENTS.md). Traces to: AC2.
- `DecisionTrace` lives in a **new `core/domain/src/trace.rs` module**; it carries an `agent: Actor` field (the decision-maker). **No `promote` method ships in Phase 1** — the never-auto-promote invariant is documented in `domain-data-model.md`; the `promote(actor)` method lands in Phase 4. Traces to: AC3.
- `ontology_class_refs: Vec<OntologyClassId>` — reuses the existing `OntologyClassId` alias rather than introducing a new ref struct (minimal surface, follows the `concept_refs` precedent at the field level). Traces to: AC4.

### Data & schema

| Addition | Location | Shape | AC |
|---|---|---|---|
| `ContextSubgraph` | `core/domain/src/retrieval.rs` | `{ nodes: Vec<RetrievalResult>, edges: Vec<KnowledgeRelationship>, omitted: Vec<OmittedResult>, budget: Option<ContextBudget>, created_at: Timestamp }`; serde round-trip | AC1 |
| `ApplicabilityRule` | `core/domain/src/rule.rs` (new) | `{ id, condition, target (RuleTarget = EntityRef\|ConceptRef), binding: Option<String>, scope, policy, provenance, valid_from, valid_until, created_at }`; `skip_serializing_if` on optionals | AC2 |
| `DecisionTrace` | `core/domain/src/trace.rs` (new) | `{ id, scope, agent: Actor, items_consulted: Vec<EvidenceRef>, traversal_path: Vec<String>, policy_applied: Option<Policy>, precedent: Option<EvidenceRef>, output: String, provenance, created_at }` (no `promote` method in Phase 1) | AC3 |
| `KnowledgeEntity.ontology_class_refs` | `core/domain/src/knowledge.rs` | `Vec<OntologyClassId>`, `skip_serializing_if = "Vec::is_empty", default` | AC4 |
| `RetrievalTargetType` +4 variants | `core/domain/src/retrieval.rs` | `Rule`, `Policy`, `Axiom`, `DecisionTrace` (PascalCase ↔ snake_case schema) | AC5 |

All new structs: `#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]`; `KnowledgeEntity`-style `skip_serializing_if` discipline. Draft-extension — not added to frozen v1 `$defs`.

### Interfaces & contracts

- **`contracts/v1/schemas/engram-v1.schema.json`** — the `RetrievalTargetType` enum gains `"rule"`, `"policy"`, `"axiom"`, `"decision_trace"`. A `jsonschema` contract, edited directly (no OpenAPI-style authoring skill applies; the `engram-contract` skill governs the edit). Traces to: AC5, AC6 · implements `contracts/v1/schemas/engram-v1.schema.json`.
- **`contracts/v1/compatibility.md`** — the enum-add tolerance note (consumers must tolerate unknown `RetrievalTargetType` values), mandatory per `compatibility.md:7-8`. Traces to: AC8.
- **`docs/domain-data-model.md`** — the contract surface for draft-extension types: documents `ContextSubgraph`, `ApplicabilityRule`, `DecisionTrace`, `ontologyClassRefs`, the `RetrievalTargetType` extension, and the `DecisionTrace` never-auto-promote invariant. Traces to: AC7.

## Tasks

### T1: KnowledgeEntity.ontology_class_refs field

**Depends on:** none

**Tests:**
- Serde round-trip: construct a `KnowledgeEntity` with `ontology_class_refs`, serialize, deserialize, assert equality (verifies AC4).
- `skip_serializing_if`: an entity with empty `ontology_class_refs` serializes without the field.
- Existing `KnowledgeEntity` tests remain green (no regression).

**Approach:**
- Add `#[serde(skip_serializing_if = "Vec::is_empty", default)] pub ontology_class_refs: Vec<OntologyClassId>` to `KnowledgeEntity` in `core/domain/src/knowledge.rs` (after `concept_refs`).
- Confirm `OntologyClassId` is already in scope (from `OntologyClass`); if not, add the alias in `core/domain/src/ontology.rs`.
- Update any `KnowledgeEntity` constructors in tests/adapters to use `..Default` or explicit `ontology_class_refs: Vec::new()` where structs are built literally.

**Done when:** `cargo test -p engram-domain` green; the round-trip + skip-if-empty tests pass.

**Touches:** `core/domain/src/knowledge.rs`, `core/domain/src/ontology.rs` (if alias needed), `core/domain/tests/*`.

### T2: ApplicabilityRule struct + rule.rs module

**Depends on:** none

**Tests:**
- Serde round-trip for `ApplicabilityRule` (construct → serialize → deserialize → equality) (AC2).
- `skip_serializing_if` on optional fields (`binding`, `valid_from`, `valid_until`).
- The struct compiles standalone (no dependency on unwired behavior).

**Approach:**
- Create `core/domain/src/rule.rs` with `ApplicabilityRule` (`#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]`): `{ id: Id, condition: String, target: RuleTarget, binding: Option<String>, scope: Scope, policy: Policy, provenance: Provenance, valid_from: Option<Timestamp>, valid_until: Option<Timestamp>, created_at: Timestamp }`.
- Define `RuleTarget` as a small enum (`EntityRef(EntityRef) | ConceptRef(ConceptRef)`) so a rule can bind either a graph entity or a taxonomy concept.
- Add `mod rule;` + `pub use rule::{ApplicabilityRule, RuleTarget};` in `core/domain/src/lib.rs`.
- Mirror `KnowledgeEntity`'s serde discipline exactly.

**Done when:** `cargo test -p engram-domain` green; `ApplicabilityRule` + `RuleTarget` re-exported from the crate root.

**Touches:** `core/domain/src/rule.rs` (new), `core/domain/src/lib.rs`, `core/domain/tests/*`.

### T3: DecisionTrace struct + trace.rs module

**Depends on:** none

**Tests:**
- Serde round-trip for `DecisionTrace` (AC3).
- `skip_serializing_if` on optional fields (`policy_applied`, `precedent`).
- Confirm **no `promote` method** exists on the struct (grep `impl DecisionTrace` → only derives, no methods).

**Approach:**
- Create `core/domain/src/trace.rs` with `DecisionTrace`: `{ id: Id, scope: Scope, agent: Actor, items_consulted: Vec<EvidenceRef>, traversal_path: Vec<String>, policy_applied: Option<Policy>, precedent: Option<EvidenceRef>, output: String, provenance: Provenance, created_at: Timestamp }`.
- Do **not** add a `promote` method — the never-auto-promote invariant is documented in `domain-data-model.md` (T7); the `promote(actor)` method lands in Phase 4.
- Re-export from `core/domain/src/lib.rs`.

**Done when:** `cargo test -p engram-domain` green; `DecisionTrace` visible from the crate root with no methods.

**Touches:** `core/domain/src/trace.rs` (new), `core/domain/src/lib.rs`, `core/domain/tests/*`.

### T4: ContextSubgraph struct

**Depends on:** none

**Tests:**
- Serde round-trip for `ContextSubgraph` (AC1).
- An empty subgraph (no nodes/edges) round-trips; `omitted` and `budget` obey `skip_serializing_if`.
- Reuses existing `RetrievalResult`, `KnowledgeRelationship`, `OmittedResult`, `ContextBudget` types (no duplication).

**Approach:**
- Add `ContextSubgraph` to `core/domain/src/retrieval.rs` (alongside `ContextPayload`): `{ nodes: Vec<RetrievalResult>, edges: Vec<KnowledgeRelationship>, omitted: Vec<OmittedResult>, budget: Option<ContextBudget>, created_at: Timestamp }` — `nodes` is the included set, `omitted` the excluded set, mirroring `ContextPayload`.
- Re-export from `core/domain/src/lib.rs` (it's in the `retrieval` module already re-exported via `pub use retrieval::*`).
- Do **not** touch `compose_context` or `ContextComposer` (Phase 3).

**Done when:** `cargo test -p engram-domain` green; `ContextSubgraph` compiles and round-trips; `compose_context` unchanged.

**Touches:** `core/domain/src/retrieval.rs`, `core/domain/tests/*`.

### T5: RetrievalTargetType variants + match-site updates

**Depends on:** T2, T3 (the `Rule`/`DecisionTrace` variants land alongside their types for review coherence)

**Tests:**
- Serde round-trip: each new variant serializes to its snake_case string and back (AC5).
- `cargo check --workspace` green — proves the one exhaustive match (`core/eval/src/lib.rs:194`) now handles the new variants (AC6).
- Unit test on the `target_type_name` helper returns `"rule"`, `"policy"`, `"axiom"`, `"decision_trace"` for the new variants.
- The two wildcard sites return correct slugs/values for the new variants (not the fallback `"item"` / `Memory`).

**Approach:**
- Add `Rule`, `Policy`, `Axiom`, `DecisionTrace` to the `RetrievalTargetType` enum in `core/domain/src/retrieval.rs:109` (confirm the enum's `#[serde(rename_all = "snake_case")]` attr applies).
- **Exhaustive match (compile-required):** add arms to `core/eval/src/lib.rs:194` (`Rule => "rule"`, `Policy => "policy"`, `Axiom => "axiom"`, `DecisionTrace => "decision_trace"`).
- **Wildcard sites (correctness-required):** add explicit arms to `adapters/knowledge/sqlite/src/retrieval.rs:166` (so Rule/Policy/Axiom/DecisionTrace get a meaningful slug, not `"item"`) and add explicit string mappings to `adapters/integration/src/fixtures/recall.rs:560` (`"rule"`→`Rule`, etc., not the `Memory` fallback).
- Do **not** touch `core/belief/src/contradiction.rs:50` (`ContradictionTargetType`) or `adapters/retrieval/sqlite-vec/src/index.rs:417` (`EmbeddingTargetType`) — different enums, unaffected.

**Done when:** `cargo check --workspace` + `cargo test --workspace` green; all 3 `RetrievalTargetType` sites handle the new variants correctly.

**Touches:** `core/domain/src/retrieval.rs`, `core/eval/src/lib.rs`, `adapters/knowledge/sqlite/src/retrieval.rs`, `adapters/integration/src/fixtures/recall.rs`.

### T6: Frozen-v1 schema enum edit + contract regen + compatibility note

**Depends on:** T5

**Tests:**
- `contracts:check-generated` passes (schema regenerates; generated TS in sync) (AC5).
- The generated `RetrievalTargetType` in `packages/contracts/src/generated/types.schema.generated.json` includes the 4 new values.
- `contracts/v1/compatibility.md` carries the enum-add tolerance note (AC8).

**Approach:**
- Edit `contracts/v1/schemas/engram-v1.schema.json` `RetrievalTargetType` enum: add `"rule"`, `"policy"`, `"axiom"`, `"decision_trace"`.
- Add the tolerance note to `contracts/v1/compatibility.md` stating consumers must tolerate unknown `RetrievalTargetType` values (mandatory per `compatibility.md:7-8`).
- Run `pnpm run contracts:generate`; commit the regenerated `packages/contracts/src/generated/`.
- Invoke the `engram-contract` skill to govern the edit and confirm freeze-policy compliance.

**Done when:** `pnpm run contracts:check-generated` exits 0; generated TS reflects the 4 new values; compatibility.md carries the note.

**Touches:** `contracts/v1/schemas/engram-v1.schema.json`, `contracts/v1/compatibility.md`, `packages/contracts/src/generated/*`.

### T7: domain-data-model.md documentation

**Depends on:** T1, T2, T3, T4, T5

**Tests:**
- `check-docs.sh` passes (AC7).
- The four additions + the `RetrievalTargetType` extension + the `DecisionTrace` never-auto-promote invariant are documented under the correct model sections.

**Approach:**
- Add `ContextSubgraph` to the Retrieval Model section of `docs/domain-data-model.md`.
- Add `ApplicabilityRule` (+ `RuleTarget`) to a new "Applicability rules" subsection under **Policy and Provenance** (rules are governed records — scope/policy/provenance — that bind targets; the natural home).
- Add `DecisionTrace` to the Operations/Compatibility area; state the never-auto-promote invariant explicitly (promotion requires an explicit `Actor`; the `promote` method is Phase 4).
- Add `ontologyClassRefs` to the `KnowledgeEntity` field table.
- Add the 4 `RetrievalTargetType` values to the enum list.
- Invoke the `engram-contract` skill (domain-model integrity).

**Done when:** `check-docs.sh` green; a reader can find all five additions in `domain-data-model.md`.

**Touches:** `docs/domain-data-model.md`.

### T8: Full gate pass + no-wiring confirmation

**Depends on:** T1, T2, T3, T4, T5, T6, T7

**Tests:**
- All gates green: `cargo fmt --all`, `cargo check --workspace`, `cargo clippy --workspace --all-targets`, `contracts:check-generated`, `check-contracts.sh`, `check-engine-neutrality.sh`, `check-docs.sh` (AC9).
- No-wiring grep: `ContextSubgraph` is not constructed inside `compose_context` or any adapter; no `ApplicabilityRule`/`DecisionTrace` writer call sites and no `DecisionTrace::promote` method exist (AC10).

**Approach:**
- Run `.codex/hooks/pre-implementation-check.sh`, then the full gate set.
- Grep `rg -n "ContextSubgraph" core/retrieval/ adapters/` → expect zero hits (the type lives in `core/domain`, constructed only in tests). Grep `rg -n "fn promote" core/domain/src/trace.rs` → expect zero. Grep `rg -n "ApplicabilityRule|DecisionTrace" adapters/` → expect only test fixtures, no impl.

**Done when:** every gate exits 0; the no-wiring greps return zero implementation hits; spec `Status` → `Implementing` (then `Shipped` on merge).

**Touches:** none (verification only) — or trivial fixes if a gate surfaces drift.

## Rollout

- **Delivery:** additive library types, no flag, no migration. Reversible except the frozen-enum addition (forward-compatible; removal would require a v2 contract). Phases 2–4 wire behavior onto these types.
- **Infrastructure:** none — pure Rust + schema + docs.
- **External-system integration:** none.
- **Deployment sequencing:** schema regen + compatibility note (T6) lands after the Rust enum (T5) so generated TS never references a non-existent variant; docs (T7) land after the types are settled.

## Risks

- **Frozen-enum regen drift** — generated TS out of sync with the schema. Mitigated by `contracts:check-generated` (fails the gate).
- **A missed `RetrievalTargetType` match site** beyond the 3 found — mitigated by `cargo check --workspace` (the exhaustive match at `core/eval/src/lib.rs:194` is a compile error if unupdated); wildcard sites don't break compilation but the no-wiring/correctness grep in T8 surfaces drift.
- **`DecisionTrace` invariant under-specified** — mitigated by the `domain-data-model.md` doc (T7); the `promote(actor)` method + writer-surface enforcement lands in Phase 4.
- **`OntologyClassId` vs a new `OntologyClassRef`** — T1 prefers the minimal `Vec<OntologyClassId>`; if a ref struct is later needed (to carry an ontology id), it's an additive change.

## Changelog

- 2026-07-13: initial plan — Phase 1 of RFC-0013; four contract types + one frozen-enum touch, no wiring.
- 2026-07-13: spec-mode review folds — corrected the `RetrievalTargetType` match-site list (1 exhaustive + 2 wildcards; `contradiction.rs`/`sqlite-vec index.rs` are different enums, dropped); `ContextSubgraph` `included` field removed (nodes = included set, mirroring `ContextPayload`); `ApplicabilityRule`/`DecisionTrace` field sets reconciled with ACs; `DecisionTrace.promote` method dropped from Phase 1 (invariant documented, method deferred to Phase 4); `compatibility.md` tolerance note made mandatory; `ApplicabilityRule` doc home fixed to Policy and Provenance.
