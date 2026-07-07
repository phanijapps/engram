# Plan: db-adapter-skill

### T1 — Create the skill
- **Approach:** `.claude/skills/new-db-adapter/SKILL.md` following the new-spec skill pattern. Bundled assets: adapter spec template, Cargo.toml fragment, test fixture template. The skill asks: target DB, vector strategy, graph strategy, cloud provider. Produces a spec + plan in `docs/specs/<db>-adapter/`.

### T2 — First invocation: Postgres knowledge adapter spec
- **Approach:** Run the skill to produce `docs/specs/postgres-knowledge-adapter/` as the first worked example.
