# Scan Reliability + Code-Intel Honesty

- **Status:** Shipped <!-- Draft | Implementing | Shipped | Deferred -->
- **Spec:** scan-reliability
- **Area:** `mcp/engram-mcp` (scan_repo, graph_health), `codegraph/queries`, `core/code`
- **Depends on:** engram-code T0–T12 (shipped; the running pre-T5 binary is the
  trigger for this spec — see Problem)

## Problem

Live evaluation against a populated store (30k entities, 57k relationships,
2026-08-26) found four defects in the scan → code-intel path:

1. **`scan_repo` never returns within client timeouts on a populated store.**
   Two O(store) costs ran inside the tool response:
   (a) post-commit, the handler re-fed *every* entity in the scope to the
   lexical lane (`list_entities` → `upsert_batch`, ~17.7k entries on the eval
   store) on every scan; (b) the embed step listed *every* chunk in the scope
   and embedded the entire un-embedded backlog — on the eval store ~81.6k
   legacy chunks (some megabyte-sized) through the shared model mutex, at a
   measured ~0.05 chunks/sec. Cost was O(store), not O(scan delta); the tool
   timed out while work continued server-side with no cancellation. Even a
   2-file probe directory hit the 60s adapter timeout.
2. **Maintenance tools crash on a missing `scope`.** `graph_health` (and the
   other maintenance tools) deserialize `args["scope"]` strictly — `null`/absent
   yields `invalid type: null, expected struct Scope` instead of defaulting to
   the launch scope every other tool uses.
3. **Agent-facing output is unbounded.** `code_health` returned 138 KB (3,452
   dead symbols inlined) and `architecture` 255 KB (full community map) — both
   exceed any usable agent context. The dead list was also dominated by false
   positives: React components referenced only via JSX.
4. **Analytics treat unresolved legacy edges as evidence.** Pre-resolution
   scans (built before the T5 cross-file upgrade) left name-only relationship
   endpoints (no entity id) for bare generics — `new` ×4,529, `clone`, `get`.
   `dead_code` counts them as caller evidence (false liveness) and
   `central_symbols`/`bridge_symbols` rank them as hubs (the eval store's top
   "central symbols" were `new`, `get`, `clone`, `lock`).

Additionally (cause, not defect): the running server binary predated the T5–T12
scan fixes, so its scans emitted no `calls` edges at all — `symbol_context`
returned empty contexts for every engram-repo symbol. The fix requires a
rebuild + fresh scan; this spec makes the fresh scan cheap and its analytics
honest.

## Acceptance Criteria

- [x] AC1 — `scan_repo` feeds the lexical lane only with entities whose
  provenance source matches this scan (the git-enriched `source_name`), in
  batches; the embed step is likewise scoped to this scan's source, capped at
  256 chunks per call with the remainder reported ("re-run scan_repo to
  continue"), and each chunk text bounded to 8 KiB before the tokenizer.
  Verified live: a 1,192-file scan of this repository returned in 54.7s
  (scan ~18s + embed 256) where it previously never returned.
- [x] AC2 — maintenance tools (`graph_health`, `list_maintenance_candidates`,
  `build_maintenance_plan`, `apply_maintenance_plan`) default a missing/null
  `scope` arg to the launch scope instead of erroring. Verified live:
  `graph_health({})` returns the launch-scope aggregates.
- [x] AC3 — `code_health` truncates the dead-symbol list (first 100 + `… and N
  more`) and always prints stats; `architecture` truncates the community map to
  the top 10 by member count. Verified live: 138 KB → 3.2 KB and 255 KB →
  0.9 KB responses.
- [x] AC4 — analytics-grade edges (`dead_code`, `central_symbols`,
  `bridge_symbols`, `call_communities`, `repository_stats`) count only
  relationships whose both endpoints carry resolved entity ids; navigation
  queries (`blast_radius`, `symbol_context`, `dependency_path`) keep the
  name-based edge set unchanged. Unit-tested in `engram-codegraph-queries`.
- [x] AC5 — JSX element usages (`<Button …>`) in tsx/jsx count as references
  from the enclosing component, so dead-code stops flagging JSX-referenced
  components; lowercase DOM intrinsics (`<div>`) do not emit edges.
  Unit-tested in `engram-code`.
- [x] AC6 — live verification: rebuilt binary, fresh scan of this repository
  into the populated store completed in 54.7s and 52.6s (two consecutive runs) with ~9.6k entities /
  27,150 relationships / 0 errors, and `symbol_context` for a known hub
  (`scan_repository`) returns 15+ real callers.

## Non-goals

- Scan cancellation / job model (separate slice; the delta + cap removes the
  pathological runtime but orphan handling deserves its own spec).
- Purging legacy unresolved rows from existing stores (re-scan supersedes them;
  a store-doctor slice can sweep later).
- Per-repo partitioning of analytics output. Known limitation observed live:
  legacy scans that RESOLVED bare generics (`as_str`, `lock`, `is_empty` —
  real entities, legitimately called hundreds of times) still dominate
  cross-repo centrality because their edges carry ids. Partitioning analytics
  by source repository, or re-scanning those repositories under the post-T5
  noise filter, is the follow-up.
- Tuning the ONNX thread count (fastembed 5.17 exposes no thread option);
  the per-call cap makes the response bounded regardless.

## Assumptions

- Entity `provenance.source` is stamped with the scan's enriched source name
  (`engram-mcp-scan [remote@branch:sha]`, or bare for non-git roots) — verified
  against the eval store's rows.
- Navigation queries intentionally keep unresolved name edges: suffix seed
  resolution depends on names, and dropping them changes shipped behavior
  outside this spec's blast radius.
