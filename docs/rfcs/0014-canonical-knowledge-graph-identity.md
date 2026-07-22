# RFC-0014: Canonical knowledge-graph identity and consolidation

- **Status:** Draft
- **Author:** AgentZero integration findings; Engram maintainers TBD
- **Approver:** TBD
- **Date opened:** 2026-07-21
- **Date closed:**
- **Decision weight:** heavy
- **Related:** RFC-0009 (knowledge-graph retraction and convergence), ADR-0022 (engine neutrality), `docs/domain-data-model.md`, AgentZero RFC-0012 (Engram upstream risk reduction)

## Reviewer brief

- **Decision:** Add storage-neutral, caller-policy-driven identity operations
  for graph entities and exact relationships, plus transactional duplicate
  consolidation.
- **Recommended outcome:** accept the capability boundary and implement it in
  phases; keep existing ID-only puts compatible.
- **Why now:** an Engram-backed AgentZero installation produced durable entity
  duplicates for case variants and visually redundant graph connections. The
  host bug is fixable locally, but Engram currently offers no atomic
  resolve-or-create or safe consolidation primitive that can prevent or repair
  the generic failure class.
- **Affected surface:** `core/domain`, `core/knowledge`, the SQLite knowledge
  adapter, integration facade/capability reporting, N-API/TypeScript surfaces,
  contracts, and cross-adapter fixtures.
- **Stakes:** costly-to-reverse. Identity and merge semantics become a public
  contract, SQLite gains new identity indexes or tables, and consolidation
  rewrites graph references.
- **Review focus:** identity boundaries, merge safety, compatibility with
  ID-only writes, concurrent convergence, and the division between exact
  relationship identity and ontology-defined semantic equivalence.
- **Not in scope:** AgentZero wards, agents, REST DTOs, domain synonym tables,
  automatic embedding-based merges, or graph visualization.

## The ask

Approve six additive capabilities:

| ID | Decision | Recommendation |
| --- | --- | --- |
| D1 | Entity identity | Add caller-selected, scope-aware identity modes: ID-only, stable key, and scoped kind plus normalized name. |
| D2 | Write semantics | Add atomic `resolve_or_put_entity` with structured `Created`, `Matched`, or `Merged` outcomes. |
| D3 | Consolidation | Add dry-run duplicate discovery and transactional entity consolidation with relationship redirection and audit results. |
| D4 | Relationship identity | Define exact identity by scope, graph, canonical endpoints, and caller-canonicalized predicate. |
| D5 | Compatibility | Preserve existing ID-based `put_entity` and `put_relationship`; normalized identity is explicit, not universal. |
| D6 | Portability | Ship cross-adapter fixtures and expose supported operations consistently through the Rust facade, capability report, and N-API/TypeScript surface. |

Recommended defaults if the RFC is accepted:

- Identity never crosses scope or kind.
- Graph is included in identity unless the caller explicitly chooses otherwise.
- Unicode normalization and case folding are generic, deterministic, and
  versioned; domain synonyms remain caller policy.
- Different predicates and directions remain distinct unless a caller-supplied
  ontology policy canonicalizes them before identity evaluation.
- Existing ID-only callers keep their current behavior.

## Incident evidence

The live AgentZero Engram sidecar contained 313 active entities and 441 active
relationships at inspection time.

Three identities had two active entity IDs each:

| Host scope | Kind | Stored names | Copies |
| --- | --- | --- | ---: |
| `ward:software-engineering` | `project` | `FastIndex`, `fastindex` | 2 |
| `root` | `concept` | `MCP Server`, `MCP server` | 2 |
| `builder-agent` | `organization` | `Uber`, `UBER` | 2 |

The relationship table had no duplicate rows for the exact directed key
`(scope, source ID, target ID, normalized predicate)`. It did have 49 unordered
entity pairs with parallel relationships: 128 lines in total, 79 more lines
than a one-line-per-pair visualization, and up to six lines for one pair.
Examples included inverse or overlapping predicates such as
`contains`/`part_of`, `uses`/`usedin`, and
`characterin`/`characterof`/`mentions`/`instance_of`.

