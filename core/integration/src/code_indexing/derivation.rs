//! Canonical parent → deterministic tokenizer-bounded segment descriptions.
use crate::{EmbeddingProvider, code_representation::CodeRepresentation};
use engram_code::{
    CodeUnitSyntax, TreeSitterChunker, analyze_code_unit, segment_source_ranges_with_context,
    source_import_ranges, structural_boundaries,
};
use engram_domain::*;

use engram_retrieval::CodeIndexConfig;
use engram_runtime::{CoreError, CoreResult};
use sha2::{Digest, Sha256};

fn invalid(reason: &str) -> CoreError {
    CoreError::ValidationFailed {
        reason: reason.into(),
    }
}

pub(super) struct DerivedSegments {
    pub inventory: CodeSegmentInventory,
    pub segments: Vec<CodeEmbeddingSegment>,
}
pub(super) fn derive(
    source: &CodeSourceReadResult,
    parents: &[KnowledgeChunk],
    model: &dyn EmbeddingProvider,
    config: &CodeIndexConfig,
    representation: &CodeRepresentation,
) -> CoreResult<DerivedSegments> {
    if !model.is_local() {
        return Err(invalid("code tokenization requires a local model"));
    }
    let extension = source
        .manifest
        .file
        .path
        .rsplit_once('.')
        .map(|(_, ext)| ext)
        .unwrap_or("")
        .to_ascii_lowercase();
    let parser = TreeSitterChunker::new()?;
    let tree = parser.parse(&source.text, &extension).ok();
    let boundaries = tree.as_ref().map(structural_boundaries).unwrap_or_default();
    let imports = tree.as_ref().map(source_import_ranges).unwrap_or_default();
    let m = &source.manifest;
    let mut segments = Vec::new();
    for parent in parents {
        if parent.text.is_empty() {
            continue;
        }
        let code = parent
            .code
            .as_ref()
            .ok_or_else(|| invalid("code unit has no canonical coverage"))?;
        if code.role == CodeChunkRole::SourceSnapshot {
            return Err(invalid("source snapshots cannot become embedding parents"));
        }
        let unit = code.unit.as_ref().ok_or_else(|| {
            invalid("code unit needs verified normalized identity; rescan its source")
        })?;
        let syntax = tree
            .as_ref()
            .map(|tree| analyze_code_unit(&source.text, &m.file.path, tree, code.declaration_range))
            .unwrap_or(CodeUnitSyntax {
                category: CodeSourceCategory::Unknown,
                signature_range: None,
            });
        let plans = segment_source_ranges_with_context(
            &source.text,
            &code.ranges,
            &code.slice_separator,
            &boundaries,
            config.body_token_target,
            |text| {
                let prefix = (text.len() - text.trim_start().len()) as u64;
                Ok(model
                    .tokenize_untruncated(text.trim(), false)?
                    .offsets
                    .into_iter()
                    .map(|r| CodeByteRange {
                        start: r.start + prefix,
                        end: r.end + prefix,
                    })
                    .collect())
            },
        )?;
        for plan in plans {
            let body = engram_knowledge::code_segment_text(parent, &plan.ranges)?;
            let context = super::context::select(
                parent,
                parents,
                super::context::SyntaxContext {
                    signature: syntax.signature_range,
                    imports: &imports,
                    source: &source.text,
                },
                &body,
                model,
                config,
                representation,
            )?;
            let (_, coverage) =
                super::input::build_input(&body, &context, model, config, representation)?
                    .ok_or_else(|| {
                        invalid("complete segment input exceeds its declared tokenizer contract")
                    })?;
            let identity = serde_json::to_vec(&(
                "code-segment-v1",
                &representation.id,
                &parent.id,
                unit,
                &code.fingerprint,
                &plan.ranges,
                &body,
                &context,
                syntax.category,
            ))
            .map_err(|e| invalid(&e.to_string()))?;
            let digest: String = Sha256::digest(identity)
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect();
            segments.push(CodeEmbeddingSegment {
                id: format!("segment-{digest}"),
                parent_chunk_id: parent.id.clone(),
                parent_unit_id: unit.clone(),
                file: m.file.clone(),
                generation: m.generation.clone(),
                attempt_id: m.attempt_id.clone(),
                source_hash: m.source_hash.clone(),
                representation_id: representation.id.clone(),
                ranges: plan.ranges,
                continuation: plan.continuation,
                source_category: syntax.category,
                context,
                embedding_input: coverage,
            });
        }
    }
    Ok(DerivedSegments {
        inventory: CodeSegmentInventory {
            file: m.file.clone(),
            generation: m.generation.clone(),
            attempt_id: m.attempt_id.clone(),
            representation_id: representation.id.clone(),
            segment_ids: segments.iter().map(|s| s.id.clone()).collect(),
            state: CodeSourceState::Updating,
        },
        segments,
    })
}
