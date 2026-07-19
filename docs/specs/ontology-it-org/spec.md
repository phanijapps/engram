# Spec: ontology-it-org (RFC 0004 Slice 3 / PHASE60)

- **Status:** Shipped
- **Shape:** mixed (data + service + ui)
- **Constrained by:** RFC-0004 D3 (ontology reversal) + ADR-0007 (N-API surface extension pattern); records **ADR-0008** (reversal of RFC 0003's ontology deferral)
- **Contract:** none (the `OntologyRepository` port + `core/domain/src/ontology.rs` types already exist; transport stays `unknown`-typed like taxonomy — no generated-contract change)

## Objective

An IT-organization ontology governs the demo knowledge graph: classes (Team, Service, Runbook, Incident, Person, …), properties (owns, depends_on, responds_to, authored), and axioms, plus a sample taxonomy of service tiers / incident severity. The `OntologyRepository` port — declared in `engram-knowledge` but never persisted — gets a durable SQLite implementation in `engram-store-knowledge-sqlite`, is exposed over the N-API binding and the demo backend, and is browsable in a minimalist enterprise UI panel. `validate_graph` runs as an advisory check (relationship predicates not declared as properties → warning; entity kinds not declared as classes → info); it never rejects writes. Taxonomy already works end-to-end and is reused as-is.

## Decision (aligns with RFC D3)

RFC 0003 deferred ontology as a non-goal. RFC 0004 D3 reverses that. This slice implements the reversal: a real `OntologyRepository` SQLite adapter (mirroring the existing `TaxonomyRepository` wiring), binding + transport exposure, demo routes, a UI panel, and an IT-org sample. ADR-0008 records the reversal.

## Assumptions

- Technical: the `OntologyRepository` port (`core/knowledge/src/lib.rs:108`) and all domain types (`core/domain/src/ontology.rs`) already exist and the knowledge crate compiles — no new domain types. (verified — `cargo check -p engram-knowledge` clean)
- Technical: `engram-store-knowledge-sqlite` already implements `KnowledgeGraphRepository` + `TaxonomyRepository`; ontology mirrors the taxonomy tables/methods (`concept_schemes`/`concepts`/`concept_relations` → `ontologies`/`ontology_classes`/`ontology_properties`/`ontology_axioms`), scope on the parent record, children inherit scope. (verified — Explore map of `adapters/knowledge/sqlite/`)
- Technical: the N-API binding (`bindings/node/src/lib.rs`) + TS transport (`packages/node/src/{binding,transport}.ts`) mirror the taxonomy `*Json` pattern; the native module rebuilds via `pnpm --filter @engram/node build:native`. (verified — build script present)
- Technical: `validate_graph` has no taxonomy analog; implemented as advisory only. (design choice)
- Process: lighter single-pass adversarial review. (user standing preference)

## Boundaries

**Always do**
- Mirror the taxonomy implementation shape exactly (table-per-record + `record_json` blob, idempotent `CREATE TABLE IF NOT EXISTS`, scope on the parent only, `ON CONFLICT(id) DO UPDATE` upsert).
- Keep `validate_graph` advisory — it returns findings, never rejects a write.
- Keep the transport `unknown`-typed (no hand-authored v1 contract for ontology); Rust remains the single source of truth for the shapes.
- Rebuild the native binding after Rust/binding changes and smoke-test the new routes.

**Ask first**
- Enforcing ontology constraints (rejecting writes on validation failure).
- Generating a typed ontology TS contract.

**Never do**
- Change the `OntologyRepository` port signature or the `core/domain` ontology types.
- Put ontology persistence anywhere except `engram-store-knowledge-sqlite`.
- Make `validate_graph` reject writes, or add a second graph DB / new top-level dependency.

## Testing Strategy

- **TDD (unit, Rust):** `adapters/knowledge/sqlite/tests/` — put + get round-trip for ontology/class/property/axiom; scope filtering on `get_ontology`; `validate_graph` returns a warning for an undeclared predicate and empty for a conforming graph.
- **Goal-based (build):** `cargo fmt --all && cargo check --workspace && cargo test -p engram-store-knowledge-sqlite`; rebuild native binding; backend + frontend `typecheck` + `build`.
- **Goal-based (plumbing, no creds):** `/ontology/*` routes round-trip via curl; the IT-org sample loads.
- **Manual QA:** load the IT-org sample, browse classes/properties in the OntologyPanel, run validate against an ingested graph.

## Acceptance Criteria

- [x] `OntologyRepository` is implemented in `engram-store-knowledge-sqlite` (put_ontology, put_class, put_property, put_axiom, get_ontology, validate_graph) with idempotent schema + Rust unit tests.
- [x] The N-API binding exposes `putOntology`/`getOntology`/`putClass`/`putProperty`/`putAxiom`/`validateGraph`; the TS transport wraps them; the native module rebuilds.
- [x] `/ontology/ontology`, `/ontology/class`, `/ontology/property`, `/ontology/axiom`, `/ontology/get`, `/ontology/validate` routes work via curl.
- [x] `validate_graph` is advisory (warnings/info only, never rejects).
- [x] An OntologyPanel browses classes + properties; an IT-org sample ontology + taxonomy loads from a fixture.
- [x] ADR-0008 records the ontology reversal; `cargo fmt/check/test` + backend/frontend typecheck/build pass.
