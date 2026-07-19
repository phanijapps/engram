# Ingest Adapter Instructions

Read ../../AGENTS.md and ../AGENTS.md first.

This crate currently owns concrete filesystem and Git source readers alongside
deterministic ingestion helpers. Treat it as an adapter crate until the pure
ingestion orchestration is split into core.

Do not add durable knowledge storage, vector indexing, memory write semantics,
or provider embedding/model behavior here.
