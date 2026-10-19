---
name: engram-learn
description: Distill durable learnings from a session, doc, or fix into engram — decisions with rationale, gotchas and resolutions, validated procedures, source-grounded facts, belief updates. Trigger on "remember this", "learn from this session", after a meaningful bug fix, or at the end of a substantive work session.
---

# engram-learn

Turn what just happened into durable memory. The agent does the extraction
and judgment; engram stores and recalls. Learners capture **decisions,
gotchas, and procedures** — not transcripts.

## Contract

1. No trivia: if a future agent wouldn't act on it, don't write it.
2. No duplicates: recall first; update or skip if it's already known.
3. Decisions record the *why*, not just the *what*.
4. Source-grounded facts cite their source (`store_knowledge`, not `write_memory`).

## Workflow

### 1. What kind of learning is this?

| Kind | Tool | Shape |
|---|---|---|
| Decision + rationale | `write_memory` | "Decided X over Y because Z" |
| Gotcha / resolution | `write_memory` | "X bites when Y; fix: Z" |
| Validated procedure | `procedure_put` | exact steps that worked |
| Fact from a source | `store_knowledge` | entities/facts with the doc path |
| Changed stance | `belief_get` → update or let contradiction stand | subject + new position |
| Replayable doc | `index_docs` | verbatim, chunked for recall |

### 2. Recall before writing (dedup)

```
recall({ "query": "<the learning's key terms>" })
```

If it's already there: skip (nothing new), or write only the delta
("update: also fails on Windows"), or bump the procedure's accuracy by
re-putting it.

### 3. Write

```
write_memory({ "content": "Decided to gate npm publish on the scan gate: a package is the most-attacked surface (lesson: OpenClaw). Context: RFC-0021." })
```

```
procedure_put({ "name": "re-tag-release", "trigger": "publish retry",
  "steps": ["gh release delete vX --yes", "git tag -d vX",
            "git push origin :refs/tags/vX", "git tag vX", "git push origin vX"] })
```

For stance changes, check the belief state first:

```
belief_get({ "subject": "engram-index auto-triggers" })
```

If a belief contradicts the new learning, let the contradiction stand —
`contradiction_list` will surface the tension later. Never silently
overwrite a belief with history behind it.

For source material (RFCs, ADRs, design docs) the project should recall:

```
store_knowledge({ "entities": [], "facts": [], "relationships": [], "idempotency_key": "<doc-id>" })
```

Bridge concepts to code where you can — a concept not linked to what
realizes it is an orphan. Use `search({ "query": "<term>" })` to find
real symbol names before linking.

### 4. Confirm

One read-back per critical write:

```
recall({ "query": "<the decision's core terms>" })
```

## When it fails

- `write_memory` errors → report; never retry in a loop.
- Nothing worth learning → say "nothing durable here" and stop. Silence
  is a valid outcome; forced learnings are noise.

## Anti-patterns

- Session-transcript dumps ("we ran tests, then edited X, then…").
- Duplicating what `recall` already returns.
- Unvalidated procedures (steps never executed).
- Belief overwrites that erase history.

## Final check

- Would a future agent in a fresh session act differently because of these writes?
- Every decision has its why?
- Procedures were actually run?
