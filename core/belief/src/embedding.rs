//! Embedding compatibility helpers for belief search.
//!
//! Engram belief records reference embeddings instead of embedding raw vectors
//! in the domain model. Compatibility adapters that must preserve an existing
//! raw f32-byte ranking contract can use these pure helpers while keeping the
//! sidecar bytes outside domain truth.

use std::cmp::Ordering;

use engram_runtime::{CoreError, CoreResult};

/// One candidate vector to rank against a query vector.
#[derive(Debug, Clone, PartialEq)]
pub struct BeliefEmbeddingCandidate<T> {
    pub item: T,
    pub embedding: Vec<f32>,
}

/// Candidate plus cosine score.
#[derive(Debug, Clone, PartialEq)]
pub struct BeliefEmbeddingScore<T> {
    pub item: T,
    pub score: f32,
}

/// Decodes little-endian `f32` bytes into a vector.
///
/// The function rejects incomplete byte groups and non-finite values so ranking
/// cannot silently treat malformed compatibility bytes as useful signal.
pub fn decode_f32_le(bytes: &[u8]) -> CoreResult<Vec<f32>> {
    let chunks = bytes.chunks_exact(4);
    if !chunks.remainder().is_empty() {
        return Err(CoreError::InvalidRequest {
            reason: "embedding byte length must be a multiple of 4".to_owned(),
        });
    }

    let mut values = Vec::with_capacity(bytes.len() / 4);
    for chunk in chunks {
        let value = f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        if !value.is_finite() {
            return Err(CoreError::InvalidRequest {
                reason: "embedding values must be finite f32 numbers".to_owned(),
            });
        }
        values.push(value);
    }
    Ok(values)
}

/// Computes cosine similarity between two vectors.
pub fn cosine_similarity(left: &[f32], right: &[f32]) -> CoreResult<f32> {
    if left.len() != right.len() {
        return Err(CoreError::InvalidRequest {
            reason: "embedding dimensions must match".to_owned(),
        });
    }
    if left.is_empty() {
        return Err(CoreError::InvalidRequest {
            reason: "embedding vectors must not be empty".to_owned(),
        });
    }

    let mut dot = 0.0_f32;
    let mut left_norm = 0.0_f32;
    let mut right_norm = 0.0_f32;
    for (left_value, right_value) in left.iter().zip(right.iter()) {
        dot += left_value * right_value;
        left_norm += left_value * left_value;
        right_norm += right_value * right_value;
    }
    if left_norm == 0.0 || right_norm == 0.0 {
        return Ok(0.0);
    }
    Ok(dot / (left_norm.sqrt() * right_norm.sqrt()))
}

/// Ranks embedding candidates by descending cosine score.
pub fn rank_embedding_candidates<T>(
    query: &[f32],
    candidates: impl IntoIterator<Item = BeliefEmbeddingCandidate<T>>,
    min_score: Option<f32>,
    limit: Option<usize>,
) -> CoreResult<Vec<BeliefEmbeddingScore<T>>> {
    let mut ranked = Vec::new();
    for candidate in candidates {
        let score = cosine_similarity(query, &candidate.embedding)?;
        if min_score.is_none_or(|threshold| score >= threshold) {
            ranked.push(BeliefEmbeddingScore {
                item: candidate.item,
                score,
            });
        }
    }
    ranked.sort_by(|left, right| {
        right
            .score
            .partial_cmp(&left.score)
            .unwrap_or(Ordering::Equal)
    });
    if let Some(limit) = limit {
        ranked.truncate(limit);
    }
    Ok(ranked)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bytes(values: &[f32]) -> Vec<u8> {
        values
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .collect()
    }

    #[test]
    fn decodes_little_endian_f32_bytes() {
        assert_eq!(
            decode_f32_le(&bytes(&[1.0, -2.5])).expect("decode"),
            vec![1.0, -2.5]
        );
        assert!(decode_f32_le(&[1, 2, 3]).is_err());
        assert!(decode_f32_le(&bytes(&[f32::NAN])).is_err());
    }

    #[test]
    fn cosine_similarity_scores_and_rejects_bad_dimensions() {
        let score = cosine_similarity(&[1.0, 0.0], &[1.0, 0.0]).expect("score");
        assert!((score - 1.0).abs() < f32::EPSILON);
        assert_eq!(
            cosine_similarity(&[0.0, 0.0], &[1.0, 0.0]).expect("score"),
            0.0
        );
        assert!(cosine_similarity(&[1.0], &[1.0, 0.0]).is_err());
    }

    #[test]
    fn ranks_candidates_by_descending_score() {
        let ranked = rank_embedding_candidates(
            &[1.0, 0.0],
            [
                BeliefEmbeddingCandidate {
                    item: "b",
                    embedding: vec![0.0, 1.0],
                },
                BeliefEmbeddingCandidate {
                    item: "a",
                    embedding: vec![1.0, 0.0],
                },
            ],
            Some(0.1),
            Some(1),
        )
        .expect("rank");

        assert_eq!(ranked.len(), 1);
        assert_eq!(ranked[0].item, "a");
    }
}
