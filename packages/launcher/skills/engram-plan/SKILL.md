---
name: engram-plan
description: Plan a change by mining the engram index first — past issues, patterns, invariants, blast radius — using the right workflow for the job. Triage (classify + route), Bugfix (root cause + regression guard), or Story/Feature (precedents + per-layer steps). Trigger when asked to solve/plan/triage/implement anything non-trivial, before editing.
---

# engram-plan

Plan from evidence, not vibes. The index already knows this repo's
structure, invariants, history, and past failures — mine it before
writing a line of the plan.

**Pick the workflow first** — they are different disciplines:

| The request smells like… | Workflow | Reference |
|---|---|---|
| "this is broken", unclear severity, red CI | **Triage** — classify + route, don't fix | `references/plan-triage.md` |
| Reproduced failure, regression, crash | **Bugfix** — root cause + regression guard | `references/plan-bugfix.md` |
| New capability, story, integration | **Story/Feature** — precedents + per-layer steps | `references/plan-story.md` |

All three share the moves below; the references carry each one's
specific discipline.

## Contract

1. Mine before writing: past work, past failures, invariants, blast radius, tensions.
2. **Every plan starts with a past-pattern search** — a fix/feature that ignores how this repo solved it before is planning amnesia.
3. Findings are evidence-labeled; risks are quantified from the graph.
4. Every step is independently verifiable — it names the command that proves it.
5. The plan is written to memory so `engram-implement` can recall it; recurring shapes become procedures.

## Workflow (shared moves)

### 1. Frame the change

One paragraph: what changes, for whom, what "done" means. Ambiguity →
list your interpretation in Open Questions; don't silently pick.

### 2. Search the past (engram-search discipline)

```
recall({ "query": "<domain terms> fix design decisions" })
search({ "query": "<likely touchpoint symbols>" })
procedure_list({})
contradiction_list({})
```

Mine for: past fixes with the same symptom · precedents for the same
feature shape · indexed invariants the change must respect · existing
runbooks (if one covers this, following it beats planning) · open
tensions. **Record the search result either way** — "no past
occurrences" is a finding.

### 3. Map the surface

```
architecture({ "limit": 15 })
symbol_context({ "symbol": "<touchpoint>", "depth": 2 })
change_impact({ "target": "<touchpoint>" })
```

Blast radius per touchpoint → Risks. Crossed layers → explicit
per-layer steps.

### 4. Write the plan (fixed sections)

**Overview** (the repo's vocabulary) · **Findings** (labeled, including
the past-pattern search result) · **Risks** (blast radius + invariants
threatened + mitigations) · **Steps** (ordered, each with its verify
command) · **Open Questions** (what the index could NOT answer).

### 5. Persist plan + workflow

```
write_memory({ "content": "PLAN <slug>: <overview>. Precedents: <found |
  none>. Steps: <N>. Top risks: <...>" })
```

When this shape recurs, capture the workflow that worked:

```
procedure_put({ "name": "plan-<workflow-type>", "trigger": "<when this
  shape appears>", "steps": ["<the moves that worked, in order>"] })
```

## When it fails

- No index (recall/search empty) → plan against `scan_repo` + reads,
  findings marked `[guessed]` where the graph would have proven them.
  Suggest engram-index.
- A recalled past fix contradicts the request's assumptions → surface
  it before planning; the past fix is evidence, not noise.

## Anti-patterns

- Skipping the past-pattern search (the #1 planning failure).
- Triage that fixes, bugfix plans without a regression test, story
  plans without a precedent search.
- Steps with no verification command.
- Risks without graph evidence ("might be tricky" is not a risk).
- Planning past an open contradiction without flagging it.

## Final check

- Right workflow for the request?
- Did the past-pattern search run, and is its result in Findings?
- Could a fresh agent implement from Findings + Steps alone?
