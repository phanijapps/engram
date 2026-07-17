//! Vector byte encoding for sqlite-vec.
//!
//! This module owns conversion from Rust `f32` slices to sqlite-vec's expected
//! little-endian byte representation. Dimension policy remains in the index
//! module where the table shape is known.

use engram_runtime::{CoreError, CoreResult};

/// Serializes `f32` vectors as little-endian bytes for sqlite-vec.
///
/// Empty vectors are rejected because sqlite-vec tables are created with an
/// explicit non-zero dimensionality.
pub fn serialize_f32_vector(vector: &[f32]) -> CoreResult<Vec<u8>> {
    if vector.is_empty() {
        return Err(CoreError::InvalidRequest {
            reason: "vector must not be empty".to_owned(),
        });
    }
    let mut bytes = Vec::with_capacity(std::mem::size_of_val(vector));
    for value in vector {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    Ok(bytes)
}
