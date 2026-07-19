# TypeScript Packages

Engram uses TypeScript for generated contracts, SDK ergonomics, native binding
packaging, and JavaScript-side integrations.

Current packages:

- `contracts`: `@engram/contracts`, generated from `contracts/v1/`.
- `client`: `@engram/client`, a typed facade over an injected transport.
- `node`: `@engram/node`, a Node native binding transport over Rust behavior.
- `adapters`: `@engram/adapters`, framework-neutral integration utilities.

Reserved packages:

- `eval`: evaluation fixture helpers and CLI wrappers.

TypeScript packages must not redefine domain truth. Public types should come
from `@engram/contracts`.