Chrome confirmed the data path: the API returned each durable ID and
relationship, and the visualization faithfully rendered each record. The UI
was not duplicating one DOM datum.

The immediate AgentZero failure chain was:

```text
case or surface-form variant
  -> host exact-name fallback compares display names case-sensitively
  -> lookup misses the existing logical identity
  -> adapter persists a fresh caller-generated ID
  -> API and graph render both durable records
```

AgentZero must fix that fallback. Engram's broader gap is that the knowledge
contract and SQLite adapter accept ID-based puts but do not offer an atomic,
portable identity operation or a safe repair primitive. A host-side
probe-then-put remains race-prone even after the case comparison is corrected.

## Problem and goals

`KnowledgeEntity` already carries `name`, `aliases`, `kind`, `scope`, optional
`graph_id`, provenance, and source references. These are sufficient inputs for
a caller-defined identity policy. `KnowledgeRepository::put_entity`, however,
is keyed by record ID, and the SQLite adapter applies `ON CONFLICT(id)`. Two
callers can persist two IDs for one logical identity without violating an
Engram contract or database constraint.

The same applies to relationships: `put_relationship` updates by relationship
ID. There is no portable exact identity contract spanning scope, graph,
endpoints, and predicate.

Goals:

1. Make logical identity explicit and storage-neutral.
2. Make resolve-and-write atomic within each adapter's supported consistency
   boundary.
3. Preserve names that legitimately collide across scopes, graphs, kinds, or
   stable keys.
4. Repair known duplicate IDs without private database access or dangling
   graph references.
5. Deduplicate exact relationships without treating arbitrary predicates as
   semantic synonyms.
6. Preserve provenance, evidence, aliases, temporal fields, and conflicts.
7. Keep existing ID-based behavior compatible.

Non-goals:

- No universal claim that display names are globally unique.
- No AgentZero ward, session, agent, API, or UI types in Engram contracts.
- No hard-coded financial, literary, software, or other domain vocabulary.
- No automatic semantic merge based only on embedding similarity.
- No visualization edge bundling in Engram.
- No automatic mutation of an AgentZero user's database.

## Proposal

### D1: Explicit entity identity policy

Add a storage-neutral identity input capable of expressing:

- scope boundary
- optional graph boundary
- entity kind
- normalized name or caller-supplied stable key
- optional alias matching
- normalization policy version

The default remains ID-only. A normalized-name policy is opt-in and must not
merge across scope, graph, or kind unless the caller explicitly selects a
broader boundary.

Normalization must be deterministic and versioned. Case folding, Unicode
normalization, whitespace handling, and stable separator rules are reasonable
generic primitives. Honorifics, tickers, abbreviations, ontology equivalence,
and domain synonyms remain caller policy.

### D2: Atomic resolve-or-create

Expose a focused repository or service operation equivalent to:

```rust
resolve_or_put_entity(candidate, identity_policy, merge_policy)
    -> EntityWriteOutcome::{Created, Matched, Merged}
```

Required behavior:

- Resolution and write occur in one adapter transaction or equivalent atomic
  backend boundary.
- Concurrent writes for one identity converge on one canonical ID.
- The outcome always returns the canonical entity and ID.
- Retries are idempotent.
- Aliases, source references, concept/ontology references, provenance,
  temporal fields, and metadata merge deterministically.
- Conflicting values are reported rather than silently discarded.

This operation should live on a focused identity port if adding resolution,
merge policy, collision discovery, and audit behavior would make
`KnowledgeRepository` a god trait.

### D3: Transactional entity consolidation

Expose two maintenance operations:

1. Dry-run collision discovery under a declared identity policy.
2. Transactional consolidation of explicitly selected duplicate IDs into a
   canonical entity.

