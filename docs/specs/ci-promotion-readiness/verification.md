# CI repair verification

Verified locally on 2026-10-16 with Rust 1.99.1, matching the stable compiler used by the failing CI run.

## Reproduced failures and repairs

- PR #123's contract log failed because `rg` was absent. Continuous and release jobs that invoke repository scripts now install ripgrep explicitly.
- Removing package `dist` directories reproduced the consumer's missing `@engram/node` module. Building workspace exports restored typechecking. Both workflows now build native bindings and package exports before consumer checks.
- Current Clippy exposed nested conditional, slice-array, sorting, enum-size and test-helper warnings. Applied equivalent rewrites, grouped syntax context arguments, boxed document-write chunks and updated `async-trait` to fix generated warnings. No lint was disabled.
- Updated vulnerable dependency paths, retaining SQLite, pgvector and SurrealDB support. Rust minimum is now 1.88; setup guides agree.

## Evidence

- Format, workspace check and workspace Clippy with `-D warnings` pass. FastEmbed feature test check and Clippy pass.
- `cargo test --workspace`: 1,208 passed, 10 ignored. Live Postgres recipe conformance: five passed.
- Contract validation, generated-contract drift, documentation, engine-neutrality and surface-parity checks pass.
- Root `pnpm run build`, followed by `pnpm run typecheck` and `pnpm run test`, passes with freshly rebuilt native bindings. TypeScript tests: 208 passed, 4 skipped.
- Fresh RustSec audit: zero vulnerabilities, zero advisory ignores, eight informational warnings. Database commit: `479c888ce4401313bca61ba4aba2d0f6244c4a09`.
- Bounded independent review found no blocking regressions.
- Rebuilt release MCP passed `scripts/dev/code_source_smoke.py`: fresh/reopened stores, exact polyglot bytes, complete nested bodies, exclusion rules, repeat scans, invalid encoding and removed sources.
- Additional disposable-store vector smoke: capabilities ready in 4.719s; scan in 0.228s embedded two chunks; exact source read; code evidence returned the fixture in 0.083s with two vector candidates and effective cross-encoder; repeat reindex embedded zero chunks and reported backlog drained. This smoke used default unvalidated calibration and proves wiring, not retrieval accuracy or calibrated confidence.

Informational audit warnings concern `atomic-polyfill`, `bincode`, `instant`, `paste`, `libyml`, `serde_yml` and two `lru` advisories. These are not suppressed. Their major upstream migrations remain separate work. Historical accuracy evaluations and model configuration are unchanged by this repair.

## CI-only model-cache regression

The first repair run passed five GitHub jobs, then exposed a reindex handler test that assumed installed model assets. The generic test fixture can boot without an embedding provider on a clean runner, so expecting embedded-count progress in that state was incorrect. Reindex handler tests now compose their provider explicitly: a non-inferencing stub checks progress/limit formatting, an absent provider checks the honest unavailable response, and scope widening is rejected in either environment. Storage/backfill behavior remains covered by integration tests and the real-model live smoke above. No production behavior, model provisioning, or feature gate was changed for this follow-up.

Follow-up verification with `ENGRAM_MODEL_CACHE` pointed to an empty disposable directory: full workspace suite passed (1,209 passed, 10 ignored), all five live pgvector conformance tests passed, and MCP all-target Clippy passed. The three handler tests perform no inference.
