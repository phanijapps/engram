#![cfg(feature = "local-onnx")]
use engram_domain::CodeIntelligenceProfile;
use engram_rerank_cross_encoder::{LocalOnnxScorer, RerankScorer};
use engram_retrieval::CodeIntelligenceConfig;

#[test]
fn missing_or_corrupt_admitted_cache_never_falls_back_to_asset_download() {
    let root = tempfile::tempdir().unwrap();
    let config =
        CodeIntelligenceConfig::for_profile(CodeIntelligenceProfile::BalancedV1).local_scorer;
    assert!(LocalOnnxScorer::open(root.path(), &config).is_err());
    let (manifest, _) = engram_local_model_assets::AdmittedModel::JinaRerankerV1TurboEn
        .manifest()
        .unwrap();
    let directory = root
        .path()
        .join("jina-reranker-v1-turbo-en")
        .join(manifest.revision)
        .join("onnx");
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(directory.join("model.onnx"), b"corrupt").unwrap();
    assert!(LocalOnnxScorer::open(root.path(), &config).is_err());
}

#[test]
#[ignore = "requires explicitly provisioned pinned ONNX assets; actual CPU inference"]
fn actual_jina_pairs_change_code_ranking_and_preserve_batch_alignment_beyond_512_tokens() {
    let config =
        CodeIntelligenceConfig::for_profile(CodeIntelligenceProfile::BalancedV1).local_scorer;
    let scorer = LocalOnnxScorer::open(
        &engram_local_model_assets::default_cache_root().unwrap(),
        &config,
    )
    .unwrap();
    let query = "delete a user account by its identifier";
    let documents=vec![
        "path: user.service.ts\nsymbol: UserService.getUser\n\nasync getUser(id: string) { return db.query('SELECT * FROM users WHERE id = ?', id); }".to_owned(),
        "path: greeting.ts\n\nfunction greet() { return 'hello world'; }".to_owned(),
        "path: user.service.ts\nsymbol: UserService.deleteUser\n\nasync deleteUser(id: string) { return db.query('DELETE FROM users WHERE id = ?', id); }".to_owned(),
    ];
    let scores = scorer.score_batch(query, &documents, 2).unwrap();
    assert!(
        scores[2] > scores[0] && scores[2] > scores[1],
        "real scorer must promote the deletion implementation: {scores:?}"
    );
    let reversed: Vec<_> = documents.iter().rev().cloned().collect();
    let reverse_scores = scorer.score_batch(query, &reversed, 1).unwrap();
    for (a, b) in scores.iter().zip(reverse_scores.iter().rev()) {
        assert!(
            (a - b).abs() < 0.001,
            "batch padding/order changed pair scores: {scores:?} {reverse_scores:?}"
        );
    }
    let long = format!(
        "{}\nDeletes a user account by the supplied identifier.",
        "value ".repeat(600)
    );
    let coverage = scorer.inspect_pair(query, &long, 0).unwrap();
    assert!(coverage.code_tokens > 512 && coverage.total_tokens <= 1024);
    assert_eq!(coverage.special_tokens, 4);
    assert!(!coverage.truncated);
    let long_scores = scorer.score_batch(query, &[long], 1).unwrap();
    assert!(long_scores[0].is_finite());
    assert!(
        scorer
            .score_batch(&"word ".repeat(200), &documents, 2)
            .is_err()
    );
    assert!(
        scorer
            .score_batch(query, &["word ".repeat(1100)], 1)
            .is_err()
    );
    assert!(scorer.inspect_pair(query, "π text", 1).is_err());
    let context = "path: user.service.ts\n\n";
    let counted = scorer
        .inspect_pair(
            query,
            &format!("{context}async deleteUser() {{}}"),
            context.len(),
        )
        .unwrap();
    assert!(counted.context_tokens > 0 && counted.code_tokens > 0 && counted.query_tokens > 0);
    println!(
        "ACTUAL_LOCAL_SCORER={}",
        serde_json::json!({"identity":scorer.identity(),"scores":scores,"reverseScores":reverse_scores,"longInput":coverage,"longScores":long_scores,"promotedOriginalIndex":2})
    );
}
