//! Grounded same-file context and a stable rendering for local model inputs.
use engram_domain::*;
use engram_runtime::{CoreError, CoreResult};

pub fn code_context_text(facts: &[CodeContextFact]) -> String {
    facts
        .iter()
        .map(|fact| {
            let name = match fact.kind {
                CodeContextKind::Path => "path",
                CodeContextKind::Symbol => "symbol",
                CodeContextKind::Signature => "signature",
                CodeContextKind::Parent => "parent",
                CodeContextKind::Import => "import",
                CodeContextKind::UnresolvedImport => "unresolved import",
            };
            format!("{name}: {}", fact.value)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Validates fact content against its actual canonical contributor. Storage must
/// separately verify current inventory membership. Same-file policy/owner equality
/// prevents an independently restricted contributor broadening a parent's feed.
pub fn validate_code_context_fact(
    fact: &CodeContextFact,
    parent: &KnowledgeChunk,
    donor: &KnowledgeChunk,
    representation: &str,
) -> CoreResult<()> {
    let invalid = || CoreError::ValidationFailed {
        reason: "code context is not grounded in a compatible canonical contributor".into(),
    };
    if fact.sources.len() != 1
        || fact.value.chars().count() > 8192
        || donor.policy != parent.policy
        || donor.provenance.actor != parent.provenance.actor
    {
        return Err(invalid());
    }
    let source = &fact.sources[0];
    let code = donor.code.as_ref().ok_or_else(invalid)?;
    let owner = parent.code.as_ref().ok_or_else(invalid)?;
    if code.role == CodeChunkRole::SourceSnapshot
        || code.file != owner.file
        || code.generation != owner.generation
        || code.source_hash != owner.source_hash
        || code.fingerprint != owner.fingerprint
        || code.unit.as_deref() != Some(&source.unit_id)
        || source.repository != code.file.repository
        || source.path != code.file.path
        || source.generation != code.generation
        || source.source_hash != code.source_hash
        || source.representation_id != representation
        || source.start_byte >= source.end_byte
    {
        return Err(invalid());
    }
    let range = CodeByteRange {
        start: source.start_byte,
        end: source.end_byte,
    };
    let text = crate::code_segment_text(donor, &[range])?;
    let anchor = donor.location.as_ref().and_then(|l| l.anchor.as_deref());
    let grounded = match fact.kind {
        CodeContextKind::Path => donor.id == parent.id && fact.value == code.file.path,
        CodeContextKind::Symbol => donor.id == parent.id && anchor == Some(fact.value.as_str()),
        CodeContextKind::Parent => {
            owner.parent_unit.as_deref() == code.unit.as_deref()
                && anchor == Some(fact.value.as_str())
        }
        CodeContextKind::Signature => donor.id == parent.id && text.trim() == fact.value,
        CodeContextKind::UnresolvedImport => {
            text.trim() == fact.value
                && ["import ", "from ", "use ", "pub use ", "extern crate "]
                    .iter()
                    .any(|prefix| text.trim_start().starts_with(prefix))
        }
        // Syntax-only imports cannot claim independently resolved bindings.
        CodeContextKind::Import => false,
    };
    if !grounded {
        return Err(invalid());
    }
    Ok(())
}
