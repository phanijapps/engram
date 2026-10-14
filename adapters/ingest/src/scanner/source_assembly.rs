//! Source-aware candidate assembly; persistence and leases live elsewhere.
use crate::{
    CodeSymbolChunker, DocumentIngestRequest, IngestedKnowledge, KnowledgeIngestor, content_hash,
};
use engram_code::{ChunkCandidate, CoveredCodeCandidate, snapshot_ranges};
use engram_domain::*;
use engram_knowledge::{CoreError, CoreResult};

pub(super) const FINGERPRINT: &str =
    "source-v1/ts-0.26/mapping-2026-10-14/context-1/snapshot-65536";

pub(super) fn prepare(
    request: DocumentIngestRequest,
    manifest: &mut CodeSourceGeneration,
    mut units: Vec<CoveredCodeCandidate>,
    quality: CodeParseQuality,
) -> CoreResult<IngestedKnowledge> {
    if units.is_empty() && !request.text.trim().is_empty() {
        units.push(CoveredCodeCandidate {
            candidate: ChunkCandidate {
                kind: KnowledgeChunkKind::CodeBlock,
                text: request.text.clone(),
                location: None,
            },
            role: CodeChunkRole::Residual,
            ranges: vec![CodeByteRange {
                start: 0,
                end: request.text.len() as u64,
            }],
            declaration_range: None,
            parse_quality: quality,
            complete: false,
            unsupported_features: vec!["symbol_mapping".into(), "structural_edges".into()],
        });
    }
    let mut candidates = vec![];
    let mut descriptors = vec![];
    for range in snapshot_ranges(&request.text, 65536)? {
        candidates.push(ChunkCandidate {
            kind: KnowledgeChunkKind::CodeBlock,
            text: request.text[range.start as usize..range.end as usize].to_owned(),
            location: Some(SourceLocation {
                path: None,
                start_line: None,
                end_line: None,
                start_offset: None,
                end_offset: None,
                anchor: Some(format!("snapshot:{}-{}", range.start, range.end)),
            }),
        });
        descriptors.push(descriptor(
            manifest,
            CodeChunkRole::SourceSnapshot,
            vec![range],
            None,
            quality,
            true,
            vec![],
            None,
            None,
        ));
    }
    let unit_ids: Vec<String> = units
        .iter()
        .map(|u| {
            content_hash(format!(
                "{}:{}:{}",
                manifest.generation,
                u.candidate
                    .location
                    .as_ref()
                    .and_then(|l| l.anchor.as_deref())
                    .unwrap_or("residual"),
                serde_json::to_string(&u.ranges).unwrap()
            ))
        })
        .collect();
    for (i, unit) in units.iter().enumerate() {
        let parent = unit.declaration_range.and_then(|span| {
            units
                .iter()
                .enumerate()
                .filter_map(|(j, p)| {
                    if i == j {
                        return None;
                    }
                    let outer = p.declaration_range?;
                    (outer.start <= span.start && outer.end >= span.end && outer != span)
                        .then_some((outer.end - outer.start, j))
                })
                .min()
                .map(|(_, j)| unit_ids[j].clone())
        });
        candidates.push(unit.candidate.clone());
        descriptors.push(descriptor(
            manifest,
            unit.role,
            unit.ranges.clone(),
            unit.declaration_range,
            unit.parse_quality,
            unit.complete,
            unit.unsupported_features.clone(),
            Some(unit_ids[i].clone()),
            parent,
        ));
    }
    let mut prepared =
        KnowledgeIngestor::new(CodeSymbolChunker).prepare_code_candidates(request, candidates)?;
    for (chunk, code) in prepared.chunks.iter_mut().zip(descriptors) {
        chunk.id = Id::from(format!(
            "chunk-{}",
            content_hash(
                serde_json::to_vec(&(
                    &code.file,
                    &code.generation,
                    code.role,
                    &code.ranges,
                    &code.unit,
                    &chunk.content_hash
                ))
                .map_err(|e| CoreError::InvalidRequest {
                    reason: e.to_string()
                })?
            )
        ));
        if code.role == CodeChunkRole::SourceSnapshot {
            manifest.snapshot_ids.push(chunk.id.clone());
        }
        chunk.code = Some(code);
    }
    manifest.document_id = Some(prepared.document.id.clone());
    manifest.records.push(CodeSourceRecord {
        kind: CodeSourceRecordKind::Source,
        id: prepared.source.id.to_string(),
    });
    manifest.records.push(CodeSourceRecord {
        kind: CodeSourceRecordKind::Document,
        id: prepared.document.id.to_string(),
    });
    manifest
        .records
        .extend(prepared.chunks.iter().map(|c| CodeSourceRecord {
            kind: CodeSourceRecordKind::Chunk,
            id: c.id.to_string(),
        }));
    Ok(prepared)
}
#[allow(clippy::too_many_arguments)]
fn descriptor(
    m: &CodeSourceGeneration,
    role: CodeChunkRole,
    ranges: Vec<CodeByteRange>,
    declaration_range: Option<CodeByteRange>,
    parse_quality: CodeParseQuality,
    complete: bool,
    unsupported_features: Vec<String>,
    unit: Option<String>,
    parent_unit: Option<String>,
) -> CodeChunkDescriptor {
    CodeChunkDescriptor {
        role,
        file: m.file.clone(),
        generation: m.generation.clone(),
        source_hash: m.source_hash.clone(),
        fingerprint: m.fingerprint.clone(),
        ranges,
        slice_separator: if role == CodeChunkRole::ContainerContext {
            "\n".into()
        } else {
            String::new()
        },
        declaration_range,
        complete,
        parse_quality,
        unsupported_features,
        unit,
        parent_unit,
    }
}
