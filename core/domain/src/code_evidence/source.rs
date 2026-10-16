use serde::{Deserialize, Serialize};

/// Exact canonical descriptor-unit identity. `unit_id` is not a chunk key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", try_from = "SourceWire")]
pub struct CodeEvidenceSourceRef {
    pub repository: String,
    pub path: String,
    pub unit_id: String,
    pub source_hash: String,
    pub generation: String,
    pub representation_id: String,
    pub start_byte: u64,
    pub end_byte: u64,
    pub indexed_revision: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SourceWire {
    repository: String,
    path: String,
    unit_id: String,
    source_hash: String,
    generation: String,
    representation_id: String,
    start_byte: u64,
    end_byte: u64,
    #[serde(deserialize_with = "super::required_nullable")]
    indexed_revision: Option<String>,
}

impl TryFrom<SourceWire> for CodeEvidenceSourceRef {
    type Error = &'static str;
    fn try_from(w: SourceWire) -> Result<Self, Self::Error> {
        if [
            &w.repository,
            &w.path,
            &w.unit_id,
            &w.source_hash,
            &w.generation,
            &w.representation_id,
        ]
        .iter()
        .any(|s| s.is_empty())
            || w.start_byte >= w.end_byte
            || w.end_byte > 9_007_199_254_740_991
            || w.path.len() > 4096
            || w.path.starts_with('/')
            || w.path.contains('\\')
            || w.path.split('/').any(|s| matches!(s, "" | "." | ".."))
        {
            return Err("invalid code evidence source reference");
        }
        Ok(Self {
            repository: w.repository,
            path: w.path,
            unit_id: w.unit_id,
            source_hash: w.source_hash,
            generation: w.generation,
            representation_id: w.representation_id,
            start_byte: w.start_byte,
            end_byte: w.end_byte,
            indexed_revision: w.indexed_revision,
        })
    }
}
