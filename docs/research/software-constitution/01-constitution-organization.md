# Organizing an Engram Software Constitution

Date: 2026-10-31.
Reader: the repository owner and maintainers deciding Q1, “How to organize a Constitution for this repo?”
Budget: under 350 lines; the research brief's six worked deliverables require more than the factory template's 600 narrative words.
Status: analysis and proposed adoption design, not an adopted constitution or conformance attestation.
Repository evidence: `bfa43213fe8696487b18633d5a457853dc3ba2c0` plus the inspected working tree.

## Intent and authority

[HUMAN-DECIDED] Answer Q1 for this repository through analysis only; do not create `constitution/` or modify implementation files.
[DELEGATED: the brief requests an organization design] Recommend paths, rails, gate reuse, and a phased adoption below; maintainers must ratify these proposals before enforcement.
The brief contains no LED ledger or registered factory job.
Factory `validate-artifact --artifact analysis.md` is not applicable to this owner-commissioned research artifact.
Facts carry [proven], [likely], or [assumed]; recommendations are not facts about present compliance.

## 1. Concrete organization

### DEC-001 — Paths and ownership

[DELEGATED: reuse existing versioned contracts and gate infrastructure] Set `<spec-root>` to `contracts/constitution/v1` and `<gates-root>` to `scripts/ci`.
Do not use legacy `contracts/schemas/`, which `contracts/README.md` explicitly prohibits extending.
Keep governance schemas separate from public memory API schemas in `contracts/v1/` and generated TypeScript in `packages/contracts/`.
The implementation registry indexes canonical owners; it does not copy domain definitions or replace the ADR-0022 capability × engine model.
Reject a second `scripts/gates/` tree because it would split the existing CI ownership boundary.

```text
constitution/
  00-INDEX.md
  01-engine-neutrality.md           # engine boundaries and permitted composition zones
  02-surface-parity.md              # Rust facade, N-API, MCP, CapabilityReport
  03-canonical-implementations.md   # one owner per capability; generated contracts
  04-distribution-drift.md          # source ↔ release artifacts ↔ managed installations
  05-production-completeness.md     # no reachable stubs/placeholders/mock behavior
  06-canonical-evidence.md          # privileged actions and event-family declarations
  07-zero-deviations.md             # empty Active section; expiry semantics
  08-coherent-integration.md        # registries ↔ crates/packages/transports/gates/assets
  DEVIATIONS.md
  DECAY-WINDOWS.md
contracts/constitution/v1/
  implementation/registry.json
  events/registry.json
  gates/registry.json
  distributions/registry.json
  components/registry.json
  schemas/software-constitution-implementation-registry.v1.json
  schemas/event-registry.schema.json
  schemas/gate-registry.schema.json
  schemas/distribution-registry.schema.json
  schemas/component-registry.schema.json
  schemas/evidence-event.schema.json
scripts/ci/
  check-contracts.sh                # existing
  check-engine-neutrality.sh        # existing
  check-surface-parity.sh           # existing
  check-docs.sh                     # existing documentation umbrella
  check-code-docs.sh                # existing rustdoc/TypeScript documentation checks
  check-bound-code-matrix.py        # existing bound-code behavioral executor
  check-research-parity-docs.sh     # existing research documentation check
  check-constitution.sh            # proposed schema/index/zero-deviation umbrella
  check-implementation-registry.py  # proposed canonical-owner/import drift gate
  check-distribution-drift.py       # proposed distribution reconciliation
  check-production-completeness.py # proposed production-reachable static analysis
  check-canonical-evidence.py       # proposed canonical-first/event validation
  check-coherent-integration.py    # proposed bidirectional component coverage
```

Every path marked “proposed” and every constitution/governance-registry path above is a design, not a file created by this analysis.
Use JSON Schema 2020-12 for the new structured artifacts, with the upstream registry schema pinned to a reviewed revision.
Keep the upstream implementation-registry shape: `schema`, `version`, and `concepts` containing `concept_id`, `name`, `ssot_module`, and `transport_wrappers` with `module`, `transport`, and `must_import`.
A capability's canonical behavior may delegate through a port to selected adapters; SQLite and pgvector are legitimate distinct engine implementations, not automatically competing copies of one behavior.
Register engine-specific implementations under distinct scoped identifiers while keeping shared policy, ranking, and orchestration canonical.
Do not use that distinction to relabel duplicate business logic as a new capability.

