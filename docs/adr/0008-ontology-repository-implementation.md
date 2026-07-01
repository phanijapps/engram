# ADR 0008: OntologyRepository durable implementation

## Status

Accepted

## Context

`OntologyRepository` is declared as a port in `engram-knowledge`
(`core/knowledge/src/lib.rs`) and its domain types live in
`core/domain/src/ontology.rs` (`Ontology`, `OntologyClass`, `OntologyProperty`,
`OntologyAxiom`, `OntologyValidationFinding`, …). Until now the port had **no
durable implementation**: `engram-store-knowledge-sqlite` implemented
`KnowledgeRepository`, `KnowledgeGraphRepository`, and `TaxonomyRepository`, but
not `OntologyRepository`. The ontology vocabulary was therefore unreachable from
TypeScript and unpersisted.

RFC 0003 deliberately deferred ontology as a non-goal for the demo. RFC 0004
Decision D3 reverses that: the enterprise knowledge-platform demo needs an
IT-organization ontology to govern the knowledge graph (classes such as Team,
Service, Runbook, Incident; properties such as `owns`, `depends_on`,
`responds_to`; axioms). Implementing a previously-deferred, frozen non-goal is a
recorded decision an ADR must capture.

The N-API binding surface extension follows the pattern established in
[ADR-0007](0007-napi-binding-surface-extension.md): the binding stays a JSON
transport over Rust behavior, TypeScript owns ergonomics.

## Decision

Implement `OntologyRepository` for `SqlKnowledgeStore` in
`engram-store-knowledge-sqlite`, mirroring the existing `TaxonomyRepository`
wiring:

- **Schema** — four new tables (`ontologies`, `ontology_classes`,
  `ontology_properties`, `ontology_axioms`) following the established
  table-per-record + `record_json` pattern. The parent `ontologies` row carries
  scope columns; classes/properties/axioms inherit visibility from their owning
  ontology (mirroring how concepts inherit from a concept scheme). Idempotent
  `CREATE TABLE IF NOT EXISTS` migrations.
- **Port methods** — `put_ontology`, `get_ontology`, `put_class`, `put_property`,
  `put_axiom` follow the taxonomy upsert/lookup/scope-filter patterns exactly.
- **`validate_graph` is advisory only.** It returns `OntologyValidationFinding`
  records (a warning for each relationship whose predicate is not declared as an
  ontology property by label or URI) but **never rejects a write** — the port
  contract says validation is advisory unless an adapter or policy chooses to
  reject. Enforced validation is explicitly deferred.
- **Binding** — `NativeKnowledgeEngine` exposes `putOntologyJson` /
  `getOntologyJson` / `putClassJson` / `putPropertyJson` / `putAxiomJson` /
  `validateGraphJson`, wrapped by `NativeKnowledgeTransport` and surfaced as
  `/ontology/*` demo routes. The transport stays `unknown`-typed, exactly like
  taxonomy — no hand-authored ontology contract; Rust remains the single source
  of truth for the shapes.

No `OntologyRepository` port signature or `core/domain` ontology type is
changed. Ontology persistence lives nowhere except `engram-store-knowledge-sqlite`.

## Consequences

- The ontology port is now durable, reachable from TypeScript, and browsable in
  the demo UI; an IT-org sample ontology + taxonomy can govern an ingested graph.
- `validate_graph` gives advisory feedback without blocking ingestion, so a graph
  can carry relationships the ontology does not yet declare — useful while an
  ontology is evolving, and consistent with the advisory contract.
- `engram-store-knowledge-sqlite` gains a regular `chrono` dependency (was
  dev-only) because `validate_graph` stamps findings with a `Timestamp`.
- Enforced (write-rejecting) ontology validation, generated typed ontology
  contracts, and ontology import resolution remain deliberately out of scope and
  would require a follow-up decision.
