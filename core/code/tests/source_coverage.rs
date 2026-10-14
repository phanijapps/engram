use engram_code::{TreeSitterChunker, snapshot_ranges, source_residual_ranges};
use engram_domain::{CodeByteRange, CodeChunkRole, CodeParseQuality};

#[test]
fn snapshots_preserve_utf8_crlf_bom_and_empty_boundaries() {
    for text in ["", " \r\n\t", "\u{feff}π🦀\r\nimport x\n", "const x = 1;\n"] {
        let ranges = snapshot_ranges(text, 4).unwrap();
        let reconstructed: String = ranges
            .iter()
            .map(|r| &text[r.start as usize..r.end as usize])
            .collect();
        assert_eq!(reconstructed.as_bytes(), text.as_bytes());
        assert!(!ranges.is_empty());
        assert!(ranges.iter().all(|r| r.end - r.start <= 4));
    }
}
#[test]
fn residual_ranges_cover_imports_initializers_and_unmapped_syntax() {
    let ranges = source_residual_ranges(
        12,
        &[
            CodeByteRange { start: 4, end: 7 },
            CodeByteRange { start: 5, end: 8 },
        ],
    )
    .unwrap();
    assert_eq!(
        ranges,
        vec![
            CodeByteRange { start: 0, end: 4 },
            CodeByteRange { start: 8, end: 12 }
        ]
    );
    assert!(source_residual_ranges(3, &[CodeByteRange { start: 4, end: 5 }]).is_err());
}
#[test]
fn container_context_has_multiline_header_and_fields_without_method_duplication() {
    let source = "import { Base } from './base';\r\nexport class UserService\n  extends Base {\n  private fee = 42;\n  async getUser(\n    id: string\n  ) { return id; }\n  private late = 'field-after-method';\n}\nbootstrap();\n";
    let parser = TreeSitterChunker::new().unwrap();
    let tree = parser.parse(source, "ts").unwrap();
    let units = parser
        .covered_chunks_with_tree(&tree, source, "ts")
        .unwrap();
    let class = units
        .iter()
        .find(|u| u.role == CodeChunkRole::ContainerContext)
        .unwrap();
    assert!(class.candidate.text.contains("extends Base"));
    assert!(class.candidate.text.contains("private fee = 42"));
    assert!(class.candidate.text.contains("field-after-method"));
    assert!(!class.candidate.text.contains("return id"));
    assert!(class.ranges.len() > 1);
    let method = units
        .iter()
        .find(|u| u.role == CodeChunkRole::Symbol)
        .unwrap();
    assert!(
        method
            .candidate
            .text
            .contains("id: string\n  ) { return id; }")
    );
    let residual_text: String = units
        .iter()
        .filter(|u| u.role == CodeChunkRole::Residual)
        .map(|u| u.candidate.text.as_str())
        .collect();
    assert!(residual_text.contains("import { Base }"));
    assert!(residual_text.contains("bootstrap()"));
}
#[test]
fn recovered_ast_is_not_reported_clean() {
    let parser = TreeSitterChunker::new().unwrap();
    let text = "function broken() { if (";
    let tree = parser.parse(text, "ts").unwrap();
    assert!(tree.root_node().has_error());
    let units = parser.covered_chunks_with_tree(&tree, text, "ts").unwrap();
    assert!(
        units
            .iter()
            .all(|u| u.parse_quality == CodeParseQuality::Recovered)
    );
}

#[test]
fn unsupported_and_failed_parse_attempts_have_distinct_quality() {
    use engram_code::source_parse_outcome;
    let (tree, quality) = source_parse_outcome(None);
    assert!(tree.is_none());
    assert_eq!(quality, CodeParseQuality::Fallback);
    let (tree, quality) =
        source_parse_outcome(Some(Err(engram_runtime::CoreError::ProviderUnavailable {
            provider: "test-parser".into(),
        })));
    assert!(tree.is_none());
    assert_eq!(quality, CodeParseQuality::Failed);
    let parser = TreeSitterChunker::new().unwrap();
    for (text, expected) in [
        ("function a() {}", CodeParseQuality::Clean),
        ("function a() { if (", CodeParseQuality::Recovered),
    ] {
        let (tree, quality) = source_parse_outcome(Some(parser.parse(text, "ts")));
        assert!(tree.is_some());
        assert_eq!(quality, expected);
    }
}
