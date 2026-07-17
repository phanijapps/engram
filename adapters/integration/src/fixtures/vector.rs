//! Vector capability fixture.
//!
//! Exercises raw vector insert/search and embedding-space mismatch rejection
//! against the in-memory `SqliteVectorIndex` via the `VectorIndex` trait.

use engram_domain::{EmbeddingSpace, EmbeddingTargetType, Id};
use engram_retrieval::VectorIndex;
use engram_runtime::{CoreError, CoreResult};
use engram_store_sqlite::SqliteVectorIndex;

/// Runs the vector capability fixture.
///
/// # Errors
///
/// Returns `CoreError::Adapter` if insert/search round-trip fails, or if a
/// mismatched embedding space is not rejected.
pub fn run_vector_fixture() -> CoreResult<()> {
    let dims = 4u32;
    let space = EmbeddingSpace::new("conformance", "bge-small", dims, "query", None::<String>);
    let index = SqliteVectorIndex::open_in_memory(dims)?.with_embedding_space(space.clone());

    let target = Id::from("chunk-1");
    VectorIndex::insert(&index, &target, &space, vec![0.1, 0.2, 0.3, 0.4])
        .map_err(|e| err("insert")(e))?;

    let hits = VectorIndex::search(&index, &space, vec![0.1, 0.2, 0.3, 0.4], 1)
        .map_err(|e| err("search")(e))?;
    if hits.len() != 1 {
        return Err(err("search")(CoreError::Conflict {
            reason: format!("expected 1 hit, got {}", hits.len()),
        }));
    }
    if hits[0].0 != target {
        return Err(err("search")(CoreError::Conflict {
            reason: "returned target id did not match inserted id".to_string(),
        }));
    }

    // Embedding-space mismatch must be rejected on both insert and search.
    let wrong_space = EmbeddingSpace::new("other-provider", "nomic", dims, "query", None::<String>);
    let insert_mismatch =
        VectorIndex::insert(&index, &target, &wrong_space, vec![0.1, 0.2, 0.3, 0.4]);
    if insert_mismatch.is_ok() {
        return Err(err("space_mismatch")(CoreError::Conflict {
            reason: "insert with mismatched embedding space was accepted".to_string(),
        }));
    }

    let search_mismatch = VectorIndex::search(&index, &wrong_space, vec![0.1, 0.2, 0.3, 0.4], 1);
    if search_mismatch.is_ok() {
        return Err(err("space_mismatch")(CoreError::Conflict {
            reason: "search with mismatched embedding space was accepted".to_string(),
        }));
    }

    // Dimension mismatch must also be rejected.
    let dim_mismatch = VectorIndex::insert(&index, &target, &space, vec![0.1, 0.2, 0.3]);
    if dim_mismatch.is_ok() {
        return Err(err("dimension_mismatch")(CoreError::Conflict {
            reason: "insert with wrong dimensionality was accepted".to_string(),
        }));
    }

    // Silence unused-import warning for EmbeddingTargetType (re-exported by the
    // trait surface for callers that build entries directly).
    let _ = EmbeddingTargetType::Chunk;
    Ok(())
}

fn err<'a>(op: &'a str) -> impl Fn(CoreError) -> CoreError + 'a {
    move |e| CoreError::Adapter {
        adapter: "conformance.vector".to_string(),
        message: format!("{op}: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vector_fixture_passes() {
        if let Err(e) = run_vector_fixture() {
            panic!("vector fixture failed: {e}");
        }
    }
}
