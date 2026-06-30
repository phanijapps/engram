# @engram/node

Native Node transport over the Rust engine.

The package loads the compiled `engram-node` Node-API addon and exposes a
transport compatible with `@engram/client`. It converts generated contract
objects to JSON strings for the native boundary and parses Rust-produced
contract responses back into TypeScript types. It does not implement memory
semantics in TypeScript.

This package should expose Rust-backed behavior instead of reimplementing core
memory semantics in TypeScript.
