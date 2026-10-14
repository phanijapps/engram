# Draft contract examples

These are authored examples, not accepted v1 payloads or proof that a model ran.

- [Balanced effective config](config-balanced.json) expands all profile fields.
- [Fast effective config](config-fast.json) preserves the same index settings,
  disables scoring and anchored expansion, and retrieves 20 candidates per lane.
  Dormant scorer bounds remain configured; actual pair calls are zero.
- [Scored packet](packet-scored.json) demonstrates two segment traces, a parent
  maximum score, token coverage, indexed/source identity and partial exact source.
  Its scores, counts, timing and manifest identifiers are illustrative.
- [Source fixture](source-fixture.txt) supplies the literal bytes for excerpt
  mapping and source-hash checks; it is not executable application code.

Validate config examples against `$defs/Config` and the packet against
`$defs/Packet` in the [draft schema](../../../../contracts/jsonschema/reliable-code-intelligence.draft.schema.json).
Runtime cross-field budgets, source authorization, calibration, actual token
counts and the serialized-packet limit require construction tests as specified
in the plan. JSON Schema default annotations do not implement profile merging.
