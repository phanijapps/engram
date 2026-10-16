//! Deterministic, whole-fact context admission from authorized canonical units.
use crate::{EmbeddingProvider, code_representation::CodeRepresentation};
use engram_domain::*;
use engram_retrieval::CodeIndexConfig;
use engram_runtime::CoreResult;

fn reference(
    parent: &KnowledgeChunk,
    range: CodeByteRange,
    representation: &str,
) -> Option<CodeEvidenceSourceRef> {
    let code = parent.code.as_ref()?;
    (range.start < range.end).then(|| CodeEvidenceSourceRef {
        repository: code.file.repository.clone(),
        path: code.file.path.clone(),
        unit_id: code.unit.clone().unwrap_or_default(),
        source_hash: code.source_hash.clone(),
        generation: code.generation.clone(),
        representation_id: representation.into(),
        start_byte: range.start,
        end_byte: range.end,
        indexed_revision: None,
    })
}

pub(super) fn select(
    parent: &KnowledgeChunk,
    parents: &[KnowledgeChunk],
    signature: Option<CodeByteRange>,
    imports: &[CodeByteRange],
    source: &str,
    body: &str,
    model: &dyn EmbeddingProvider,
    config: &CodeIndexConfig,
    representation: &CodeRepresentation,
) -> CoreResult<Vec<CodeContextFact>> {
    if config.context_token_budget == 0 {
        return Ok(vec![]);
    }
    let Some(code) = parent.code.as_ref() else {
        return Ok(vec![]);
    };
    let mut candidates: Vec<(CodeContextKind, String, &KnowledgeChunk, CodeByteRange)> = Vec::new();
    if let Some(range) = code.ranges.iter().copied().find(|r| r.start < r.end) {
        candidates.push((CodeContextKind::Path, code.file.path.clone(), parent, range));
        if let Some(symbol) = parent.location.as_ref().and_then(|l| l.anchor.clone()) {
            candidates.push((CodeContextKind::Symbol, symbol, parent, range));
        }
    }
    let ancestor = code.parent_unit.as_ref().and_then(|unit| {
        parents
            .iter()
            .find(|p| p.code.as_ref().and_then(|c| c.unit.as_ref()) == Some(unit))
    });
    if let Some(donor) = ancestor {
        if let (Some(value), Some(range)) = (
            donor.location.as_ref().and_then(|l| l.anchor.clone()),
            donor.code.as_ref().and_then(|c| c.ranges.first().copied()),
        ) {
            candidates.push((CodeContextKind::Parent, value, donor, range));
        }
    }
    if let Some(range) = signature {
        if let Some(value) = source.get(range.start as usize..range.end as usize) {
            candidates.push((
                CodeContextKind::Signature,
                value.trim().into(),
                parent,
                range,
            ));
        }
    }
    let relevant_text = format!(
        "{}\n{}",
        parent.text,
        ancestor.map(|p| p.text.as_str()).unwrap_or("")
    );
    let identifiers: std::collections::HashSet<_> = relevant_text
        .split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .filter(|s| !s.is_empty())
        .collect();
    for range in imports {
        let Some(value) = source.get(range.start as usize..range.end as usize) else {
            continue;
        };
        if !value
            .split(|c: char| !(c.is_alphanumeric() || c == '_'))
            .any(|name| {
                !matches!(
                    name,
                    "" | "import"
                        | "from"
                        | "use"
                        | "pub"
                        | "as"
                        | "crate"
                        | "super"
                        | "self"
                        | "type"
                        | "extern"
                ) && identifiers.contains(name)
            })
        {
            continue;
        }
        let donor = parents.iter().find(|p| {
            p.code.as_ref().is_some_and(|c| {
                c.role != CodeChunkRole::SourceSnapshot
                    && c.ranges
                        .iter()
                        .any(|r| r.start <= range.start && r.end >= range.end)
            })
        });
        if let Some(donor) = donor {
            // Binding resolution is not provided by a syntactic imports edge.
            candidates.push((
                CodeContextKind::UnresolvedImport,
                value.trim().into(),
                donor,
                *range,
            ));
        }
    }
    let mut selected = Vec::new();
    for (kind, value, donor, range) in candidates {
        if selected.len() == 20 {
            break;
        }
        let Some(source) = reference(donor, range, &representation.id) else {
            continue;
        };
        let fact = CodeContextFact {
            kind,
            value,
            sources: vec![source],
        };
        if engram_knowledge::validate_code_context_fact(&fact, parent, donor, &representation.id)
            .is_err()
        {
            continue;
        }
        selected.push(fact);
        if super::input::build_input(body, &selected, model, config, representation)?.is_none() {
            selected.pop();
        }
    }
    Ok(selected)
}
