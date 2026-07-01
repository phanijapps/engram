//! Orchestration facade for the engram engine.
//!
//! `engram-core` is the compatibility facade and re-export layer above the
//! dedicated behavior crates. It owns no behavior ports itself: memory ports
//! live in `engram-memory`; knowledge, graph, ontology, source, and ingestion
//! ports in `engram-knowledge`; belief and contradiction ports in
//! `engram-belief`; hierarchy ports in `engram-hierarchy`; consolidation ports
//! in `engram-consolidation`; evaluation ports in `engram-eval`. Concrete
//! infrastructure belongs behind adapters.

pub use engram_belief::*;
pub use engram_consolidation::*;
pub use engram_eval::{EvaluationCaseReport, EvaluationReport, EvaluationRunner};
pub use engram_hierarchy::*;
pub use engram_knowledge::*;
pub use engram_memory::*;
pub use engram_retrieval::{ContextComposer, RetrievalFusion, RetrievalIndex};
pub use engram_runtime::{CoreError, CoreResult};
