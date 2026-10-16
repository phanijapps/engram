use engram_code::{CoveredCodeCandidate, TreeSitterChunker};
use engram_domain::CodeChunkRole;

fn chunks(source: &str, ext: &str) -> Vec<CoveredCodeCandidate> {
    let parser = TreeSitterChunker::new().unwrap();
    let tree = parser.parse(source, ext).unwrap();
    parser.covered_chunks_with_tree(&tree, source, ext).unwrap()
}

#[test]
fn exact_callable_chunks_keep_attached_comments_and_declaration_identity() {
    for (ext, source, anchor, declaration) in [
        (
            "ts",
            "// Poll after an empty editor save.\n// Retry when the timestamp changes.\nexport async function retry() { return ''; }\n",
            "function retry",
            "async function",
        ),
        (
            "rs",
            "/// Persists the café record.\r\nfn save() {}\r\n",
            "fn save",
            "fn save",
        ),
        (
            "py",
            "# Validates the incoming payload.\ndef validate(payload):\n    pass\n",
            "def validate",
            "def validate",
        ),
        (
            "ts",
            "/** Waits for the file timestamp.\n * Then reads again. */\nfunction retry() {}\n",
            "function retry",
            "function retry",
        ),
    ] {
        let units = chunks(source, ext);
        let unit = units
            .iter()
            .find(|u| {
                u.candidate
                    .location
                    .as_ref()
                    .and_then(|l| l.anchor.as_deref())
                    == Some(anchor)
            })
            .unwrap();
        assert_eq!(unit.role, CodeChunkRole::Symbol);
        assert_eq!(
            unit.ranges[0].start, 0,
            "attached source documentation lost: {ext}"
        );
        assert_eq!(unit.candidate.text, source[..unit.ranges[0].end as usize]);
        assert_eq!(
            unit.declaration_range.unwrap().start,
            source.find(declaration).unwrap() as u64
        );
        assert!(
            !units
                .iter()
                .filter(|u| u.role == CodeChunkRole::Residual)
                .any(|u| u.candidate.text.contains("Poll after")
                    || u.candidate.text.contains("Persists")
                    || u.candidate.text.contains("Validates")
                    || u.candidate.text.contains("Waits"))
        );
    }
}

#[test]
fn detached_trailing_and_noncomment_text_never_attach_to_a_callable() {
    for source in [
        "// File-level notice.\n\nfunction retry() {}\n",
        "const x = 1; // Previous statement comment.\nfunction retry() {}\n",
        "const x = '// This is a string';\nfunction retry() {}\n",
    ] {
        let units = chunks(source, "ts");
        let unit = units
            .iter()
            .find(|u| u.role == CodeChunkRole::Symbol)
            .unwrap();
        assert_eq!(
            unit.ranges[0].start,
            source.find("function retry").unwrap() as u64
        );
    }
}

#[test]
fn method_documentation_belongs_to_the_method_without_its_sibling_body() {
    let source = "class Cache {\n  // Drops cached entries.\n  clear() {}\n  // Returns an entry.\n  get() {}\n}\n";
    let units = chunks(source, "ts");
    let clear = units
        .iter()
        .find(|u| {
            u.candidate
                .location
                .as_ref()
                .and_then(|l| l.anchor.as_deref())
                == Some("fn Cache::clear")
        })
        .unwrap();
    assert!(clear.candidate.text.contains("Drops cached entries."));
    assert!(!clear.candidate.text.contains("Returns an entry."));
    let container = units
        .iter()
        .find(|u| u.role == CodeChunkRole::ContainerContext)
        .unwrap();
    assert!(!container.candidate.text.contains("Drops cached entries."));
    assert!(!container.candidate.text.contains("Returns an entry."));
}

#[test]
fn attachments_are_bounded_by_whole_comment_nodes_and_bytes() {
    let source = format!("/*{}*/\nfunction retry() {{}}\n", "x".repeat(8192));
    let units = chunks(&source, "ts");
    let unit = units
        .iter()
        .find(|u| u.role == CodeChunkRole::Symbol)
        .unwrap();
    assert_eq!(
        unit.ranges[0].start,
        source.find("function retry").unwrap() as u64
    );
    let source =
        "// first\n// second\n// third\n// fourth\n// fifth\n// sixth\nfunction retry() {}\n";
    let units = chunks(source, "ts");
    let unit = units
        .iter()
        .find(|u| u.role == CodeChunkRole::Symbol)
        .unwrap();
    assert_eq!(
        unit.ranges[0].start,
        source.find("// second").unwrap() as u64
    );
    assert!(
        units
            .iter()
            .filter(|u| u.role == CodeChunkRole::Residual)
            .any(|u| u.candidate.text.contains("// first"))
    );
}
