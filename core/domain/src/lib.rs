//! Portable domain contract for engram.
//!
//! This crate contains storage-neutral data models that mirror
//! `docs/domain-data-model.md`. It is allowed to define serialized shapes,
//! identifiers, enums, lightweight validation helpers, and compatibility-facing
//! value objects. It must not define persistence, vector indexing, model
//! provider calls, gateway behavior, or TypeScript binding logic.

pub mod belief;
pub mod evaluation;
pub mod hierarchy;
pub mod identity;
pub mod knowledge;
pub mod memory;
pub mod ontology;
pub mod operations;
pub mod policy;
pub mod provenance;
pub mod retrieval;
pub mod taxonomy;
pub mod types;

pub use belief::*;
pub use evaluation::*;
pub use hierarchy::*;
pub use identity::*;
pub use knowledge::*;
pub use memory::*;
pub use ontology::*;
pub use operations::*;
pub use policy::*;
pub use provenance::*;
pub use retrieval::*;
pub use taxonomy::*;
pub use types::*;
