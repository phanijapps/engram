# Rail: 04-distribution-drift.md (absorbed into release)

<!-- Rule IDs unchanged; gates unchanged. -->

# Rail 04 — Distribution drift

Rule class: RULE-2 (zero repo↔prod drift), honestly reinterpreted for a library/SDK +
MCP-server repo: "prod" = published and installed surfaces.
Gate: `tools/ci/check_distribution_drift.py` (planned, month 1).
Registry: `docs/contracts/docs/constitution/v1/distributions/registry.json`.
