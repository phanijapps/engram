//! Portable domain contract for engram.
//!
//! This crate contains storage-neutral data models that mirror
//! `docs/domain-data-model.md`. It is allowed to define serialized shapes,
//! identifiers, enums, lightweight validation helpers, and compatibility-facing
//! value objects. It must not define persistence, vector indexing, model
//! provider calls, gateway behavior, or TypeScript binding logic.

pub mod assertion;
pub mod belief;
pub mod capability;
pub mod community;
pub mod embedding;
pub mod evaluation;
pub mod hierarchy;
pub mod identity;
pub mod knowledge;
pub mod memory;
pub mod ontology;
pub mod operations;
pub mod paging;
pub mod policy;
pub mod procedures;
pub mod provenance;
pub mod retrieval;
pub mod rule;
pub mod taxonomy;
pub mod trace;
pub mod types;

pub use assertion::*;
pub use belief::*;
pub use capability::*;
pub use community::*;
pub use embedding::*;
pub use evaluation::*;
pub use hierarchy::*;
pub use identity::*;
pub use knowledge::*;
pub use memory::*;
pub use ontology::*;
pub use operations::*;
pub use paging::*;
pub use policy::*;
pub use procedures::*;
pub use provenance::*;
pub use retrieval::*;
pub use rule::{ApplicabilityRule, RuleTarget};
pub use taxonomy::*;
pub use trace::DecisionTrace;
pub use types::{ScopeMappingStrategy, *};
