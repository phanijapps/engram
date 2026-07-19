# Eval: 50-Question Code-Intelligence Suite

A larger suite for **tracking Q&A accuracy over time** — the 8-question pilot
is a smoke test; this one has enough breadth across categories and enough N to
spot regressions instead of noise. Same grounding as the 8-question suite: the
knowledge graph alone (entities + relationships + chunk text + agentic Q&A),
no vector embeddings.

- **Target repo:** [microsoft/terminal](https://github.com/microsoft/terminal)
  (e58bd4bdab, main)
- **Index:** tree-sitter AST (C/C++/C#), no embeddings
- **LLM:** `gemma4:31b-cloud` via ollama cloud (pi SDK), agentic Q&A with
  `search_entities` / `get_neighbors` / `get_code` tools
- **Cost:** ~50 LLM turns, ~8 min wall-clock
- **Relationship to the 8-question suite:** a strict superset. Questions 1–8
  mirror the [8-question suite](./eval-8-question.md) verbatim so the pilot
  stays a fast subset of the full run.

## Categories

| Category | Count | Posture |
| --- | --- | --- |
| entity_lookup | 10 | Find/explain a named entity |
| concept | 8 | Explain how a mechanism works |
| relationship | 8 | How two entities relate |
| structural | 7 | List main components of an area |
| call_graph | 6 | Trace a call chain |
| navigation | 6 | Where defined / who calls |
| cross_file | 3 | Interaction across files/layers |
| aggregation | 2 | Count/list |

## Scoring rubric

Identical to the [8-question suite](./eval-8-question.md#scoring-rubric) —
keyword-over-substring, same thresholds (`bench.ts:score`). Report
**correct + partial** as the headline accuracy; track per-category deltas to
see *which* retrieval shape regressed, not just that the headline moved.

## Question set

| # | Cat | Question | Expected keywords |
| --- | --- | --- | --- |
| 1 | entity_lookup | What does the TerminalHandle class do? | `TerminalHandle`, `handle` |
| 2 | entity_lookup | What does the Settings class manage? | `Settings` |
| 3 | entity_lookup | What is the role of the TextBuffer? | `TextBuffer`, `text` |
| 4 | entity_lookup | What does the KeyChord class represent? | `KeyChord`, `key` |
| 5 | entity_lookup | What does the Renderer do? | `Renderer`, `render` |
| 6 | entity_lookup | What is the CommandHistory responsible for? | `CommandHistory`, `history` |
| 7 | entity_lookup | What does the TerminalConnection interface define? | `TerminalConnection`, `Connection` |
| 8 | entity_lookup | What does the FontInfo class hold? | `FontInfo`, `font` |
| 9 | entity_lookup | What is the purpose of the GlyphAtlas? | `GlyphAtlas`, `glyph` |
| 10 | entity_lookup | What does the TextAttribute class represent? | `TextAttribute`, `attribute` |
| 11 | concept | How does text rendering work in the terminal? | `render`, `text` |
| 12 | concept | How are keyboard shortcuts handled? | `key`, `shortcut` |
| 13 | concept | How does the terminal handle VT sequences? | `VT`, `sequence` |
| 14 | concept | How does selection work? | `selection` |
| 15 | concept | How is the color table applied to text? | `color` |
| 16 | concept | How does mouse input get processed? | `mouse`, `input` |
| 17 | concept | How does the terminal manage scrolling? | `scroll` |
| 18 | concept | How does search work in the buffer? | `search` |
| 19 | relationship | What is the relationship between Terminal and TerminalConnection? | `Terminal`, `Connection` |
| 20 | relationship | How do Tab and Pane relate? | `Tab`, `Pane` |
| 21 | relationship | What is the relationship between Renderer and DxRenderer? | `Renderer`, `Dx` |
| 22 | relationship | How do KeyChord and ActionAndArgs relate? | `KeyChord`, `Action` |
| 23 | relationship | What is the relationship between Profile and Settings? | `Profile`, `Settings` |
| 24 | relationship | How do TextBuffer and ROW relate? | `TextBuffer`, `ROW` |
| 25 | relationship | What is the relationship between TerminalPage and TermControl? | `TerminalPage`, `control` |
| 26 | relationship | How do CommandHistory and the shell relate? | `CommandHistory`, `shell` |
| 27 | structural | List the main classes in the renderer module. | `renderer`, `class` |
| 28 | structural | What are the main components of the terminal architecture? | `component`, `module` |
| 29 | structural | List the main types in the settings model. | `Settings`, `Profile` |
| 30 | structural | What are the main classes involved in text buffering? | `TextBuffer`, `ROW` |
| 31 | structural | List the renderer backends. | `Renderer`, `Dx` |
| 32 | structural | What are the main parts of the input pipeline? | `input`, `key` |
| 33 | structural | List the main components of the connection layer. | `Connection`, `terminal` |
| 34 | call_graph | What is the call chain for writing text to the screen? | `write`, `screen`, `render` |
| 35 | call_graph | Trace how a key press reaches an action handler. | `key`, `action` |
| 36 | call_graph | What is the call chain from VT sequence to text buffer? | `VT`, `TextBuffer` |
| 37 | call_graph | How does a render request propagate to the GPU? | `render`, `Dx` |
| 38 | call_graph | Trace the path from user input to character output. | `input`, `output` |
| 39 | call_graph | What functions are called when the terminal scrolls? | `scroll`, `function` |
| 40 | navigation | Where is the Terminal class defined? | `Terminal` |
| 41 | navigation | Who calls the Renderer's Paint method? | `Paint`, `Renderer` |
| 42 | navigation | What file contains the TextBuffer? | `TextBuffer` |
| 43 | navigation | Which classes use the GlyphAtlas? | `GlyphAtlas` |
| 44 | navigation | Where is the Settings class implemented? | `Settings` |
| 45 | navigation | What calls into the TerminalConnection? | `TerminalConnection` |
| 46 | cross_file | How does the renderer interact with the text buffer across files? | `Renderer`, `TextBuffer` |
| 47 | cross_file | How do the settings flow from JSON to runtime objects? | `Settings`, `json` |
| 48 | cross_file | How does the control layer connect to the connection layer? | `control`, `Connection` |
| 49 | aggregation | How many renderer backends are there? | `renderer`, `backend` |
| 50 | aggregation | List all the classes in the terminal core module. | `class`, `terminal` |

### Question set as data

The suite is authored here, not yet wired into `bench.ts`. To run it, paste
this block into `QUESTIONS` in `demo/backend/src/bench.ts` (or load it from a
file and pass to `runBenchmark`):

```json
[
  {"q":"What does the TerminalHandle class do?","category":"entity_lookup","expectContains":["TerminalHandle","handle"]},
  {"q":"What does the Settings class manage?","category":"entity_lookup","expectContains":["Settings"]},
  {"q":"What is the role of the TextBuffer?","category":"entity_lookup","expectContains":["TextBuffer","text"]},
  {"q":"What does the KeyChord class represent?","category":"entity_lookup","expectContains":["KeyChord","key"]},
  {"q":"What does the Renderer do?","category":"entity_lookup","expectContains":["Renderer","render"]},
  {"q":"What is the CommandHistory responsible for?","category":"entity_lookup","expectContains":["CommandHistory","history"]},
  {"q":"What does the TerminalConnection interface define?","category":"entity_lookup","expectContains":["TerminalConnection","Connection"]},
  {"q":"What does the FontInfo class hold?","category":"entity_lookup","expectContains":["FontInfo","font"]},
  {"q":"What is the purpose of the GlyphAtlas?","category":"entity_lookup","expectContains":["GlyphAtlas","glyph"]},
  {"q":"What does the TextAttribute class represent?","category":"entity_lookup","expectContains":["TextAttribute","attribute"]},
  {"q":"How does text rendering work in the terminal?","category":"concept","expectContains":["render","text"]},
  {"q":"How are keyboard shortcuts handled?","category":"concept","expectContains":["key","shortcut"]},
  {"q":"How does the terminal handle VT sequences?","category":"concept","expectContains":["VT","sequence"]},
  {"q":"How does selection work?","category":"concept","expectContains":["selection"]},
  {"q":"How is the color table applied to text?","category":"concept","expectContains":["color"]},
  {"q":"How does mouse input get processed?","category":"concept","expectContains":["mouse","input"]},
  {"q":"How does the terminal manage scrolling?","category":"concept","expectContains":["scroll"]},
  {"q":"How does search work in the buffer?","category":"concept","expectContains":["search"]},
  {"q":"What is the relationship between Terminal and TerminalConnection?","category":"relationship","expectContains":["Terminal","Connection"]},
  {"q":"How do Tab and Pane relate?","category":"relationship","expectContains":["Tab","Pane"]},
  {"q":"What is the relationship between Renderer and DxRenderer?","category":"relationship","expectContains":["Renderer","Dx"]},
  {"q":"How do KeyChord and ActionAndArgs relate?","category":"relationship","expectContains":["KeyChord","Action"]},
  {"q":"What is the relationship between Profile and Settings?","category":"relationship","expectContains":["Profile","Settings"]},
  {"q":"How do TextBuffer and ROW relate?","category":"relationship","expectContains":["TextBuffer","ROW"]},
  {"q":"What is the relationship between TerminalPage and TermControl?","category":"relationship","expectContains":["TerminalPage","control"]},
  {"q":"How do CommandHistory and the shell relate?","category":"relationship","expectContains":["CommandHistory","shell"]},
  {"q":"List the main classes in the renderer module.","category":"structural","expectContains":["renderer","class"]},
  {"q":"What are the main components of the terminal architecture?","category":"structural","expectContains":["component","module"]},
  {"q":"List the main types in the settings model.","category":"structural","expectContains":["Settings","Profile"]},
  {"q":"What are the main classes involved in text buffering?","category":"structural","expectContains":["TextBuffer","ROW"]},
  {"q":"List the renderer backends.","category":"structural","expectContains":["Renderer","Dx"]},
  {"q":"What are the main parts of the input pipeline?","category":"structural","expectContains":["input","key"]},
  {"q":"List the main components of the connection layer.","category":"structural","expectContains":["Connection","terminal"]},
  {"q":"What is the call chain for writing text to the screen?","category":"call_graph","expectContains":["write","screen","render"]},
  {"q":"Trace how a key press reaches an action handler.","category":"call_graph","expectContains":["key","action"]},
  {"q":"What is the call chain from VT sequence to text buffer?","category":"call_graph","expectContains":["VT","TextBuffer"]},
  {"q":"How does a render request propagate to the GPU?","category":"call_graph","expectContains":["render","Dx"]},
  {"q":"Trace the path from user input to character output.","category":"call_graph","expectContains":["input","output"]},
  {"q":"What functions are called when the terminal scrolls?","category":"call_graph","expectContains":["scroll","function"]},
  {"q":"Where is the Terminal class defined?","category":"navigation","expectContains":["Terminal"]},
  {"q":"Who calls the Renderer's Paint method?","category":"navigation","expectContains":["Paint","Renderer"]},
  {"q":"What file contains the TextBuffer?","category":"navigation","expectContains":["TextBuffer"]},
  {"q":"Which classes use the GlyphAtlas?","category":"navigation","expectContains":["GlyphAtlas"]},
  {"q":"Where is the Settings class implemented?","category":"navigation","expectContains":["Settings"]},
  {"q":"What calls into the TerminalConnection?","category":"navigation","expectContains":["TerminalConnection"]},
  {"q":"How does the renderer interact with the text buffer across files?","category":"cross_file","expectContains":["Renderer","TextBuffer"]},
  {"q":"How do the settings flow from JSON to runtime objects?","category":"cross_file","expectContains":["Settings","json"]},
  {"q":"How does the control layer connect to the connection layer?","category":"cross_file","expectContains":["control","Connection"]},
  {"q":"How many renderer backends are there?","category":"aggregation","expectContains":["renderer","backend"]},
  {"q":"List all the classes in the terminal core module.","category":"aggregation","expectContains":["class","terminal"]}
]
```

## How to run

```bash
# 1. Index the target repo (clean DB, no manifest cache), same as the 8Q suite.
# 2. Paste the JSON above into QUESTIONS in demo/backend/src/bench.ts,
#    then:
pnpm --filter demo-backend exec tsx src/bench.ts
```

## Status

**Not yet run.** This document defines the question set and scoring so the
suite is reproducible; it does not record a baseline. When the first run lands,
record correct / partial / wrong / no_answer (headline = correct + partial)
and a per-category breakdown here, then promote the numbers into
[PERFORMANCE.md](./PERFORMANCE.md).

## Limitations

- **Single repo, keyword-scored.** Same caveats as the 8-question suite — read
  answers before declaring regression.
- **Navigation/aggregation are weakly scored.** Questions like "where is X
  defined?" match on a single keyword; treat their category deltas as
  directional, not precise.
- **Grounded in terminal's entity names.** Re-pointing at another repo needs
  the expected keywords re-derived from *that* graph (the questions themselves
  transfer; the keyword expectations may not).

## See also

- [8-question suite](./eval-8-question.md) — the fast pilot (strict subset)
- [Performance benchmark](./PERFORMANCE.md) — methodology + results
- [Benchmark source](../../demo/backend/src/bench.ts) — the runnable harness
- [Q&A logic](../../demo/backend/src/qa.ts)
