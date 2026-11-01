# Adversarial review: federated software constitution

**Verdict: proceed with changes, not as five independently conforming constitutions.**
This is a design review, not ratification. Preserve the root and all nine existing
rails, their identifiers, histories, scope and current enforcement claims. Add domain
packs only as subordinate, indexed parts of one project constitution. An additive
layout is materially different from moving the existing rails: several failures below
are silent omissions even if no old path moves.

## 1. Circular references: distinguish authority from information

Product requirements constrain architecture; architecture feasibility constrains product;
build implements architecture; build and test feedback can force changes to both. A
purported DAG of *all references* breaks as soon as product links to an architecture
realization and architecture links back to the product rationale. Bidirectional
references are desirable traceability, not a circular definition of authority. RULE-6
itself calls for bidirectional integration (SPEC.md §4).

**Attack:** make the architecture↔product links normative imports in both directions and
a conflict cannot be resolved by topological sort. Worse, a pack could indirectly
amend its own superior. Separate edge types: (a) an acyclic authority/inheritance
relation, root → domain packs, where the root prevails; (b) non-authoritative
`implements`, `motivates`, `tested-by` and backlink references, which may cycle.
A downstream pack may add constraints within scope, not repeal the root or change
another pack through a link. On sibling conflict, stop and obtain owner adjudication;
no implicit product-over-architecture precedence. Check references resolve and their
types are valid, but don't reject traceability cycles. A root index pointing down and
a child pointing back up are not an authority cycle.

## 2. Coverage and the standard: a naming trap

