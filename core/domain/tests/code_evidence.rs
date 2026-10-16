// STUB: AC1, AC7, AC8 — additive portable code-evidence contracts.
use engram_domain::{CodeEvidencePacket, CodeTokenCoverage};
use engram_domain::{CodeEvidenceQuery, CodeEvidenceSourceRef};
use serde_json::json;

#[test]
fn query_admission_is_byte_bounded_and_rejects_authority_fields() {
    let q: CodeEvidenceQuery =
        serde_json::from_value(json!({"query":"find the refund guard","resultLimit":10})).unwrap();
    assert!(q.validate().is_ok());
    for invalid in [
        json!({"query":""}),
        json!({"query":" "}),
        json!({"query":"q","scope":{"tenant":"other"}}),
        json!({"query":"q","requester":{"actor":{"id":"other","kind":"agent"}}}),
        json!({"query":"q","modelId":"external"}),
        json!({"query":"q","resultLimit":0}),
        json!({"query":"q","repository":"r".repeat(2049)}),
        json!({"query":"é".repeat(8193)}),
    ] {
        assert!(serde_json::from_value::<CodeEvidenceQuery>(invalid).is_err());
    }
    // Embedded callers also pass admission before any tokenizer/model input.
    let mut oversized = q;
    oversized.query = "q".repeat(16385);
    assert!(oversized.validate().is_err());
}

#[test]
fn source_reference_preserves_opaque_generation_hash_and_unit_identity() {
    let reference = json!({"repository":"repo","path":"a.ts","unitId":"unit-hash",
        "generation":"legacy-valid-generation","sourceHash":"opaque-source-hash",
        "representationId":"segment-v1","startByte":0,"endByte":5,"indexedRevision":null});
    let r: CodeEvidenceSourceRef = serde_json::from_value(reference.clone()).unwrap();
    assert_eq!(r.generation, "legacy-valid-generation");
    assert_eq!(serde_json::to_value(r).unwrap(), reference);
    for invalid in [
        json!({"generation":1}),
        json!({"generation":""}),
        json!({"startByte":6}),
    ] {
        let mut candidate = reference.clone();
        candidate
            .as_object_mut()
            .unwrap()
            .extend(invalid.as_object().unwrap().clone());
        assert!(serde_json::from_value::<CodeEvidenceSourceRef>(candidate).is_err());
    }
}

#[test]
fn structured_packet_example_preserves_exact_excerpt_mapping_and_actual_coverage() {
    let packet: CodeEvidencePacket = serde_json::from_str(include_str!(
        "../../../docs/specs/reliable-code-intelligence/examples/packet-scored.json"
    ))
    .unwrap();
    let source =
        include_str!("../../../docs/specs/reliable-code-intelligence/examples/source-fixture.txt");
    for item in &packet.items {
        for part in &item.excerpts {
            assert_eq!(
                &source.as_bytes()[part.source.start_byte as usize..part.source.end_byte as usize],
                part.text.as_bytes()
            );
        }
        for segment in &item.segments {
            assert!(segment.embedding_input.is_valid());
            assert!(segment.rerank_input.as_ref().unwrap().is_valid());
        }
    }
    let encoded = serde_json::to_value(&packet).unwrap();
    let roundtrip: CodeEvidencePacket = serde_json::from_value(encoded).unwrap();
    assert_eq!(packet, roundtrip);
}

#[test]
fn token_coverage_cannot_hide_truncation_or_invent_totals() {
    let mut coverage = CodeTokenCoverage {
        code_tokens: 384,
        context_tokens: 64,
        query_tokens: 0,
        special_tokens: 2,
        total_tokens: 450,
        truncated: false,
    };
    assert!(coverage.is_valid());
    coverage.total_tokens = 449;
    assert!(!coverage.is_valid());
    coverage.total_tokens = 450;
    coverage.truncated = true;
    assert!(!coverage.is_valid());
}

#[test]
fn packet_rejects_support_units_as_primary_and_generic_rerankers() {
    let packet: serde_json::Value = serde_json::from_str(include_str!(
        "../../../contracts/v1/examples/code-evidence-packet.json"
    ))
    .unwrap();
    for (key, value) in [
        ("kind", "document"),
        ("sourceCategory", "test"),
        ("sourceCategory", "generated"),
        ("sourceCategory", "unknown"),
    ] {
        let mut invalid = packet.clone();
        invalid["items"][0][key] = json!(value);
        assert!(serde_json::from_value::<CodeEvidencePacket>(invalid).is_err());
    }
    for strategy in ["mmr", "llm_judge", "policy_priority"] {
        let mut invalid = packet.clone();
        invalid["diagnostics"]["effectiveReranker"] = json!(strategy);
        assert!(serde_json::from_value::<CodeEvidencePacket>(invalid).is_err());
    }
}
