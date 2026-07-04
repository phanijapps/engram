# Spec: structured-repo-identity

- **Status:** Shipped
- **Owner:** phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** ADR-0017, ADR-0018, RFC-0008, RFC-0009
- **Brief:** none
- **Contract:** none (internal ingestion/adapter change; exposes no engram interface surface)
- **Shape:** data

> **Spec contract:** this document defines what "done" means. The implementing
> PR must match this spec, or update it. Verification must be derivable from it.

## Objective

A developer indexing several repositories needs each repository to have a
**stable, structured identity** so knowledge can be grouped and filtered by repo,
and so a later reconciler has a commit-stable key to diff against. When a
repository is ingested, its per-document `KnowledgeGraph`s are recorded with a
**SHA-free stable-source-key** — a normalized git remote (`host/org/name`, with
scheme, credentials, and a trailing `.git` stripped), falling back to the
un-enriched source name / repo root for non-git sources — that **does not change
across commits** — plus the document's source-relative `path`. A git repository
is tagged `SourceKind::GitRepository` (not `Filesystem`). Because entities and
relationships carry their owning `graph_id`, they are attributable to a repo
through their graph, and "what did this repo emit?" is a queryable lookup. The
repository also appears as a single `EntityKind::Repository` node, with a
`belongs_to` edge from each of its document graphs. Nothing here deletes or
reconciles records — this is the identity foundation the retraction reconciler
(knowledge-graph-retraction) builds on.

## Boundaries

The three-tier guard that keeps an implementing agent inside the lines.
*Always do* applies without asking; *Ask first* requires human sign-off
before proceeding; *Never do* is a hard rule, even under time pressure.

### Always do

- Derive the stable-source-key by normalizing the git remote to `host/org/name`
  (strip scheme, credentials, and a trailing `.git`; lowercase host); for a
  non-git source use the un-enriched source name / repo root — never the
  SHA-enriched `source_name` and never `document_id`.
- Record the stable-source-key and the document's source-relative `path` on each
  `KnowledgeGraph` (in its existing `metadata`), and lift both into indexed
  columns on `knowledge_graphs`.
- Ensure every emitted `KnowledgeEntity` and `KnowledgeRelationship` carries its
  owning `graph_id`, lifted into an indexed column (relationships already have a
  `graph_id` column; entities gain one), so records are reachable from their
  repo via their graph.
- Tag a git-backed scanned source as `SourceKind::GitRepository`.
- Emit exactly one `EntityKind::Repository` node per source (keyed by the
  stable-source-key), with a `belongs_to` edge from each document graph to it.

### Ask first

- Adding a **domain field** to any `core/domain` type — the stable-source-key
  rides in `KnowledgeGraph.metadata`; a first-class field is a contract change
  and needs sign-off + contract regen.
- Changing how `source_id`/`document_id` are derived (this spec adds attribution
  alongside them; it does not re-key existing ids).

### Never do

- No new top-level crate or module boundary; this lives in `adapters/ingest` and
  `adapters/knowledge/sqlite`.
- No deletion/retraction/reconciliation logic here (that is knowledge-graph-retraction).
- No cross-tenant behavior; all repos remain within one scope.
- No new runtime dependency for URL normalization — deterministic string handling only.

## Testing Strategy

- **Remote normalization** (`git@github.com:Org/Repo.git`, `https://u:p@github.com/Org/Repo.git`, and `https://github.com/Org/Repo` all → `github.com/org/repo`; SHA/branch never appear): **TDD** — a compressible pure-function invariant.
- **Stable-source-key is commit-stable** (re-deriving from two different SHAs of the same remote yields the same key): **TDD** — the load-bearing property from ADR-0018.
- **Graph attribution stamped + queryable** (an ingested graph carries the stable-source-key + path in metadata and the lifted columns; a query by stable-source-key returns exactly that repo's graphs, and via `graph_id` its entities and relationships): **goal-based check**, exercised by an **integration** test over ingest→store.
- **GitRepository tagging + Repository node** (a git-backed scan yields `SourceKind::GitRepository` and one `EntityKind::Repository` node with `belongs_to` edges from its document graphs): assertion-based **integration** test.
- **No domain contract change** (generated contracts + `docs/domain-data-model.md` unchanged): **goal-based check** — `pnpm run contracts:generate` produces no diff.

## Acceptance Criteria

- [x] A git-backed scanned source is recorded as `SourceKind::GitRepository`, and its document graphs carry a normalized stable-source-key (`host/org/name`, scheme/credentials/`.git` stripped) + the document `path` in `KnowledgeGraph.metadata`, lifted into indexed columns on `knowledge_graphs`.
- [x] Re-deriving the stable-source-key from two different commit SHAs of the same remote yields the **same** key (commit-stable).
- [x] Every `KnowledgeEntity` (except the per-source `EntityKind::Repository` node, which has `graph_id = None`) and every `KnowledgeRelationship` carries its owning `graph_id` in an indexed column (entities gain the column; relationships already have it), so each is reachable from its repo via its graph. The Repository node is reached by kind / its `belongs_to` edges, not the `graph_id` join.
- [x] A query filtered by stable-source-key returns exactly the graphs emitted by that repository, and — joined via `graph_id` — exactly that repository's document entities and relationships (the per-source Repository node is reached via its `belongs_to` edges, not this join).
- [x] The source is represented by exactly one `EntityKind::Repository` node (keyed by stable-source-key, `graph_id = None`) with a `belongs_to` edge from each of its document graphs; each `belongs_to` edge carries that document graph's `graph_id` (so it is reachable via the join and retracts with the graph); the Repository node and `belongs_to` edges carry no `path` (they are not file-scoped).
- [x] A non-git source falls back to the un-enriched source name / repo root as its stable-source-key.
- [x] No domain contract change: `pnpm run contracts:generate` yields no diff and `docs/domain-data-model.md` is unchanged.

## Assumptions

- Technical: extraction has source context (`extract`/`extract_with_calls`/`extract_into` take `&KnowledgeSource`, extractor.rs:42,54,248), so the Repository node + graph attribution emit without new plumbing (source: repo read).
- Technical: `KnowledgeGraph` (and `KnowledgeEntity`, `KnowledgeSource`) carry `metadata: Option<Metadata>` = `BTreeMap<String,Value>` (knowledge.rs:65,215; types.rs:16); `KnowledgeRelationship` has **no** metadata field (knowledge.rs:218-236) — so attribution is anchored on the graph, and entities/relationships are attributed via their `graph_id` (source: repo read).
- Technical: `knowledge_relationships` has a `graph_id` column; `knowledge_entities` and `knowledge_graphs` do not carry a lifted attribution column (schema.rs) — additive columns needed (source: repo read).
- Technical: no git-remote normalizer exists; `detect_git` captures the remote via `git remote get-url origin` (scanner.rs:112-126) — normalization is new deterministic logic (source: repo read).
- Technical: `EntityKind::Repository` + `SourceKind::GitRepository` exist; scanner tags git repos `Filesystem` (scanner.rs:361) — fix is scanner-side (source: repo read).
- Product: data-layer only; demo UI repo-grouping is out of scope (source: user confirmation 2026-07-04).
- Process: exposes no engram interface surface, so no `contracts/` artifact is authored; retraction is deferred to knowledge-graph-retraction (source: user confirmation 2026-07-04).
