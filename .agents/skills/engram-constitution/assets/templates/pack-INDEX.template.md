# {{PACK}} sub-constitution

SUBORDINATE to the root `00-INDEX.md` — one constitution, one root; this pack
narrows it locally. Rules carry prefix `{{PREFIX}}-`; every rule must be FALSE
for a different codebase (anti-generic test) and live one-per-file.

## Charter

{{CHARTER: what this pack owns — one paragraph, domain-specific}}

## Structure (one home per concern)

| Concern | Lives at |
|---|---|
| Knowledge | `knowledge.md` (STE descriptive register) |
| Rules | `rules/<ID>.md` — one rule per file (STE procedural register) |
| Runbooks | `runbooks/` — versioned incident procedures (when they exist) |
| Deviations | below; global: root DEVIATIONS / DECAY-WINDOWS |

## Rule index

| ID | Rule (one line, STE) | File |
|---|---|---|
{{RULE_ROWS}}

## Deviations & decay (domain)

- none open (or: {{WINDOW}} — due {{DATE}})

## Notes

{{ENFORCEMENT_HONESTY: what is gate-enforced vs review-enforced — stated, not smoothed}}
