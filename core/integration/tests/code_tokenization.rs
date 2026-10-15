#![cfg(feature = "fastembed")]
use engram_code::{TreeSitterChunker, segment_source_ranges, structural_boundaries};
use engram_domain::CodeByteRange;
use engram_store_sqlite::FastEmbedBgeSmallQueryProvider;

#[test]
#[ignore = "requires explicitly provisioned local BGE model assets"]
fn actual_bge_tokenizer_covers_method_tail_without_truncation() {
    let model = FastEmbedBgeSmallQueryProvider::new().unwrap();
    let source = format!(
        "fn enormous() {{\n{}\nreturn final_tail_anchor();\n}}\n",
        " let value = \"π 🦀 unicode body\";\n".repeat(220)
    );
    let full = model.tokenize_untruncated(&source, true).unwrap();
    assert_eq!(full.window, 512);
    assert_eq!(full.special_tokens, 2);
    assert!(
        full.total_tokens() > full.window as usize,
        "tokenization itself must not truncate"
    );
    let parser = TreeSitterChunker::new().unwrap();
    let tree = parser.parse(&source, "rs").unwrap();
    let plans = segment_source_ranges(
        &source,
        &[CodeByteRange {
            start: 0,
            end: source.len() as u64,
        }],
        "",
        &structural_boundaries(&tree),
        384,
        |text| Ok(model.tokenize_untruncated(text, false)?.offsets),
    )
    .unwrap();
    assert!(plans.iter().all(|p| p.ranges.len() <= 20));
    let bodies: Vec<String> = plans
        .iter()
        .map(|p| {
            p.ranges
                .iter()
                .map(|r| &source[r.start as usize..r.end as usize])
                .collect()
        })
        .collect();
    assert_eq!(bodies.concat(), source);
    assert!(bodies.last().unwrap().contains("final_tail_anchor"));
    let context = "path: src/example.rs\nsymbol: enormous\n";
    assert!(
        model
            .tokenize_untruncated(context, false)
            .unwrap()
            .total_tokens()
            <= 64
    );
    for (plan, body) in plans.iter().zip(&bodies) {
        let body_tokens = model.tokenize_untruncated(body, false).unwrap();
        assert_eq!(body_tokens.total_tokens(), plan.code_tokens as usize);
        assert!(plan.code_tokens <= 384);
        let combined = model
            .tokenize_untruncated(&format!("{context}\n{body}"), true)
            .unwrap();
        assert!(combined.total_tokens() <= combined.window as usize);
        assert_eq!(combined.tokenizer_id, full.tokenizer_id);
    }
    let vectors = model.embed_passage_batch(&bodies).unwrap();
    assert_eq!(vectors.len(), bodies.len());
    assert!(
        vectors
            .iter()
            .all(|v| v.len() == 384 && v.iter().all(|x| x.is_finite()))
    );
}

#[test]
#[ignore = "requires explicitly provisioned local BGE model assets"]
fn actual_bge_large_discontinuous_container_preserves_blank_gaps_without_empty_inference() {
    let model = FastEmbedBgeSmallQueryProvider::new().unwrap();
    let methods: String = (0..30)
        .map(|i| format!(" method_{i}() {{ return {i}; }}\n\n"))
        .collect();
    let source = format!(
        "export class Example {{\n// {}\n{methods}}}\n",
        "header explanatory comment token ".repeat(300)
    );
    let parser = TreeSitterChunker::new().unwrap();
    let tree = parser.parse(&source, "ts").unwrap();
    let parent = parser
        .covered_chunks_with_tree(&tree, &source, "ts")
        .unwrap()
        .into_iter()
        .find(|p| p.role == engram_domain::CodeChunkRole::ContainerContext)
        .unwrap();
    assert!(parent.ranges.len() > 20);
    assert!(
        parent
            .ranges
            .iter()
            .any(|r| source[r.start as usize..r.end as usize].trim().is_empty())
    );
    let plans = segment_source_ranges(
        &source,
        &parent.ranges,
        "\n",
        &structural_boundaries(&tree),
        384,
        |text| Ok(model.tokenize_untruncated(text, false)?.offsets),
    )
    .unwrap();
    assert!(plans.iter().all(|p| p.ranges.len() <= 20));
    let bodies: Vec<String> = plans
        .iter()
        .map(|p| {
            p.ranges
                .iter()
                .map(|r| &source[r.start as usize..r.end as usize])
                .collect::<Vec<_>>()
                .join("\n")
        })
        .collect();
    assert!(
        bodies.iter().all(|body| !body.trim().is_empty()),
        "blank gap must be covered by a neighboring segment"
    );
    let expected: Vec<u8> = parent
        .ranges
        .iter()
        .flat_map(|r| {
            source.as_bytes()[r.start as usize..r.end as usize]
                .iter()
                .copied()
        })
        .collect();
    let actual: Vec<u8> = plans
        .iter()
        .flat_map(|p| &p.ranges)
        .flat_map(|r| {
            source.as_bytes()[r.start as usize..r.end as usize]
                .iter()
                .copied()
        })
        .collect();
    assert_eq!(actual, expected);
    assert!(bodies.iter().all(|body| {
        model
            .tokenize_untruncated(body.trim(), true)
            .unwrap()
            .total_tokens()
            <= 386
    }));
    let vectors = model.embed_passage_batch(&bodies).unwrap();
    assert_eq!(vectors.len(), plans.len());
    assert!(
        vectors
            .iter()
            .all(|v| v.len() == 384 && v.iter().all(|n| n.is_finite()))
    );
}
