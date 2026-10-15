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
scored but failed confidence filtering; `scorer_pool` means no scoring opportunity;
the other rejection reasons identify result, per-file, token, metadata-validity
or serialized-byte limits. `selected` means retained in the final primary items.
`omittedParentDecisions` reports trace rows removed by count/byte bounds; absent
legacy trace fields mean no trace was recorded, not that nothing was rejected.
Trace trimming never removes source evidence, and source/authority checks remain
the same as for the returned packet. Scores are not probabilities.

The [first live proof](../../../research/reliable-code-parent-decisions-live-2026-10-15.json)
uses two development queries with actual local models and identical pre-change
evidence. Vite `getEnvFilesForMode` was scored at -2.6768866 and rejected below
-0.97909373; `initPublicFiles` was selected at -0.97173107. This diagnostic change
does not alter calibration or claim a relevance improvement.