### Day-one exception files

`DEVIATIONS.md` starts with exactly `## Active deviations` followed by no entries, then `## Closed deviations` with no historical entries invented.
Do not put “None”, a table header, or an explanatory bullet inside Active; put explanatory prose before the heading.
A future exception records the rule, scope, rationale, approving maintainer, expiry, and closure evidence, but an Active entry still makes RULE-5 fail.
An approval is not permission to report a conformant green build while that entry remains active.
`DECAY-WINDOWS.md` starts with no active windows and explains the required fields: rule reference, measured baseline, closure trajectory, deadline, responsible maintainer, and gate behavior before and after expiry.
The neutrality scan's excluded layers are candidates for measured windows, not measured debt counts or authorized deadlines today.
Do not create a fictional window just to claim complete neutrality; block a broader adoption claim until maintainers measure and ratify its scope.

## 2. Filled-in example `constitution/00-INDEX.md`

The following is proposed file content, not a claim that its referenced new files or gates already exist.

```markdown
# Engram Software Constitution — Master Index

Status: proposed; no conformance level asserted.
Standard: software/constitution website v1.1; canonical SPEC.md currently labels itself 1.0 Draft/RFC.
Spec root: contracts/constitution/v1.
Gates root: scripts/ci.
Scope: Rust library, TypeScript SDK/native binding, unified MCP server, and repository-owned distribution/agent assets.

## Universal rules

RULE-1: Every named capability MUST have one canonical implementation; transports MUST import or delegate to it rather than copy its behavior.
RULE-2: Every declared shipped artifact or managed installation MUST reconcile to a recorded source revision and declared build configuration.
RULE-3: Production-reachable code MUST NOT contain stubs, placeholders, or mock behavior.
RULE-4: Privileged actions MUST emit canonical evidence; event families MUST be registered before references enter code.
RULE-5: The Active deviations section MUST remain empty; any active exception MUST fail the zero-deviation gate.
RULE-6: Every named component MUST resolve from its registry and MUST reference its canonical declaration in the reverse direction.

## Permanent rail numbers

| Rail | Rule classes | Gate ownership |
|---|---|---|
| [01 Engine neutrality](01-engine-neutrality.md) | RULE-1, RULE-6 | check-engine-neutrality.sh; check-coherent-integration.py |
| [02 Surface parity](02-surface-parity.md) | RULE-1, RULE-6 | check-surface-parity.sh; check-bound-code-matrix.py |
| [03 Canonical implementations](03-canonical-implementations.md) | RULE-1 | check-implementation-registry.py; check-contracts.sh; generated-contract diff |
| [04 Distribution drift](04-distribution-drift.md) | RULE-2 | check-distribution-drift.py |
| [05 Production completeness](05-production-completeness.md) | RULE-3 | check-production-completeness.py |
| [06 Canonical evidence](06-canonical-evidence.md) | RULE-4 | check-canonical-evidence.py |
| [07 Zero deviations](07-zero-deviations.md) | RULE-5 | check-constitution.sh |
| [08 Coherent integration](08-coherent-integration.md) | RULE-6 | check-coherent-integration.py |

## Canonical structured declarations

Implementation ownership: ../contracts/constitution/v1/implementation/registry.json.
Event families: ../contracts/constitution/v1/events/registry.json.
Rule-to-gate mapping: ../contracts/constitution/v1/gates/registry.json.
Distribution declarations: ../contracts/constitution/v1/distributions/registry.json.
Component inventory: ../contracts/constitution/v1/components/registry.json.
Validation schemas: ../contracts/constitution/v1/schemas/.

## Change and exception policy

A rail number MUST remain permanent once locked; amendments preserve rule identifiers and record supersession.
A constitution change MUST update its gate mapping and acceptance evidence in the same review.
DEVIATIONS.md records exceptions; DECAY-WINDOWS.md records temporal clauses of rules, not waivers.
Every declared rule MUST have a blocking gate before L2 is asserted.
The six class gates MUST run on every push and pull request, with merge protection requiring their success.
No passing syntax check substitutes for behavioral or deployment evidence.
```

