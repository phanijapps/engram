# Plan: structured-repo-identity

- **Spec:** [`spec.md`](spec.md)
- **Status:** Drafting

> **Plan contract:** this is the implementation strategy. Unlike the spec, this
> document is allowed to change as you learn. When it changes substantially
> (a different approach, not just a re-ordering), note why in the changelog.

## Approach

Add a deterministic, SHA-free stable-source-key alongside the existing
(SHA-embedding) `source_id`, thread it through ingestion, and anchor it on each
per-document `KnowledgeGraph` (which already has a `metadata` map) rather than on
relationships (which have none). The key is derived once per source by
normalizing the git remote `detect_git` already captures; it and the document
`path` are stamped into the graph's `metadata` and lifted into indexed columns on
`knowledge_graphs`. Entities and relationships are attributed to a repo through
their `graph_id` — relationships already have a `graph_id` column; entities gain
one — so "records for a repo" is a graph-by-key lookup joined on `graph_id`. A
single `EntityKind::Repository` node per source, with `belongs_to` edges from each
document graph, gives the graph a repo anchor. The riskiest parts are (a)
commit-stable, lossless-enough normalization and (b) keeping it a metadata+column
change with zero domain-contract diff. Testing is TDD for normalization,
integration for the ingest→store attribution and the Repository node.

## Constraints

- **ADR-0017** — repo = `KnowledgeSource` + `EntityKind::Repository` node in one
  scope; normalized git remote is the key.
- **ADR-0018 / RFC-0009** — the stable-source-key must be SHA-free so the
  retraction reconciler can diff on `(stable-source-key, path)`.
- **AGENTS.md** — no domain-truth change from adapters; the key rides in
  `KnowledgeGraph.metadata` (open map) + adapter columns, not a new domain field.
  `KnowledgeRelationship` has no `metadata`, so relationships are attributed via
  `graph_id`, never a stamped field.

## Construction tests

**Integration tests:** (T4/T5/T6) ingest a git-backed fixture → assert graphs
carry the stable-source-key + path (metadata + columns), entities/relationships
carry `graph_id`, a query by key returns exactly that repo's graphs +
graph_id-joined records, `SourceKind::GitRepository` is set, and one
`EntityKind::Repository` node exists with `belongs_to` edges.
**Cross-cutting checks:** (T7) `pnpm run contracts:generate` yields no diff;
`docs/domain-data-model.md` unchanged (no domain field added).
**Manual verification:** none.

## Design (LLD)

Shape: `data`. Stack: Rust workspace — `adapters/ingest` (normalization, request
carrier, extraction stamping, Repository node), `adapters/knowledge/sqlite`
(columns + indexed lookup), `core/domain` types unchanged (key rides in
`Metadata`).

### Data & schema
- Additive columns on `knowledge_graphs`: `stable_source_key TEXT`, `path TEXT`,
  each indexed. Populated on write by lifting from `record_json.metadata`.
  Traces to: AC-1, AC-4 · none.
- Additive column `graph_id TEXT` + index on `knowledge_entities`
  (`knowledge_relationships` already has `graph_id`). Populated from the entity's
  `record_json.graph_id`. Traces to: AC-3 · none.
- Metadata keys on `KnowledgeGraph`: `stableSourceKey`, `path` (constants in the
  ingest crate). Traces to: AC-1 · none.

### Interfaces & contracts
- No engram interface surface. A scoped read `list_graphs_by_source(scope, stable_source_key)`
  on the SQLite adapter; entities/relationships for a repo are read by joining
  those graph ids (`WHERE graph_id IN (...)`). Traces to: AC-4 · none.

## Tasks

### T1: Stable-source-key normalization (pure function)

**Depends on:** none · **Verifies:** AC-1, AC-2, AC-6

**Tests:**
- TDD: `git@github.com:Org/Repo.git`, `https://user:pw@github.com/Org/Repo.git`,
  `https://github.com/Org/Repo`, `ssh://git@github.com/Org/Repo` all normalize to
  `github.com/org/repo` (AC-1).
- TDD: two SHAs of the same remote → identical key; no SHA/branch in output (AC-2).
- TDD: non-git input falls back to the given source name/root (AC-6).

**Approach:**
- Add `fn stable_source_key(remote: Option<&str>, fallback: &str) -> String` in a
  focused module in `adapters/ingest/src/` (strip scheme/credentials/`.git`,
  lowercase host; `host/org/repo` shape).

**Done when:** normalization unit tests are green.

### T2: Carry the key from scan into ingest

**Depends on:** T1 · **Verifies:** AC-1 (wiring)

**Tests:**
- Integration: a git-backed scan sets `SourceKind::GitRepository` and the derived
  key reaches the extractor (asserted downstream in T4).

