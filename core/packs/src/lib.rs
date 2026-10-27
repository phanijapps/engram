//! Standard ontology pack engine — catalogue, converter, slice rules.
//!
//! Pure crate (DEC-P01): no network, no filesystem reads, no clocks.
//! Everything here is deterministic behavior over bytes and values. The
//! modules land with their tasks: `catalogue` (pinned in-repo catalogue of
//! 33 marketplace entries plus integrity validation), `converter` (Turtle /
//! RDF/XML pack bytes into structural ontology records with hierarchy,
//! DEC-P02), and `slice` (deterministic bounded-slice + widening rules,
//! DEC-P05: top-K with K = 150 classes / 40 predicates, byte-identical
//! two-run output, no embedded timestamps).
//!
//! The `engram-mcp pack` one-shot subcommand owns fetch, cache, facade
//! writes, and reporting; this crate owns the pure rules they run.
