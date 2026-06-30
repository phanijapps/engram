# Binding Instructions

Read ../AGENTS.md first.

This directory owns native language bridges over Rust behavior.

Bindings may compose core crates and selected adapters to expose runtime
ergonomics, but they must not redefine memory, knowledge, retrieval, policy, or
domain contract behavior. Keep serialization and native error mapping separate
from engine logic.
