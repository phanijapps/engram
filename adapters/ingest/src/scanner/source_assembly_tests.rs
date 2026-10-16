use super::*;
use serde_json::json;

#[test]
fn unchanged_code_with_new_source_or_document_has_distinct_immutable_chunks() {
    let request: DocumentIngestRequest = serde_json::from_value(json!({
        "sourceKind":"filesystem","sourceName":"repo [main@old]",
        "scope":{"tenant":"t","workspace":"w"},"documentKind":"code",
        "document":{"path":"a.ts","language":"typescript","version":"old"},
        "text":"export function f() { return 1; }\n",
        "policy":{"visibility":"workspace","retention":"durable","allowedUses":["retrieval"]},
        "actor":{"id":"owner","kind":"agent"},"stableSourceKey":"github.com/acme/repo"
    }))
    .unwrap();
    let manifest: CodeSourceGeneration = serde_json::from_value(json!({
        "file":{"scope":request.scope,"repository":"github.com/acme/repo","path":"a.ts"},
        "generation":"same-file-content-generation","attemptId":"attempt", "state":"updating",
        "sourceHash":content_hash(&request.text),"fingerprint":FINGERPRINT,
        "byteLength":request.text.len(),"documentId":null,"snapshotIds":[],"records":[],
        "policy":request.policy,"provenance":{"source":"scan","actor":request.actor,
          "observedAt":"2026-10-15T00:00:00Z","evidence":[],"derivations":[]}
    }))
    .unwrap();
    let units = vec![CoveredCodeCandidate {
        candidate: ChunkCandidate {
            kind: KnowledgeChunkKind::CodeBlock,
            text: request.text.clone(),
            location: Some(SourceLocation {
                anchor: Some("function f".into()),
                path: None,
                start_line: None,
                end_line: None,
                start_offset: None,
                end_offset: None,
            }),
        },
        role: CodeChunkRole::Symbol,
        ranges: vec![CodeByteRange {
            start: 0,
            end: request.text.len() as u64,
        }],
        declaration_range: Some(CodeByteRange {
            start: 0,
            end: request.text.len() as u64,
        }),
        parse_quality: CodeParseQuality::Clean,
        complete: true,
        unsupported_features: vec![],
    }];
    let make = |request| {
        prepare(
            request,
            &mut manifest.clone(),
            units.clone(),
            CodeParseQuality::Clean,
        )
        .unwrap()
    };
    let first = make(request.clone());
    let repeat = make(request.clone());
    assert_eq!(
        first.chunks.iter().map(|c| &c.id).collect::<Vec<_>>(),
        repeat.chunks.iter().map(|c| &c.id).collect::<Vec<_>>()
    );
    for change_source in [false, true] {
        let mut next = request.clone();
        next.document.version = Some("new".into());
        if change_source {
            next.source_name = "repo [main@new]".into();
        }
        let second = make(next);
        assert_ne!(first.document.id, second.document.id);
        if change_source {
            assert_ne!(first.source.id, second.source.id);
        } else {
            assert_eq!(first.source.id, second.source.id);
        }
        for (old, new) in first.chunks.iter().zip(second.chunks.iter()) {
            assert_eq!(old.text, new.text);
            assert_eq!(
                old.code.as_ref().unwrap().generation,
                new.code.as_ref().unwrap().generation
            );
            assert_eq!(
                old.code.as_ref().unwrap().unit,
                new.code.as_ref().unwrap().unit
            );
            assert_ne!(
                old.id, new.id,
                "new immutable ownership must not reuse old canonical chunk ID"
            );
        }
        assert!(
            second
                .chunks
                .iter()
                .any(|c| c.code.as_ref().unwrap().unit.is_some())
        );
    }
}
