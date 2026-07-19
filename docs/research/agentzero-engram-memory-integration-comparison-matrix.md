# Agent Zero -> Engram memory integration feasibility

> Discipline: applied (practitioner-pattern survey)

## Question

Can the concepts from `~/projects/agentzero`'s API-exposed memory, knowledge,
wiki, belief, and hierarchy layer be brought into Engram while keeping Engram's
Rust library, ontology model, and API contracts pristine?

## Short verdict

**Feasible, but the stable contract should be Agent Zero's API/data surface, not
a new Engram-facing product surface.** Engram already has the right conceptual
slots:
`MemoryRecord`, `KnowledgeSource`/`SourceDocument`/`KnowledgeChunk`,
`KnowledgeGraph`, `Ontology`, `ConceptScheme`, `Belief`, `Contradiction`,
`HierarchyNode`, and the `RetrievalIndex -> RetrievalFusion -> ContextComposer`
read-path seam. The clean path is to add a minimal `zbot-engram-adapter` crate
inside the Agent Zero side of the integration. That adapter preserves Agent
Zero's existing routes, DTOs, settings, UI expectations, and store trait shapes
while translating calls to Engram library operations behind the scenes.

Do **not** change Engram's core layer just to look like Agent Zero. Keep Engram
contract-first and reusable. The compatibility burden should sit in the adapter:
Agent Zero remains the host/application contract; Engram is a backing provider
that can be selected without changing what the Agent Zero UI and API clients
receive.

The second-pass code review changes the architecture recommendation: Agent Zero
should continue to own scheduling, settings persistence, and UI-facing HTTP
routes. Engram should expose deterministic Rust library operations and typed
configuration objects that Agent Zero can call from its existing gateway
scheduler. That preserves Agent Zero's product workflows while preventing
Engram from inheriting a gateway-shaped god service. It also keeps the migration
experience seamless: switching to Engram should be a backing-store/provider
change from Agent Zero's perspective, not a new API for callers to learn.

For the belief-network and bitemporal cutover slice, the detailed applied
research and implementation contract live in
[`zbot-engram-belief-bitemporal-cutover.md`](zbot-engram-belief-bitemporal-cutover.md);
the belief/bitemporal capability is covered in
[`docs/product/engram.md`](../product/engram.md).
The current implementation note is that Agent Zero exposes valid-time `as_of`
behavior and Engram's shipped belief SQLite adapter now supports valid-time
belief reads. The remaining temporal gap is record-time history: Engram rejects
record-time belief history for the current SQLite store rather than pretending
current rows are a full bitemporal audit log.

The broader build-and-integrate contract for Agent Zero is documented in
[`agentzero-engram-adapter-integration`](../product/engram.md).
It treats the belief/bitemporal work as a specialist slice and adds the
provider-selection, store coverage, recall, hierarchy, migration dry-run,
capability-reporting, API/UI parity, and rollout gates needed for Agent Zero to
actually run its memory jobs through Engram.

## Evidence base

### Local primary sources

- Agent Zero memory explainer: `~/projects/agentzero/docs/memory-explained.md`.
  It describes the live layers: fragments, knowledge graph, belief network,
  hierarchical memory, bi-temporal intervals, hybrid recall, and sleep-cycle
  maintenance.
- Agent Zero gateway routes: `~/projects/agentzero/gateway/src/http/mod.rs`.
  The live route table exposes memory, memory search, consolidation, procedures,
  beliefs, contradictions, and hierarchy stats.
- Agent Zero OpenAPI spec:
  `~/projects/agentzero/gateway/src/http/openapi.yaml`. It declares OpenAPI
  3.0.3 but currently does not cover the live memory/belief/hierarchy routes.
- Agent Zero domain and trait crates:
  `stores/zbot-stores-domain/src/*`,
  `stores/zbot-stores-traits/src/*`, and
  `stores/zbot-stores/src/knowledge_graph.rs`.
- Agent Zero memory gateway code:
  `gateway/gateway-memory/src/lib.rs`,
  `gateway/gateway-memory/src/services.rs`,
  `gateway/gateway-memory/src/sleep/worker.rs`,
  `gateway/gateway-memory/src/recall/{scored_item,mmr,query_gate}.rs`, and
  `gateway/gateway-memory/src/sleep/hierarchy_builder.rs`.
