---
name: engram-domain
description: Set up a standard-domain ontology pack for this project so distillation classifies facts under real standard vocabulary (FIBO for banking, schema.org for travel, and 31 more pinned packs). Scan the repo, pick the matching domain, install the pack via engram-mcp pack install, ingest, then distill under the standing vocabulary. Run at project setup, before the first engram-distill of a domain-heavy repo, or whenever the project has no domain pack active. The domain pick and classification are the agent's own reasoning; the server never calls an LLM.
---

# engram-domain

Make standard-domain vocabulary **stand ready** before any distillation, so
facts classify under FIBO / schema.org / SKOS-style standard terms instead of
ad-hoc naming. One-time per project: scan → pick the domain → install the
pack → restart the server → ingest + distill. Coverage gaps are never silent:
install reports sliced-vs-available counts, and you widen on demand.

## When to run

- A new domain-heavy repo (banking, travel, geospatial, IoT, provenance, …) is
  about to be scanned or distilled.
- The owner asks for "the FIBO setup" / "standard taxonomy for this project".
- A distill pass produces ad-hoc classes where a standard vocabulary exists.

## Prerequisites

The `engram` MCP server is configured for this project (`--storage`, `--project`).
The one-shot `engram-mcp pack …` subcommands run in a shell, not over MCP.
First install of a pack needs the network ONCE (sha256-verified against the
in-repo pin); every later operation — including all distills — is offline.

## Step 1 — Scan the repo to infer the domain

Read the repo's docs and code structure (`README`, domain terms, package
names). List the candidate domains with their pinned packs:

```bash
engram-mcp pack list
```

Each entry prints `id`, domain, license, and name (e.g. `fibo` finance,
`schema-org` general/travel, `geosparql` geospatial, `ssn`/`sosa` IoT,
`prov-o` provenance, `goodrelations` commerce).

## Step 2 — Pick the domain (both modes are encoded)

**Propose-then-confirm (interactive session):**
propose exactly one pack with a one-line justification
("banking repo: loans + deposit accounts + institutions → `fibo`"), then
WAIT for the user's confirmation before installing. Do not install on an
unconfirmed interactive pick.

**Autonomous (delegated run, no user present):**
proceed unconfirmed, but LOG the pick with its justification in your output
("autonomous domain pick: fibo — banking terms dominate the docs") and in
the session log if one is active. You are still accountable for the pick.

If two domains genuinely compete, propose both interactively, or pick the
broader one autonomously and say why.

## Step 3 — Install the pack

```bash
engram-mcp pack install <id> --storage <dir> --project <project>
```

What happens: the pinned pack is fetched once and sha256-verified (a mismatch
is a hard error — never edit the pin to pass), cached under
`<storage>/packs/cache/`, the FULL class/property/hierarchy set is written
into the structural ontology + taxonomy repositories, and a bounded launch
slice (top-150 classes / 40 predicates by descendant count) is emitted at
`<storage>/packs/launch/<project>/{ontology,taxonomy}.json`.

Read the install report: it prints **sliced-vs-available counts** ("classes
150/942 available"). That reduction is reported, never silent.

**RESTART the server** now — the launch config is read once at server start,
so the pack activates on the next server launch. After restart,
`ontology_read` shows the default 3-layer technical vocabulary PLUS one
`<pack-id>` layer, and `taxonomy_read` shows the pack's concepts.

## Step 4 — Ingest + distill under the standing vocabulary

1. `scan_repo { "path": "/abs/path/to/repo" }` — index the code.
2. Read the active vocabulary: `ontology_read`, then `taxonomy_read`.
3. Distill docs/facts exactly as `engram-distill` teaches, but classify
   entities and facts against the **pack layer's standard terms** (e.g.
   `Loan`, `DepositAccount` from FIBO; `Hotel`, `Flight` from schema.org).
   Use declared predicates only; the pack's property labels are in the
   `within` predicate list.
4. Stamp verdicts on every fact written via `store_knowledge`:
   `verdict: "grounded_in_source"` when a doc/source quote supports it,
   otherwise the default `asserted_by_agent`. `derived` is RESERVED — the
   write rejects it.
5. Zero hand-built taxonomy: do not author custom concept schemes for a
   domain a pack already covers; widen the pack instead.

## Step 5 — Coverage check, widen when terms are missing

If classification needs a standard term that `ontology_read` does NOT show
(the slice dropped it — the counts told you how much was sliced):

```bash
engram-mcp pack widen <id> --slice-size all --storage <dir> --project <project>
```

Widening regenerates a broader launch config OFFLINE from the cache
(deterministic, no fetch). **Restart the server again** — config is read at
start. Then re-check `ontology_read` and continue the distill.

## Owner proof runbook (banking + travel)

The live proofs are **owner-run on fresh real repos** — this runbook is the
procedure; record outcomes in the owner's notes.

**Banking (FIBO):**
1. Fresh clone of a banking repo with real docs (lending, accounts,
   institutions).
2. Run steps 1–4 with pack `fibo`. Confirm the pick interactively.
3. After distill, sample ~20 facts/entities from `recall`: they must carry
   FIBO vocabulary terms (Loan, DebtInstrument, FinancialInstitution, …).
4. Verify ZERO hand-built taxonomy: `taxonomy_read` shows the
   `pack:fibo` scheme plus the default software scheme, nothing else.
5. Verdict spot-check: doc-quoted facts read `grounded_in_source`; agent
   inferences read `asserted_by_agent`.

**Travel (schema.org):**
1. Two fresh travel repos (booking site, itinerary planner).
2. Run steps 1–4 with pack `schema-org` on EACH repo (separate
   `--project` scopes; per-project isolation keeps their vocabularies
   separate even in one storage).
3. Sample facts from each: they carry schema.org travel terms (`Hotel`,
   `Flight`, `TouristAttraction`, …) on BOTH repos.
4. Shared-terms check: recall the same domain concept from each project —
   both return under the shared standard terms of the one common pack.

Pass = sampled facts carry standard terms, zero hand-built taxonomy, both
verdict words used correctly. Record: repo names, pack ids, sample counts,
terms seen, date.

## Failure notes

- `sha256 mismatch at first fetch` — upstream served different bytes than
  the pin (RISK-001). STOP; report the entry id. Re-pinning is a reviewed
  change to `core/packs/data/catalogue.json`, never a silent edit.
- `pack … is not cached` on widen — run `pack install` once (first install
  needs the network); widening itself is offline-only.
- Busy SQLite during install while a server runs — the WAL + 5s busy
  timeout retries; if a busy error names them, retry the install; writes
  are idempotent upserts and converge.
- Pack installed but `ontology_read` unchanged — the server was not
  restarted, or an explicit `--ontology` flag is overriding the
  conventional path (flags always win).