## 3. Fully worked rail: `01-engine-neutrality.md`

### DEC-002 — An honest initial enforcement boundary

[DELEGATED: map a real rail to an existing gate] Lock the existing scanned boundary first, then widen it through separately reviewed rules and measured migration work.
[proven] `scripts/ci/check-engine-neutrality.sh` is a regex lint, not a dependency-graph proof or whole-repository SQL prohibition.
Its executable `GATED_PATHS` includes domain, memory, knowledge, retrieval, belief, hierarchy, consolidation, procedures, proposals, packs, selected integration files, and `mcp/engram-mcp/src`.
Its selected integration files are `provider.rs`, `capability.rs`, `provenance.rs`, `batch.rs`, `recall.rs`, `export_import.rs`, `observability.rs`, `knowledge_query.rs`, and `lexical_feed.rs`.
It does not scan all of `core/runtime`, `core/eval`, `bindings/node`, or `core/integration/src/config.rs`; do not infer those are proven neutral.
The comments describing “8 clean port-trait crates” are stale relative to the executable list.

### Proposed normative rail text

- EN-001 (MUST): When CI scans the neutral boundary, the gate shall reject every engine type, engine import, or SQL literal matched by the committed lint patterns in that boundary.
- EN-002 (MUST): When a pull request changes `GATED_PATHS` or the forbidden patterns, the rule review shall reject any unexplained reduction in enforcement scope.
- EN-003 (MUST): When backend composition changes, implementation owners shall preserve core port contracts and keep engine operations in declared adapter or composition zones.
- EN-004 (MUST): When a declared neutral path is absent, CI shall fail rather than silently skip that path.
- EN-005 (SHOULD): When neutrality coverage expands to a presently excluded layer, maintainers shall declare its exact paths and measured temporal obligations before asserting that layer is governed.

Gate for EN-001: `scripts/ci/check-engine-neutrality.sh`, already invoked by `.github/workflows/ci.yml`.
Gate gaps: the current script silently continues for missing paths, so EN-004 needs a fail-closed enhancement before this rail can be declared fully gate-enforced.
EN-002 needs a scope-regression comparison against the accepted baseline; EN-003 needs registry/dependency-direction validation and backend conformance tests rather than regex alone.
Those enhancements are recommendations, not changes made here.
The rule registry MUST list these gaps; a single green regex run does not discharge EN-002 through EN-005.

Positive check executed: `scripts/ci/check-engine-neutrality.sh` returned exit 0 on the inspected tree.
Negative checks proposed: forbidden engine import, `Sql*` type, and uppercase SQL literal in each scanned path class must fail; a missing declared path must fail after EN-004 is implemented.
Run those mutation tests only in an isolated fixture/worktree; this research task does not write source fixtures.

Deviation path: remove or relocate the forbidden dependency first; if temporary noncompliance remains, record a scoped, approved, expiring exception in `DEVIATIONS.md` and retain the RULE-5 failure.
A planned expansion may instead carry a ratified decay clause with a measured baseline and terminal strict gate; that clause must not retroactively legalize a new EN-001 regression.
Expiry MUST close the window or fail CI; neither a script allowlist nor an ADR alone is a green bypass.

## 4. Six-class mapping for Engram

