# Debate brief B — FOR modularizing the big knowledge.md files

ROLE: You are the advocate for SPLITTING the two large knowledge.md files
into composable topic files under knowledge/ directories, for progressive
context loading. Write your strongest honest case to:
docs/research/debate-knowledge/position-B-modular.md

## Ground truth to argue from (measure, don't guess)
- architecture/knowledge.md: 304 lines, ~6,000 tokens (absorbed overview +
  reference + domain summary + STE frontmatter)
- product/knowledge.md: 297 lines, ~4,400 tokens (absorbed full product doc:
  mission, substrates, surfaces, capability tables)
- Consumers: factory agents (analyzer/planner) load pack knowledge per job;
  every token loaded competes with spec/plan/tasks + repo code in context
- The owner's goal: "progressive loading of context" — load what the task
  needs, not everything

## Your case must address
1. Context economics: agents don't just read knowledge — they read brief +
   spec + plan + tasks + code + gates; 6k tokens of knowledge is ~6% of a
   100k window spent before work starts; worse for the 5-10% tail models
2. Selective loading wins: an analyzer writing a spec about, say, retrieval,
   needs the retrieval seam knowledge, NOT the launcher install story
3. A concrete proposed split for each big file (topic files with one-line
   purposes + an INDEX.md manifest with sizes so agents choose)
4. The fragment-coherence objection: how to prevent link-chasing and partial
   views (manifest discipline, cross-links, gate checking every fragment is
   linked from the INDEX)
5. When would you concede? Name the condition where monolithic wins.

Rules: cite the actual numbers above; no strawmen; concede real costs.
Under 150 lines. Write ONLY that file.
