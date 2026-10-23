# Spec: code-edge resolution evidence

**Status:** accepted (implementation slice 1 of the top-3 enhancements adoption)
**ADR:** 0028 amendment (2026-10-22) · **Domain:** `docs/domain-data-model.md` §Code indexing

## Problem

Code edges were born from different mechanisms — AST facts, deterministic
name-table bindings, receiver-capitalization guesses, text co-occurrence,
framework-pattern synthesis, orphan-sweep heals — but the only persisted
signal was a numeric confidence. Agents could not tell a parsed structural
fact from a heuristic guess on the same edge.

## Accepted design

1. **Domain contract.** `ResolutionTier` (closed: parsed, scoped_name,
   heuristic, legacy_unknown) + `CodeEdgeResolution { tier, resolvedBy }`,
   optional on `KnowledgeRelationship` (`codeResolution`), absent on non-code
   predicates. Ordering-default confidences (1.0/0.8/0.5/0.5) are diagnostic
   defaults, never calibrated probabilities; explicit confidence is preserved.
2. **Shared ladder.** `engram_code::edge_resolution::attempt` is the single
   resolution ladder (EN-9 receiver rules, apex aliasing, static-module
   paths, same-document handling); it returns the settling branch.
   `resolve_refs` and the orphan sweep both use it. Branch → tier:
   name-table → scoped_name; receiver-capitalization → heuristic.
3. **Producer stamping** (extraction): ast-local-v1 (scoped_name),
   ast-calls-v1 (parsed, binding pending), cooccurrence-v1 (heuristic),
   framework-pattern-v1 (heuristic), parsed-imports/contains/heritage/routes-v1
   (parsed). Sweep heals: orphan-sweep-v1 with the branch's tier.
   `belongs_to` and other non-code predicates carry no evidence.
4. **Binding replaces spelling.** When the ladder binds a name-only object,
   the extraction-time `parsed` evidence is replaced by the binding tier.
   Unresolved edges keep their spelling evidence and `resolved=false`.
5. **Legacy honesty.** Rows without `codeResolution` get a conservative
   effective tier via `legacy_code_resolution`: orphan-sweep method →
   scoped_name; ingestion + confidence 0.5 → heuristic (the documented
   co-occurrence convention); ingestion + imports/contains → parsed (pure
   syntax, never ladder-bound); everything else → legacy_unknown. A tier is
   never inferred from a high numeric confidence alone.
6. **Query surface.** `EdgeEvidence` gains `tier` + `resolvedBy` (effective
   tier for legacy rows); endpoint dedup is deterministic (explicit evidence
   > legacy fallback, then confidence, then smallest relationship id).
   MCP `symbol_context` renders tier + producer on every evidence row.
   First-hop only: a multi-hop path is never stamped with one edge's tier.

## Non-goals

SCIP/LSP/compiler-precision tiers, confidence recalibration, ranking changes,
edge-history snapshots, schema columns beyond the JSON field (projection
only if a measured query needs it).

## Acceptance criteria (verified)

- [x] Every extractor producer stamps the contracted producer id (tested in
      `adapters/ingest/tests/extractor.rs`).
- [x] EN-9 dot-hinted references never bind by bare name; capitalized-receiver
      binds land heuristic (`core/code` edge_resolution tests).
- [x] Legacy classifier recognizes only proven signals; 0.9 calls edges stay
      legacy_unknown (`core/code` tests).
- [x] Evidence round-trips through relationship JSON (optional field, serde
      default; workspace tests green).
- [x] Query dedup is enumeration-order independent (`codegraph/queries` tests).
- [x] Resolve/ladder behavior regression net: existing `engram-code` +
      `engram-ingest` suites unchanged-green (apex aliasing now also applies
      to sweep healing — the documented intent, previously dead code).

## Compatibility

Additive, compatible: new optional field + new closed enums; no existing
field semantics, ids, or closed vocabularies change. Old validators with
`additionalProperties: false` on relationship payloads may reject enriched
rows (documented; no such validator exists in-tree today). Adding the Rust
struct field is source-breaking for struct literals (mechanical `None`
insertion, done across the workspace).