| Class | Engram-specific instantiation | Existing evidence and missing enforcement |
|---|---|---|
| RULE-1 Zero Competing Systems | Register behavior owners in `core/*`, adapter cells in `adapters/*`, recipes in `backends/*` or declared integration composition zones, and transport wrappers in `bindings/node`, `packages/*`, and `mcp/engram-mcp`. | Neutrality and generated-contract drift checks exist; canonical-owner, duplicate-behavior, and wrapper-import checks remain proposed. |
| RULE-2 Zero Repo↔Prod Drift | Reconcile declared native/SDK/MCP release contents and managed launcher/skill installations to a source revision, artifact hashes, platform/features, and configuration. | Release and release-verification workflows exist; a constitution-scoped distribution declaration and actual installed-state comparison are not established by this analysis. |
| RULE-3 Zero production stubs | Scan shipping Rust feature targets and TS package/launcher/MCP entrypoints for TODO/FIXME/XXX, placeholder responses, and production-reachable mocks. | `check-code-docs.sh` is a documentation gate, not this gate; build the production-reachability inventory and failing static checks without counting test-only fixtures as shipping code. |
| RULE-4 Self-Governance + Canonical-First | Register governance event families before instrumenting release publication, constitution amendments, exception decisions, installation, and privileged mutation/policy decisions. | Domain lifecycle events do not alone establish governance evidence; add an event registry, undeclared-reference rejection, and canonical CI evidence receipts. |
| RULE-5 Zero-Deviation Mandate | Keep `constitution/DEVIATIONS.md` Active empty and fail on active headings or substantive active content. | Add a parser-based gate; expiry or approval never turns an active exception into PASS. |
| RULE-6 Coherent Integration | Resolve both registry→component and component→registry for Cargo crates, TS packages, engines, transports, schemas, gates, and managed skill/agent assets. | Surface parity and bound-code matrix checks cover selected seams, not the complete inventory; add on-disk discovery and reverse-reference checks. |

### DEC-003 — RULE-2 without an imaginary SaaS deployment

[DELEGATED: the brief explicitly requests a library/SDK interpretation] Define “production” as the artifacts Engram ships and the installations its own release/launcher tooling manages, not arbitrary customer machines.
Record release tag, source commit, npm or crate identity where actually published, native target, Cargo features, artifact digest, and expected package contents in distribution receipts.
On every push, reconcile the build/package candidate to its declaration; at publication and on a scheduled cadence, compare the published or managed installed artifact to that receipt.
Compare installed launcher skills/agents to `.agents/skills` and other declared source roots only after confirming the installer-owned destination manifest.
Do not guess destination directories or silently overwrite a user's customized installation.
A dry-run drift check SHOULD fail with the mismatched source/destination and declared configuration; installation repair requires separate authorization.
A package build from a commit is not evidence that the current remote npm/crate artifact or local installed agent is identical.
If no published or managed surface is selected and reconciled, RULE-2 is incomplete, not “N/A because this is a library”.

## 5. L1 → L2 → L3 roadmap

[proven] The website describes v1.1 and a passing gate per class on every push, while fetched canonical `SPEC.md` labels itself Version 1.0 (Draft), RFC, adopted 2026-05-19.
The fetched spec describes L2 gates on every merge to the default branch and RULE-2 reconciliation on a defined cadence.
Pin the adopted spec/schema/SCCT revision and record that discrepancy before making an external claim; the conservative proposal below satisfies both push and merge scheduling.

| Stage | Concrete work and exit evidence | SCCT interpretation for this repository | Effort estimate |
|---|---|---|---|
| L1 Structural | Ratify rails and topology, inventory owners, commit all required artifacts and schemas, declare all six classes, validate JSON Schema 2020-12. | Expect required paths, valid structured artifacts, and at least one declared rule per class; obtain and inspect actual SCCT JSON before asserting a level. | [assumed] 2–4 engineer-days plus owner review. |
| L2 Gate-enforced | Map every rule to a blocking gate, implement six-class gaps, add positive/negative fixtures, run every push/PR, and require successful checks for merge. | Expect L1 plus each rule's CI gate mapping/execution; independently verify required-check settings because repository files cannot prove remote branch protection. | [assumed] 8–15 additional engineer-days; debt repair and external release access may extend elapsed time. |
| L3 Self-Governed | Emit registered RULE-4 evidence every CI run, retain revision/rule/gate/result/action references, and reconstruct a sampled governance decision from its audit chain. | Expect L2 plus canonical event evidence and a re-derivable chain; inspect checker implementation and corroborate with a real CI replay. | [assumed] 5–10 additional engineer-days after L2. |
| L4 optional | Generate `platform/meta-index.generated.json` from domain declarations and test bidirectional coverage of every named surface. | Expect L3 plus generated meta-index/bijection evidence; do not label the partial RULE-6 inventory as L4. | [assumed] 5–10 additional engineer-days after L3 and a stable inventory. |

