---
schema_version: 2
artifact: "brief"
job_id: "admin-console-v1"
cycle: 1
status: "ratified"
author_role: "engram-frame"
profiles: []
source_snapshot: null
input_artifacts:
  - "runs/frame/admin-console-v1/feature.md"
updated_at: "2026-11-01T21:16:21-05:00"
size_tier: "M"
owner: "videogamer (project owner, pi session)"
ratified_at: "2026-11-01T21:16:21-05:00"
constitution_rules_touched: [PRD-101, PRD-102, PRD-107, rail-engine-neutrality, rail-surface-parity]
brief_ref: null
---

# Brief: read-only workspace partition console — local-first admin console v1

> **Functional nature**: this brief describes what the system must do, for
> whom, and why — behavior and outcomes, not mechanisms.

## Why act now?

The owner is preparing to move Engram from a local single-user deployment to a
central server deployment (pgvector, three node-bindings modules: Ingestion,
MCP, Maintenance). The safety of that move depends on the logical partitions —
workspace / project / org — that exist to isolate memories and knowledge. Today
those partitions are invisible: there is no workspace registry, no inventory,
no per-partition view; operating Engram means juggling `--project`/`--org`
flags, one-shot CLIs, and 34 MCP tools. The bet: one local-first console
surface that makes every partition and its contents visible and isolatable
now, on both engine recipes (sqlite and pgvector), so the later central
deployment inherits an operable memory estate instead of an opaque one.

Framing inputs: a datamodel/DDL analysis of the repo (scope spine
`tenant/subject/workspace/session/environment` on every governed table; no
workspaces registry; policy enums `Visibility` and `Retention` in
`core/domain/src/policy.rs`), and a three-advisor framing debate
(OpenAI GPT via codex CLI, Claude via Claude Code CLI, fork-context oracle;
run ids 7a4ecf05 / 80dacf59 / 7d0251c9) whose convergence is reflected below:
read-only partition visibility first; every console mutation (later
increments) must route through existing governed paths; Tencent's
collaboration ontology is a non-goal.

## The ask — in the owner's words

> Analyze the datamodel/ddl of Engram. Beacuse I want an admin console for maintaining workspaces. This will be like https://github.com/TencentCloud/TencentDB-Agent-Memory/ . Check screenshots for visual reference. https://github.com/TencentCloud/TencentDB-Agent-Memory/tree/feat/server_team/assets/images has different images for reference. Analyze and use some debates using gpt models as well and frame my problem

On "maintaining", the owner elaborated (verbatim):

> the reaosn why workspace, project, org are there is to logically partition knowledge and memories

> Console is future of monitoring the jobs of ingest and maintain and health of knowledge and memories. In the future, it can be used to prune memories as well.

The judgment moment is OUT-001: pointing the console at the owner's real
local store, every workspace/org partition present in the data appears in one
inventory with per-partition counts (memories, knowledge entities, beliefs),
with zero CLI commands — judged by the owner (dogfood proof). OUT-002 aids the
same judgment (verbatim): "Stunning ui to visually see the memories getting
added, a stunning knowledge graph will aid in to say this paid for itself too".

## What matters, and what does not?

`MOS-001` is MUST because the inventory outcome (OUT-001) is the success
signal — partitions become visible in one place with zero CLI.
`MOS-002` is MUST because selecting a workspace must show only that
workspace's memories/knowledge, mirroring the engine's own scope isolation
(the point of the partitions).
`MOS-003` is MUST because v1 writes nothing, ever — the console is a new
surface over the engine, never a second writer (PRD-102 guardrail).
`MOS-004` is MUST because the owner set full parity as part of done: every v1
view works read-only on sqlite and pgvector with equivalent results.
`MOS-005` is MUST because the judgment moment is dogfood proof on the real
local store.
`SH-001` is SHOULD because the stunning visual layer (recent memory additions
rendered beautifully; a striking per-partition knowledge-graph view) is part
of the owner's paid-for signal (OUT-002) but is not the blocking outcome.
`SH-002` is SHOULD (delegated, LED-003) because surfacing cross-partition
anomalies read-only is high value but non-essential.
`SH-003` is SHOULD because index binding/status next to a partition is useful
context, explicitly not the success signal.
`C-001`–`C-004` are COULD follow-ups (recall telemetry, proposal audit queue
view, live streaming, job monitoring — the LED-004 trajectory).
`W-001`–`W-006` are WON'T in this job: no writes/maintenance/prune actions, no
policy enforcement, no workspace lifecycle or registry table, no user
management/authz/Entra/multi-operator, no central deployment requirement, no
Tencent collaboration model (teams/agents/sharing/equipping).

**In scope:** MOS-001..MOS-005, SH-001..SH-003, C-001..C-004 (as follow-up
candidates only).
**Out of scope:** W-001..W-006 plus boundaries B-1..B-8 below.
**Open:** `none`.

## Decision ledger — already-bound choices

All rows trace to owner messages in the framing pi session (this thread,
2026-11-01/02; council memo and interview rounds 1–6).

`LED-001` [HUMAN-DECIDED]: v1 is local-first and keeps working unchanged for
the central/pgvector future, exercised on both sqlite and pgvector; the owner
said "A is fine but PGvector bindings exist as well, why not test both."
Revisit when the central deployment brief lands.

