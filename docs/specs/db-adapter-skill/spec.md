# Spec: db-adapter-skill (Claude Code skill for adding database backends)

- **Status:** Draft
- **Shape:** mixed (skill + spec template)
- **Constrained by:** AGENTS.md (adapters are replaceable infrastructure); adapter port pattern
- **Contract:** none

## Objective

A Claude Code skill (`.claude/skills/new-db-adapter/`) that drafts a spec + plan
for adding a new database backend to engram (Postgres, Neo4j, Redis, etc.).
The skill knows the adapter port pattern (`KnowledgeRepository`, `BeliefRepository`,
`OntologyRepository`), the schema conventions, and the test harness — so a
developer says "add a Postgres knowledge adapter" and the skill produces a
ready-to-implement spec with: the trait to implement, the schema migration
strategy, the Cargo.toml deps, the test fixtures, and the boundary rules.

Supports sizing/scaling decisions: Postgres + pgvector (all-in-one), Postgres +
Neo4j (split graph), or cloud-native (Supabase, Neon, PlanetScale). The skill
asks about: target DB, vector store strategy, graph store strategy, connection
pooling, migration strategy.

## Acceptance Criteria

- [ ] `.claude/skills/new-db-adapter/SKILL.md` with the procedure.
- [ ] Invoked with "add Postgres adapter" → produces a spec + plan following the adapter port pattern.
- [ ] Covers: schema migration, connection pooling, testing, boundary rules.
- [ ] Supports cloud sizing (Supabase, Neon, etc.).
