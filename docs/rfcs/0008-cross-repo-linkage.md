# RFC-0008: Cross-repo linkage

- **Status:** Draft
- **Author:** phanijapps
- **Approver:** phanijapps
- **Date opened:** 2026-07-04
- **Date closed:**
- **Decision weight:** standard
- **Related:** RFC-0007 (federated assertion reconciliation), RFC-0004 (enterprise knowledge platform), ADR-0012 (source-assertion domain type), ADR-0013 (validation-event trigger family), `docs/specs/graph-explorer/`, `docs/specs/background-repo-indexer/`, research note `docs/research/cross-repo-linkage.md`

## Reviewer brief

- **Decision:** Model a repository as a first-class `KnowledgeSource` **+** an `EntityKind::Repository` node **within one shared scope**, and commit a near-term secondary track (Tier 0 + Tier 1) toward cross-repo linkage; sequence deeper tiers as follow-on specs.
- **Recommended outcome:** accept.
- **Change if accepted:** (1) repo gains a structured identity (normalized git remote) instead of a free-text `source_name`; (2) entities become groupable/filterable by repo; (3) a cross-repo graph view (cluster by repo, edges on shared `(name, kind)`) returns, recomputed from existing data.
- **Affected surface:** `adapters/ingest`, `adapters/knowledge/sqlite` (additive column), `demo/backend` + `demo/frontend` read path; `core/domain` gains *no* new required fields at Tier 0–1. No public contract break.
- **Stakes:** reversible at Tier 0–1 (additive, no contract break; nothing persisted that a re-scan can't reshape). The repo-modeling choice (D1) only becomes costly to reverse once Tiers 2–3 build on it — each a separate, gated RFC/spec.
- **Review focus:** (a) D1 — repo as `KnowledgeSource`+node vs a `Scope.workspace` dimension; (b) that the choice rests on real merits (type reuse + keeping Tier-3 reconciliation single-scope), **not** on a retrieval-layer prohibition — cross-repo *reads* work within a tenant either way (`scope.rs:8-14`).
- **Not in scope:** cross-*tenant* federation; semantic/LLM cross-repo linking; the Registry assertion adapter (Tier 3); multi-repo job queuing.

## The ask

**Recommendation (BLUF):** Approve modeling a repository as a `KnowledgeSource` plus an `EntityKind::Repository` graph node inside one shared scope (not as a `Scope.workspace`), and approve delivering **Tier 0 + Tier 1** — structured repo identity and a name-matched cross-repo graph view — as the near-term secondary track. Tiers 2–3 are sequenced as follow-on specs, not decided here.

**Why now (SCQA):**
- *Situation.* Engram indexes repos into a knowledge graph and (via RFC-0007) reconciles competing source assertions into beliefs. Its stated ambition (RFC-0004, framing synthesis) is org-wide knowledge.
- *Complication.* Nothing links knowledge *across* repos. Repo identity survives only as free text in `source_name`; each file becomes its own graph keyed `hash(document.id)`; entity identity dies at the file boundary; the extractor emits cross-file references it never resolves (the promised "cross-file resolver" does not exist). A cross-repo graph view was specced (`graph-explorer`) and then pruned by `demo-reimagine` (`explorer.tsx` is deleted).
- *Question.* How should a repository be represented so that cross-repo linkage is additive today **and** keeps the deeper tiers simple — especially belief reconciliation, which the `source-assertion-reconciliation` spec requires to run over a single scope — and what is the smallest useful first slice?

**Decisions requested:**

| ID | Question | Recommendation | Why | Decide by | Reviewer action |
| --- | --- | --- | --- | --- | --- |
| D1 | How is a repository modeled? | A `KnowledgeSource` + `EntityKind::Repository` node within one shared scope | Reuses existing types; additive; keeps all repos in one scope so Tier-3 reconciliation stays single-scope | this review | Confirm repo model; rule against `Scope.workspace` |
| D2 | What does this RFC commit to now? | Commit Tier 0 + Tier 1; sequence Tiers 2–3 as specs | Matches "secondary track"; deeper tiers need own specs | this review | Confirm phasing |
| D3 | Canonical repo key | Normalized git remote URL; fall back to user-supplied name for non-git (mechanics deferred to Tier-1 spec) | Stable, already captured; mirrors SCIP "package name" | this review | Confirm direction |
| D4 | How are Tier-0 cross-repo edges computed? | Client/read-model, ephemeral, matched on `(name, kind)` restricted to high-signal kinds | Refines `graph-explorer`'s bare-name match; no contract change; avoids name-noise | this review | Confirm approach |
| D5 | Relationship to the reconcile single-scope rule | Stay intra-scope; no relaxation now | Repos share one scope, so Tier-3 reconciliation needs no relaxation; reads already span a tenant | this review | Confirm no scope-rule change |

## Problem & goals

**Diagnosis.** There is no cross-repo *identity* for anything (verified against the code — see research note):

1. **Repo identity is unstructured.** The scanner interpolates the git triple into `source_name` (`"scan:repo [remote@branch:sha]"`, `adapters/ingest/src/scanner.rs:262-267`) and tags git repos as `SourceKind::Filesystem`, not `GitRepository` (`adapters/ingest/src/scanner.rs:361`). There is no queryable repo key.
2. **Entity identity is file-local.** `graph_id = hash(document.id)` (one graph per file), `entity_id = hash(graph_id, name)`. The same symbol in two repos gets two ids and is never merged (`adapters/ingest/src/extractor.rs:355-367`).
3. **Cross-graph links are emitted but never resolved.** Unresolved calls produce a name-only `EntityRef { id: None }` with a comment referencing a "cross-file resolver" that does not exist (`adapters/ingest/src/extractor.rs:168`); and `neighbors()` is hard-scoped `WHERE graph_id = ?1` (`adapters/knowledge/sqlite/src/service.rs:471`).
4. **The one prior cross-repo surface was removed.** `graph-explorer` specced a repo-clustered view with shared-name edges; `demo-reimagine` deleted `explorer.tsx`.

**Goals.**
- Give a repository a stable, structured, queryable identity.
- Make entities groupable and filterable by repository.
- Return a cross-repo graph view that shows how repos connect, recomputed from existing data, with no contract break.
- Lay the identity foundation that cross-graph entity resolution (Tier 2) and belief-level cross-repo reconciliation (Tier 3) will reuse.

**Non-goals.**
- Cross-*tenant* federation (the domain doc reserves a future "explicit federated query type"; we deliberately do not build it).
- Semantic/LLM-based cross-repo linking (deterministic name+kind only for now).
- The Registry assertion adapter and promotion triggers (RFC-0007 follow-ons, Tier 3).
- Multi-repo job queuing / scheduler (already a deferred `background-repo-indexer` item).

## Proposal

**D1 — Repo = `KnowledgeSource` + `Repository` node, one shared scope.** Represent each ingested repository as (a) the `KnowledgeSource` record it already produces, upgraded to carry structured git identity and `SourceKind::GitRepository`, and (b) an `EntityKind::Repository` graph node its files/symbols link to via `belongs_to`. All repos in a workspace share one `Scope`; repo is *not* a scope dimension. This keeps every cross-repo operation an ordinary intra-scope read.

**D3 — Structured repo identity (Tier 1).** Parse the git triple into structured provenance / a `RepoRef` keyed by a **normalized git remote URL** (host/org/name; strip credentials and protocol), falling back to the user-supplied name for non-git sources. Fix the `SourceKind::GitRepository` mislabel. Add a lifted, indexed column (e.g. `source_id`) on the knowledge tables so entities can be grouped and filtered by repo. Migration: existing rows keep working (the free-text name remains); a re-scan or a one-off backfill parses structured identity — additive, no destructive migration.

**D4 — Cross-repo graph view (Tier 0).** Re-introduce a repo-clustered graph view. Cluster nodes by their repo (`entity.graphId → KnowledgeGraph → source`), and draw cross-repo edges where two distinct graphs share an entity **`(name, kind)`**, restricted to high-signal kinds (`Repository`, `Project`, `Module`, `Service`, `Concept`) — not tier-3 functions/variables. Computed in the read model / client, ephemeral, no new domain types. This *refines* `graph-explorer`, which matched on **bare entity name** (`graph-explorer/spec.md:18`) — a match this RFC's own pre-mortem faults as over-linking — by adding the `(name, kind)` + high-signal-kind qualifier.

**Migration path.** Nothing to convert destructively: Tier 0 recomputes from existing `graphId` data; Tier 1 is an additive column plus richer provenance parsing on re-scan.

## Options considered

**Axis: where repository identity lives in the data model** (exhaustive over the structures that could carry it — scope, source record, new type, or nowhere).

| Option | Description | Trade-offs | |
| --- | --- | --- | --- |
| **A. `KnowledgeSource` + `Repository` node, one scope** | Repo = existing source record (structured) + a graph node; all repos share a scope | Linkage stays intra-scope (legal); reuses types; additive. Cost: a lifted column + provenance parsing | ★ recommended |
| B. `Scope.workspace` dimension | Repo = a distinct `workspace` value | Clean isolation, and cross-repo *reads* still work (within a tenant an unset `workspace` filter spans all workspaces — `scope.rs:8-14`), so B is **not** blocked for retrieval. But it repurposes `workspace`'s meaning (today: the indexed workspace) to "one repo", and puts each repo in a distinct scope — so Tier-3 belief reconciliation would run per-repo or need the reconcile single-scope rule relaxed | |
| C. New `RepoId` / `Repository` domain type | First-class repo type across the contract | Most explicit, but a contract change with no near-term payoff over A; A already has `EntityKind::Repository` | |
| D. Do-nothing | Keep free-text `source_name` | Zero cost now; but cross-repo remains impossible to query, the pruned `graph-explorer` view stays gone, and RFC-0004's org-wide ambition stalls. Cost of delay: the identity gap compounds as more repos are ingested | |

**Prior-art grounding.** A/C mirror how code-intelligence systems attach a stable package/symbol identity (Sourcegraph SCIP; see Evidence). B is the "partition by workspace" pattern — correct for isolation, but it changes the meaning of an existing field and complicates Tier-3 reconciliation. D is the status quo.

## Risks & what would make this wrong

**Pre-mortem.**
- *Name-match noise.* Bare-name cross-repo edges would over-link (`new`, `get`, `handler` are everywhere). **Mitigation (in D4):** match `(name, kind)` and restrict to high-signal kinds; treat the edge as a hint, not a claim.
- *Repo-key instability.* The same repo cloned via SSH vs HTTPS, or renamed, yields different keys. **Mitigation:** normalize the remote (strip protocol/creds); accept that a rename is a new key until Tier 2 resolution can merge.
- *Tier creep.* Shipping Tier 0 as "cross-repo linkage" oversells a heuristic. **Mitigation:** label it a *view* over shared names; reserve "linkage" claims for Tier 2's resolved relationships.

**Key assumptions (falsifiable).**
- *The current demo scans all repos into one scope* (`demo/backend/src/scan-defaults.ts:8`); **nothing structurally forces this** — a caller could scan under distinct `workspace`/`subject` values. Cross-repo *reads* work regardless (tenant is the only hard partition, `scope.rs:9`); the shared-scope choice matters specifically for keeping Tier-3 reconciliation single-scope.
- *Cross-graph relationships need no contract change* — only the traversal query is graph-scoped. (Verified: `KnowledgeRelationship` uses `EntityRef` + optional `graph_id`.)
- *Tier 0 is recomputable from current data* — entities still carry `graphId`. (Verified.)

**Drawbacks.** Tier 0 edges are heuristic and can mislead; Tier 1 adds an indexed column and a backfill path to maintain; committing to D1 makes the repo model costly to reverse later. These are accepted in exchange for an additive foundation the deeper tiers reuse.

## Evidence & prior art

**Spike / de-risk result.** Riskiest assumption: that Tier-0 name-matched edges are *useful* rather than noise. Checked: `graph-explorer` matches on **bare** entity name, which over code symbols over-links catastrophically (`new`, `get`, `handler`); restricting to `(name, kind)` and high-signal kinds (its own `auth-service` example is such a kind) keeps signal. Mitigation folded into D4. Secondary assumptions (repos share a tenant, no-contract-change, recomputable) were verified directly against the code — no spike needed.

**Repo precedent.**
- `docs/specs/graph-explorer/spec.md` — Tier 0 already specced ("cluster by source/repo", "cross-repo edges = entity-name matches across distinct graphs", "no new domain types, no contract change"); its route was later pruned by `demo-reimagine` (`explorer.tsx` deleted).
- `docs/rfcs/0007-federated-assertion-reconciliation.md` + ADR-0012/0013 — the belief engine Tier 3 reuses; the "federate, don't replicate" registry framing.
- `docs/specs/source-assertion-reconciliation/spec.md:52-53` ("Always do": reconcile only over assertions for one scope, never mix scopes in a single pass) — the belief-layer constraint that motivates keeping repos in one scope (D5). This lives in the *spec*, not RFC-0007's body.
- `docs/domain-data-model.md:283` — the hard partition is *tenant* ("records from different tenants must never be retrieved together" absent a future federated query type); `workspace` and below are optional narrowing filters (`adapters/knowledge/sqlite/src/scope.rs:8-14`), so cross-repo reads within a tenant are already permitted.

**External prior art.**
- [Sourcegraph scip-clang, CrossRepo](https://github.com/sourcegraph/scip-clang/blob/main/docs/CrossRepo.md) — confirmed: cross-repo symbol identity is the concatenation of *(package name, package version, qualified symbol name)*. Grounds D3 (structured, stable repo/symbol key) and Tier-2 keying beyond bare names. See also [Cross-repository code navigation](https://sourcegraph.com/blog/cross-repository-code-navigation).
- [Record linkage (Wikipedia)](https://en.wikipedia.org/wiki/Record_linkage) and [Basics of Entity Resolution](https://districtdatalabs.com/basics-of-entity-resolution) — the canonical pipeline (record linkage → deduplication → canonicalization) and deterministic-then-probabilistic matching; grounds Tier 2's "start deterministic `(name, kind)`, evolve to probabilistic/semantic" and the canonical-entity concept.

**Promoted research.** Full current-state analysis and current-vs-proposed identity diagrams live in `docs/research/cross-repo-linkage.md`.

## Open questions

1. **Does the repo key include branch/revision (SCIP-style version), or identify the repo independent of branch?** (owner: Tier-1 spec author; decide-by: Tier-1 spec). Recommended default: a **branch-independent** key (`host/org/name`); branch/sha stay as provenance, not identity — one repo is one node across branches. (This is distinct from D3, which decides the key's *source*; this asks about its *granularity*.)
2. **Where the repo→graph link is persisted** (owner: Tier-1 spec author; decide-by: Tier-1 spec). Recommended default: a lifted `source_id` column on knowledge tables plus a `belongs_to` edge from the `Repository` node, rather than overloading `graph_id`. (A Tier-1 persistence mechanic, not a decision requested in this RFC.)

## Follow-on artifacts

*Filled in on acceptance.*
- ADR: record the repo-modeling decision (D1) and the intra-scope stance (D5).
- Spec: `docs/specs/structured-repo-identity/` (Tier 1) and re-instated `docs/specs/cross-repo-graph-view/` (Tier 0).
- Later specs: `docs/specs/cross-graph-entity-resolution/` (Tier 2) and a Registry-adapter spec extending RFC-0007 (Tier 3).
