# Golden eval dataset

A regression contract for engram's code-intel + mixed-mission retrieval:
one mixed-shape fixture repo (`zbot/`), one case list (`cases.json`), one
runner (`scripts/dev/golden_eval.py`). Every case encodes a behavior that
was once broken (reproduced from a real defect) or a balance-doctrine
guarantee that must never regress.

## Run

```bash
cargo build --release -p engram-mcp
python3 scripts/dev/golden_eval.py            # exit 0 = all green
python3 scripts/dev/golden_eval.py --keep     # keep the scratch store for inspection
python3 scripts/dev/golden_eval.py --binary $(which engram-mcp)   # grade an installed build
```

The runner starts ONE server process, scans the fixture, runs a warm-up
recall, then grades every case in order (single-process keeps hydration
deterministic). Exit code: 0 all pass, 1 any failure, 2 scan failure.

## What each case guards

| Case | Guards |
|---|---|
| G-EXACT-01/02 | EN-4: substring injections labeled `name-match`, never `[exact-match]`; true identifiers keep the exact tier |
| G-NOISE-01/02/03 | honest-empty for unrelated/stopword/template-word queries; no disconnected associative residue (EN-5) |
| G-RECV-01 | EN-9: unknown-receiver `.iter()` stays unresolved — never binds `Facets::iter` |
| G-ATTRIB-01/02 | EN-10/11: callers/impact at method granularity, not the enclosing class |
| G-DISCLOSE-01 | bare-name blast radius over unresolved refs is disclosed |
| G-SQL-01 | EN-15: unknown extensions lossless (`type TEXT,` trap, both markers, untruncated) |
| G-UND-01 | underscore-prefixed symbols findable |
| G-SUPPRESS-01/02 | ADR-0030: artifact kinds suppressed by default, re-admitted by `include_code_artifacts` |
| G-PATH-01 | path-shaped queries auto-include file artifacts |
| G-STOPWORD-01 | stopword policy does not break natural-query recall |
| G-EXT-01 | EN-8 balance: external calls remain visible evidence (classified, not deleted) |
| G-DOC-01 | mixed mission: docs remain first-class retrieval citizens |
| G-NEST-01 | nested functions are first-class symbols |

## Known gaps (deliberately NOT encoded)

Asserting these would fail by design; they are the documented backlog:

- **EN-14** — a function containing a nested declaration loses its own
  body from chunk text (container synthesis keeps doc + signature only).
- **EN-13/8** — navigation output carries no per-edge confidence or
  provenance (storage has it; the query layer strips it).
- **Cold-start hydration race** — the first tool call in a fresh server
  process can return empty while lanes hydrate (pre-existing; the runner
  warms up once after scan to stay deterministic).
- **No `Variable` entities** — ADR-0030 suppresses `Variable`, but the
  chunker never emits Variable-kind entities for TS/JS `const` bindings,
  so that suppression class is currently vacuous (found by this dataset).
- **Fallback files have no file entity** — unknown-extension files
  (`.sql`) scanned via the fallback get chunks but no file/module/graph
  entities, so path-shaped queries cannot reach them by name (found by
  this dataset).
- **Import aliases unlink callers** — `from x import y as z` calls record
  the alias `z`, which never resolves to `y`; aliased callers are absent
  from caller lists.

## Adding a case

1. Reproduce the defect against `main` first (a case that never failed is
   not guarding anything).
2. Add the fixture shape to `zbot/` if needed, then the case to
   `cases.json` with a `guard` one-liner.
3. Run the eval — the new case must pass on the fixed build and be
   confirmed failing on the pre-fix commit.
