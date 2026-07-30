# Spec — engram-mcp P4 Scoping + Unified Composites (RFC-0016)

- **Status:** Implemented (P4 complete 2026-07-29; backward-compatible with --project)
- **Mode:** full (CLI/scope change + composite enhancement)
- **Constrained by:** RFC-0016 D2 (Org→Domain→Subdomain scoping), D3 (code over unified graph)
- **Branch:** `feat/engram-mcp-core`

## Objective

1. **Scoping model (D2)** — recast Zbot's `ward` as Org → Domain → Subdomain via
   `Scope { tenant: Org, workspace: Domain[/Subdomain] }`. New `--org`/`--domain`/
   `--subdomain` flags; `--project` kept as a backward-compatible workspace alias.
   Strict matching isolates subdomains.
2. **Code composites over the unified graph** — `get_context` surfaces the
   doc/concept `describes` links alongside recall + the code neighborhood, so an
   agent asking about a symbol gets code + its documentation in one packet.

## Boundaries

**Always do** — scope uses existing `Scope` fields (no `core/domain` change);
`--project` stays a working alias; composite change is additive.
**Never do** — no hierarchical (cross-subdomain) matching yet (Strict only); no new
provider handle; no `core/domain` break.

## Acceptance Criteria

1. `--org acme --domain checkout --subdomain payments` → `tenant="acme"`,
   `workspace="checkout/payments"`; `--org acme --domain checkout` → `"checkout"`.
2. `--project foo` (no org/domain) → `tenant="default"`, `workspace="foo"` (backward compat).
3. `get_context(focus)` output includes the `describes`/concept links for the focus
   when present (a `[Graph]` section), in addition to `[Recall]` + `[Code]`.
4. Existing scope tests still pass; `--project` deployments unaffected.
5. Gates: `cargo fmt/check/test -p engram-mcp`, neutrality, docs pass.

## Tasks
- **S1** `config.rs`: `--org`/`--domain`/`--subdomain` flags + fields. · TDD · none.
- **S2** `scope.rs`: `resolve_scope(org, domain, subdomain, project)` + tests. · TDD · S1.
- **S3** `main.rs`: wire `resolve_scope`; update usage string. · goal-based · S2.
- **S4** `codegraph.rs`: `get_context` adds a `[Graph]` section (describes/mentions
  edges for the focus). · TDD · none.
- **S5** Gates + commit + deploy. · S1–S4.
