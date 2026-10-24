# ADR-0032: Branch-aware code indexes

**Status:** accepted · **Date:** 2026-10-24

## Context

Agents need to search different branches of the same repository
simultaneously: operations agents search the deployed code (main), developer
agents search the dev branch. Today, scanning the same repo at different
branches causes destructive collisions — the reconciliation step deletes all
prior graphs for the same `stable_source_key` + path, regardless of which
branch produced them.

## Decision

> **Status note:** This ADR records the architectural decision. Phase 1
> (contract fields + docs) is implemented. Phase 2 (ingestion — qualified
> reconciliation) and Phase 3 (search — index selection) are planned but
> NOT implemented. `codeIndexKeys` in `QueryFilter` is accepted but not
> enforced until Phase 3.

> Code indexes are identified by branch-qualified source keys:
> `repository_key@git_ref` (e.g. `github.com/acme/payments@refs/heads/main`).
> Both branch indexes coexist in the same workspace; reconciliation
> partitions by the qualified key; search selects via
> `QueryFilter.codeIndexKeys`.

**What this is NOT:**
- NOT a new `Scope.project` field — a branch is a code-revision variant,
  not a product/access grouping. Projects (users/groups/roles) are a future
  policy-layer concern on the existing `PolicyAuthorizer` port.
- NOT `Scope.environment` — that means deployment environment, not code
  revision.
- NOT separate workspaces — that loses cross-branch queries and shared
  workspace context.

**Domain changes (all additive/compatible):**
- `KnowledgeSource`: + `repositoryKey`, + `gitRef` (typed, promoted from
  metadata)
- `QueryFilter`: + `codeIndexKeys` (selectable index filter)
- `ScanCheckpoint`: + `gitRef`
- `CodeGraphStaleness`: + `gitRef`

## Consequences

- Same repo, multiple branches, same workspace — no contamination.
- Cross-branch comparison becomes possible (both indexes co-resident).
- Legacy unqualified keys work as single-index during migration.
- True authorization (ops can't see dev) is a future policy feature, not
  this ADR.