`LED-002` [HUMAN-DECIDED]: v1 scope is read-only start — partition inventory
plus isolated browsing; capabilities are added slowly in later increments; the
owner said "Start with A then we will slowly add features." Revisit at any
follow-up increment.

`LED-003` [DELEGATED: the owner said "your call" on show/verify/enforce]:
Show is Must, Verify is Should, Enforce is Won't-for-v1 because it is a write.
The spec MAY refine Show/Verify presentation; it MUST NOT add Enforce (or any
write) without a new owner decision.

`LED-004` [HUMAN-DECIDED]: the console's trajectory is monitoring the jobs of
ingest and maintain and the health of knowledge and memories, with pruning as
a future capability — owner's words verbatim above. Not v1 Must.

`LED-005` [HUMAN-DECIDED]: success is the inventory outcome (OUT-001), judged
by the owner on the real local store, with full sqlite↔pgvector parity part of
done (owner picked A/A/A on outcomes round).

`LED-006` [HUMAN-DECIDED]: the stunning UI / stunning knowledge graph is part
of the paid-for signal — "Stunning ui to visually see the memories getting
added, a stunning knowledge graph will aid in to say this paid for itself too."

`LED-007` [HUMAN-DECIDED]: user management, authz, Entra, and multi-operator
roles are out of scope; the owner said "User managemtn is out of scope for
this." Revisit only in the central-deployment brief.

`LED-008` [HUMAN-DECIDED]: TencentDB-Agent-Memory is a visual reference only;
its teams/sharing/ACL ontology is a non-goal (W-006, ratified "as-is").

`LED-009` [HUMAN-DECIDED]: single trusted operator in v1 — "local it will be
just me" — the console is trusted like the CLI (boundary B-1).

## What constrains the bet?

Constitution (product pack): `PRD-101` — Engram must not become an
application server or hosted control plane without an explicit scope
decision; the console stays a local surface, and the central deployment is a
separate future brief. `PRD-102` — memory writes must produce governed state
and auditable lifecycle events; v1 writes nothing, and future increments must
route through governed paths (operator_proposals / procedures / maintenance
preview-apply), never direct-to-store. `PRD-107` — a capability is not shipped
until callable through provider, N-API/TS and supported MCP surfaces; any new
read capability the console needs lands in the engine/SDK surface with parity,
never as console-private logic.
Constitution (architecture rails): `rail-engine-neutrality` and
`rail-surface-parity` — this is also what makes MOS-004 honest: the owner's
own constraint, "the data model contracts should be the same irrespective of
which undelyaing database is being used," is already the law of this repo.

Boundaries (ratified "as-is"): B-1 no authentication/authorization in v1;
B-2 no central/multi-user serving assumptions — local surface only; B-3 no
schema or migration changes (no registry table, no new columns); B-4 no
adapter-specific data access — console relies only on capabilities that exist
(or are added with full parity) across both engine recipes; B-5 no bulk
data export/import; B-6 no embedding/vector-space management; B-7 no
code-intelligence browsing (index binding/status may appear as the SH-003
health line only); B-8 no deep belief/contradiction views in v1 (belief
counts stay per MOS-001).

Index mining (mandatory hook, run 2026-11-01; `engram-mcp recall --query
'admin console workspace management UI' --project agentzero --json --limit 5`,
5 hits): (1) "The lifecycle order is register (locator) then bind (canonical
ref) then rebuild (request-id idempotency key) then status/refresh" (evidence:
exact); (2) "Reset is destructive and owner-gated: reset-preview first,
ambiguity must be empty, an explicit human go-ahead tied to the preview hash,
then reset-apply" (evidence: exact); (3) live deployment state: agentzero
store bound to mem-alpha@refs/heads/main, 7961 entities / 14549 relationships.
These ground SH-003 and the W-001 guardrails. Code-symbol index was not
queried (one-shot `search` verb not present in this build; `code --op status`
is the documented surface) — datamodel evidence comes from direct schema
reads cited throughout.

## Owner suggestions — non-binding

- Delivery "via the node bindings" for the future three-module deployment
  (Ingestion / MCP / Maintenance) — mechanism belongs to analysis.
- Future Entra connection for module roles — context for later briefs; user
  management is out of scope here (LED-007).
- TencentDB-Agent-Memory screenshots (notably `assets/images/asset.png` —
  panel with ownership, versions, status, visibility, usage counts) as visual
  inspiration for the inventory and browsing surfaces (LED-008: visuals only).
- The repo's existing D3 force-directed graph precedent (the
  `engram-visualize` skill) may inform SH-001's knowledge-graph view.

## Consumption and supersession

- Consumption contract: the spec receives this brief by content hash; the
  decision ledger is BINDING; only explicitly `DELEGATED` boundaries may the
  spec decide.
- Rework supersession: a rework cycle supersedes earlier cycles with no
  inherited authority; the brief itself survives rework (intent contract).

## Owner ratification — the human gate

| Actor | Channel / message reference | Date | Verbatim confirmation |
|---|---|---|---|
| videogamer (owner) | pi framing session (this thread), Round 6 reply | 2026-11-01 | "Q1 B , Q2 ratified" |

Size tier M, whole brief ratified as presented: MoSCoW, boundaries,
outcomes, and decision ledger above (interview rounds 1–6 retained in session
history). The spec receives this brief by content hash and MAY decide only
the explicitly delegated `LED-*` boundaries.
