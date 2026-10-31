# Rail 04 — Distribution drift

Rule class: RULE-2 (zero repo↔prod drift), honestly reinterpreted for a library/SDK +
MCP-server repo: "prod" = published and installed surfaces.
Gate: `tools/ci/check_distribution_drift.py` (planned, month 1).
Registry: `docs/contracts/docs/constitution/v1/distributions/registry.json`.

## Rules

- **DD-001 (MUST)**: Every declared published surface (npm package contents, crates.io
  publication set, MCP server distribution) MUST reconcile to a recorded source revision
  at the time of publication.
- **DD-002 (MUST)**: The installed agent bundle MUST match the repo-owned source it
  claims: `packages/launcher/skills/` MUST be in sync with `.agents/skills/` (authored
  source), and launcher agents with `mcp/engram-mcp/agents/`.
- **DD-003 (MUST)**: Build artifacts MUST NOT be committed to the repository tree (see
  DECAY-WINDOWS: two stale launcher tarballs are under a removal window).
- **DD-004 (SHOULD)**: Publication events (npm publish, cargo publish) SHOULD emit
  canonical evidence events (rail 06) recording the source revision published.

## Enforcement boundary

Initial gate scope: repo-side reconciliation only — launcher payload vs authored source
(directory inventory + SKILL.md name-tag comparison), no committed tarballs/artifacts,
npm `files` manifest vs on-disk payload. Registry-side publication tracking (crates.io /
npm state vs declared revision) is declared in the registry but its gate is deferred
(network-dependent checks stay out of blocking CI; run scheduled).

## Deviation path

Out-of-band publication (hotfix released from a fork) requires an immediate DEVIATIONS
entry with a reconciliation deadline ≤ 7 days.

## History

- 2026-10-31: rail locked at adoption.
