# Plan workflow: BUGFIX

Root cause first, minimal fix second, regression guard always.

## When this workflow
- Reproduced failure, cause unknown or partially known
- Regression (something that used to work)
- Post-triage routing (fast lane if a past pattern was recalled)

## Steps

### 1. Search the past — every bugfix starts here
```
recall({ "query": "<symptom terms> fix root cause" })
search({ "query": "<failing symbol}" })
procedure_list({})
```
- A past fix for the same symptom = the pattern to check first
  (was it fully fixed, or did the same class resurface elsewhere?)
- An indexed sharp edge near the failure = prime suspect
- **Record the search**: "no past occurrences found" is itself a
  finding ([proven] by absence, worth saying)

### 2. Localize with the graph
```
symbol_context({ "symbol": "<failing symbol>", "depth": 2 })
change_impact({ "target": "<suspect symbol>" })
```
- The failing symbol's neighborhood = the suspect pool
- `fetch_content` the load-bearing file(s) — never theorize about code
  you haven't read this session
- If recently-changed code is in the neighborhood:
  `whats_changed {}` to see what moved lately

### 3. Root cause (the plan's Findings section)
State it as a testable claim: "X fails because Y, when Z."
Label it. `[proven]` requires either a repro trace or source evidence.
A root cause you can't state in one sentence isn't found yet.

### 4. Minimal fix + regression guard (Steps section)
- Smallest change that removes the cause (not the symptom)
- A test that fails before the fix and passes after — the regression
  guard is mandatory, not optional
- Blast radius from step 2 becomes the Risks section

### 5. Persist the pattern (procedures for recurring classes)
If this is the second+ occurrence of a failure class:

```
procedure_put({ "name": "fix-<failure-class>",
  "trigger": "<symptom signature>",
  "steps": ["<localization step>", "<root-cause check>",
            "<the fix>", "<the regression test>"] })
```
Next triage of this class should hit the runbook, not rediscover it.

## Output (hand to engram-implement)
- **Findings**: symptom, root cause [labeled], past-occurrence search result
- **Risks**: blast radius, what the regression test must cover
- **Steps**: fix + test + verify command (exact)

## Stop conditions
- Root cause unprovable after localization → plan the investigation
  (instrument, log, bisect) — don't plan a fix on a guess.
- Fix would touch a bridge symbol → plan-wide test pass is a step, not
  an afterthought.
