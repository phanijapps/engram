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
