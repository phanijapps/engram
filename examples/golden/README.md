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

The runner starts ONE server process, scans an ISOLATED COPY of the
fixture (provenance stays `engram-mcp-scan`), runs a warm-up recall, then
grades every case in order, with per-call deadlines, strict empty-envelope
detection (a `No results` line mixed with results FAILS), stderr capture,
and reaped processes (single-process keeps hydration
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
| G-SRC-01/02 | verbatim on-disk fetch is the default (nested parents whole); symbol slicing |
| G-VAR-01/02 | Variable entities exist and follow ADR-0030 suppression |
| G-PATH-03 | fallback files (SQL) have file entities — path queries reach them |
| G-ALIAS-01 | import-alias callers link to the original symbol |
| G-EVID-01 | navigation surfaces resolved-vs-unresolved edge evidence |
| G-HOP-01 | impact carries hop distances, nearest dependents first |
| G-REPO-01/02 | repository exclusion binds exact AND partial-name injections (round-1 BLOCKER) |
| G-DOC-02 | a successful Doc `get_context` packet is not assessed as failed code discovery |
| G-RECV-02 | EN-9 positive control: qualified method callers survive unknown-receiver conservatism |
| G-EXACT-03 | the true identifier is the FIRST typed symbol result |
| G-PATH-02 | path queries return the File entity, not merely symbols in that path |
| G-EXT-02 | external calls occupy actual callee edges (exact callee set) |
| G-SQL-02 | fallback SQL is byte-exact lossless |
| G-DISCLOSE-02 | nonempty unresolved-reference impact keeps its disclosure |
| G-RECALL-01 | general `recall` retains document evidence |
| G-EXPLORE-01 | `explore` keeps unresolved receiver evidence without false binding |
| G-SCHEMA-01 | runner-level: `search` advertises `include_code_artifacts` in `tools/list` |

## Known gaps (deliberately NOT encoded)

Asserting these would fail by design; they are the documented backlog:

- **EN-14** — a function containing a nested declaration loses its own
  body from chunk text (container synthesis keeps doc + signature only).
- **EN-13/8** — navigation output carries no per-edge confidence or
  provenance (storage has it; the query layer strips it).
- **Cold-start hydration race** — the first tool call in a fresh server
  process can return empty while lanes hydrate (pre-existing; the runner
  warms up once after scan to stay deterministic).
- **Chunk-mode reconstruction is lossy by design** — `mode=chunks` still
  reconstructs from retrieval chunks (containers keep doc + signature);
  the DEFAULT is now verbatim on-disk source, which closes EN-14/EN-16
  whenever the scan root is resolvable.
- **Module-level import aliases** — `import x.y as z` / `import * as z`
  leave receiver hints unrewritten (symbol aliases ARE resolved).
- **Cold-start hydration race** — documented but UNREPRODUCED after
  8 fresh-process + 5 same-process + 3 focused trials on current builds;
  the runner keeps its warm-up as cheap insurance. Report a reproduction
  if you see a first-call empty on a populated store.

## Adding a case

1. Reproduce the defect against `main` first (a case that never failed is
   not guarding anything).
2. Add the fixture shape to `zbot/` if needed, then the case to
   `cases.json` with a `guard` one-liner.
3. Run the eval — the new case must pass on the fixed build and be
   confirmed failing on the pre-fix commit.
