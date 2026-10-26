# RFC-0022: engram agent skills + hooks — four-platform recall layer

- **Status:** Proposed
- **Date:** 2026-10-18
- **Depends on:** RFC-0021 (npm distribution), RFC-0015 (unified MCP server)

## TL;DR

Ship five professional agent skills (`engram-index`, `engram-learn`,
`engram-search`, `engram-plan`, `engram-implement`) and a **recall-on-submit
hook** that runs on every user prompt across **Claude Code, Codex, GitHub
Copilot, and pi**. The hook recalls from engram, applies a deterministic
relevance gate, and injects context **only when the memory is actually
relevant** — otherwise it stays silent. Skills land in the cross-tool
`~/.agents/skills/` standard (one copy serves Codex + Copilot; Claude and pi
get their own dir), installed by one command:
`engram-mcp install --agent <claude|codex|copilot|pi|all>`.

Three new pieces make it work: a one-shot `engram-mcp recall --json` CLI
subcommand (no JSON-RPC handshake, structured scores), a zero-dep Node hook
script shipped in the npm package, and an in-process pi extension.

## Context

The npm package (RFC-0021) ships a binary; the MCP tools exist and are
exercised by hand. What's missing is the **agent-side layer**: no skill tells
an agent *how* to use the 21 tools well, and no hook feeds memory into the
conversation automatically. Users must remember to ask.

Reference lessons applied (named sources; specifics verified against docs
fetched 2026-10-18):

- **Anthropic skill-authoring canon / agentskills.io open standard** —
  SKILL.md + YAML frontmatter (`name`, `description`), progressive
  disclosure (lean SKILL.md, `references/` for depth), trigger-rich
  descriptions. Codex and Copilot both consume this exact format; Codex
  documents the standard explicitly, Copilot reads `.agents/skills/` too.
- **Matt Pocock's skill-writing heuristics** (assumed referent for "Matt
  Pollock"; TS educator, writes on agent skills): skills should be small,
  focused, composable; contain exact tool-call shapes, not prose; give the
  agent explicit **stop conditions** and **checklists**; prefer
  runtime-loaded specifics over generic instructions. ("gtan" could not be
  identified; no unattributed borrowing.)