**Approach:**
- Add a `stable_source_key: Option<String>` field to `DocumentIngestRequest`
  (`adapters/ingest/src/request.rs:23-35`, adapter type — no contract change).
- In `scanner.rs`: set `SourceKind::GitRepository` when `detect_git` succeeds;
  compute the key (T1) from the remote, fallback to un-enriched `opts.source_name`
  /root; populate the request field.
- In `ingestor.rs:94-106`: carry the key onto `KnowledgeSource.metadata` (today
  `None`, :105) so extraction sees it on `&KnowledgeSource`.

**Done when:** the tagging/wiring assertion is green (via T4).

### T3: Additive attribution columns + indexes

**Depends on:** none · **Verifies:** AC-1, AC-3, AC-4

**Tests:**
- Goal-based: migrated schema has `stable_source_key`/`path` columns + indexes on
  `knowledge_graphs`, and a `graph_id` column + index on `knowledge_entities`;
  existing rows unaffected.

**Approach:**
- Add columns + `CREATE INDEX IF NOT EXISTS` in
  `adapters/knowledge/sqlite/src/schema.rs`; lift `metadata.stableSourceKey`/
  `metadata.path` (graphs) and `record_json.graph_id` (entities) into the columns
  in the respective upsert paths (`service.rs`).

**Done when:** schema check passes and inserts populate the columns.

### T4: Stamp graph attribution

**Depends on:** T1, T2 · **Verifies:** AC-1, AC-3

**Tests:**
- Integration: an ingested graph carries `stableSourceKey` + `path` in metadata
  and the lifted `knowledge_graphs` columns; its entities/relationships carry
  `graph_id` in the lifted column (AC-1, AC-3).

**Approach:**
- In `adapters/ingest/src/extractor.rs` (graph construction ~`:62-75`), stamp
  `stableSourceKey` (from `source.metadata`) and `path` (from `document`) into the
  `KnowledgeGraph.metadata`; entities/relationships already carry `graph_id` in
  `record_json` (lifted by T3).

**Done when:** the attribution integration test is green.

### T5: Repository node + belongs_to edges

**Depends on:** T1, T4 · **Verifies:** AC-5

**Tests:**
- Integration: a scan yields exactly one `EntityKind::Repository` node keyed by
  the stable-source-key, with a `belongs_to` edge from each document graph; the
  node's `graph_id` is `None` and each edge's `graph_id` is its document graph's
  id; the node and edges carry no `path` (AC-5).

**Approach:**
- Emit a single Repository entity per source (id derived from
  `(scope, stable-source-key)`, idempotent across documents, `graph_id = None`)
  and, from each document graph, a `belongs_to` relationship whose `graph_id` is
  that document graph's id — so the edge is reachable via the `graph_id` join and
  retracts with its graph.

**Done when:** the Repository-node integration test is green.

### T6: Query knowledge by repo

**Depends on:** T3 · **Verifies:** AC-4

**Tests:**
- Integration: `list_graphs_by_source(scope, key)` returns exactly that repo's
  graphs; joining on `graph_id` returns exactly that repo's entities and
  relationships, across two repos in one scope (AC-4).

**Approach:**
- Add a scoped, indexed lookup on the SQLite adapter filtering `knowledge_graphs`
  by `stable_source_key` (mirrors existing `scope_allows` list methods); document
  the `graph_id` join for entities/relationships.

**Done when:** the query integration test is green.

### T7: No-domain-contract-change check

**Depends on:** T4 · **Verifies:** AC-7

**Tests:**
- Goal-based: `pnpm run contracts:generate` yields no diff; `git status` shows no
  change under `contracts/` or `docs/domain-data-model.md`.

**Approach:**
- Confirm the key rides in `KnowledgeGraph.metadata` only; run the generator and diff.

**Done when:** the generator produces no diff.

## Rollout

Additive library/adapter change. New columns are `NULL`-able and backfilled on
re-scan; no destructive migration; reversible by ceasing to populate them.

## Risks

- **Normalization completeness** — exotic remotes (self-hosted, ports, GitLab
  subgroups) may not fit `host/org/repo`; mitigate by falling back to a
  stripped-but-unshaped remote and covering common hosts in tests.
- **Metadata-as-carrier** — lifting from `record_json.metadata` couples the column
  to a metadata key; mitigate by centralizing key names as constants.
- **Repository-node id stability** — must key on `(scope, stable-source-key)` so
  re-scans and multiple documents converge to one node.

## Changelog

- 2026-07-04: initial plan.
- 2026-07-04: anchored attribution on `KnowledgeGraph.metadata` + `graph_id` (not
  relationship metadata, which does not exist); added the request-carrier task
  (T2) threading the key scanner→ingestor→extractor; query is graph-by-key joined
  on `graph_id`. Fixes spec-review blockers 1–4.
