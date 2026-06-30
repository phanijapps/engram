# Core Instructions

Read ../AGENTS.md first.

This directory owns storage-neutral Rust crates: domain contracts, runtime
primitives, memory and knowledge ports, retrieval composition, orchestration,
and evaluation contracts.

Do not add SQL, vector indexes, in-memory stores, filesystem or Git readers,
Node/N-API bindings, TypeScript tooling, provider SDKs, or gateway integration
code here. Core crates depend on other core crates only.

Crate roots stay facades. Put behavior in focused modules named for the
responsibility they own.
