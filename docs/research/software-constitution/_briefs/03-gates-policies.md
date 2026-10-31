# Task brief: Gates & policies mapping (perspective: CI enforcement / devops)

ROLE: Map the Software Constitution rule classes onto concrete, enforceable CI gates and
policies for the engram repo — including gates over the SDD artifact pipeline itself
(brief/spec/plan/tasks). The owner's real pain: jobs are noisy and slow, and nothing catches
a bad artifact until expensive review rounds. You answer the owner's **Q6: appropriate gates
and policies applicable via constitution**.

## Standard sources (fetch to verify)
- https://softwareconstitution.com/#rules and
  https://raw.githubusercontent.com/Software-Constitution/spec/main/SPEC.md

Six rule classes (conformance bar: ≥1 rule + ≥1 passing gate per class, every push):
- RULE-1 single-implementation: registry.json capabilities→canonical module + drift gate.
- RULE-2 zero repo↔prod drift: declared state == deployed state.
- RULE-3 zero stubs/TODO/mocks in production-reachable paths (static analysis).
- RULE-4 canonical-first self-governance: evidence events declared before referenced;
  undeclared refs fail CI.
- RULE-5 zero-deviation: DEVIATIONS.md Active section empty by default; non-empty = violation.
- RULE-6 no orphans: bidirectional registry↔component refs; on-disk-but-undeclared = violation.
Levels L1 structural → L2 gate-enforced → L3 self-governed (evidence events per CI run) →
L4 platform-bijected. SCCT: `npx scct <repo>`.

## The repo (cwd = /home/videogamer/projects/mem-alpha)
- Existing gates in scripts/ci/ — READ each header to learn what it checks:
  check-contracts.sh, check-docs.sh, check-engine-neutrality.sh, check-surface-parity.sh,
  check-code-docs.sh, check-bound-code-matrix.py, check-research-parity-docs.sh, tests/.
  These are the seed of the constitution gate set.
- AGENTS.md: engine neutrality, surface parity Rust facade↔N-API, crate responsibilities,
  no god modules, feature-specific invariants — each a rail candidate. Also AGENTS.md
  "Validation" section (cargo fmt/check, pnpm contracts:generate/typecheck/test/build).
- SDD pipeline: .agents/skills/engram-factory/SKILL.md drives jobs under
  .engram-factory/jobs/<NN>-<slug>/ (brief, analysis, plan, tasks, build, cycle-*/ review
  rounds, verification, release). Templates at .agents/skills/engram-factory/assets/templates/.
  Sample jobs: 002-expose-agent-skills, 006-zstd-advisory-remediation, 007-clippy-cleanup,
  010-project-bound-branch-indexing.
- NOTE: packages/launcher/skills/ mirrors .agents/skills/ (delivered payload) — a real
  drift surface for RULE-2-style enforcement.

## Deliver (Q6)
1. **Inventory & mapping table**: existing scripts/ci gate → which rule class it already
   satisfies (be honest: many are rule-class-adjacent, not exact) → gap notes.
2. **New gates proposed** — for each: name, one-line contract ("fails when…"),
   ≤10-line shell/python implementation sketch, cost (fast/slow), blocking vs advisory:
   a. SDD-artifact gates: check-job-artifacts.sh — frontmatter schema, REQ↔step↔task ID
      referential integrity, coverage-matrix totality, no TBD/TODO stubs in ratified
      sections (RULE-3 applied to specs, not just code), brief-has-owner-ratification
      before build starts (RULE-4 canonical events applied to job lifecycle).
   b. Rule-class gates for a library/SDK repo: orphan-crate gate (RULE-6: every
      crate/package/dir declared in a registry manifest — propose the registry format),
      duplicate-capability gate (RULE-1: two crates exposing the same capability),
      stub-marker gate scoped to shipped code (RULE-3), evidence-event gate for factory
      privileged actions (RULE-4: approve/rework/release events in a canonical format the
      registry declares).
   c. RULE-2 honest reinterpretation for a library+MCP-server repo: published-surface
      interpretation (npm package files vs repo list, crates.io vs workspace members,
      launcher skills/agents payload vs .agents/skills source) — or defer with a decay window.
3. **Policy set** (owner-level, not script): ratification policy (what needs owner approval
   before merge), deviation-approval policy (who, time-box, max active), tiering policy
   (S/M/L job → which gates run), amendment policy for the constitution itself.
4. **Sequencing plan**: week 1 (cheap high-signal: SDD artifact schema gate + stub gate),
   month 1 (orphan registry + RULE-1 drift), later (evidence events, L3). For each phase:
   which SCCT level engram reaches.
5. **Decay windows to open day one**: real current debt needing time-boxed trajectories
   instead of instant enforcement. Run the greps and report REAL numbers, e.g.
   rg -c 'TODO|FIXME' per crate (core/, adapters/, packages/); check .engram-factory/
   worktrees + tmp dirs and propose in-scope vs excluded.

## Rules
- BEFORE writing, run and cite actual results (or "index empty for <query>"):
  ./target/release/engram-mcp recall --query "CI gate engine neutrality parity" --json --limit 5 --storage /home/videogamer/.engram --project agentzero
  ./target/release/engram-mcp recall --query "job artifact verification gate" --json --limit 5 --storage /home/videogamer/.engram --project agentzero
  (Parent saw hit_count 0 on template-adjacent queries — record what you actually see.)
- Write full markdown to:
  /home/videogamer/projects/mem-alpha/docs/research/software-constitution/03-gates-policies.md
  (write tool, absolute path). Date 2026-10-31. Cite files read.
- Analysis only — do NOT create/modify any script; do NOT create constitution/.
- Real numbers only: run the greps you propose and report actuals.
- Under ~400 lines.
