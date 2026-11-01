# Dev — knowledge

<!-- ASD-STE100 descriptive register: short sentences, one topic per paragraph. -->

## Knowledge — how artifacts are made (dev pack)

Cargo workspace + pnpm workspace (packages/, prototype/). `pnpm
contracts:generate` regenerates TS types from `docs/contracts/v1` — CI diffs
the output (drift = failure). N-API binding builds through packages/node.
The launcher agent bundle mirrors `.agents/skills` (installed payload must
equal authored source — DD-002). `vendor/` is read-only vendored truth.
All repo operations are Python under `tools/` (ci/, contracts/, dev/,
release/, hooks/, lib/).
