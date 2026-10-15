//! Construction tests for tokenizer-bounded structural derivatives.
use engram_code::{TreeSitterChunker, segment_source_ranges, structural_boundaries};
use engram_domain::CodeByteRange;
use engram_runtime::CoreResult;

fn scalar_tokens(text: &str) -> CoreResult<Vec<CodeByteRange>> {
    Ok(text
        .char_indices()
        .map(|(offset, ch)| CodeByteRange {
            start: offset as u64,
            end: (offset + ch.len_utf8()) as u64,
        })
        .collect())
}

#[test]
fn unicode_terminals_have_complete_byte_cover_and_marked_continuations() {
    let source = format!("const giant = '{}';\n", "🦀π".repeat(80));
    let full = CodeByteRange {
        start: 0,
        end: source.len() as u64,
    };
    let plans = segment_source_ranges(&source, &[full], "", &[], 17, scalar_tokens).unwrap();
    assert!(plans.len() > 1);
    assert!(plans.iter().all(|p| p.continuation && p.code_tokens <= 17));
    let reconstructed: String = plans
        .iter()
        .flat_map(|p| &p.ranges)
        .map(|r| &source[r.start as usize..r.end as usize])
        .collect();
    assert_eq!(reconstructed, source);
    assert!(segment_source_ranges(&source, &[full], "", &[], 0, scalar_tokens).is_err());
}

#[test]
fn small_discontinuous_shell_stays_one_parent_without_claiming_separator_bytes() {
    let source = "class Example {\n  omitted_method();\n}\n";
    let ranges = [
        CodeByteRange { start: 0, end: 15 },
        CodeByteRange {
            start: (source.len() - 2) as u64,
            end: source.len() as u64,
        },
    ];
    let plans = segment_source_ranges(source, &ranges, "\n", &[], 30, scalar_tokens).unwrap();
    assert_eq!(plans.len(), 1);
    assert_eq!(plans[0].ranges, ranges);
    assert!(!plans[0].continuation);
    assert_eq!(plans[0].code_tokens, 18);
}

#[test]
fn rust_typescript_and_python_prefer_structural_boundaries() {
    let parser = TreeSitterChunker::new().unwrap();
    for (extension, source) in [
        (
            "rs",
            "fn sample() {\n let first = 1;\n let second = 2;\n let third = 3;\n}\n",
        ),
        (
            "ts",
            "function sample() {\n const first = 1;\n const second = 2;\n const third = 3;\n}\n",
        ),
        (
            "py",
            "def sample():\n    first = 1\n    second = 2\n    third = 3\n",
        ),
    ] {
        let tree = parser.parse(source, extension).unwrap();
        let boundaries = structural_boundaries(&tree);
        let full = CodeByteRange {
            start: 0,
            end: source.len() as u64,
        };
        let plans =
            segment_source_ranges(source, &[full], "", &boundaries, 38, scalar_tokens).unwrap();
        assert!(plans.len() > 1, "{extension}");
        assert!(plans.iter().all(|p| p.code_tokens <= 38));
        assert!(boundaries.contains(&plans[0].ranges[0].end), "{extension}");
        assert!(!plans[0].continuation, "{extension}");
        let reconstructed: String = plans
            .iter()
            .flat_map(|p| &p.ranges)
            .map(|r| &source[r.start as usize..r.end as usize])
            .collect();
        assert_eq!(reconstructed, source);
    }
}

#[test]
fn invalid_ranges_and_unsplittable_token_offsets_are_explicit_failures() {
    let range = CodeByteRange { start: 0, end: 4 };
    assert!(
        segment_source_ranges(
            "🦀",
            &[CodeByteRange { start: 1, end: 4 }],
            "",
            &[],
            1,
            scalar_tokens
        )
        .is_err()
    );
    let indivisible = |_: &str| Ok(vec![range; 4]);
    assert!(segment_source_ranges("🦀", &[range], "", &[], 1, indivisible).is_err());
}

#[test]
fn many_method_container_honors_range_bound_even_below_token_budget() {
    let methods: String = (0..30)
        .map(|i| format!(" m{i}() {{ return {i}; }}\n\n"))
        .collect();
    let source = format!("export class Example {{\n{methods}}}\n");
    let parser = TreeSitterChunker::new().unwrap();
    let tree = parser.parse(&source, "ts").unwrap();
    let parent = parser
        .covered_chunks_with_tree(&tree, &source, "ts")
        .unwrap()
        .into_iter()
        .find(|p| p.role == engram_domain::CodeChunkRole::ContainerContext)
        .unwrap();
    assert!(parent.ranges.len() > 20);
    let plans = segment_source_ranges(
        &source,
        &parent.ranges,
        "\n",
        &structural_boundaries(&tree),
        384,
        |text| {
            let prefix = text.len() - text.trim_start().len();
            Ok(scalar_tokens(text.trim())?
                .into_iter()
                .map(|r| CodeByteRange {
                    start: r.start + prefix as u64,
                    end: r.end + prefix as u64,
                })
                .collect())
        },
    )
    .unwrap();
    assert!(plans.len() > 1);
    assert!(
        plans
            .iter()
            .all(|p| p.ranges.len() <= 20 && p.code_tokens <= 384)
    );
    let bytes: Vec<u8> = plans
        .iter()
        .flat_map(|p| &p.ranges)
        .flat_map(|r| {
            source.as_bytes()[r.start as usize..r.end as usize]
                .iter()
                .copied()
        })
        .collect();
    let expected: Vec<u8> = parent
        .ranges
        .iter()
        .flat_map(|r| {
            source.as_bytes()[r.start as usize..r.end as usize]
                .iter()
                .copied()
        })
        .collect();
    assert_eq!(bytes, expected);
}
