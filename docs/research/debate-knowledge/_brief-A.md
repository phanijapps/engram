# Debate brief A — FOR monolithic knowledge.md (they are fine as-is)

ROLE: You are the advocate for KEEPING knowledge.md files at their current
size (or even consolidated larger). Write your strongest honest case to:
docs/research/debate-knowledge/position-A-monolithic.md

## Ground truth to argue from (measure, don't guess)
- architecture/knowledge.md: 304 lines, ~6,000 tokens (includes absorbed
  overview + reference + domain summary)
- product/knowledge.md: 297 lines, ~4,400 tokens (includes absorbed full
  product doc)
- The other 5 packs: 12-14 lines each (~150-175 tokens)
- Consumers: engram-frame, engram-analyzer, engram-planner agents load pack
  knowledge before writing artifacts (references/context.md routes them)

## Your case must address
1. Context-window math: modern models (100k-1M tokens); 6k tokens = what % of
   a working context? Is one-shot loading actually a problem at this size?
2. Coherence cost of splitting: link-chasing, partial views, an agent loading
   3 fragments and missing the 4th; the "one file = one mental model" value
3. Where progressive loading ALREADY exists (INDEX tables, rules one-per-file,
   context.md routing) — is that enough?
4. The maintenance reality: 2 big files vs 20 small files — drift between
   fragments, orphaned shards, gate complexity to validate them
5. When would you concede? Name the size threshold where you'd flip sides.

Rules: cite the actual numbers above; no strawmen; concede real costs.
Under 150 lines. Write ONLY that file.