These estimates assume one engineer familiar with the Rust/TS/MCP stack; they are planning ranges, not measured commitments or approved staffing.
Website command: `npx scct /home/videogamer/projects/mem-alpha`.
SCCT execution is NOT_RUN: this analysis does not install or execute an unreviewed external npm checker.
The table states standard-defined level expectations, not verified claims about the current checker implementation or flags.
Even if SCCT reports structural L1, do not advertise full six-class operational conformance before passing per-class gates exist.
[proven] Current CI triggers on pull requests and pushes to `main`, not every branch push; Rust checks also use path-based job selection.
An always-required governance job must not disappear behind unrelated path filters or claim skipped checks are passing evidence.

## 6. Division of labor

**Constitution:** Own permanent cross-cutting constraints, stable rule IDs, the rule-to-gate mapping, canonical implementation/event/component ownership, and exception/decay semantics.
It must say what fails and how enforcement proves it, rather than become an exhaustive implementation manual.

**`AGENTS.md`:** Own agent entry instructions, repository navigation, safe workflow, and commands to run before changes.
After adoption it should link to canonical rails instead of maintaining competing normative copies; this analysis does not edit it.

**`docs/adr/`:** Own historical context, alternatives, accepted decisions, amendments, and supersession.
ADR-0003 establishes Rust/TypeScript ownership, while ADR-0022 explains engine neutrality; reconcile its recipe-submodule and consolidated-backend amendments against the actual `backends/sqlite`, `backends/pgvector`, `adapters/sqlite`, and `adapters/pgvector` layout before locking layout rules.

**`docs/specs/`:** Own feature-specific requirements, acceptance criteria, implementation plans, and verification evidence.
Specs cite relevant constitution rule IDs but do not grant themselves permission to bypass a rail; `docs/architecture/domain-data-model.md`, accepted `contracts/v1/`, and generated `packages/contracts/` retain their distinct contract roles.

## Evidence and verification record

| ID | Observation | Evidence |
|---|---|---|
| EVD-001 | [proven] The three mandated recalls succeeded with no hits. | Commands below; `ok:true`, `hit_count:0`, `results:[]` for each. |
| EVD-002 | [proven] Existing gates predate the proposed constitution. | All seven brief-listed `scripts/ci` files inspected; CI calls neutrality/parity, contracts, docs, generation drift, and Rust/TS checks. |
| EVD-003 | [proven] Neutrality and parity gates pass their current checks. | Executed both scripts: exit 0 each; parity reports 25 capabilities, 2 special cases, 0 acknowledged debt. |
| EVD-004 | [proven] Parity checks names rather than every runtime operation. | `check-surface-parity.sh` compares facade `require_*` with N-API proxies and exempts knowledge/consolidation special surfaces. |
| EVD-005 | [proven] Index-mining guidance is stale for this binary's code-search command. | `python3 tools/hooks/index-mining.py` ran; searches for `EngramProvider` and `ReciprocalRankFusion` failed with `unknown argument: search`. |
| EVD-006 | [proven] Website and canonical-spec version labels differ. | All four standard sources fetched on 2026-10-31; no version label silently corrected. |

Required recall invocation pattern, run separately with each query:
`./target/release/engram-mcp recall --query "<query>" --json --limit 5 --storage /home/videogamer/.engram --project agentzero`.
- index empty for constitution gates — elapsed 456 ms.
- index empty for engine neutrality lint — elapsed 455 ms.
- index empty for ADR-0022 backend recipe — elapsed 562 ms.
Empty memory results are not proof that no relevant code or prior decisions exist.
The code-search failure prevents graph-derived blast-radius claims; local scripts, paths, and ADRs supply the evidence used here instead.

