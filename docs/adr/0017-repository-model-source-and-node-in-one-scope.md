# ADR-0017: Repository model: KnowledgeSource + Repository node in one shared scope

- **Status:** Accepted
- **Date:** 2026-07-04
- **Decision-makers:** phanijapps
- **Supersedes:** none
- **Related:** RFC-0008 (cross-repo linkage), ADR-0016 (contract-node linking model), `docs/research/cross-repo-linkage.md`

## Decision summary

- **Decision:** Model a repository as a structured `KnowledgeSource` plus an `EntityKind::Repository` node, with all repos in a workspace sharing one `Scope`; a repository is **not** a `Scope` dimension.
- **Because:** it keeps cross-repo linkage a legal intra-scope operation and keeps later belief reconciliation single-scope, while reusing existing domain types.
- **Applies to:** how ingested repositories are identified and partitioned for cross-repo linkage.
- **Tradeoff accepted:** repo identity rides on a source record + a node rather than a first-class `RepoId` type; less explicit.
- **Revisit if:** cross-*tenant* federation becomes required, or a first-class `RepoId` domain type proves necessary for downstream features.

## Context

The hard partition in Engram is **tenant**: `scope_allows` requires `tenant` to match exactly, while `subject`/`workspace`/`session`/`environment` are optional narrowing filters (`adapters/knowledge/sqlite/src/scope.rs:8-14`). So cross-repo *reads* already work within a tenant regardless of how repos are modeled. Constraints:

- A repository today survives only as free text in `source_name` (`adapters/ingest/src/scanner.rs:262-267`), tagged `SourceKind::Filesystem` rather than `GitRepository` (`:361`) — not queryable.
- `EntityKind::Repository` (`core/domain/src/knowledge.rs:178`) and `SourceKind::GitRepository` (`:19`) already exist.
- Belief reconciliation is required by the `source-assertion-reconciliation` spec to run over a single scope (`spec.md:52-53`), and its derived records inherit the winning assertion's scope.
- The demo already writes every repo into one fixed scope; nothing structurally forces this.

## Decision

**We will model a repository as a structured `KnowledgeSource` (normalized git identity, `SourceKind::GitRepository`) plus an `EntityKind::Repository` node, with all repos in a workspace sharing one `Scope`; a repository is not a `Scope` dimension.**

- Repo files, chunks, and contract nodes link to the `Repository` node (e.g. via a `belongs_to` edge) and carry the source's identity, so knowledge can be grouped and filtered by repo.
- The canonical repo key is a normalized git remote URL (direction set by RFC-0008 D5; mechanics deferred to the implementing spec).
- **Boundary:** this covers repository identity and partitioning within a tenant. Cross-*tenant* federation is out of scope (`docs/domain-data-model.md:283` reserves a future federated query type).

## Decision drivers

- **Keep linkage intra-scope** — cross-repo linkage should not collide with the belief-layer single-scope rule or require cross-tenant federation.
- **Substrate reuse / additive** — prefer existing types and additive columns over a new domain type or a contract break.
- **Queryability** — repo must become groupable/filterable, unlike today's free-text name.

## Consequences

**Positive:**
- Cross-repo linkage stays a legal intra-scope operation; later reconciliation sees repos as one scope and needs no relaxation.
- Reuses `KnowledgeSource` and the existing `EntityKind::Repository`/`SourceKind::GitRepository` variants; additive (a lifted `source_id` column, richer provenance) with no contract break.
- Repo becomes a first-class, addressable, groupable thing.

**Negative:**
- Repo identity is carried by a source record + node rather than a dedicated `RepoId` type — less explicit, and a rename yields a new key until entity resolution can merge.
- Committing to this model is costly to reverse once later phases build on it.

**Revisit if:** cross-tenant federation becomes required, or a first-class `RepoId` domain type proves necessary for downstream features.

## Confirmation

- **Mode:** reviewer-checked
- **Signal:** ingestion records a repository as a `SourceKind::GitRepository` `KnowledgeSource` with structured git identity + a `Repository` node, all under one shared scope, and knowledge is queryable by repo.
- **Owner:** maintainer (phanijapps).

## Alternatives considered

- **Repo = a `Scope.workspace` value.** Rejected against *keep linkage intra-scope*: it repurposes `workspace`'s meaning and puts each repo in a distinct scope, so later belief reconciliation would run per-repo or need the single-scope rule relaxed. (Cross-repo *reads* would still work, since `workspace` is an optional narrowing filter — so this is rejected on the reconciliation concern, not a retrieval prohibition.)
- **A new first-class `RepoId`/`Repository` domain type.** Rejected against *substrate reuse*: a contract change with no near-term payoff over reusing `KnowledgeSource` + the existing `EntityKind::Repository`.
- **Do-nothing (free-text `source_name`).** Rejected against *queryability*: a repo cannot be grouped, filtered, or joined today.

## References

- RFC-0008 (cross-repo linkage) and `docs/research/cross-repo-linkage.md`.
- `adapters/knowledge/sqlite/src/scope.rs` (tenant-only hard match); `docs/specs/source-assertion-reconciliation/spec.md:52-53` (single-scope rule); `docs/domain-data-model.md:283` (tenant partition).
