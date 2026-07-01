# Plan: ontology-it-org (RFC 0004 Slice 3 / PHASE60)

Implement the `OntologyRepository` port end-to-end, mirroring the existing
`TaxonomyRepository` wiring. Shipped in two commits: **A** capability (Rust +
binding + backend, API-testable), **B** UX + IT-org sample.

## Tasks

### T1 — ADR-0008 (ontology reversal)
- **Tests:** `.codex/hooks/check-docs.sh`.
- **Depends on:** none
- **Approach:** Write `docs/adr/0008-ontology-repository-implementation.md` following ADR-0007's format: context (RFC 0003 deferred ontology; RFC 0004 D3 reverses), decision (implement `OntologyRepository` in `engram-store-knowledge-sqlite`, advisory `validate_graph`, transport `unknown`-typed), consequences, conformance to ADR-0007 binding-extension pattern. Update `docs/adr/README.md` if indexed.

### T2 — SQLite OntologyRepository impl (+ tests) [Commit A]
- **Tests (TDD):** `adapters/knowledge/sqlite/tests/repository.rs` — put+get round-trips for ontology/class/property/axiom; `get_ontology` scope filtering; `validate_graph` warning on undeclared predicate + empty on conforming graph.
- **Depends on:** T1
- **Approach:** `schema.rs`: add `ontologies` (id + scope cols + record_json, mirrors `concept_schemes`), `ontology_classes`/`ontology_properties`/`ontology_axioms` (id + ontology_id + record_json, mirror `concepts`) + indexes. `service.rs`: impl `OntologyRepository` on `SqlKnowledgeStore` — `put_ontology`/`get_ontology` mirror `put_concept_scheme`/`get_concept_scheme`; `put_class`/`put_property`/`put_axiom` mirror `put_concept`; `validate_graph` loads the ontology's properties + classes, loads the graph's entities + relationships (via existing `KnowledgeGraphRepository`), emits a warning finding per relationship whose predicate matches no property label/uri, info per entity whose kind matches no class label — advisory, never rejects. Add `OntologyRepository` to the crate's impl + any `impl ... for SqlKnowledgeStore` aggregate.

### T3 — N-API binding + TS transport [Commit A]
- **Tests:** goal-based — `cargo check`; rebuild native module; backend typecheck.
- **Depends on:** T2
- **Approach:** `bindings/node/src/lib.rs`: add `#[napi]` `put_ontology_json`/`get_ontology_json`/`put_class_json`/`put_property_json`/`put_axiom_json`/`validate_graph_json` mirroring the taxonomy `*Json` methods (decode → `block_on(self.store.<method>)` → encode; `get`/`validate` take `{id|graphId+ontologyId, scope}`). `packages/node/src/binding.ts`: extend `NativeKnowledgeEngineBinding`. `packages/node/src/transport.ts`: add to `NativeKnowledgeTransport` interface + `JsonNativeKnowledgeTransport` impl (`putOntology`/`getOntology`/`putClass`/`putProperty`/`putAxiom`/`validateGraph`). Rebuild via `pnpm --filter @engram/node build:native && build`.

### T4 — Backend `/ontology/*` routes [Commit A]
- **Tests:** goal-based — typecheck + curl round-trip; manual.
- **Depends on:** T3
- **Approach:** `demo/backend/src/app.ts`: `/ontology/ontology`, `/ontology/class`, `/ontology/property`, `/ontology/axiom` (body → `putOntology`/`putClass`/`putProperty`/`putAxiom`), `/ontology/get` (`{id, scope}` → `getOntology`), `/ontology/validate` (`{graphId, ontologyId, scope}` → `validateGraph`). Thin pass-throughs, mirroring `/taxonomy/*`.

### T5 — OntologyPanel + IT-org sample [Commit B]
- **Tests:** goal-based — frontend typecheck + build; manual load.
- **Depends on:** T4
- **Approach:** `demo/frontend/src/OntologyPanel.tsx`: minimalist panel — "Load IT-org ontology" button (POSTs the sample via `/ontology/*`), then lists classes (label + parent classes) and properties (label + domain→range). `demo/backend/src/itOrgOntology.ts`: the IT-org sample as a TS fixture (ontology + classes: Team, Service, Runbook, Incident, Person, SRE; properties: owns, depends_on, responds_to, authored, member_of; a couple of axioms) + a service-tier/severity taxonomy, posted through the existing routes. Wire `OntologyPanel` into `App.tsx`. Add a "Validate graph" action that calls `/ontology/validate` against the last ingested graph and lists findings.

### T6 — Validate + lighter adversarial pass
- **Tests:** `cargo fmt --all && cargo check --workspace && cargo test -p engram-store-knowledge-sqlite`; rebuild native binding; backend + frontend typecheck/build; curl smoke; single-pass review focused on scope-handling, advisory-only validation, and boundary conformance.
- **Depends on:** T5

## Out of scope (logged)
- Enforced (write-rejecting) ontology validation; generated typed ontology contract; ontology imports resolution; hierarchy (deferred program-wide).