## Requirements and THE acceptance map

- REQ-001 (MUST): When maintainers adopt the organization, the constitution shall declare all six classes with stable rails, canonical registries, and rule-to-gate references.
- REQ-002 (MUST): When CI detects a governed violation, the mapped gate shall fail and prevent a conformant merge claim.
- REQ-003 (MUST): When distribution or privileged governance changes, the system shall reconcile declared artifacts and produce registered evidence before asserting L3.
- NFR-001 (MUST): When this research is delivered, it shall modify only the assigned output and remain below 350 lines.

| AC | Given | When | Then | Trace |
|---|---|---|---|---|
| AC-001 | Existing repo and standard | Owner evaluates the proposal | Sections 1–4 supply paths, index, worked rules, six-class mapping, and explicit gate gaps. | REQ-001; DEC-001; DEC-002 |
| AC-002 | A proposed adoption | A rule lacks enforcement or has an active exception | Section 5 prevents an unsupported L2/full-conformance claim. | REQ-002; DEC-002 |
| AC-003 | SDK/MCP distributions rather than SaaS | Release or privileged action changes | Sections 4–5 require distribution reconciliation and canonical evidence, not an invented deployment. | REQ-003; DEC-003 |
| AC-004 | Analysis-only authorization | Artifact is checked | Only this research file is written and its line count is below 350. | NFR-001 |

## Negative evidence, risks, and review applicability

- Omitted: SCCT installation/run and checker-code audit — analysis-only research avoids unreviewed package execution — owner: adopting maintainer — revisit before a level claim.
- Omitted: remote branch protection, npm/crates publication state, and managed installed-state audit — no authoritative environment inventory/access supplied — owner: release maintainer — revisit at RULE-2/L2 adoption.
- Omitted: full build/tests, production reachability audit, and mutation fixtures — this task changes no implementation — owner: gate implementer — revisit before locking affected gates.
- Omitted: graph blast-radius enumeration — the installed CLI rejects the documented search subcommand — owner: index-tool maintainer — revisit after a supported graph-query interface is selected.
- RISK-001: Conflicting topology guidance can encode stale architecture; the adopting maintainer must reconcile ADR amendments, AGENTS instructions, executable paths, and dependency evidence before locking structural assertions.
- RISK-002: Regex and proxy-name success can mask semantic leaks or missing operations; the gate implementer must add negative fixtures and behavioral transport coverage before broad compliance claims.
- RISK-003: Unmeasured legacy debt can invalidate the estimates; the adopting maintainer must baseline debt before approving time-boxed clauses.

Review applicability: code for rule/gate accuracy; security for privileged-action evidence, production completeness, and installation permissions; platform for CI/release/managed-install reconciliation.
These are recommended adoption reviews, not completed independent reviews or named human approvals.
No live deployment inventory is asserted for this local research task.

## Glossary

- Rail — a permanent-numbered constitution document containing enforceable rules.
- SSOT — the single canonical owner of a concept or behavior, not a duplicate registry implementation.
- Gate — an executable check that fails on a governed violation.
- Deviation — a scoped exception whose active presence still violates RULE-5.
- Decay window — a time-dependent clause of a rule with baseline, trajectory, deadline, and terminal enforcement.
- Distribution — a shipped artifact or explicitly managed installation, not every downstream customer's environment.
- Canonical evidence — schema-valid events referencing registered families and sufficient inputs/results to reconstruct governance decisions.

## Standard sources

Retrieved 2026-10-31:
- https://softwareconstitution.com — especially `#what`, `#rules`, and `#levels`; v1.1 website overview, six-class push bar, and SCCT command.
- https://softwareconstitution.com/principles — rationale for canonical implementations, enforceable boundaries, and governance rather than prose alone.
- https://softwareconstitution.com/adoption — staged adoption guidance; its installation framing is not a measured effort estimate for Engram.
- https://raw.githubusercontent.com/Software-Constitution/spec/main/SPEC.md — normative artifact list, six classes, registry shape, temporal clauses, and L1–L4 definitions; fetched header says 1.0 Draft/RFC.
