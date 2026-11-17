use engram_embedding_fastembed::{FastEmbedBgeSmallQueryProvider, FastEmbedModelSelection};

#[test]
fn catalogue_identity_and_native_dimensions_need_no_model_assets() {
    for (name, model_id, dimensions) in [
        ("BGESmallENV15", "BAAI/bge-small-en-v1.5", 384),
        ("BAAI/bge-base-en-v1.5", "BAAI/bge-base-en-v1.5", 768),
        ("BAAI/bge-m3", "BAAI/bge-m3", 1024),
        ("JinaEmbeddingsV2BaseCode", "JinaEmbeddingsV2BaseCode", 768),
    ] {
        let selection = FastEmbedModelSelection::resolve(name, 0).unwrap();
        assert_eq!(selection.model_id, model_id);
        assert_eq!(selection.dimensions, dimensions);
    }
}

#[test]
fn invalid_dimensions_and_forged_identity_fail_before_initialization() {
    assert!(FastEmbedModelSelection::resolve("missing", 384).is_err());
    assert!(FastEmbedModelSelection::resolve("BGESmallENV15", 768).is_err());
    let mut selected = FastEmbedModelSelection::resolve("BGESmallENV15", 384).unwrap();
    selected.model_id = "forged".into();
    assert!(FastEmbedBgeSmallQueryProvider::from_selection(selected, None, None).is_err());
}
