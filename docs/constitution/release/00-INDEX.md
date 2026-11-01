# Release sub-constitution

SUBORDINATE to `docs/constitution/00-INDEX.md` — one constitution, one root;
this pack narrows it locally with ENGRAM-GROUNDED law: every rule below is
false for a different codebase and carries repository evidence. Reference
grammar: `governs` / `refines` / `enforced-by` / `see`. Source analysis:
`docs/research/software-constitution/14-engram-grounded-packs.md` §2.7.

## Charter

Published/installed behavior, not a fictional production cluster: compatibility classification per surface, npm/crates/bundle reconciliation to source revisions, owner approval + L3 released event, rollback path.

## Knowledge — how shipping works

Surfaces: crates.io (workspace crates), npm (@engramlabs/*: mcp launcher,
client, contracts, node), agent bundle (skills+agents installed by launcher).
Every surface reconciles to a recorded source revision (DD-001). Merge
requires: 3 PASS reviews, owner approval at the human gate, L3 job.released
event with artifact hashes, ff-only merge. Compatibility classification per
surface change (compatible/breaking) is mandatory in specs. Rollback path
stated per plan step. Tarballs never committed (*.tgz ignored).

## Rules

| ID | Candidate statement | Typed references / present enforcement | Repository evidence |
|---|---|---|---|
| REL-101 | `@engramlabs/mcp` releases MUST keep launcher/platform package versions and the binary's compiled `ENGRAM_BUILD_VERSION` in lockstep across linux-x64-gnu, linux-arm64-gnu, darwin-arm64 and win32-x64. One canonical executable per target MUST serve normal and runtime `--slim` modes, not resurrect full/slim artifact forks. | refines: DD-001, DD-002, CI-001; release stamping/packed verifier; workflow path repair needed. | `.github/workflows/release.yml:42-72,114-130`; `tools/release/README.md:3-9`; `tools/release/verify-launcher-package.py:68-87`. |
| REL-102 | Release acceptance MUST exercise the actual packed launcher plus platform tarball in isolation, verify version and slim startup and validate the authored skill set/hygiene; testing the source checkout alone MUST NOT establish installed-bundle correctness. The bytes published MUST be the bytes whose acceptance was recorded. | refines: DD-001, DD-002, CE-003; packed verifier covers startup/set/hygiene, not all installed hosts or publication-byte identity. | `tools/release/verify-launcher-package.py:31-49,58-90`; `.github/workflows/release.yml:137-158,213-235`; `tools/release/README.md:16-20`. |
| REL-103 | An `@engramlabs/mcp` tag release MUST select a fresh immutable launcher/platform version from the exact `ci.yml`/`security.yml`-reviewed revision; partial npm publication MUST NOT be retried by moving that tag to different content. A `workflow_dispatch` dry run MUST NOT be described as publication. | refines: DD-001, CE-003; release runbook/manual evidence; no executable exact-SHA security prerequisite proved. | `tools/release/README.md:11-20`; `.github/workflows/release.yml:10-22,195-207,227-240`; `.github/workflows/security.yml:8-14`. |
| REL-104 | npm publication MUST reconcile source revision and accepted artifact hashes through registered `publication.released` evidence, retain checksums, and verify every launcher/platform version becomes registry-fetchable; an npm success message MUST NOT be the final receipt. | refines: DD-001, DD-004, CE-001, CE-002, CE-003; registry polling/checksums exist; publication-event emission and hash/revision reconciliation remain gaps. | `docs/contracts/constitution/v1/events/registry.json:15`; `docs/contracts/constitution/v1/distributions/registry.json:7-34`; `.github/workflows/release.yml:171-176,277-307`. |
| REL-105 | Factory merge MUST consume explicit owner approval bound to the reviewed snapshot, fast-forward the approved object rather than a movable branch, and append `job.released` after merge. A memory assertion, automatic approver recommendation or pre-merge release event MUST NOT substitute for that decision. | refines: CE-001, CE-003; factory state checks plus check_canonical_evidence.py (record shape/stage subset). | `.agents/skills/engram-factory/scripts/factory.py:455-502`; `docs/contracts/constitution/v1/events/registry.json:9-11`; `docs/constitution/release/00-INDEX.md:16`. |
| REL-106 | Accepted memory/retrieval/evaluation v1 changes MUST preserve field/enum meaning and optionality or introduce a new contract version; draft belief/hierarchy/consolidation extensions MUST NOT be advertised as frozen v1 commitments. Requiring branch binding MUST be disclosed as an activated breaking behavior change, not hidden behind additive payload fields. | refines: CI-004, PR-001, SP-002; enforced-by: check_contracts.py (structural/examples only); compatibility/behavior review required. | `docs/architecture/domain-data-model.md:5-14,16-57,65`; `docs/contracts/v1/compatibility.md:3-34`. |

## Deviations & decay (domain)

- none open
- Global exceptions: root `DEVIATIONS.md` (zero-deviation law) and
  `DECAY-WINDOWS.md` (master ledger).

## Notes

- Enforcement claims verified against gate code + fixtures in the source
  analysis; partial enforcement is stated, not smoothed over.
- The ci/ pack holds the reciprocal enforcement view; the machine catalogue
  is `docs/contracts/constitution/v1/constitution-catalogue.json`.
