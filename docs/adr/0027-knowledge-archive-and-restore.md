# ADR-0027: Knowledge entity/relationship archive and restore (soft-delete over hard delete)

- **Status:** Accepted
- **Date:** 2026-08-07
- **Decision-makers:** phanijapps
- **Supersedes:** ADR-0018 (partial — only its hard-delete / no-audit-log storage-mode consequence, for knowledge entities and relationships)
- **Related:** RFC-0009 (knowledge-graph retraction), ADR-0022 (engine neutrality — backend-dependent atomicity), ADR-0019 (bi-temporal entities — distinct from audit history), RFC-0014 (transactional consolidation — apply precedent), `docs/specs/graph-maintenance-api/` (the feature that raises this decision)

## Decision summary

- **Decision:** Knowledge entities and relationships gain an archived soft-delete state with restore; hard delete becomes an explicit, escalated step rather than the default retraction path for these two objects.
- **Because:** a graph-maintenance API needs reversible mutation so a host application can repair graph topology without an irreversible commit and without direct SQLite access.
- **Applies to:** `KnowledgeEntity` and `KnowledgeRelationship` only. Graph-level and by-source retraction (ADR-0018), and the memory `forget` layer, are unchanged.
- **Tradeoff accepted:** entities/relationships gain a status/archive field (domain + schema surface), and active reads must filter archived rows; ADR-0018's convergence machinery keeps operating on the active set.
- **Revisit if:** read-time filtering cost or archived-row growth becomes material, or a cross-store maintenance operation needs durability the archive state alone cannot provide.

## Context

ADR-0018 added retraction plus per-source declared-set reconciliation to the knowledge layer and chose **hard delete** with no built-in history — "retraction is convergence, not an audit log" — while explicitly naming the revisit trigger: *"a graph-history/audit requirement makes soft-delete necessary."* The graph-maintenance-api feature is that requirement.

Today, knowledge entities and relationships carry full `Provenance` — actor, method, observed_at, confidence, source/evidence refs (`core/domain/src/provenance.rs:67`, `core/domain/src/knowledge.rs:201-248`) — but they have **no** status, archive, or review field, and deletion is a hard `DELETE` (`adapters/sqlite/src/knowledge/`). A `Policy.delete_mode` enum does enumerate `Delete | Redact | Tombstone | Archive`, but `Policy` is not carried by entities or relationships (only by `KnowledgeSource`, `KnowledgeGraph`, `SourceDocument`, `KnowledgeChunk`).

The maintenance API needs reversible mutation — archive/restore an entity or relationship, merge two entities while rewiring edges, manage aliases, rewrite or delete a relationship — previewed in a dry-run plan and applied as one atomic unit. Hard delete makes every such mutation irreversible, forcing a host to keep its own shadow copy or accept data loss. There is no public transaction/batch port on the knowledge traits today; atomicity exists only implicitly inside `KnowledgeGraphRepository::delete_graph` and `EntityIdentityRepository::consolidate_entities` (each opens `conn.transaction()`). ADR-0022 makes any durability guarantee backend-dependent and engine-neutral.

## Decision

> We will give `KnowledgeEntity` and `KnowledgeRelationship` a soft-delete **archive** state with **restore**, superseding ADR-0018's hard-delete-as-default stance for these two objects.

- Archive sets an entity or relationship to an archived (hidden, non-contractual) state; restore reverses it. Permanent delete remains available as an explicit, escalated operation, not the default maintenance path.
- The reversibility primitive is **archive/restore** — the maintenance API's undo mechanism. No separate durable audit table and no export-snapshot rollback are introduced (see Alternatives).
- Attribution for every mutation rides the existing `Provenance` (actor/method/observed_at/confidence) already on each entity and relationship; there is no new audit table.
- **Atomicity is backend-dependent** (ADR-0022): the maintenance port declares an atomic-apply *intent*; the SQLite adapter implements a plan's apply as a single transaction (precedent: `delete_graph`, `consolidate_entities`). The contract does not hard-claim single-transaction atomicity — it surfaces the guarantee level so a caller knows what holds.
- ADR-0018's retraction ports, per-source reconciliation, SHA-free attribution, and ref-counting of shared nodes remain in force and operate over the **active** set; this ADR reverses only ADR-0018's storage-mode consequence (hard-delete / no-audit-log) for entities and relationships.
- **Boundary (spec-level, not decided here):** the field representation (a new `status`/`archived_at` field vs. extending `Policy`/`DeleteMode::Archive` onto these objects), the archive→restore cascade rules (a node and its incident edges archive and restore together so the active graph never shows dangling edges), the maintenance port shape, and the candidate-detection rules are decided in `docs/specs/graph-maintenance-api/`. Candidate detection (orphan / low-confidence / unsupported / duplicate) is deterministic and policy-driven — threshold/filter rules reusing `discover_collisions` and `OntologyRepository::validate_graph`, no LLM — keeping the Rust core LLM-free; this bounds the reversibility model and is recorded here only for that reason.

