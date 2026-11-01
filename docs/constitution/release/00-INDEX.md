# Release sub-constitution

SUBORDINATE to `docs/constitution/00-INDEX.md` — one constitution, one root;
this pack narrows it locally with ENGRAM-GROUNDED law: every rule below is
false for a different codebase and carries repository evidence. Reference
grammar: `governs` / `refines` / `enforced-by` / `see`. Source analysis:
`docs/research/software-constitution/14-engram-grounded-packs.md` §2.7.

## Charter

Published/installed behavior, not a fictional production cluster: compatibility classification per surface, npm/crates/bundle reconciliation to source revisions, owner approval + L3 released event, rollback path.

## Structure (one home per concern)

| Concern | Lives at |
|---|---|
| Knowledge | `knowledge.md` (ASD-STE100 descriptive register) |
| Rules | `rules/<ID>.md` — one rule per file (procedural register) |
| Deviations | below; global: root `DEVIATIONS.md` / `DECAY-WINDOWS.md` |

## Rule index

| ID | Rule (one line, STE) | File |
|---|---|---|
| REL-101 | `@engramlabs/mcp` releases MUST keep launcher/platform package versions and the binary's compiled `ENGRAM_BUILD_VERSION` in lockstep across linux-x64-gnu, linux-arm64-gnu, darwin-arm64 and win32-x64. | `rules/REL-101.md` |
| REL-102 | Release acceptance MUST exercise the actual packed launcher plus platform tarball in isolation, verify version and slim startup and validate the authored skill set/hygiene; | `rules/REL-102.md` |
| REL-103 | An `@engramlabs/mcp` tag release MUST select a fresh immutable launcher/platform version from the exact `ci.yml`/`security.yml`-reviewed revision; | `rules/REL-103.md` |
| REL-104 | npm publication MUST reconcile source revision and accepted artifact hashes through registered `publication.released` evidence, retain checksums, and verify every launcher/platform version becomes registry-fetchable; | `rules/REL-104.md` |
| REL-105 | Factory merge MUST consume explicit owner approval bound to the reviewed snapshot, fast-forward the approved object rather than a movable branch, and append `job.released` after merge. | `rules/REL-105.md` |
| REL-106 | Accepted memory/retrieval/evaluation v1 changes MUST preserve field/enum meaning and optionality or introduce a new contract version; | `rules/REL-106.md` |
| (see rail) | 04-distribution-drift.md | `rules/rail-distribution-drift.md` |

## Deviations & decay (domain)

- none open
- Global exceptions: root `DEVIATIONS.md` (zero-deviation law) and
  `DECAY-WINDOWS.md` (master ledger).

## Notes

- Enforcement claims verified against gate code + fixtures in the source
  analysis; partial enforcement is stated, not smoothed over.
- The ci/ pack holds the reciprocal enforcement view; the machine catalogue
  is `docs/contracts/constitution/v1/constitution-catalogue.json`.



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