Consolidation must:

- verify canonical and duplicate IDs against the requested scope
- merge entity data using the declared policy
- redirect inbound and outbound relationship endpoints
- handle self-loops deterministically
- coalesce exact relationships created by redirection
- preserve evidence and provenance from every contributing record
- preserve earliest creation and latest update/observation timestamps
- delete or tombstone losing records according to explicit policy
- return remaps, counts, conflicts, and an audit identifier
- roll back as a unit and remain idempotent when repeated

### D4: Exact relationship identity

Define a portable exact relationship key from:

- scope
- graph boundary
- canonical subject ID
- canonical object ID
- canonical predicate

Provide atomic resolve-or-put and deterministic merge semantics for evidence,
provenance, confidence, and temporal fields.

Engram must not infer that arbitrary predicates are synonymous. Predicate
equivalence and inverse rules belong to an ontology or caller policy. Engram
may expose a canonicalization hook; the caller can then map, for example,
`used_by` to inverse `uses` before the exact key is evaluated.

### D5: Compatibility

Keep current `put_entity` and `put_relationship` behavior available as ID-only
writes. The new operations are additive. Existing records do not become
implicitly merged during schema migration.

When an identity index is introduced over an existing database, collisions
must be reported for explicit dry-run and consolidation rather than resolved by
arbitrary row order.

### D6: Cross-adapter and transport parity

Identity behavior is a port contract, not a SQLite feature. Add reusable
fixtures that every supporting adapter can run. SQLite may use normalized
columns, hashes, unique indexes, or a dedicated identity table; those are
adapter details.

Once the capability is public, expose it through `EngramProvider`, capability
reporting, N-API, generated contracts, and the TypeScript client in accordance
with Engram's surface-parity rule. Staging core and SQLite work first is fine;
the capability is not fully shipped until supported transports agree.

## Illustrative contract shape

Names are illustrative and should be aligned with existing Engram conventions
during specification.

```rust
pub enum EntityIdentityMode {
    IdOnly,
    StableKey(String),
    ScopedKindAndNormalizedName {
        normalization_version: String,
        include_graph: bool,
        match_aliases: bool,
    },
}

pub struct EntityWriteRequest {
    pub entity: KnowledgeEntity,
    pub identity: EntityIdentityMode,
    pub merge_policy: EntityMergePolicy,
}

pub enum EntityWriteOutcome {
    Created { entity: KnowledgeEntity },
    Matched { entity: KnowledgeEntity },
    Merged {
        entity: KnowledgeEntity,
        changed_fields: Vec<String>,
        conflicts: Vec<EntityMergeConflict>,
    },
}

pub struct EntityMergeRequest {
    pub canonical_id: EntityId,
    pub duplicate_ids: Vec<EntityId>,
    pub scope: Scope,
    pub policy: EntityMergePolicy,
}
```

The contract specifies behavior and outcomes. SQLite locking, generated
columns, hashes, constraints, and transaction strategy remain adapter-private.

## Ownership boundary

| Concern | Engram | Host application |
| --- | --- | --- |
| Atomic identity resolution | Own | Consume |
| Storage-neutral identity and merge contracts | Own | Configure |
| SQLite uniqueness and transactions | Own | Do not access directly |
| Cross-adapter fixtures | Own | Run integration parity tests |
| Host scope mapping | No | Own |
| Domain synonym rules | No | Own |
| Ontology predicate aliases and inverses | Provide policy seam | Supply policy |
| Host database repair rollout | Provide safe primitive | Authorize and operate |
| Host APIs and DTOs | No | Own |
| Graph visualization | No | Own |

## Implementation plan

### E0: Settle identity and compatibility contracts

**Depends on:** RFC acceptance

**Likely areas:** `docs/domain-data-model.md`, `docs/specs/`,
`core/domain/src/knowledge.rs`, `contracts/v1/`

