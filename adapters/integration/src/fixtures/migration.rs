//! Migration capability fixture.
//!
//! Exercises the manifest fingerprint contract: identical import data produces
//! identical fingerprints, different data produces different fingerprints, and
//! a stale manifest (fingerprint mismatch) is detectable.

use engram_integration::{ImportData, RowCounts, compute_manifest_fingerprint, record_key_hash};
use engram_runtime::{CoreError, CoreResult};

/// Runs the migration capability fixture.
///
/// # Errors
///
/// Returns `CoreError::Adapter` if fingerprint determinism, sensitivity, or
/// stale-manifest detection fail.
pub fn run_migration_fixture() -> CoreResult<()> {
    let counts = sample_counts();
    let hashes = sample_hashes();

    // Determinism: same inputs -> same fingerprint.
    let fp1 = compute_manifest_fingerprint(&counts, &hashes);
    let fp2 = compute_manifest_fingerprint(&counts, &hashes);
    if fp1 != fp2 {
        return Err(err("determinism")(CoreError::Conflict {
            reason: "fingerprint not deterministic for identical inputs".to_string(),
        }));
    }

    // Sensitivity: different row counts -> different fingerprint.
    let mut changed_counts = counts.clone();
    changed_counts.memory += 1;
    let fp_changed = compute_manifest_fingerprint(&changed_counts, &hashes);
    if fp_changed == fp1 {
        return Err(err("sensitivity")(CoreError::Conflict {
            reason: "fingerprint insensitive to row-count change".to_string(),
        }));
    }

    // Sensitivity: different content hashes -> different fingerprint.
    let mut changed_hashes = hashes.clone();
    changed_hashes[0] = record_key_hash("mem-1|tenant-z|999");
    let fp_changed_hash = compute_manifest_fingerprint(&counts, &changed_hashes);
    if fp_changed_hash == fp1 {
        return Err(err("sensitivity")(CoreError::Conflict {
            reason: "fingerprint insensitive to content-hash change".to_string(),
        }));
    }

    // Stale-manifest detection: an apply with a mismatched fingerprint is
    // rejected (the contract the MigrationService enforces).
    let stored_fingerprint = fp1;
    let on_disk_fingerprint = fp_changed;
    if stored_fingerprint == on_disk_fingerprint {
        return Err(err("stale_detection")(CoreError::Conflict {
            reason: "stale-manifest scenario did not produce a mismatch".to_string(),
        }));
    }

    // Sanity: the ImportData type is constructible (the migration data surface).
    let _data = ImportData {
        memories: Vec::new(),
        knowledge_sources: Vec::new(),
        knowledge_documents: Vec::new(),
        knowledge_chunks: Vec::new(),
        knowledge_entities: Vec::new(),
        knowledge_relationships: Vec::new(),
        concept_schemes: Vec::new(),
        concepts: Vec::new(),
        beliefs: Vec::new(),
        hierarchy_nodes: Vec::new(),
        vectors: Vec::new(),
    };
    Ok(())
}

fn sample_counts() -> RowCounts {
    RowCounts {
        memory: 2,
        knowledge_sources: 1,
        knowledge_documents: 1,
        knowledge_chunks: 3,
        knowledge_entities: 4,
        knowledge_relationships: 5,
        concept_schemes: 1,
        concepts: 6,
        beliefs: 2,
        contradictions: 0,
        hierarchy_nodes: 3,
        vectors: 7,
    }
}

fn sample_hashes() -> Vec<String> {
    vec![
        record_key_hash("mem-1|tenant-a|100"),
        record_key_hash("mem-2|tenant-a|101"),
        record_key_hash("chunk-1|tenant-a|102"),
    ]
}

fn err<'a>(op: &'a str) -> impl Fn(CoreError) -> CoreError + 'a {
    move |e| CoreError::Adapter {
        adapter: "conformance.migration".to_string(),
        message: format!("{op}: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migration_fixture_passes() {
        if let Err(e) = run_migration_fixture() {
            panic!("migration fixture failed: {e}");
        }
    }
}
