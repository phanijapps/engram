# RFC-0009: Knowledge-graph retraction and convergence

- **Status:** Draft
- **Author:** phanijapps
- **Approver:** phanijapps
- **Date opened:** 2026-07-04
- **Date closed:**
- **Decision weight:** heavy <!-- adds public-port methods + hard-delete (two CONVENTIONS §6 risk triggers) and is costly-to-reverse; requires explicit Approver sign-off before Open -->

- **Related:** RFC-0008 (cross-repo linkage) + `docs/specs/contract-first-ingestion/` (blocked on this), ADR-0016/0017, RFC-0001 (memory layer scope — `forget` precedent), ADR-0011 (consolidation trigger policy)

## Reviewer brief

- **Decision:** Give the knowledge layer **retraction** — deletion ports plus per-source *declared-set reconciliation* on re-ingest — so the graph converges to each source's current state instead of only accreting.
- **Recommended outcome:** accept.
- **Change if accepted:** (1) `KnowledgeRepository` gains `delete_*` ports (mirroring memory's `forget`); (2) entities/relationships gain a SHA-free stable-source-key (+ path) attribution column; (3) re-ingest reconciles a source's declared set — add/update/**retract** — deleting records the source no longer emits, ref-counted by source-attribution refs (entity `source_refs` / relationship `evidence`).
- **Affected surface:** `core/knowledge` (port trait), `adapters/knowledge/sqlite` (delete + attribution column), `adapters/ingest` (reconcile pass). Public-contract change to `KnowledgeRepository`.
- **Stakes:** costly-to-reverse — it adds methods to a public port and changes re-ingest semantics graph-wide; additive at the schema level, but downstream code depends on the new convergence behavior once shipped.
- **Review focus:** (a) D1 — declared-set reconciliation with prune vs alternatives; (b) D3 — the stable diff key must be a SHA-free `(stable-source-key, path)`, not the SHA-embedding `source_id` nor content-addressed `document_id`.
- **Not in scope:** memory-layer forget (already exists); the belief/assertion layer; embeddings-index GC beyond noting it; the specific ingestion trigger cadence (rides ADR-0011).

## The ask

**Recommendation (BLUF):** Approve adding **retraction** to the knowledge layer — `delete_*` ports on `KnowledgeRepository` and per-source **declared-set reconciliation** on re-ingest (compute what a source declares now, diff against what it declared before, then add/update/**delete** the difference, ref-counted by source-attribution refs) — so the derived graph converges to current state. Apply it graph-wide (code-symbol and contract nodes).

**Why now (SCQA):**
- *Situation.* Engram ingests repos into a knowledge graph and re-scans them continuously; memory already supports `forget`.
- *Complication.* The knowledge layer has **no** deletion — no `delete`/`prune` on the ports or adapter — and re-ingest is **add-only** (the manifest skips unchanged files; a changed file upserts and nothing removes what it superseded). So removed/renamed symbols and dropped declarations linger forever: the shipped code-symbol graph drifts stale, and cross-repo `contract-first-ingestion` (RFC-0008) is blocked on this.
- *Question.* How should the knowledge graph stay current under continuous re-ingestion — what deletion primitives, what reconciliation model, and what stable basis for the diff?

**Decisions requested:**

| ID | Question | Recommendation | Why | Decide by | Reviewer action |
| --- | --- | --- | --- | --- | --- |
| D1 | How does re-ingest converge? | Per-source **declared-set reconciliation with prune** (diff current vs prior → add/update/delete) | The standard desired-state pattern (kubectl prune, Terraform, controller loops); precise, no churn | this review | Confirm model; rule against add-only / delete-all-reinsert |
| D2 | What deletion primitives? | Add `delete_entity`/`delete_relationship`/`delete_graph` + coarse `delete_by_source` to `KnowledgeRepository` | Mirrors memory's `forget`; fine-grained for the reconciler, coarse for source removal | this review | Confirm the port surface |
| D3 | What is the stable diff basis? | An additive **SHA-free stable-source-key** (+ path) attribution column on entities/relationships | The existing `source_id` embeds the commit SHA and `document_id` is content-addressed — both change every commit; the diff key must be the SHA-free source identity (ADR-0017's normalized repo key) + path | this review | Confirm attribution key `(stable-source-key, path)` |
| D4 | Multi-source node lifecycle | Reference-count via source-attribution refs (entity `source_refs` / relationship `evidence`): retract a source's contribution; delete a node only when its last source retracts | A shared contract node must survive while any repo still declares it | this review | Confirm ref-counting |
| D5 | Scope | Graph-wide (code-symbol + contract) | The gap is general — the code-symbol graph already drifts on edit | this review | Confirm graph-wide, not contract-only |

## Problem & goals

**Diagnosis.** Continuous ingestion needs the graph to *converge* to what sources currently say; today it only *accretes*. Verified:

- **No deletion operation anywhere.** No `delete`/`prune`/`remove` *operation or port* for entities, relationships, or graphs in `core/knowledge/src` or `adapters/knowledge/sqlite/src`. (A `DeleteMode::Tombstone` *policy* field exists at `retrieval.rs:210`, but no operation consumes it to delete.)
- **Re-ingest is add-only.** `ingestor.rs`/`scanner.rs` never delete prior records; the manifest only skips unchanged files (`scanner.rs:353`). A changed file upserts new records and orphans the old — the extractor keys a graph per document (`graph_id = hash(document.id)`), and `document_id` changes with content, so an edit spawns a *new* graph and strands the previous one.
- **Asymmetry.** Memory already supports retraction (`forget`, `core/memory/src/lib.rs:79`, with `ForgetStatus::Deleted/Archived`); knowledge has no equivalent.
- **Attribution gap.** `knowledge_documents`/`knowledge_chunks` carry a `source_id` column (indexed); `knowledge_entities`/`knowledge_relationships` do **not** — so "what did this source emit?" is not queryable for the diff. And the existing `source_id` embeds the commit SHA (via `source_name`, `scanner.rs:262-267`), so it is not commit-stable — the diff needs a SHA-free key (D3).

**Goals.**
- Deletion primitives on the knowledge port, mirroring memory's `forget`.
- Per-source declared-set reconciliation on re-ingest: add new, update changed, retract removed; delete orphaned nodes.
- Multi-source correctness: a shared node survives until its last contributing source retracts.
- A stable attribution basis for the diff that survives content edits.

**Non-goals.**
- Memory-layer forget (already exists) and the belief/assertion layer.
- The ingestion trigger cadence — reconciliation runs within the existing ingest/consolidate path (ADR-0011), not a new scheduler.
- Embeddings-index garbage collection beyond noting retracted records must not leave dangling embeddings.
- A general graph history/versioning system — retraction converges to current state, it is not an audit log.

## Proposal

**Reconciliation model (D1).** Each ingest of a source `S` (at document/path granularity) produces the complete set of records `S` currently yields. The reconciler diffs this against what `S` yielded before and applies the difference: insert new, update changed, **delete** records `S` no longer emits — the desired-state "apply with prune" pattern. No full rebuild, no delete-all-then-reinsert churn.

**Deletion ports (D2).** `KnowledgeRepository` gains `delete_entity(id)`, `delete_relationship(id)`, `delete_graph(id)`, and a coarse `delete_by_source(source_id)` (for whole-source removal), implemented by the SQLite adapter. These mirror the shape of memory's `forget`. Note the god-trait risk: `KnowledgeRepository` is already broad (~25 methods across source/document/chunk/entity/relationship/graph/ontology/concept/reader/ingest). The fine-grained `delete_*` mirror the existing `put_*` and belong there; but the coarse `delete_by_source` + the reconcile surface may fit better on a **narrower reconciler-facing port** — resolved in the spec (see Drawbacks).

**Diff basis (D3).** Add a stable attribution column to `knowledge_entities` and
`knowledge_relationships`: a **SHA-free stable source key** plus `path`. This is
*not* the existing `source_id` — that is
`content_hash(tenant, uri, source_name, source_kind)` (`ingestor.rs:187-199`) and
for git repos `source_name` embeds `remote@branch:sha` (`scanner.rs:262-267`), so
it changes on **every commit**; `document_id` (which itself embeds `source_id`
plus a content hash) changes on every edit. The only commit-stable handle is the
SHA-free source identity — the normalized repo key from ADR-0017 (or the
un-enriched `source_name` / repo root) — combined with the file `path`. The diff
is keyed by `(stable-source-key, path)`, which makes ADR-0017's stable repo
identity a prerequisite of this reconciler.

**Multi-source lifecycle (D4).** Records are reference-counted by their
source-attribution refs — entity `source_refs` (`knowledge.rs:207`) and
relationship `evidence` (`knowledge.rs:229`). Retracting `S` removes `S`'s ref and
`S`'s edges from a node; the node itself is deleted only when its refs become
empty. A shared contract node (two repos declaring the same key) survives until
the last repo stops declaring it.

**Scope (D5).** Graph-wide: the same reconciliation serves code-symbol extraction and contract-first ingestion.

**Migration.** The attribution column is additive. Only `path` is recoverable
from existing records (via `source_refs.location.path`); a stable SHA-free source
key is **not** backfillable — `EvidenceRef` has no `source_id` (`provenance.rs:26-35`)
and `provenance.source` embeds the SHA — so it is recomputed on the first re-scan
after rollout, which converges existing records.

## Options considered

**Axis: how re-ingest reconciles a source's contribution** (exhaustive over convergence strategies — reconcile-diff, replace-wholesale, mark-deleted, or nothing).

| Option | Strategy | Trade-offs | |
| --- | --- | --- | --- |
| **A. Declared-set reconciliation + prune** | diff current vs prior; add/update/delete | Precise, stable ids, no churn; needs a diff basis + delete ports. Matches kubectl prune / Terraform / controller loops | ★ recommended |
| B. Delete-all-then-reinsert per source | drop everything `S` emitted, re-insert | Simple, no diff; but churns every id each scan, breaks stable identity and any embedding/edge keyed on it, and thrashes shared nodes | |
| C. Tombstone / soft-delete | mark retracted, filter on read | Keeps history; but every read pays a filter, storage grows unbounded, and it still needs GC eventually | |
| D. Do-nothing (add-only) | current behavior | Zero work; graph drifts permanently stale — the status quo this RFC exists to fix | |

**Prior-art grounding.** A is the desired-state-with-prune pattern (Kubernetes `apply --prune` + controller reconcile; Terraform destroy-on-removal; Elasticsearch `delete_by_query`; Sourcegraph upload supersede/expire). B/C are the replace-wholesale and soft-delete alternatives common in indexing systems. D is the status quo.

## Risks & what would make this wrong

**Pre-mortem.**
- *Wrong diff key.* Keying the diff on content-addressed `document_id` would make every edit look like "old gone, new added," thrashing ids and embeddings. **Mitigation (D3):** key on the SHA-free `(stable-source-key, path)`.
- *Lost shared node.* Naively deleting a node when one source retracts would drop contract nodes other repos still declare. **Mitigation (D4):** ref-count on source-attribution refs (entity `source_refs` / relationship `evidence`).
- *Dangling embeddings.* Deleting a record without its embedding leaves orphaned vectors. **Mitigation:** retraction must cascade to the embedding index (called out; mechanics to the spec).
- *Concurrent re-ingest.* Two scans of overlapping sources racing on a shared node. **Mitigation:** reconcile within the store's transactional/critical section, per-scope.

**Key assumptions (falsifiable).**
- A source's *current* declared set is fully enumerable per ingest — true for a filesystem/git scan (the scanner walks the whole tree).
- Prior emission is queryable once `(stable-source-key, path)` is recorded — an additive column on entities/relationships. The existing `source_id` is not reusable as that key (it embeds the commit SHA); the stable key is SHA-free (D3).
- Deletion belongs on the knowledge port — supported by memory's existing `forget` (same architectural layer).

**Drawbacks.** Adds methods to a public port (a contract change), an attribution column, and a reconcile pass to maintain; hard deletion forgoes built-in history (accepted — retraction converges to current state, it is not an audit log; the changelog/provenance remain the record). It also grows an already-broad `KnowledgeRepository` (~25 methods) — the coarse `delete_by_source` + reconcile surface may warrant a separate narrower reconciler port rather than piling onto the trait; the split is a spec-level decision.

## Evidence & prior art

**Spike / de-risk result.** Riskiest assumption: that "what a source previously emitted" is enumerable for a diff without a full rebuild. Checked against the **code**, not just the schema: adding an attribution column to entities/relationships makes prior emission an indexed lookup — **but the existing `source_id` cannot be that key.** `source_id` is `content_hash(tenant, uri, source_name, source_kind)` (`ingestor.rs:187-199`) and `source_name` embeds the commit SHA for git repos (`scanner.rs:262-267`), so it changes every commit — as does `document_id` (which embeds `source_id`). The commit-stable handle is a **SHA-free** source identity (ADR-0017's normalized repo key, or the un-enriched `source_name`) + `path`. This corrects the diff key to `(stable-source-key, path)` and makes ADR-0017's stable repo identity a prerequisite — folded into D3 and Open questions.

**Repo precedent.**
- `core/memory/src/lib.rs:79` — memory's `forget` (`ForgetRequest`→`ForgetResult`, `ForgetStatus::Deleted/Archived`); knowledge should mirror this at the same layer.
- `adapters/knowledge/sqlite/src/schema.rs` — `source_id` present + indexed on documents/chunks, absent on entities/relationships (the attribution gap).
- `adapters/ingest/src/{scanner.rs:353,ingestor.rs}` + `extractor.rs` — add-only re-ingest; per-document `graph_id` orphans old graphs on edit.
- RFC-0008 + `docs/specs/contract-first-ingestion/` — blocked on this capability.

**External prior art** (all fetched and confirmed to contain the cited claim).
- [Kubernetes `kubectl apply --prune`](https://kubernetes.io/docs/reference/kubectl/generated/kubectl_apply/) — deletes objects previously applied but absent from the current config set (declared-set prune).
- [Kubernetes controllers](https://kubernetes.io/docs/concepts/architecture/controller/) — control loops drive current state toward declared desired state.
- [Elasticsearch `delete_by_query`](https://www.elastic.co/docs/api/doc/elasticsearch/operation/operation-delete-by-query) — removes documents matching a query (index retraction).
- [Sourcegraph precise-code-intel uploads](https://sourcegraph.com/docs/code-navigation/explanations/uploads) — an upload is deleted when superseded by a newer one or aged out.
- [Terraform destroy-on-removal](https://developer.hashicorp.com/terraform/language/resources/destroy) — removing a resource from config and applying destroys the real resource.

## Open questions

1. **Exact attribution mechanism** (owner: spec author; decide-by: implementing spec). Recommended default: a lifted `(stable-source-key, path)` column pair on entities/relationships, where `stable-source-key` is the SHA-free repo identity from ADR-0017; consider a per-`(source, path)` manifest only if the column diff proves insufficient. Neither the content-addressed `document_id` nor the existing SHA-embedding `source_id` is the diff key.
2. **Hard delete vs tombstone for retracted-record storage** — distinct from D1, which rejects tombstoning as the *convergence strategy*; this is the narrower question of how a *deleted* record is stored (row removed vs row marked deleted) (owner: spec author; decide-by: implementing spec). Recommended default: hard delete of graph records (retraction is convergence, not history), with a mandatory cascade to the embedding index.
3. **Where the reconcile pass runs** (owner: spec author; decide-by: implementing spec). Recommended default: inside the existing per-scope ingest/scan pass at end-of-source, not a new scheduler (consistent with ADR-0011's caller-invoked model).

## Follow-on artifacts

*Filled in on acceptance.*
- ADR: record the retraction/convergence decision (D1) and the port-surface change (D2).
- Spec: `docs/specs/knowledge-graph-retraction/` — delete ports + attribution column + reconcile pass.
- Unblocks `docs/specs/contract-first-ingestion/` task T8 (per-source convergence) and the RFC-0008 continuous-update acceptance criteria.
