# Contract examples

These authored fixtures match the accepted additive v1 shapes. Their scores and
model identifiers are synthetic; they are not proof that a model ran.

- [Balanced effective config](config-balanced.json) expands all profile fields.
- [Fast effective config](config-fast.json) preserves the same index settings,
  disables scoring and anchored expansion, and retrieves 20 candidates per lane.
  Dormant scorer bounds remain configured; actual pair calls are zero.
- [Scored packet](packet-scored.json) demonstrates two segment traces, a parent
  maximum score, token coverage, indexed/source identity and partial exact source.
  Its scores, counts, timing and manifest identifiers are illustrative.
- [Source fixture](source-fixture.txt) supplies the literal bytes for excerpt
  mapping and source-hash checks; it is not executable application code.

Validate config examples against `$defs/CodeIntelligenceConfig` and the packet against
`$defs/CodeEvidencePacket` in the [accepted schema](../../../../contracts/v1/schemas/engram-v1.schema.json).
Runtime cross-field budgets, source authorization, calibration, actual token
counts and the serialized-packet limit require construction tests as specified
in the plan. JSON Schema default annotations do not implement profile merging.

Operator files contain a partial code configuration, in JSON or TOML. Select a
file through stdio `--config <path>`, SDK `code_config_path`, or
`ENGRAM_CODE_CONFIG`; no code config is discovered in the scanned repository.
Select a profile with `ENGRAM_CODE_PROFILE`; field overrides use, for example,
`ENGRAM_CODE__RETRIEVAL__CANDIDATES_PER_LANE=50` or
`ENGRAM_CODE__FUSION__RERANK__STRATEGY=cross_encoder`.

Resolution is profile defaults, operator file, namespaced environment, then
explicit SDK fields. An omitted field preserves lower-layer values, including
reranker defaults. Configuration is captured once at startup and reported with
an effective hash and field origins. Generic `.engram/recall.json` discovery stays
separate. The HTTP runtime's existing `--config` accepts the full `EngramConfig`
JSON; its `code_config_path`/`code_intelligence` fields use the same Rust resolver.
## Inspecting rejected implementation parents

Evidence packets now include `diagnostics.parentDecisions`, capped at 200 rows
and the packet's serialized byte bound. Match `parentUnitId` to the canonical
code unit. Each row reports its best fused score, actual reranker score (null
when unscored), applied threshold (null when unvalidated), retrieved/scored
segment counts and one terminal reason. `below_threshold` means the parent was
scored but its packet failed confidence filtering; `scorer_pool` means no scoring opportunity;
the other rejection reasons identify result, per-file, token, metadata-validity
or serialized-byte limits. `selected` means retained in the final primary items.
`omittedParentDecisions` reports trace rows removed by count/byte bounds; absent
legacy trace fields mean no trace was recorded, not that nothing was rejected.
Trace trimming never removes source evidence, and source/authority checks remain
the same as for the returned packet. Scores are not probabilities.

After the packet-confidence correction, `appliedThreshold` gates the maximum
returnable primary score for the whole query. An accepted packet can therefore
include selected implementation parts scoring below that threshold. A packet
with no returnable passing parent clears its primary items; their trace reason
is `below_threshold`. Older calibration bindings become unvalidated and require
a separately sealed artifact for the new confidence contract.

The [confidence correction proof](../../../research/reliable-code-packet-confidence-live-2026-10-15.json)
records actual-model recovery of four out of five deliberately selected failed
development questions, with unchanged rejection of fourteen out of fifteen
development negatives. This is a focused regression check, not a new held-out
assessment or a beta-readiness result.

## Controlling scorer breadth and source coverage

`local_scorer.max_parents` limits distinct implementation parents;
`local_scorer.max_candidates` limits total scored pairs, including additional
retrieved segments and scorer continuations. The balanced development preset
uses 50 parents and up to 75 pairs. Every admitted parent receives one segment
before additional segments are admitted in round-robin order. This preserves
the old 50-parent frontier and avoids spending extra pair slots on more parents.

Additional retrieved segments and lazy continuation views are eligible only for
parents with typed `CodeChunkRole::Symbol` metadata. Container context retains
its first scoring opportunity; extra interface/class context does not consume
body-coverage slots. This is a language-neutral role policy, not a filename or
symbol-name heuristic. The selection policy is included in calibration binding
v3, so earlier artifacts require resealing against the new score distribution.

Equivalent environment overrides are:

```sh
ENGRAM_CODE__LOCAL_SCORER__MAX_PARENTS=50
ENGRAM_CODE__LOCAL_SCORER__MAX_CANDIDATES=75
```

Existing full JSON/TOML configs without `max_parents` default to 50 and preserve
their explicit `max_candidates`. Both fields participate in config origins and
calibration identity. A changed budget needs a matching calibration artifact
checked on development negatives; reusing an older artifact reports unvalidated.
This expands scoring of retrieved source, not discovery of missing candidates.

The [first live proof](../../../research/reliable-code-parent-decisions-live-2026-10-15.json)
uses two development queries with actual local models and identical pre-change
evidence. Vite `getEnvFilesForMode` was scored at -2.6768866 and rejected below
-0.97909373; `initPublicFiles` was selected at -0.97173107. This diagnostic change
does not alter calibration or claim a relevance improvement.

Development verification preserves Vite parent recall at 8/10 while increasing
required-range coverage from 69.55% to 80.00%. The warm sample median increases
from 10.12s to 11.07s. Negative rejection stays at 14/15 with the same threshold.
Two short-function misses remain, and these development
results do not replace the frozen held-out assessment. See the
[coverage live proof](../../../research/reliable-code-scorer-coverage-live-2026-10-15.json).