Define identity modes, normalization versioning, exact relationship identity,
merge policy, conflict records, consolidation audit outcomes, and compatibility
classification. Decide whether the behavior extends `KnowledgeRepository` or
uses focused identity and consolidation ports.

Tests: serde round trips, generated contract reproducibility, compatibility
fixtures, documentation checks.

### E1: Add pure normalization and merge behavior

**Depends on:** E0

**Likely areas:** a focused `core/knowledge/src/identity.rs`, repository port
exports, and `core/knowledge/tests/`

Implement deterministic versioned normalization and pure merge functions for
aliases, evidence, references, provenance, timestamps, and metadata. Surface
conflicts. Keep SQL, locks, and backend policy outside core.

Tests: case folding, Unicode normalization, whitespace, aliases, stable keys,
scope/kind/graph separation, deterministic ordering, conflicts, and
idempotency.

### E2: Implement atomic SQLite entity identity

**Depends on:** E1

**Likely areas:** `adapters/sqlite/src/knowledge/schema.rs`, a focused
`knowledge/identity.rs`, and `adapters/sqlite/tests/knowledge_identity.rs`

Add indexed identity columns or a dedicated identity table. Backfill identity
material without auto-selecting winners for existing collisions. Perform
resolve, insert/update, and outcome construction transactionally. Use a
uniqueness constraint with conflict retry or equivalent SQLite semantics so
concurrent writers converge. Preserve lossless `record_json`.

Tests: fresh create, repeated match, case variant, stable-key rename, separate
scope/graph/kind, concurrent writers, existing collision report, rollback, and
reopen.

### E3: Implement atomic exact relationship identity

**Depends on:** E2

**Likely areas:** core identity contracts, SQLite identity implementation, and
the same focused integration suite.

Enforce the exact canonical relationship key and merge evidence, provenance,
confidence, and timestamps deterministically. Preserve different predicates
and opposite directions unless caller policy canonicalizes them.

Tests: repeat edge, canonical predicate spelling, caller-provided inverse map,
distinct predicates, opposite directions, concurrent writes, and idempotency.

### E4: Implement transactional consolidation

**Depends on:** E2-E3

**Likely areas:** a focused knowledge consolidation port/module, SQLite
consolidation implementation, and `knowledge_consolidation.rs` tests.

Add dry-run collision discovery and explicit apply. Merge entity data, redirect
endpoints, coalesce exact edges, preserve provenance, enforce scope, and return
an audit result inside one transaction.

Tests: inbound/outbound/self-loop redirection, relationship coalescing,
evidence and provenance preservation, conflicting metadata, cross-scope
rejection, missing IDs, rollback, dry run, repeated apply, and reopen
integrity.

### E5: Wire facade, capability, and transport surfaces

**Depends on:** E2-E4

**Likely areas:** `core/integration/`, `adapters/integration/`,
`bindings/node/`, `packages/contracts/`, and `packages/client/`

Expose typed handles and capability-report entries without leaking raw adapter
handles. Add equivalent N-API and TypeScript operations with stable typed error
mapping.

Tests: capability/handle agreement, unsupported backend behavior, Rust and
TypeScript parity, and generated contracts.

### E6: Add cross-adapter conformance fixtures

**Depends on:** E2-E5

**Likely areas:** `adapters/integration/src/fixtures/knowledge.rs`,
`core/integration/src/sqlite/conformance.rs`, and future backend fixture
runners.

Cover entity identity, exact relationship identity, concurrent convergence
where supported, dry-run discovery, consolidation integrity, and unchanged
ID-only puts.

### H0: Host adoption and repair

**Depends on:** an Engram revision containing E2-E6 or an explicitly documented
temporary waiver

Hosts should replace local probe-then-put logic with Engram identity outcomes,
remap relationship endpoints to canonical IDs, dry-run collision discovery,
authorize consolidation explicitly, and preserve their public API contracts.

