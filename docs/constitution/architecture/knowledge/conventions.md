# Conventions across crates and packages

<!-- prerequisite: layout -->

## Conventions you'll see across crates/packages

- Every Rust crate is small with one responsibility; crate `lib.rs` files are
  facades (module declarations + narrow re-exports + top-level docs only).
- Errors are typed (`CoreError`); no stringly public contracts.
- Store/vector/embedding/model integrations sit behind traits; tests inject
  deterministic stubs.
- One crate per storage engine; all calls for an engine live inside its cell.
- FastEmbed is a compile-time Cargo feature (`--features fastembed`); the demo
  degrades gracefully without it.
- TypeScript packages keep `index.ts` as narrow facades; the native binding is a
  transport, not a second implementation.
- Validation gates in `AGENTS.md` § Validation (`cargo fmt/check`,
  `pnpm contracts:generate/typecheck/test`, `.codex/hooks/*`).

## Where to start

1. Read [`docs/product/engram.md`](../product/engram.md) — mission, scope, principles.
2. Read [`reference.md`](reference.md) (normative golden path) then this overview
   (descriptive map).
3. Read [ADR-0022](../adr/0022-engine-grid-vs-backend-recipe.md) (engine grid +
  one-crate-per-backend) and [ADR-0009](../adr/0009-retrieval-composition-seam.md)
  (the load-bearing retrieval-composition decision).
4. Skim [`docs/research/README.md`](../research/README.md) for the synthesized
   research that grounds the model.
5. Pick a recent spec in `docs/specs/` (e.g. `fetch-content-backend`) and read its
   `spec.md` + `plan.md` alongside the code they produced.
6. Run the demo: see the [build guide](../guides/how-to/build-and-run.md), then
   index a repo via the dashboard and watch the RRF-hybrid Q&A.


---

