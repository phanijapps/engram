# Position A: keep each pack's knowledge.md monolithic

## Position

Keep the present pack-level files, including architecture and product. At this
size, a complete orientation is a reasonable context investment, not a context
emergency. Preserve sections and selective pack loading; do not introduce a
second fragment hierarchy without evidence that it improves task outcomes.
This defends the current loading unit, not stale text or unlimited growth.

## Measured baseline, not a guessed token count

I ran `wc -l -w -c docs/constitution/{architecture,product,dev,testing,ci,security,release}/knowledge.md`:

| Pack | Lines | Words | Bytes |
|---|---:|---:|---:|
| architecture | 304 | 2,809 | 24,112 |
| product | 297 | 2,177 | 17,580 |
| dev | 13 | 83 | 619 |
| testing | 13 | 78 | 602 |
| ci | 14 | 74 | 697 |
| security | 12 | 93 | 647 |
| release | 13 | 87 | 660 |
| **Total** | **666** | **5,401** | **44,917** |

These confirm the brief's 304/297 lines and 12–14-line smaller packs. Its
~6,000 architecture tokens and ~4,400 product tokens are **estimates**, not
measurements from `wc`: bytes/4 gives ~6,028 and ~4,395 respectively. The other
five are roughly 150–175 tokens each on that same heuristic. Actual token
counts depend on the tokenizer, especially for code identifiers and diagrams.
Architecture absorbed overview/reference material; product absorbed its full
product overview. They are larger because they carry substantial orientation.

Index evidence: I ran `python3 tools/hooks/index-mining.py`, then the prescribed
`engram-mcp recall` against storage `/home/videogamer/.engram`, project
`agentzero`: **index empty for "constitution knowledge.md monolithic progressive loading"**
(`ok: true`, `hit_count: 0`). No historical index result supports either side;
the case below rests on the inspected files and explicit trade-offs.

## 1. Context math favors one-shot orientation at today's size

Using the brief's estimates and decimal token capacities:

| Loaded material | 100k context | 200k context | 1M context |
|---|---:|---:|---:|
| Architecture: 6k | 6% | 3% | 0.6% |
| Product: 4.4k | 4.4% | 2.2% | 0.44% |
| Both: 10.4k | 10.4% | 5.2% | 1.04% |

Modern 100k–1M windows can hold this orientation alongside substantial task
material. On a 100k window, both leave 89.6k **before** instructions, history,
code, tool output, and response reservation. That is capacity arithmetic,
not proof of equal attention quality or zero latency/cost. If only 40k remains
in the working budget, architecture consumes 15%, and both consume 26%.
Repeated uncached loading also costs real tokens. Narrow fixes may need little
of this material; cache savings depend on the host, so assume none here.

Nevertheless, for framing, analysis, and planning, establishing the whole
product/architecture model is part of the task. One complete read avoids
several dependent discovery/read cycles. No measured truncation, latency, or
quality failure has been supplied that makes a 6k orientation an actual
bottleneck. A large advertised window alone would not settle this question,
but neither does a 304-line file establish a problem.

## 2. Coherence is a useful thing to buy with those tokens

Architecture puts the pipeline, crate ownership, adapter boundaries, facade,
and composition seam in one view. Product puts memory, source-grounded
knowledge, revisable belief, code intelligence, governance, and delivery
surfaces together. These relationships constrain one another: an agent
planning a retrieval change should not learn fusion while missing policy or
surface parity. The orientation is useful precisely across feature boundaries.

Splitting can improve precision, but transfers dependency selection to the
reader or router. An agent can load three plausible fragments and miss the
fourth that changes the design. Following links then requires more selection,
reads, and reconciliation. A single file does not guarantee comprehension;
it does reduce the chance of a required section never entering context.
“One file = one mental model” means shared ordering and visible relationships,
not a claim that packaging repairs contradictions.

## 3. Progressive loading already has a meaningful boundary

The [master index](../../constitution/00-INDEX.md) selects seven domain packs.
The [product](../../constitution/product/00-INDEX.md) and
[architecture](../../constitution/architecture/00-INDEX.md) INDEX tables point
to knowledge and individually named rule files. Rules are already one-per-file:
a reader can obtain the orientation, then retrieve the exact obligation.
The [factory context manifest](../../../.agents/skills/engram-factory/references/context.md)
routes frame to product, analyzer to architecture/product and relevant rails,
and planner to that evidence plus inventories and consumed ADRs.

Important qualification to the brief's consumer claim: that manifest explicitly
names INDEX files, not a universal direct `knowledge.md` load. The pack tables
provide the next hop; this is evidence of routing, not proof that every agent
reads all knowledge today. Even so, pack selection + focused rules + stage
routing is a credible progressive-loading design at present scale. We need
not make every paragraph another routing decision. Headings also permit
search/range reads without creating new canonical files.

## 4. Maintenance favors fewer canonical orientation units

“Two big files versus twenty small files” is a proposed alternative, not the
current inventory: there are seven knowledge files, only two substantial.
Splitting their text does not remove its maintenance burden. It adds shard
names, navigation, cross-links, and a coverage question: did the router include
every prerequisite? Shared definitions either require link-chasing or risk
duplication. Retired or renamed shards can become unreachable. Gates would
need to check those new relationships to provide equivalent confidence;
file-existence checks alone cannot prove semantic completeness.

The opposing benefits are real: smaller files enable clearer topic ownership,
more focused diffs, less irrelevant loading, and fewer same-file conflicts.
But sections already support focused edits, while one file lets reviewers
search related claims and inspect contradictions together. There is existing
drift: product's “SurrealDB (2nd engine)” maturity row conflicts with its
backend paragraph saying SurrealDB was retired and pgvector is the second
recipe. Absorbed architecture text also retains old relative links. These are
real content defects, not reasons to defend the text unchanged; splitting
would move them unless accompanied by a content audit. Prefer that audit over
assuming fragmentation is the cure. This position does not amend constitution.

## 5. A concrete point where I would switch sides

I would favor splitting a pack when its required one-shot knowledge exceeds
**15,000 tokenizer-measured tokens**, or **20% of the usable task-input budget**
(after fixed instructions and response reserve), whichever limit is lower,
sustained across representative consuming tasks. These are proposed decision
thresholds, not empirical laws. Earlier evidence of truncation, missed
constraints, or substantial avoidable loading cost would also justify a trial.
Compare complete-pack versus routed-fragment runs on the same tasks, counting
omitted constraints, all follow-up reads, tokens, latency, and maintenance work.
At today's ~6k/~4.4k sizes, keep the coherent units; let observed task costs,
not file-length aesthetics, trigger the next boundary.
