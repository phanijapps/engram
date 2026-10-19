# Plan workflow: STORY / FEATURE

New capability work. The mine-first discipline matters most here:
stories die from unknown unknowns, and the index is where the unknowns
are already written down.

## When this workflow
- New feature, story, or capability request
- Refactor with a goal (not a bug, not a rename)
- Integration of a new component

## Steps

### 1. Search the past — patterns, precedents, near-misses
```
recall({ "query": "<feature domain terms> design decisions" })
search({ "query": "<adjacent existing capability>" })
procedure_list({})
```
Mine for:
- **Precedents**: was a similar feature planned/built/abandoned
  before? (abandoned plans + their reasons are gold)
- **Invariants** the feature must respect (indexed `[proven]` rules)
- **Adjacent patterns**: how does this repo solve analogous problems?
  (find the closest existing feature and mirror its shape)
- **Open tensions**: `contradiction_list({})` — building on top of an
  unresolved contradiction is planning to fail

### 2. Map the integration surface
```
architecture({ "limit": 15 })
symbol_context({ "symbol": "<primary touchpoint>", "depth": 2 })
change_impact({ "target": "<each touchpoint>" })
```
- Where does the feature ATTACH? (ports, facades, tool registries)
- What must NOT move? (bridges, invariants)
- Which layers does it cross? (crossing layers = the plan needs
  explicit steps per layer, or it will stop at the first boundary)

### 3. Design in the repo's shape
One paragraph: the feature in this repo's own vocabulary, mirroring
the closest existing pattern found in step 1. A design that invents a
new shape when a proven one exists needs a stated reason (that's an
Open Question, not a surprise).

### 4. Steps with per-layer verification
Each step lands ONE layer and names its verify command:
```
1. Domain types + contracts (core/<crate>)
   Verify: cargo check -p engram-<crate> + check-contracts.sh
2. Adapter behavior (adapters/<area>)
   Verify: cargo test -p engram-<adapter>
3. Facade/registry wiring
   Verify: <the exact test that proves reachability>
4. Agent surface (tool/skill/README)
   Verify: <driver call or surface-parity check>
```
Steps without a verify command are wishes, not steps.

### 5. Persist the plan AND the workflow
```
write_memory({ "content": "PLAN <slug>: <overview>. Precedents mined:
  <what step 1 found>. Top risks: <...>. Steps: <N>, per-layer verify." })
```
If this feature type recurs, the successful shape becomes a procedure:

```
procedure_put({ "name": "add-<feature-type>",
  "trigger": "new <feature-type> request",
  "steps": ["<mine precedents>", "<map surface>", "<per-layer steps>",
            "<verify each>", "<learn pass>"] })
```

## Output
Overview / Findings (with the precedent search result — including
"none found") / Risks (invariants + bridges) / Steps (per-layer) /
Open Questions.

## Stop conditions
- Design contradicts an indexed `[proven]` invariant → stop; either
  the invariant is wrong (an ADR-level change — say so) or the design
  is. Don't plan past a contradiction.
- No precedent found AND high blast radius → the first plan step is a
  spike/prototype, explicitly labeled as such.