- **gbrain (garrytan/gbrain)** — analyzed 2026-10-18 via local clone
  (subagent research; every claim cited to a file path). Closest prior
  art: Postgres/PGLite personal-knowledge brain, 75 trigger-routed
  skills, four-platform hook framework. Patterns adopted below: fail-open
  hooks with per-event deadlines + counters-only heartbeat JSONL + one env
  kill switch; multi-arm confidence gating with **silence-on-ambiguity**
  ("silence beats a wrong pointer"; gate 0.7; cap 3 pages); cross-turn
  dedupe by reading own prior injections structurally; honest per-harness
  capability states (`native-pending` vs `native-verified` with the
  verifying experiment — "Push is never claimed from documentation
  alone"); evidence enums on retrieval results instead of raw scores;
  budget-packed context with `dropped_count`; retrieval placed at
  **boundaries** (session start, post-compaction) rather than
  per-message; injected context wrapped as **data, not instructions**;
  capture lanes consent-gated, off by default.
- **REA prompt** (`~/Documents/rea.txt`) — the inspiration for
  `engram-index`: evidence-first process (scope → surface map → trace →
  evidence → verify → report), **confidence labels per finding
  (proven / likely / guessed)**, a **final check** before claiming done, and
  the rule "never claim without evidence."
- **mem0 / OpenClaw** (carried from RFC-0021) — trust before virality; every
  hook must fail open and add near-zero latency, or users uninstall it.

### Platform matrix (verified 2026-10-18)

| Capability | Claude Code | Codex | Copilot CLI | pi |
|---|---|---|---|---|
| Skills dir (user) | `~/.claude/skills/` | `$HOME/.agents/skills` | `~/.copilot/skills` **or `~/.agents/skills`** | `~/.pi/agent/skills/` |
| Skills dir (repo) | `.claude/skills/` | `$REPO/.agents/skills` (walked up to root) | `.github/skills` · `.claude/skills` · `.agents/skills` | `.pi/agent/skills/` |
| SKILL.md format | name + description | name + description (agentskills.io) | name + description | name + description |
| Prompt-submit hook | `UserPromptSubmit` (30s timeout, `async: true` supported; `additionalContext` via stdout or JSON) | `UserPromptSubmit` (matcher ignored; plain stdout or `hookSpecificOutput.additionalContext`; `async: true` background) | `userPromptSubmitted` — **command-hook output is dropped** (SDK programmatic hooks only) | in-process extension (`before_agent_start`) — full prompt mutation |
| Session-start context | `SessionStart` → `additionalContext` | `SessionStart` → `additionalContext` | `sessionStart` → `additionalContext` ✅ | `session_start` event / factory |
| Config file | `~/.claude/settings.json` | `~/.codex/hooks.json` (+ trust review via `/hooks`) | `~/.copilot/hooks/*.json` or `~/.copilot/settings.json` hooks block (also reads `.claude/settings.json`) | `~/.pi/agent/extensions/*.ts` |
| Subagent-style hook | `"type": "agent"` hooks exist | subagent hooks exist | no | pi subagents exist |

**Key platform facts that shape the design:**

1. `~/.agents/skills/` is read by **both Codex and Copilot** — one directory
   serves two platforms. Claude and pi need their own copies. Three write
   targets cover four platforms.
2. Copilot CLI **cannot** inject per-prompt context from command hooks
   (output dropped by design). Its `sessionStart` hook **can** inject
   `additionalContext`. So Copilot gets session-scoped recall (once per
   session) — an honest, documented limitation, not a silent gap.
3. Claude and Codex both honor `hookSpecificOutput.additionalContext` on
   `UserPromptSubmit` and support async/background execution — the exact
   shape our hook emits.
4. pi gets the best implementation: an in-process extension, no subprocess
   spawn, direct prompt augmentation.

## Goals

- Every user prompt recalls relevant memory **automatically**, gated on
  relevance, on all four platforms (per-session on Copilot).
- Five skills that make agents measurably better at using engram: index with
  real memorization (REA-style), learn, search, plan, implement.
- One install command per platform; idempotent; prints what it wrote.
- Hook latency < 150 ms p95 warm; **fail-open always** (engram missing,
  store corrupt, timeout → silent exit 0).
- No LLM in the hot path (v1): deterministic score gate only.
- No telemetry, no network — consistent with the product stance.

## Non-goals (v1)

- LLM/subagent relevance judgment in the per-prompt hot path (Claude
  `type: "agent"` hook documented as a Tier-2 option, off by default).
- Copilot SDK programmatic hooks (per-prompt on Copilot).
- Windows-native hook scripts (Node script runs everywhere Node does; the
  launcher already requires Node).
- Publishing to any skills marketplace / plugin directory.
- Changes to the MCP tool surface itself (skills teach better *use* of the
  existing 21 tools; only additive CLI subcommands are new).

## Architecture

```
┌─ npm package @engramlabs/mcp ──────────────────────────────────┐
│ bin.js            launcher (resolves platform binary)          │
│ install.js        + --agent <host>: skills + hooks + MCP reg    │
│ hooks/                                                       │
│   recall-context.mjs   one hook script, all platforms          │
│   stale-index.mjs      staleness reminder (Stop hook)          │
│ skills/                  ← the five skills (also in repo)      │
│   engram-index/  engram-learn/  engram-search/                 │
│   engram-plan/   engram-implement/                             │
└────────────────────────────────────────────────────────────────┘
        │ install --agent <host> writes to:
        ├─ ~/.agents/skills/*      (Codex + Copilot)
        ├─ ~/.claude/skills/*      (Claude Code)
        ├─ ~/.pi/agent/skills/*    (pi)
        ├─ ~/.claude/settings.json          hooks.UserPromptSubmit (+Stop)
        ├─ ~/.codex/hooks.json              UserPromptSubmit (+Stop, async)
        ├─ ~/.copilot/hooks/engram.json     sessionStart
        └─ ~/.pi/agent/extensions/engram-recall.ts   (in-process)

┌─ engram-mcp binary (new one-shot subcommands) ─────────────────┐
│ engram-mcp recall --storage S --project P --query Q            │
│            [--json] [--limit N] [--lanes ...]                  │
│   → reuses the unified-recall lane; --json emits per-hit       │
│     {kind, lane, score, title, content, path} + fusion stats   │
│ engram-mcp status --storage S --project P [--json]             │
│   → last-index stamp, files-changed-since-index, symbol count  │
└────────────────────────────────────────────────────────────────┘
```

### The recall flow (boundary-first, per-prompt as a reflex)

gbrain's measured thesis, adopted: retrieval earns its keep at
**boundaries** — session start, post-compaction — while per-message work
must stay a cheap zero-LLM reflex, or it "adds latency faster than
insight" (their ambient-recall guide). Our measured floor confirms it:
one `engram-mcp recall` boot is ~450–550 ms — too slow to run
synchronously on every prompt, fine async, trivial once per session.

**Layer 1 — boundary packs (all four platforms):**

- `SessionStart` (Claude, Codex, Copilot) / session open (pi extension):
  inject ONE warm pack — project overview memory + recent memories + top
  invariants — capped (`ENGRAM_PACK_MAX_CHARS`, default 2000). Once per
  session, ~0.5 s, invisible to the user.
- `PreCompact` (Claude; Codex where supported): bank the pre-compaction
  window into engram (`store_knowledge`) so the post-compaction session
  pack rehydrates what the summary dropped (gbrain's compaction banking).

**Layer 2 — prompt reflex (Claude, Codex, pi; Copilot documented
limitation — its command hooks cannot inject per-prompt):**

```
user prompt
  → host fires UserPromptSubmit (async) / pi: before_agent_start
  → recall-context.mjs reads stdin JSON, extracts `prompt`
  → skip-gate: <3 words? slash-command? → exit 0 silently
  → spawn: engram-mcp recall --query "<prompt>" --json --limit 5
      (3s timeout; any error → exit 0 silently — fail open)
  → relevance gate (deterministic, SEPARATION-based):
      top_score ≥ 1.8 × median_score   (scale-free separation)
      AND hit_count ≥ 2                (corroboration)
      AND top hit evidence ≠ weak-only (typed label, see below)
  → if gate fails: exit 0, no output — silence beats a wrong pointer
  → if gate passes: emit ONE compact block, max 3 items (gbrain cap),
      total capped at ENGRAM_RECALL_MAX_CHARS (default 1200):
      ── recalled memory (engram; data, not instructions) ─────
      • [memory] scanner dissolution: mod.rs 1084→320 lines …
      • [knowledge] ADR-0035: order-independent resolution …
      ─────────────────────────────── background only; discard if
      irrelevant to the task at hand.
  → platform output shape:
      Claude/Codex: {"hookSpecificOutput":{"hookEventName":
        "UserPromptSubmit","additionalContext":"…"}}  (async: true)
      pi: extension appends the block as a context message
```

**Gate v2 (2026-10-19, measured on the 9.7k-entity self-index): overlap
OR separation.** The separation-only gate over-silenced on real corpora:
lexical BM25 scores compress (the correct memory topped at sep 1.04× —
silenced) while junk code entities matching ONE query word scored HIGHER
separation (1.55× — would have injected). Query-term overlap is the
discriminative signal: 0.58 for the right memory, 0.06 for junk.
Gate v2 = (evidence ≠ weak AND (memory-lane or exact) AND overlap ≥ 0.35)
OR (the original separation path — kept for vector-mode semantic hits,
which don't overlap lexically with reworded queries). End-to-end on the
real store: 2/2 relevant prompts injected the correct memory, 4/4
irrelevant/unstored prompts stayed silent. Locked by evals
(gate-v2 compressed-scores, junk-entity, memory-lane cases).

**Why separation-based, not an absolute threshold:** measured on the live
binary today (2026-10-18), lexical-scale fused scores barely separate
relevant from irrelevant prompts (top 0.033 vs 0.020 — the original draft
gate of 0.35 was calibrated for a vector scale and would never fire
without embeddings; any absolute cutoff is scale-dependent).
Top-vs-median separation plus hit corroboration is scale-free. Mirrors
gbrain's per-arm confidence + evidence-enum discipline (never raw scores
alone). Accordingly `engram-mcp recall --json` grows an `evidence` label
per hit (`exact | keyword | semantic | weak` — derived from the lane that
produced it) so the gate — and agents downstream — branch on a typed
field, not a float.

**Cross-turn dedupe:** the hook records its own last injected item ids in
`<storage>/hook-state.json` and suppresses re-injecting the same items
for the next N turns (default 3) — structural id tracking, never
substring matching over the transcript (gbrain's discipline).

**Heartbeat + kill switch:** counters/durations/error-codes ONLY (never
prompt text) at `<storage>/hook-heartbeat.jsonl`; `ENGRAM_HOOKS=0`
short-circuits every event. A broken memory hook must never break the
coding session: exit 0 on every path.

**Envelope discipline:** every injected block is framed as *data, not
instructions* — retrieved background the agent may freely discard. Both
an anti-prompt-injection measure and gbrain's `data-not-instructions`
pattern.

**Why no LLM/subagent in the reflex:** every prompt pays the hook's
budget. Claude's `type: "agent"` hook stays documented as opt-in Tier 2
for fuzzy matching at accepted latency; in-process where free (pi),
never in the hot path where it costs.

**Verification honesty (gbrain's capability table):** per-harness push
support ships as a published table — `native-verified` only after a real
fresh-session experiment observes the injection (e.g. "3 of 3 installs;
control with hooks off answered NONE"), `native-pending` otherwise.
Push is never claimed from documentation alone.

### Stale-index reminder (auto-refresh)

`Stop` hook (Claude, Codex; pi extension equivalent) runs
`engram-mcp status --json`; if `files_changed_since_index` exceeds
`ENGRAM_STALE_THRESHOLD` (default 25), injects a one-liner:
`engram index is stale (N files changed since last scan) — run
scan_repo at a natural pause`. No rescan in the hook; the agent decides.

## The five skills

Common craft rules (all five): exact tool-call JSON shapes inline; explicit
**stop conditions**; a closing **checklist**; evidence labels
`proven/likely/guessed` on claims; "discard if irrelevant" framing for
injected memory; ≤150-line SKILL.md with `references/` for depth.

### 1. `engram-index` — index *and* memorize (REA-inspired)

Trigger: "index this repo", first session in a repo, or staleness reminder.

Phases (mirroring REA, adapted from binary RE to code understanding):

0. **Scope** — what is this repo, what will "remembered" mean here? Skip to
   delta-memorize if an index exists and few files changed (`status`).
1. **Surface map** — `scan_repo {path}` → `architecture {limit: 20}` →
   `graph_health {}`. No file reading for structure; the graph is truth.
2. **Memorize** — the part the old skill never did. Write, with evidence
   labels, in a parseable format:
   - `write_memory` — the repo's nature: stack, layout invariants, sharp
     edges, non-obvious coupling. Format:
     `[proven|likely|guessed] <claim> — evidence: <tool/query that proved it>`.
   - `procedure_put` — the recurring workflows (build, test, release) with
     the exact commands that worked.
   - `put_entity`/`put_relationship` — the core abstractions (top central
     symbols) and their `calls`/`describes` edges.
3. **Verify (write→read-back)** — recall 3 probe queries ("build command",
   top symbol name, one invariant). If a probe fails, the memory was
   written badly — rewrite it. **An index that cannot be recalled is not an
   index.**
4. **Report** — N symbols, M modules, core abstractions, bridges, what was
   memorized, confidence split (proven/likely/guessed counts), unknowns.

Final check (from REA, verbatim discipline): *What am I missing? Is each
claim proven or plausible? Would this survive the next refactor?*

### 2. `engram-learn` — session distillation (rename of `engram-distill`)

Trigger: explicit "remember this/learn from this session", end of a
meaningful fix, `Stop` of a long session (agent's judgment).

What to capture: decisions + why; gotchas and their resolutions; validated
procedures (or failure counters on existing ones); source-grounded facts
(`store_knowledge` with the doc they came from); belief updates when a
stance changed (`belief_get` → new belief or `contradiction_list` grows).
Inherits distill's bridging discipline: concepts link to the code that
realizes them; no orphan entities. Anti-rules: no trivia, no
session-transcript dumps, no duplicates (recall first, update if present).

### 3. `engram-search` — agentic problem search (rename of `engram-find`)

Trigger: "where is…", "how does… work", "why did… break", "find the code
that…".

A loop, not a lookup: **hypothesis → retrieve → verify → refine**.
1. State the hypothesis ("auth lives in middleware, likely X").
2. `recall` + `search` (two angles) → `explore` the neighborhood →
   `symbol_context` on the best candidate → **verify with
   `fetch_content`** — never claim file contents from memory alone.
3. Refine query with what the graph revealed; stop when the claim is
   evidence-labeled `proven` or two iterations produced nothing (say so).
Output: answer + evidence chain (each claim tagged with the tool+query that
proved it) + confidence labels.

### 4. `engram-plan` — plan a change from the index

Trigger: "plan/fix/implement <story|bug|feature>", before any non-trivial edit.

Mine before writing: `recall` (similar past work + the repo's invariants
memorized by engram-index), `search` for the touchpoints,
`change_impact` for blast radius of each, `architecture` for surrounding
structure, `contradiction_list` for known tensions.

Output — a fixed-section plan (also `write_memory`'d with a stable title so
`engram-implement` recalls it):
- **Overview** — one paragraph, the change in the repo's own vocabulary.
- **Findings** — evidence-labeled facts from the graph.
- **Risks** — blast radius, bridges touched, invariants threatened,
  test-path exposure.
- **Steps** — ordered, each independently verifiable (command to run).
- **Open questions** — what the index could not answer.

### 5. `engram-implement` — execute with self-review

Trigger: a plan exists (recalled or given); "implement <it>".

Loop per step: implement → **verify** (run the step's command; red is
information, not failure) → **self-review** (diff vs plan step + risk list:
did we touch a bridge? an invariant? tests for the new behavior?) → fix or
proceed. Stop conditions: all steps green; OR two consecutive failed
verification attempts on one step → stop and report (no flailing). On
completion: mini `engram-learn` pass — what worked, what fought back,
procedure counters updated. Final check: every step verified, risks
re-checked, learnings written.

## Install UX

```
engram-mcp install --agent claude    # MCP reg + ~/.claude/skills + hooks
engram-mcp install --agent codex     # MCP reg + ~/.agents/skills + hooks.json
engram-mcp install --agent copilot   # MCP reg + ~/.agents/skills + sessionStart hook
engram-mcp install --agent pi        # MCP reg + ~/.pi skills + extension
engram-mcp install --agent all
engram-mcp install --agent claude --remove   # idempotent removal
```

Extends `install.js` (same file-writes-first, verify-after discipline as
`--client`). Prints every path written. Codex notes the `/hooks` trust step
(Codex requires manual trust of new hooks — surfaced in the install output,
not hidden).

## Rollout

1. **Rust one-shot CLI** — `recall --json`, `status --json` subcommands +
   unit/integration tests.
2. **Skills bundle** — skills live in the single authored tree
   `.agents/skills/` (this RFC originally placed them in
   `mcp/engram-mcp/skills/`; that fork was deleted), and the npm tarball
   packs them as the `skills/` payload generated at `prepack`.
3. **Installer** — `--agent` flows for four platforms; removal; idempotency
   tests.
4. **pi extension** — in-process recall + staleness; the reference
   implementation.
5. **Dogfood + live-verify** — run the full battery (per
   `engram-live-verify`): real `UserPromptSubmit` cycles on Claude + Codex,
   extension on pi, sessionStart on Copilot; fail-open drills (missing
   binary, corrupt store, timeout); then dogfood on mem-alpha + odyssey and
   iterate the skill copy until agents actually follow it.

## Test plan

- Unit: gate logic (thresholds, hit-count rule, char cap, malformed stdin,
  empty prompt, slash-command skip); status staleness math.
- Integration: `recall --json` shape vs MCP `recall` equivalence; `status`
  vs store stamps; hook script end-to-end against a fixture store.
- Live (blocking for ship): timed hook cycle per platform; fail-open under
  each failure mode; skills invoked for real by a host agent on mem-alpha.

## Risks

- **Latency on every prompt** — mitigated by 3s timeout, async hooks,
  skip-gate, fail-open; measured p95 in the live gate before ship.
- **Context pollution** — the gate + hard char cap + "discard if
  irrelevant" framing; if it still misfires in dogfooding, raise the
  default threshold, not the cap.
- **Platform drift** (hook shapes change) — matrix pinned in this RFC with
  dates; the live-verify battery is the regression net.
- **Skill bloat** — the ≤150-line rule + progressive disclosure; dogfooding
  trims prose that agents skip anyway.