The upstream [SPEC.md](https://raw.githubusercontent.com/Software-Constitution/spec/main/SPEC.md)
v1.0 draft §4 says **any conforming constitution** MUST contain at least one rule of
each of six classes; §5 L1 also requires all six plus §3 artifacts, and L2 requires a
blocking gate for **every declared rule**, not merely one per class. It does not
specify federated sub-constitutions or grant them inherited conformance. Therefore a
child presented as independently conforming needs to meet the full standard on its
own; inheritance is a proposed local composition rule, not an upstream exemption.
For this design use domain packs as parts of one project constitution: all six classes
remain addressed at the root/project level, and each pack addresses only the classes
relevant to its own rules. Don't pad each pack with six copied slogans, or claim SCCT
certifies the composed hierarchy. The root retains universal applicability.

The root index pins the six-class interpretation to SPEC.md §4 v1.0, but its canonical
URL points at mutable `main`, not an immutable revision. Pin an actual commit or
vendored digest before relying on future comparisons. §3 requires repo-root
`constitution/`; the root index amendment explicitly records the `docs/` relocation
and SCCT L0 consequence. Internal structural checks are not an upstream L1 verdict;
filename matches or a six-class gate count cannot prove L2. The index has conflicting
historical L1/no-conformance statements and stale `docs/contracts/docs/...` paths;
resolve those as existing documentation debt, not evidence of current conformance.

## 3. Duplication bait: ownership by obligation, not by audience

Assign each normative obligation one canonical rule ID and one owner pack; other packs
link to that rule without restating its MUST. Examples:

| Question | Canonical owner | Other pack's legitimate reference |
|---|---|---|
| What artifact is built and its reproducibility properties? | build | CI invokes the declared build; testing tests that artifact |
| What unit/contract tests or evidence establish correctness? | testing | build supplies testable outputs; CI executes selected tests |
| When, where and with what blocking policy are gates run? | CI | testing defines the test and expected evidence, not pipeline scheduling |
| What behavior is required? | product | architecture realizes it; testing traces cases to it |
| What dependency boundaries realize it? | architecture | build checks the dependency boundary using the canonical rule |

A build rule requiring a unit test and a testing rule requiring the same unit test are
two owners of one obligation. A build command that *runs* tests is not itself a second
normative test requirement. CI can own an independent, explicit merge-blocking
obligation referring to the testing rule. Maintain one rule-to-owner-to-gate mapping,
check global ID uniqueness, and review overlaps semantically: a string-level duplicate
check cannot detect paraphrased contradictions. Keep root rules canonical rather than
copying them into all five packs. The existing PC-002/OR-004 shared artifact gate and
PR-003 architecture guard illustrate one gate covering multiple distinct rules, not a
need for duplicate rules.

## 4. Breakage inventory: additive versus relocation

Evidence is the named files and direct references below; conditions matter. Search
included tracked files and active `.agents` content, excluding historical worktrees.
No assertion is made that arbitrary external consumers have been enumerated.

* **Silent omission even with root intact:** `tools/ci/check_constitution.py:33-38`
  discovers only root `0[1-9]-*.md`, checks only filenames mentioned in the root
  index, and requires only one rail. It neither discovers nor validates child indexes,
  child rails, missing back-links, duplicate IDs, or a complete root inventory. It
  checks root `DEVIATIONS.md` and `DECAY-WINDOWS.md` only. Defining independent child
  exception files would silently bypass RULE-5 and expiry. Keep exceptions centralized
  or explicitly implement and test aggregate enforcement.
* **New IDs rejected / advisory finding:** `tools/ci/check_job_artifacts.py:52-70`
  scans only `docs/constitution/0*.md`, extracting two-letter/three-digit IDs from all
  text, including the index, not definitions. A new ID solely in a child is unknown;
  mentioning it in the root index can mask the bug. The command exits nonzero only
  with `--strict`; `.github/workflows/ci.yml` invokes it without that flag. Existing
  root IDs stay recognized if old rails stay in place. Scan canonical definitions via
  the declared inventory, not recursive arbitrary prose; test unknown and duplicate IDs.
* **Registry blind spots:** `docs/contracts/constitution/v1/gates/registry.json`
  references the root index and maps rule IDs to gates, but has no pack inventory;
  `check_constitution.py:97-108` checks file existence and only three classes, not
  rule existence or coverage. `components/registry.json` references rail 08 and has
  no constitution-pack entries; `check_coherent_integration.py` inventories Cargo
  members and TS packages, and verifies registered directories and gate files, not
  child documents or backlinks. Adding a pack does not automatically fail either gate
  or prove RULE-6 coverage. Define an explicit pack/rail seam and enforce both ways.
* **If rails move, rather than remain intact:** the five registries'
  `constitution_ref` values reference the root index or rails 03, 04, 06 and 08;
  `00-INDEX.md` relative rail links, rail-local references and external references
  must be updated or kept resolvable. The umbrella gate's root-only glob can miss
  moved rails; old job artifacts can fail ID lookup. The implementation, events and
  distribution registries and their consumers (`check_implementation_registry.py`,
  `check_canonical_evidence.py`, `check_constitution.py`) use the fixed
  `docs/contracts/constitution/v1` tree; moving registries is a separate migration,
  not required for adding packs. Both constitution and coherent-integration gates
  load the fixed gates/components registry paths.
* **IDs and templates:** `docs/contracts/constitution/v1/schemas/sdd-artifact-frontmatter.v1.json`
  constrains IDs to `^[A-Z]{2}-[0-9]{3}$`; the job gate uses the same shape.
  Pack-qualified IDs or longer prefixes require a coordinated schema, parser and
  historical-compatibility change. `.agents/skills/engram-factory/assets/templates/`
  (`brief`, `spec`, `plan`, `tasks`) carry `constitution_rules_touched`; their
  `[]` syntax survives additive packs, but instructions must explain resolution and
  ownership. `.agents/skills/engram-frame/SKILL.md`, factory analyzer agent briefs,
  and `.agents/skills/engram-factory/references/context.md` must teach discovery;
  context.md's discipline-to-root-rail mapping does not discover new packs.
* **CI and distribution:** `.github/workflows/ci.yml` calls the existing umbrella,
  coherent-integration and advisory job gates; adding a checker without wiring it
  gives no merge protection. Agent skill changes also affect launcher distribution
  reconciliation (rail 04 / `check_distribution_drift.py`). `AGENTS.md`,
  `docs/README.md`, `tools/README.md` and root index are human entry points to audit;
  old historical research references should not be silently rewritten as current law.

A migration test must plant a child-only valid ID, an unknown ID, duplicate ID,
missing child index/backlink, unindexed rail, missing gate, and an active exception;
assert the intended blocking behavior, plus unchanged recognition of old IDs.

## 5. Genericity: separate portable mechanism from Engram policy

The standard describes a language-, runtime-, provider- and domain-agnostic framework;
it does not make every Engram rule universal. Do not put these into a generic template:
EN-001's SQL/engine lint paths; SP-001's `EngramProvider`, N-API and
`CapabilityReport`; CI-004/PR-001's Engram domain-model location and generated TS;
DD-001/002's npm, crates.io, MCP and launcher bundles; PR-002's Rust/TypeScript split;
PR-004's pgvector/adapter grid; PR-005's memory-operation policy paths; PR-006's
embedding/reindex/fusion choices. Even RULE-2's published-library interpretation is
an Engram adaptation, not proof of a production reconciliation gate. A generic
mechanism may offer typed rule IDs, ownership, trace links, exception handling and
configurable evidence/gate declarations; projects supply their own actual rules,
scopes, paths and applicability. Generic must not mean an empty, automatically green
pack: missing applicable declarations and unverifiable gates need explicit findings.

## 6. Self-hosting: governance of the governance checker

The gate, registry, event producer and CI workflow are themselves governed components.
A change can weaken its own checker, delete the pack from discovery, relabel a gate
nonblocking, or alter both rule and proof in one green commit. Current umbrella checks
that gate paths exist and that three classes have an `existing` blocking declaration;
it does not establish that CI actually runs them or that rule semantics are enforced.
The component gate does not inventory individual rules/gates bidirectionally. The
root's CE-001 promises amendment evidence, but `check_canonical_evidence.py` and
factory events should not be assumed to prove every amendment path merely by existing.

Break the recursion operationally: require owner approval and independently reviewed
base-versus-proposed inventory, scope and enforcement diffs for rule, registry, gate
and workflow changes; run the last accepted checker on the proposed tree *as well as*
the candidate checker; use independent mutation fixtures to prove removals and
violations fail. The old checker cannot understand a genuinely new schema, so stage a
backward-compatible transition and explicitly review its exception rather than treating
the old green result as proof. Protect the approval/CI configuration outside the code
being changed where repository controls permit. Record amendment identity, actor,
revision, artifact hashes and both results in canonical evidence. If bootstrap or
emergency repair is necessary, owner adjudicates under the existing zero-deviation
policy: an active deviation remains a failing condition, never a silent bypass.

## 7. Required changes before build

1. Ratify one root constitution with subordinate domain packs, authority precedence,
   typed traceability links and explicit sibling-conflict escalation; preserve all
   nine root rails and their stable IDs without semantic duplication.
2. Define one canonical rule owner, global IDs, pack inventory, applicable scope and
   rule-to-gate coverage; fail closed on missing/dangling/duplicate declarations.
3. Extend the actual scanners, ID validation, templates/context, registry seam and CI
   wiring together; centralize deviations/decay or implement equivalent aggregate
   enforcement. Prove the negative cases above and backward compatibility.
4. Separate reusable generic mechanics from Engram-specific policy; make applicability
   and unverified obligations visible, not assumed satisfied.
5. Protect self-amendment with owner review, base/candidate checks, mutation evidence
   and a documented bootstrap path. Pin the upstream standard immutably and report
   upstream SCCT results separately from local discipline.

## Index evidence

`engram-mcp recall --query 'constitution gates rule registry'` returned `hit_count: 0`:
**index empty for constitution gates rule registry**. A second recall for
`federated constitution rule gate` returned `hit_count: 0`: **index empty for federated
constitution rule gate**. The hook's suggested `search` command is unsupported by the
installed binary (unknown argument); findings above are grounded in repository source
and the upstream SPEC text rather than invented index results.
