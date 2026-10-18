---
name: engram-plan
description: Plan a change (story, feature, bug fix) by mining the engram index first — similar past work, repo invariants, blast radius — then produce a fixed-section plan: Overview, Findings (evidence-labeled), Risks, Steps, Open Questions. Trigger when asked to solve/plan/implement anything non-trivial, before editing.
---

# engram-plan

Plan from evidence, not vibes. The index already knows this repo's
structure, invariants, and history — mine it before writing a line of
the plan.

## Contract

1. Mine before writing: past work, invariants, blast radius, tensions.
2. Findings are evidence-labeled; risks are quantified from the graph.
3. Every step is independently verifiable — it names the command that proves it.
4. The plan is written to memory so `engram-implement` can recall it.

## Workflow

### 1. Frame the change

One paragraph: what changes, for whom, and what "done" means. If the
request is ambiguous, list the interpretation you're planning against in
Open Questions — don't silently pick.

### 2. Mine the index

```
recall({ "query": "<the change's domain terms + 'invariants decisions'>" })
search({ "query": "<likely touchpoint symbols>" })
architecture({ "limit": 15 })
```

For each likely-touched symbol:

```
change_impact({ "target": "<symbol>" })
symbol_context({ "symbol": "<symbol>", "depth": 2 })
```

Check known tensions:

```
contradiction_list({})
```

What you're mining for: Has this repo already decided something about
this area? Which invariants does the change threaten? What's the blast
radius of each touchpoint?

### 3. Write the plan (fixed sections)

**Overview** — one paragraph in the repo's own vocabulary (the words its
graph uses, not generic ones).

**Findings** — evidence-labeled facts:

```
- [proven] scanner entry point is scan_repository in codegraph.rs
  — symbol_context: 14 callers, the top traffic path
- [likely] CI enforces engine-neutrality on this crate — recall hit
  from ADR-0022 memory (indexed Oct 2026)
```

**Risks** — each with blast radius and why it matters:

```
- Risk: touching X (bridge symbol, 12 dependents; change_impact)
  Mitigation: ...
- Risk: violates invariant "domain must not name engines" ([likely])
  Mitigation: run scripts/ci/check-engine-neutrality.sh at step N
```

**Steps** — ordered, small, each with its verification command:

```
1. Add the port method to core/<crate>/ports.rs
   Verify: cargo check -p engram-<crate>
2. Implement in adapters/.../impl.rs
   Verify: cargo test -p engram-<adapter>
3. Wire the tool (mcp/engram-mcp/src/tools.rs)
   Verify: cargo test -p engram-mcp + driver call
```

**Open Questions** — what the index could NOT answer. Unanswered ≠
unknown; it's explicitly listed.

### 4. Persist the plan

```
write_memory({ "content": "PLAN <slug>: <overview>. Steps: <one-line
  summary>. Risks: <top risks>." })
```

Then implement can `recall({ "query": "PLAN <slug>" })`.

## When it fails

- No index (recall/search empty) → plan against `scan_repo` + file reads,
  and mark findings `[guessed]` where the graph would have proven them.
  Suggest engram-index.
- Ambiguous symbols in change_impact → disambiguate via `symbol_context`
  before ranking risk; don't guess which of N same-name symbols.

## Anti-patterns

- Plans with steps that have no verification command.
- Findings without evidence labels.
- Risks without blast radius ("this might be tricky" is not a risk).
- Skipping the contradiction check before large refactors.

## Final check

- Could a fresh agent implement from Findings + Steps alone?
- Does every risk name its graph evidence?
- Are open questions honest (not rhetorical)?
