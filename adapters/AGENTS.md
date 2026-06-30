# Adapter Instructions

Read ../AGENTS.md first.

This directory owns replaceable infrastructure implementations behind core
ports: memory stores, knowledge stores, retrieval indexes, source readers, and
other provider-backed integrations.

Adapters may depend on core crates. Core crates must not depend on adapters.
Keep adapter-specific policy checks and error translation visible near the
operation that crosses the infrastructure boundary.

Do not use `common`, `shared`, or broad manager modules to mix unrelated
adapters. Split by storage engine, source type, provider, or protocol.
