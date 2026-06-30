//! SQLite vector adapter for Engram retrieval candidates.
//!
//! Vector indexes are secondary adapter state. This crate stores vectors and
//! target metadata without changing canonical memory or knowledge records.

mod entry;
mod extension;
#[cfg(feature = "fastembed-provider")]
mod fastembed_provider;
mod index;
mod retrieval;
mod vector;

pub use entry::{VectorEntry, VectorSearchResult};
#[cfg(feature = "fastembed-provider")]
pub use fastembed_provider::FastEmbedBgeSmallQueryProvider;
pub use index::SqliteVectorIndex;
pub use retrieval::{
    VectorQueryProvider, VectorResolvedTarget, VectorRetrievalIndex, VectorTargetResolver,
};
pub use vector::serialize_f32_vector;
