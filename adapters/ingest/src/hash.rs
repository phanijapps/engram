//! Stable content hashing for ingestion records.
//!
//! This module owns deterministic byte hashing for sources, documents, and
//! chunks. It does not decide identity semantics beyond returning the portable
//! hash string that callers may use in content-derived IDs.

use sha2::{Digest, Sha256};

/// Returns a SHA-256 content hash using the accepted `sha256:<hex>` format.
///
/// The prefix keeps hashes self-describing when stored beside future algorithms.
pub fn content_hash(content: impl AsRef<[u8]>) -> String {
    let digest = Sha256::digest(content.as_ref());
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        write!(&mut hex, "{byte:02x}").expect("writing to String cannot fail");
    }
    format!("sha256:{hex}")
}