## Decision drivers

- **Reversibility** — a maintenance host must undo a repair without data loss; hard delete forecloses this.
- **No direct DB access** — hosts repair via the API, not SQLite; the API must offer a reversible primitive.
- **Engine neutrality** — the atomicity guarantee must not assume SQLite (ADR-0022).
- **Attribution already present** — `Provenance` already carries who/when/how/confidence, so a separate audit table duplicates an existing concept.

## Consequences

**Positive:**
- Graph repair is reversible: archive a bad entity, restore on mistake — no shadow copy or data loss.
- Maintenance apply is atomic within a backend (SQLite single transaction); a verification failure rolls the plan back.
- ADR-0018's own revisit trigger is resolved; convergence/reconciliation keep working over the active set.
- Attribution stays where it already lives (`Provenance`); no new persistence concept or write path.

**Negative:**
- Domain + schema surface: entities/relationships gain an archive/status field, and active reads must filter archived rows.
- Archived rows accumulate until explicitly deleted; because ADR-0018 convergence operates on the active set, archived rows sit outside convergence and need their own retention story.
- Atomicity is only as strong as the backend; a future non-transactional engine surfaces weaker guarantees via the surfaced guarantee level.
- We forgo a durable, queryable audit log and a point-in-time rollback snapshot (deliberate — see Alternatives).

**Revisit if:** read-time filtering cost or archived-row growth becomes material, or a cross-store maintenance operation needs durability the archive state alone cannot provide.

## Confirmation

- **Mode:** reviewer-checked
- **Signal:** archiving an entity/relationship hides it from active reads and from convergence; restore returns it; a maintenance plan whose apply fails verification leaves the graph unchanged (SQLite single-transaction rollback). The engine-neutrality half depends on the surface-parity / engine-neutrality lint (currently absent from the repo — `.codex/hooks/` was removed on 2026-08-01); reviewer-checked until that gate is reinstated.
- **Owner:** maintainer (phanijapps).

## Alternatives considered

- **Durable maintenance-run audit table** — persist a `maintenance_run` row recording the plan, mutations, actor, timestamps, and a snapshot reference. Rejected against *attribution already present*: `Provenance` already attributes every mutation (who/when/how/confidence), so a parallel audit table duplicates attribution and adds a new persistence concept and write path. Archive/restore plus `Provenance` covers reversibility and attribution without it. Revisit if a regulatory or process need for a queryable, append-only audit trail emerges.
- **ExportImport snapshot-for-rollback** — capture `ExportImport::export(scope)` before apply and restore on failure. Rejected against *reversibility*: it is a coarse scope-wide image (not per-mutation), v1 export omits hierarchy/belief (partial coverage), and `apply_import` is additive upsert rather than wipe-then-restore, so true rollback needs an extra retraction pass. Archive/restore is per-object and exact. Revisit if maintenance must roll back multi-object, cross-family changes atomically where per-object archive is insufficient.
- **Tombstone with full version history** — a bi-temporal audit log beyond ADR-0019's as-of reads. Rejected against *engine neutrality* and cost: a full version-history audit log is heavy and unneeded for reversibility; ADR-0019 already serves point-in-time reads. Revisit if a true history/point-in-time-reconstruction requirement (beyond as-of reads) arrives.
- **Keep hard delete as the maintenance primitive** — the status quo. Rejected: it makes every graph-repair mutation irreversible, which is the problem this decision exists to fix.

## References

- ADR-0018 — retraction + convergence; the hard-delete / no-audit-log consequence this ADR partially supersedes, and the revisit trigger it named.
- ADR-0022 — engine neutrality; backend-dependent atomicity.
- ADR-0019 — bi-temporal knowledge entities (as-of reads), distinct from audit history.
- RFC-0009 — knowledge-graph retraction analysis.
- RFC-0014 — canonical knowledge-graph identity; transactional-consolidation precedent for the maintenance apply.
- `docs/specs/graph-maintenance-api/` — the feature raising this decision.
- `core/domain/src/provenance.rs:67` (Provenance); `core/domain/src/knowledge.rs:201-248` (entity/relationship); `adapters/sqlite/src/knowledge/` (hard-delete storage).
