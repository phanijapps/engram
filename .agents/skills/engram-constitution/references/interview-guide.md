# Greenfield interview — building a base constitution with the owner

No code exists; the constitution encodes INTENT. Interview option-based,
<= 4 substantive questions per round. Mark everything as intent-until-code.

## Round 0 — the project in one page
- "In one sentence: what will this software do, and for whom?"
- "What is it explicitly NOT?" (product charter seed)
- Options help: offer 3 archetypes (library / service / tool) with boundaries.

## Per-pack rounds (skip packs the owner merges)
- product: "Name three outcomes a user must get." "What must NEVER happen?"
- architecture: "Sketch the parts in boxes — what may depend on what?"
- dev: "How will you build/test/release? Which tools are fixed?"
- testing: "What proves a change is safe to merge?"
- ci: "What must never reach main?" (the gates wish-list)
- security: "What are you protecting, from whom?"
- release: "How do users get this? What breaks them?"

## From answers to artifacts
- Outcome language -> knowledge.md (descriptive) + PRD rules (EARS + STE).
- "Never" answers -> rules with counterexamples from the owner's scenario.
- Tool choices -> dev/release rules; merge decisions -> pack merges.
- Close with tier/ratification: read the whole draft back; owner ratifies.

## Anti-patterns
Aspirational rules the owner will not defend; generic filler ("code must be
testable"); skipping the NOT-question; writing gates for tools not chosen.
