# Index Mining Hooks — Cross-Harness Reusable

Reusable hook pair that injects the "mine the engram index" instruction
into agent sessions and validates artifacts contain index-evidence
citations. Portable across all harnesses via plain shell execution.

## Files

| File | Role |
|---|---|
| `tools/hooks/index-mining.py` | Pre-dispatch: prints the mining instruction (with concrete CLI examples) into the agent's context |
| `tools/hooks/index-evidence-validator.py` | Post-artifact: validates analysis.md / plan.md cite ≥2 index results or state "index empty" |

Both are pure-stdlib Python, exit-0 fail-open, and controlled by `ENGRAM_HOOKS=0` to suppress.

## Wiring per harness

### Claude Code
```json
// .claude/settings.json → SessionStart hooks array
{ "hooks": { "SessionStart": [{ "hooks": [{ "type": "command",
  "command": "python3 tools/hooks/index-mining.py" }] }] } }
```
Validator: add to `UserPromptSubmit` or a `PreToolUse` gate on file-write to analysis.md/plan.md.

### Codex
```bash
# .codex/hooks/pre-session.sh (or AGENTS.md eval block)
python3 tools/hooks/index-mining.py
```
Codex reads AGENTS.md; add a line: `Before authoring analysis or plan artifacts, run: python3 tools/hooks/index-mining.py`

### GitHub Copilot
```bash
# .github/copilot-instructions.md
$(python3 tools/hooks/index-mining.py)
```
Or reference in `.github/workflows/` as a verification step.

### Pi
```bash
# Option A: AGENTS.md (pi reads it) — add the probe line
# Option B: .pi/extensions/index-mining.cjs — call the script in before_agent_start
# Option C: source it from the existing engram-recall.cjs extension
```

### Future harnesses (Kiro, Antigravity, etc.)
Any harness that can execute a shell command at session start or before
agent dispatch can wire this script. The output is plain markdown with
concrete CLI invocations — no harness-specific protocol.

## Why one-shot CLI (not MCP)

Subagents dispatched by factory orchestrators often lack MCP tool access.
The one-shot CLI (`engram-mcp recall --query X --json`) works from plain
bash, which every agent has. This bypasses the MCP-inheritance blocker
entirely.

## Environment variables

| Variable | Default | Purpose |
|---|---|---|
| `ENGRAM_HOOKS` | (unset) | Set to `0` to suppress the injection |
| `ENGRAM_STORAGE` | `~/.engram` | Store directory |
| `ENGRAM_PROJECT` | `agentzero` | Project scope |
| `ENGRAM_BIN` | auto-detect | Path to engram-mcp binary |

## Testing

```bash
python3 tools/hooks/index-mining.py --probe     # verify CLI reachable
python3 tools/hooks/index-mining.py             # see the injected instruction
python3 tools/hooks/index-evidence-validator.py <artifact> --strict  # gate
```