For AgentZero specifically, the adoption also includes fixing its
case-sensitive exact-name fallback and routing both individual and bulk graph
ingestion through the new operation. AgentZero separately owns ontology
predicate mappings and Observatory edge bundling.

## Dependency context

```text
domain identity contract
  -> pure normalization and merge policy
    -> SQLite atomic entity identity
      -> exact relationship identity
        -> transactional consolidation
          -> integration facade + capability report
            -> N-API / TypeScript parity
              -> host adoption and database repair

host ontology policy
  -> predicate canonicalization before exact relationship identity

host API payload
  -> host visualization edge bundling
```

## Acceptance criteria

- Case variants under the same scope, graph, kind, and selected normalized-name
  policy converge on one canonical entity.
- Identical names in different scopes, graphs, kinds, or stable keys remain
  distinct.
- Concurrent resolve-or-create requests converge on one canonical ID.
- Stable-key identity joins renamed entities without relying on display names.
- Entity consolidation redirects every relationship endpoint and leaves no
  dangling references.
- Relationships made identical by redirection are coalesced with evidence and
  provenance preserved.
- Different predicates remain distinct unless caller ontology policy
  canonicalizes them.
- Resolve, write, dry-run, and consolidation operations are idempotent.
- Existing ID-only callers and stored records retain their current behavior.
- SQLite and reusable cross-adapter fixtures pass.
- Capability reporting and supported public transports agree.

## Risks and controls

- **False merges:** require scope and kind, include graph by default, and offer
  stable keys. Normalized names are opt-in.
- **Normalization drift:** persist a normalization version and test upgrades as
  migrations.
- **Lost provenance:** union evidence and provenance, report conflicts, and
  transact the entire consolidation.
- **Concurrent duplicates:** database uniqueness is required; application-level
  probe-then-insert is insufficient.
- **Over-collapsed relationships:** Engram deduplicates only an exact canonical
  predicate; semantic equivalence requires explicit caller policy.
- **Breaking consumers:** retain ID-only puts and introduce new operations
  additively.
- **Transport asymmetry:** capability reporting and parity tests gate release.

## Verification

Focused suites should run before the repository-wide gates:

```bash
.codex/hooks/pre-implementation-check.sh
cargo test -p engram-knowledge
cargo test -p engram-store-knowledge-sqlite
cargo test -p engram-integration
cargo fmt --all --check
cargo check --workspace
pnpm run contracts:generate
pnpm run typecheck
pnpm run test
.codex/hooks/check-contracts.sh
.codex/hooks/check-docs.sh
.codex/hooks/check-engine-neutrality.sh
```

Host integration verification should repeat the incident conditions:

1. Ingest case variants repeatedly and concurrently.
2. Query logical collision groups from the active store.
3. Compare durable counts with host API counts.
4. Inspect repaired relationships for dangling references and provenance loss.
5. Confirm the host graph shows one logical node and inspectable multi-edges.

## Open questions

1. Should identity and consolidation extend `KnowledgeRepository` or use a
   focused port? Recommended: focused port unless the accepted contract remains
   limited to resolve-or-put alone.
2. Should normalized identity material live on entity rows or in a dedicated
   identity table? Recommended: decide in the SQLite spec after migration and
   concurrency tests; keep it out of the public contract.
3. Should consolidation auditing reuse general Engram consolidation records or
   introduce a knowledge-specific audit result? Recommended: reuse shared audit
   primitives only if they can represent endpoint remaps and merge conflicts
   without weakening either model.
4. At what stage is N-API/TypeScript parity required? Recommended: core and
   SQLite may land behind an explicitly incomplete capability, but the feature
   cannot be marked shipped until parity is complete.

## Follow-on artifacts

- An Engram feature spec after RFC acceptance, decomposed along E0-E6.
- An ADR recording the chosen identity boundary and compatibility policy.
- A separate AgentZero adoption spec covering adapter use, authorized database
  repair, and Observatory edge presentation.