- Agent Zero UI and transport code:
  `apps/ui/src/features/memory/command-deck/*`,
  `apps/ui/src/features/observatory*/**`,
  `apps/ui/src/services/transport/{types,http}.ts`,
  `gateway/src/http/{memory_search,ward_content,settings}.rs`, and
  `gateway/gateway-services/src/settings.rs`.
- Engram domain and architecture:
  `docs/domain-data-model.md`, `docs/architecture/reference.md`,
  `core/domain/src/{memory,knowledge,ontology,belief,hierarchy,retrieval}.rs`,
  and `core/{memory,knowledge,belief,hierarchy,retrieval}/src/*.rs`.
- Engram research architecture direction:
  `docs/research/architecture-design-v2.md`,
  `docs/research/memory-knowledge-architecture.md`, and
  `docs/research/synthesis.md`.

### External standards

- OpenAPI 3.1 defines API descriptions around `paths`, `components`, and
  `webhooks`, and is the better target for generated HTTP contracts than
  Agent Zero's current hand-maintained 3.0.3 YAML.
  Source:
  [OpenAPI Specification v3.1.0](https://spec.openapis.org/oas/v3.1.0.html),
  lines 202-206 and 305.
- JSON Schema draft 2020-12 is the current validation baseline used by modern
  schema tooling, with vocabularies and bundling support.
  Source:
  [JSON Schema draft 2020-12](https://json-schema.org/draft/2020-12),
  lines 58-109.
- OWL 2 provides a formal ontology model with classes, properties, individuals,
  axioms, imports, and version IRIs; it also permits relational database-backed
  ontology documents.
  Source:
  [W3C OWL 2 structural specification](https://www.w3.org/TR/owl2-syntax/),
  lines 38-40, 201-205, 417-436.
- RDF 1.1 models graph data as subject-predicate-object triples, grouped into
  graphs/datasets. This supports Engram's ontology/property-graph bridge without
  forcing an RDF store.
  Source:
  [W3C RDF 1.1 Concepts](https://www.w3.org/TR/rdf11-concepts/), lines 29-30
  and 82-87.

## Findings

### F0. Code-first update: Agent Zero should schedule; the adapter should translate

**Confidence: high.** Agent Zero's current memory layer has two separate
configuration surfaces:

- `RecallConfig` owns retrieval policy: category weights, ward affinity,
  vector/BM25 weights, max facts/episodes, graph traversal, temporal decay,
  predictive recall, session offload, and KG decay.
- `MemorySettings` owns gateway/user settings: query gate, belief network, MMR,
  hierarchy, procedure recommendation, and worker interval knobs.

The scheduler is not a library concern in the current Agent Zero code. The
gateway builds `MemoryServicesConfig`, constructs `SleepOps`, starts
`SleepTimeWorker`, and exposes `trigger()` for on-demand consolidation. The
worker runs one ordered maintenance cycle and tolerates partial failures.
Hierarchy also has its own interval throttle inside `HierarchyBuilder`.

For the adapter-first integration, the clean contract is:

- Agent Zero owns timers, UI settings persistence, manual triggers, and
  operational enable/disable switches.
- `zbot-engram-adapter` implements or wraps Agent Zero memory/knowledge/wiki/
  belief/hierarchy store traits and DTO mappers.
- Engram remains a pure backing library: `recall`, `compose_context`,
  `plan_maintenance`, `run_maintenance_step`, `build_hierarchy`,
  `synthesize_beliefs`, `detect_contradictions`, and `apply_decay`.
- The adapter converts Agent Zero settings into Engram policy structs; Engram
  does not read Agent Zero settings files, spawn background workers by default,
  or expose Agent Zero HTTP routes from its core library.

This is the most important god-class prevention rule. Copying Agent Zero's
`MemoryServices` shape into Engram would couple construction, dependencies,
feature flags, scheduling, LLM provider wiring, maintenance ordering, and
observatory metrics. Engram should instead expose small operation traits;
`zbot-engram-adapter` should be the only place that knows both worlds.

### F1. Agent Zero's concepts map cleanly to Engram's model, but names should be translated

**Confidence: high.** Agent Zero has fragments, KG entities/relationships,
beliefs, contradictions, hierarchy, wiki articles, procedures, session episodes,
and goals. Engram already separates memory, knowledge, belief, hierarchy,
policy, provenance, ontology, taxonomy, retrieval, and evaluation.

| Agent Zero concept | Engram target | Fit | Notes |
|---|---|---:|---|
| `memory_facts` fragments | `MemoryRecord` + optional `MemoryAssertion` | High | Category maps to `MemoryKind`, assertion predicate/object, or migration metadata. Do not carry `memory_facts` as a table concept. |
| `category` weights | Retrieval config / ranking profile | High | The category taxonomy is useful, but weights are retrieval policy, not record identity. |
| `valid_from` / `valid_until` | `MemoryAssertion.validFrom/validUntil`, `Belief.validFrom/validUntil`, `QueryFilter.since/until` | High | For flat fact records, Engram may need optional top-level `MemoryRecord.validFrom/validUntil` or a rule that facts must use assertions. |
| `ward_id` / `partition_id` | `Scope.workspace`, `Scope.subject`, possibly `Scope.environment` | High | Translate the term; do not import "ward" into the portable model. |
| `kg_entities` / `kg_relationships` | `KnowledgeEntity` / `KnowledgeRelationship` | High | Entity/relationship type strings should be governed by `OntologyClass` and `OntologyProperty`. |
| `kg_beliefs` | `Belief` | High | Engram's belief contract is stronger because it has typed sources, policy, provenance, status, and embedding refs. |
| `kg_belief_contradictions` | `Contradiction` | High | Engram already generalizes contradictions beyond belief pairs. |
| hierarchy columns on KG rows | `HierarchyNode`, `HierarchyMembership`, `HierarchyRelation` | High | Engram's hierarchy model is cleaner because hierarchy is not baked into KG entity rows. |
| wiki articles | `KnowledgeSource` + `SourceDocument` + `KnowledgeChunk` | Moderate-high | Treat wiki as source-grounded curated knowledge, not agent memory. Add a `SourceKind::Generated` or metadata tag for curated wiki if needed. |
| procedures | `MemoryRecord(kind=procedure)` now; possible future `Procedure` extension | Moderate | Agent Zero's procedure record has execution stats and parameterized steps. Engram can carry it as structured memory first, then promote a typed extension after evaluation. |
| session episodes | `MemoryRecord(kind=episode)` + provenance/evidence | Moderate-high | Episode summaries are memory; raw conversation logs remain external evidence, not core memory. |
| goals | Out of current Engram memory core unless modeled as memory/procedure task state | Low-moderate | Goal tracking is valuable, but it is orchestration state. Keep it outside core unless an ADR adds goal contracts. |

### F2. Engram APIs can be generated, but Agent Zero compatibility APIs must remain stable

**Confidence: high.** Agent Zero exposes useful memory routes in code, but its
OpenAPI file is stale for the memory surface: the route table includes
`/api/memory`, `/api/memory/search`, `/api/memory/consolidate`,
`/api/procedures/dedupe`, `/api/beliefs/*`, `/api/contradictions/*`, and
hierarchy stats, while `openapi.yaml` is a broader 3.0.3 gateway spec that does
not document those paths.

For Engram as a standalone library/product, generated APIs should still be a
projection of Engram operations:

- `WriteMemoryRequest` / `WriteMemoryResponse`
- `RetrievalRequest` / `ContextPayload`
- `ForgetRequest` / `ForgetResult`
- ontology/taxonomy/knowledge graph upsert and validation payloads
- belief and contradiction read/resolve payloads once promoted

For Agent Zero integration, though, the compatibility target is the existing
Agent Zero API and data shape. `zbot-engram-adapter` should make those endpoints
and UI DTOs continue to work when the backing provider is Engram. That keeps
Agent Zero callers stable while letting Engram keep its own generated contracts.

### F3. Ontology integration is the strongest "bring it in" candidate

**Confidence: high.** Agent Zero stores entity and relationship type strings
directly. Engram already has `Ontology`, `OntologyClass`, `OntologyProperty`,
`OntologyAxiom`, advisory `validate_graph`, and a durable SQLite ontology
repository. This is the right place to make Agent Zero concepts pristine:

1. Define an Agent Zero compatibility ontology as data, not code.
2. Map `entity_type` to `OntologyClass`.
3. Map `relationship_type` to `OntologyProperty`.
4. Preserve original type strings in provenance or metadata during migration.
5. Use `validate_graph` to report undeclared predicates before any enforcing
   validation is introduced.

This aligns with OWL/RDF practice: ontologies can be versioned resources, and
RDF-style triples can be represented without forcing an RDF database.

### F4. Agent Zero's sleep cycle is valuable, but it must become focused Engram operations

**Confidence: moderate-high.** Agent Zero's hourly maintenance covers
compaction, synthesis, pattern extraction, corrections abstraction, conflict
resolution, decay, orphan archival, pruning, belief synthesis, contradiction
detection, propagation, and hierarchy aggregation. Engram should not copy that
as one broad service or factory.

Recommended split:

- `engram-memory`: write/retrieve/forget memory records and lifecycle events.
- `engram-knowledge`: source ingestion, chunks, graph, ontology, taxonomy, and
  knowledge graph validation.
- `engram-retrieval`: RRF, MMR, query-gate decisions, graph traversal hints, and
  context composition. Retrieval policy is config, not persisted record truth.
- `engram-belief`: belief synthesis, contradiction detection, and propagation.
- `engram-hierarchy`: hierarchy build/navigation and hierarchy read models.
- `engram-eval`: fixtures for recall quality, leakage, belief, hierarchy, and
  consolidation regressions.
- adapters: persistence, vector indexes, embeddings, model providers, Agent
  Zero compatibility, and UI/API gateways.

This avoids god classes. Agent Zero's `MemoryServices` factory is acceptable as
a gateway composition root in its repo. `zbot-engram-adapter` may participate in
that composition root, but it should stay a translation layer, not become a new
memory runtime that duplicates Engram or Agent Zero behavior.

### F5. Migration is feasible, but direct DB compatibility is not the right first target

**Confidence: moderate.** Agent Zero's SQLite schema is operational and rich,
but it is not Engram's contract. Directly reading `knowledge.db` can work for
one migration adapter, but the stable integration target should be a typed
export/import contract:

- source: Agent Zero DB or API
- transform: compatibility mapper
- target: Engram Rust service / repository ports
- validation: generated JSON Schema + Engram eval fixtures

The first adapter can be read-only and batch-oriented. Live dual-write or
bidirectional sync should wait until idempotency, conflict policy, and
delete/forget semantics are specified.

### F6. Agent Zero's UI adds contract requirements, not domain ownership

**Confidence: high.** The Memory tab is a product workflow over multiple memory
and knowledge read models. It lists wards, fetches a ward content snapshot, runs
unified hybrid search over facts/wiki/procedures/episodes, writes facts into the
active ward, and exposes belief and contradiction sub-tabs for the selected
partition. The Observatory and Graph pages separately read graph counts,
entities, relationships, belief-network stats, hierarchy stats, and live recall
trace overlays.

Engram should preserve these semantics without importing UI vocabulary as domain
truth:

- `ward_id` maps to Engram `Scope.workspace` or a compatibility
  `ScopeDimension`, not a portable domain term.
- session-local facts map to `Scope.session` plus provenance; global facts map
  to a root/shared scope and visibility policy.
- Memory tab content snapshots are read models that join memory, wiki,
  procedure, and episode projections. They are API/adapter DTOs, not
  `engram-domain` records.
- Observatory graph data is a read model over `KnowledgeGraphRepository`,
  `HierarchyRepository`, belief stats, and retrieval telemetry. It should not
  force graph/hierarchy fields into `MemoryRecord`.
- The UI's "type chips" filter content types; it should not become a domain
  "scope" concept.

### F7. Plain graph, hierarchy, and observability must remain separate surfaces

**Confidence: high.** Agent Zero has both plain graph relationships
(`kg_entities` / `kg_relationships`) and hierarchical graph relationships
(aggregate entities, parent cluster ids, inter-cluster relations). The
Observatory consumes both, plus telemetry counters and recent activity. Engram's
model is cleaner because it already separates knowledge graph, hierarchy, and
retrieval explanation concepts.

The integration should keep three ports:

- `KnowledgeGraphRepository`: typed entities and relationships, ontology
  validation, graph traversal, graph stats.
- `HierarchyRepository` / `HierarchyBuilder`: hierarchy nodes, memberships,
  aggregate summaries, lowest-common-ancestor paths, inter-cluster relations.
- `MemoryObservabilityPort`: cycle stats, recall trace events, health counters,
  and read-only dashboards.

Do not merge these into `MemoryService`. A graph can describe world knowledge;
a hierarchy can organize graph concepts at multiple abstraction levels; an
observability stream describes system behavior. They have different reasons to
change.

## Feasibility matrix

| Track | Feasibility | Risk | Recommendation |
|---|---:|---:|---|
| Agent Zero concept preservation at adapter boundary | High | Medium | Proceed by RFC/spec. Preserve Agent Zero semantics through mapping fixtures; do not start by changing Engram domain. |
| Agent Zero API/data compatibility through `zbot-engram-adapter` | High | Medium-high | Primary path. Preserve Agent Zero routes, DTOs, settings, scopes, and UI read models while swapping the backing provider. |
| Agent Zero DB migration | Medium | Medium-high | Build one read-only migration adapter with conformance fixtures. Do not make DB shape canonical. |
| Ontology/type normalization | High | Low-medium | Start here. It improves cleanliness without requiring runtime behavior changes. |
| Wiki integration | Medium-high | Medium | Model as source-grounded knowledge documents/chunks, not as memory facts. |
| Procedure learning | Medium | Medium-high | Initially encode as `MemoryRecord(kind=procedure)` with structured content; add typed procedure contract later only if evals justify it. |
| Sleep/consolidation import | Medium-high | High | Rebuild as explicit Engram operations with dry-run/eval gates. Agent Zero owns scheduling and manual triggers. |
| Recall policy import: RRF, MMR, query gate, graph traversal | High | Medium | Keep as typed retrieval policy config. Engram computes; Agent Zero supplies config and LLM/provider adapters. |
| Memory settings/UI integration | High | Medium | Keep settings in Agent Zero gateway/UI; `zbot-engram-adapter` translates into Engram policy structs at call time. |
| Belief/contradiction import | High | Medium | Use existing Engram belief contracts; classify compatibility before freezing. |
| Hierarchy import | High | Medium | Use Engram hierarchy records rather than KG `parent_cluster_id` columns. |
| Observatory/graph UI read models | High | Medium | Build adapter DTOs over graph, hierarchy, belief, and telemetry ports. Do not move dashboards into core. |

## Proposed integration architecture

```text
Agent Zero UI / API / scheduler / settings
        |
        v
zbot-engram-adapter
  - implements Agent Zero store/provider traits
  - preserves Agent Zero DTOs and route behavior
  - translates Agent Zero settings into Engram policy structs
  - maps Agent Zero terms through compatibility ontology
  - calls Engram library operations
  - records original IDs in provenance/metadata
        |
        v
Engram Rust library
  - MemoryService
  - KnowledgeRepository / KnowledgeGraphRepository
  - OntologyRepository / TaxonomyRepository
  - BeliefRepository
  - HierarchyRepository
  - RetrievalIndex / RetrievalFusion / ContextComposer
  - Belief operations
  - Hierarchy operations
  - Maintenance operations
        |
        v
Engram adapters
  - SQLite stores
  - sqlite-vec retrieval
  - future HTTP/API/Node transports
```

Boundary rules:

- The compatibility adapter may know Agent Zero's fields, routes, store traits,
  UI DTOs, and Engram library contracts.
- `engram-domain` must not know Agent Zero, wards, REST paths, SQLite table
  names, UI tabs, model providers, or gateway settings.
- Standalone Engram API contracts should be generated from Engram contracts;
  Agent Zero-compatible HTTP endpoints should remain Agent Zero endpoints backed
  by the adapter.
- Ontology and taxonomy data are portable contract data, not provider prompts.
- Retrieval scoring knobs are configuration, not record fields.
- Scheduling stays outside the Engram core library. Engram may expose cycle
  policies and operation budgets; it should not own Agent Zero's timer loop.
- Observatory/dashboard DTOs are read models over ports, not canonical domain
  structs.

## Proposed configuration contract

Agent Zero has useful knobs, but Engram should reshape them into policy structs
with narrow ownership:

```rust
pub struct EngramRuntimePolicy {
    pub recall: RecallPolicy,
    pub retrieval: RetrievalPolicy,
    pub graph: GraphPolicy,
    pub belief: BeliefPolicy,
    pub hierarchy: HierarchyPolicy,
    pub maintenance: MaintenancePolicy,
}

pub struct RecallPolicy {
    pub category_weights: BTreeMap<MemoryKind, f64>,
    pub scope_boosts: ScopeBoosts,
    pub max_context_tokens: usize,
    pub max_memory_records: usize,
    pub max_episodes: usize,
    pub min_score: f64,
    pub high_confidence_threshold: f64,
    pub contradiction_penalty: f64,
}

pub struct RetrievalPolicy {
    pub fusion: FusionPolicy,      // RRF k, source budgets, source weights
    pub diversity: DiversityPolicy, // MMR enabled, lambda, candidate pool
    pub query_gate: QueryGatePolicy,
    pub graph_traversal: GraphTraversalPolicy,
}

pub struct MaintenancePolicy {
    pub compaction: CompactionPolicy,
    pub decay: DecayPolicy,
    pub contradiction_propagation: PropagationPolicy,
    pub budgets: CycleBudgets,
}
```

Scheduling metadata should stay outside this core policy. Agent Zero should
continue to persist `interval_hours`, `enabled`, and UI feature flags in gateway
settings. `zbot-engram-adapter` converts those settings into operation policy
only at the call boundary. If a reusable non-Agent-Zero host wants hints, expose
a separate optional `MaintenanceScheduleHint`, not a background worker.

The important split is:

- **Config in Engram:** deterministic algorithm knobs, budgets, thresholds,
  source weights, and validation behavior.
- **Config in Agent Zero:** when to run, which UI features are visible, which
  LLM provider/model to use, which stores are wired, and whether a worker is
  enabled in that product.

## Adapter mapping issues to settle before any Engram delta

The default answer should be "map in `zbot-engram-adapter`." Engram core deltas
should be proposed only after fixtures prove an Agent Zero behavior cannot be
represented without losing semantics.

0. **Belief valid-time reads:** Agent Zero's `BeliefStore::get_belief` is a
   valid-time `as_of` query. Engram's current belief SQLite adapter implements
   valid-time reads over `valid_from` / `valid_until` and rejects record-time
   history because it stores current rows, not historical versions. Keep
   AgentZero API projection, ID translation, embedding-byte compatibility, and
   scheduler integration in `zbot-engram-adapter`; propose a new Engram
   repository extension only if fixtures prove the library still cannot preserve
   a required behavior.
1. **Fact truth interval:** map Agent Zero `valid_from` / `valid_until` into
   Engram assertions or adapter metadata first. Do not add top-level Engram
   memory fields unless compatibility fixtures prove assertion mapping is
   insufficient.
2. **Superseded memory state:** Agent Zero has `superseded`; Engram memory has
   `active`, `archived`, `redacted`, `forgotten`, and `expired`. Prefer modeling
   supersession as adapter metadata plus a `MemoryLink` / lifecycle event first.
   Add a new Engram status only through a later compatibility review.
3. **Curated wiki source kind:** map wiki articles to source documents/chunks
   plus metadata for now. Add an Engram source kind only if non-Agent-Zero
   consumers need to branch on it.
4. **Procedure pattern contract:** map procedures into structured memory content
   initially. Promote a native Engram `Procedure` only after retrieval/eval shows
   stable semantics across hosts.
5. **Compatibility ontology:** add an Agent Zero ontology fixture as data used
   by the adapter. This should not change Rust types.

## God-class avoidance rules for the Agent Zero port

1. **No `MemoryManager` mega-struct.** Public service structs may compose
   dependencies, but operation behavior belongs in focused modules:
   `write`, `retrieve`, `forget`, `fusion`, `graph_traversal`,
   `belief_synthesis`, `contradiction_detection`, `hierarchy_build`,
   `maintenance_plan`, and `observability`.
2. **No scheduler in `engram-domain` or behavior crates.** Agent Zero's
   `SleepTimeWorker` shape belongs to a host adapter. Engram can expose
   `run_cycle(policy, deps, now)` and `run_step(step, deps, now)`.
3. **No API DTOs in domain truth.** Memory tab, ward content, graph snapshot,
   hierarchy stats, and belief-network panels are adapter/read-model contracts.
   In the Agent Zero integration, preserve these DTOs exactly where practical.
4. **No table-name vocabulary in contracts.** Keep `memory_facts`,
   `kg_entities`, `kg_relationships`, and `kg_beliefs` in migration metadata
   only.
5. **No mixed graph/hierarchy repository.** Plain entity relationships and
   hierarchy memberships/inter-cluster relations need separate ports even if the
   SQLite adapter stores them together.
6. **No provider-specific LLM calls in core.** Query gate, belief synthesis,
   hierarchy labels, and contradiction judging consume traits; Agent Zero
   supplies model/provider adapters.
7. **No hidden global defaults.** Every algorithm knob imported from Agent Zero
   must be explicit in typed policy defaults and covered by fixture tests.

## Implementation sequence

1. **Spec: `zbot-engram-adapter`.**
   Define the compatibility objective, accepted mappings, non-goals, and
   fixtures. Include a small Agent Zero sample export with facts, wiki,
   entities, beliefs, contradictions, hierarchy, recall settings, and a memory
   tab ward-content snapshot. The core acceptance criterion is that Agent Zero
   UI/API consumers receive the same shape when the backing provider is Engram.
2. **Map scopes and settings before code.**
   Write the exact translation for `agent` / `shared` / `ward`, session-local
   facts, global facts, `ward_id`, partition ids, category weights, RRF, MMR,
   query gate, belief network, hierarchy, and maintenance budgets.
3. **Ontology fixture first.**
   Create `contracts`/`examples` or `docs/specs` fixtures for Agent Zero entity
   classes and relationship properties, then validate an imported graph through
   `OntologyRepository::validate_graph`.
4. **Read-only importer.**
   Implement an adapter that maps Agent Zero export JSON into Engram contracts.
   Avoid live API calls in the core; API access belongs in an adapter.
5. **Golden fixtures and evals.**
   Add tests that prove no concept collapses: memory facts stay memory,
   wiki stays knowledge, graph stays graph, beliefs stay derived, hierarchy
   stays hierarchy, settings become policy, and provenance survives.
6. **Minimal Agent Zero host adapter.**
   Wire Agent Zero's scheduler and UI settings to Engram operations without
   moving scheduling into Engram. The adapter owns provider/store construction,
   implements the Agent Zero-facing store/provider traits, and turns gateway
   settings into `EngramRuntimePolicy`.
7. **Optional standalone Engram API package.**
   After Rust library integration is stable, expose native Engram HTTP routes or
   TypeScript wrappers for non-Agent-Zero hosts. Do not require Agent Zero users
   to move to those APIs.
8. **Only then consider live sync.**
   Live sync needs ADR coverage for idempotency, conflict resolution, retention,
   and delete/forget propagation.

## Known unknowns

- **Known-unknown:** exact Agent Zero memory tool schema. The docs describe
  `memory(action=...)`, but I did not find a single authoritative tool-schema
  artifact in this pass. Would be closed by tracing the runtime tool registry
  and generated tool schemas.
- **Known-unknown:** whether the two dirty Agent Zero files in the local tree
  affect memory behavior. The repo currently has uncommitted edits in
  `gateway/gateway-execution/src/middleware/intent_analysis.rs` and
  `gateway/gateway-execution/src/runner/invoke_bootstrap.rs`; I treated them as
  user work and did not edit or revert them.
- **Known-unknown:** migration volume and data quality. Feasibility of direct DB
  migration depends on real `knowledge.db` cardinality, malformed rows, and
  whether embeddings are reproducible.
- **Known-unknown:** exact UI shape for session-local versus global fact writes.
  The code clearly carries `MemoryScope`, `ward_id`, `session_id`, and
  ward-content workflows, but the active Memory tab write path I inspected writes
  ward-scoped facts. The full global/session write UX should be traced before
  freezing the compatibility contract.
- **Unknowable from static review:** whether Agent Zero's recall ranking
  weights outperform Engram's current RRF/fusion defaults on Engram workloads.
  This requires shared evaluation fixtures and benchmark runs.

## Final recommendation

Proceed, but make the first deliverable an RFC/spec for `zbot-engram-adapter`,
plus a compatibility ontology and policy-mapping contract. The architecture
should say:

> Agent Zero is the stable host/API/data contract for this integration. Engram
> remains the backing Rust library and contract model. `zbot-engram-adapter`
> is the minimal layer that lets Agent Zero keep its scheduling, settings, UI,
> API routes, store traits, and read models while delegating storage/retrieval/
> consolidation work to Engram.

That lets Engram absorb the best ideas: category-prioritized memory, visible
provenance, bi-temporal facts, wiki-as-knowledge, procedures, belief
contradictions, hierarchy navigation, and sleep-style consolidation. It avoids
the main failure mode: changing Engram to mimic Agent Zero, or changing Agent
Zero callers to understand Engram-native contracts before the adapter proves the
swap is seamless.

It also implements the direction in `docs/research/architecture-design-v2.md`,
`docs/research/memory-knowledge-architecture.md`, and
`docs/research/synthesis.md`: memory and knowledge remain distinct but
composable; retrieval is the joining layer; graph, hierarchy, taxonomy, belief,
policy, provenance, and evaluation stay independently evolvable.
