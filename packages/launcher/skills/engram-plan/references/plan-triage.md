# Plan workflow: TRIAGE

Rapid assessment of a reported problem. Triage does NOT fix — it
classifies and routes. Time-boxed: if this takes more than a few tool
calls, it's a bugfix, not a triage.

## When this workflow
- A bug/error report arrives with unclear severity
- CI is red and nobody knows why
- "Something is broken" with no repro

## Steps

### 1. Reproduce (or fail to, honestly)
Run the failing thing. If you cannot reproduce, that IS the triage
output: "not reproducible as described; likely <hypothesis [likely]>".

### 2. Search the past (engram-search core move)
```
recall({ "query": "<error text core terms>" })
search({ "query": "<failing symbol or area>" })
procedure_list({})
```
- Has this exact failure happened before? (past fixes = fastest path)
- Is there a known sharp edge here? (indexed `[proven]` memory)
- Is there a runbook that already covers it?

### 3. Classify

| Class | Criteria | Route |
|---|---|---|
| Known + runbook exists | recall hit + procedure covers it | Follow the procedure; skip planning |
| Known pattern, no runbook | similar past fix recalled | plan-bugfix (fast lane — the pattern is known) |
| New failure mode | no past hits | plan-bugfix full workflow |
| Not a bug (works as designed) | repro shows intended behavior | Report; maybe docs/engram-learn |
| Severity: blocks release/CI | red main, broken publish | plan-bugfix immediately + flag urgency |

### 4. Record the triage
```
write_memory({ "content": "TRIAGE <one-line>: <report> → <class>.
  Root-cause hypothesis: <...> [likely]. Past evidence: <recall hit or
  'none'>" })
```
If a runbook WAS missing but would have helped, note that — it feeds
engram-learn later.

## Output
One paragraph: what was reported, what reproduced, the class, the
route, the hypothesis. No fix, no plan steps — routing only.

## Stop conditions
- Cannot reproduce → report that, stop.
- Runbook found → execute it, stop (that's not triage anymore).
- Classification clear → hand to plan-bugfix, stop.
