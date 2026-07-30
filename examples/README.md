# Examples

Use this folder as the index for scenario fixtures and checked usage sketches.
Runtime examples live next to the crate or package that owns the public API.

## Local Rust Adapters

Run the SQLite adapter example:

```bash
cargo run -p engram-store-sql --example sql_memory
```

Both examples reuse the accepted v1 write and retrieval fixtures under
`contracts/v1/examples/`.

## TypeScript Client

The checked TypeScript usage sketch is:

```text
packages/client/examples/injected-transport.ts
```

It uses an injected transport so client ergonomics can be typechecked and tested
without requiring a built native addon artifact. It is covered by:

```bash
pnpm --filter @engram/client typecheck
pnpm --filter @engram/client test
```

## Scan Config

A checked sample of the `scan_repo` filter config (concept-link blocklist/
allowlist + file denylist). Copy it to `<repo>/.engram/scan.json` or pass its
path as the `scan_config` arg:

- [`scan-config/scan.json`](scan-config/scan.json) — the sample.
- [`scan-config/README.md`](scan-config/README.md) — field reference + discovery ladder.


## Future Examples

- Convert a task trace into an episode memory.
- Retrieve project-specific memories under workspace scope.
- Verify that restricted memories are filtered for unauthorized requesters.
