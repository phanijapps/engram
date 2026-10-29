//! TASK-014 focused proof: targeted warm Tantivy retirement with durable
//! receipts, proven on a REAL file-backed index that is closed and REOPENED
//! (the warm-index hazard): a retired generation's tokens can never return,
//! while mixed-lineage targets keep their surviving non-code contributions.

use engram_store_lexical::{LexicalIndex, code_contributor_key};
use tempfile::TempDir;

fn reopen(dir: &std::path::Path) -> LexicalIndex {
    LexicalIndex::open(dir).expect("reopen warm lexical index")
}

mod task_014 {
    use super::*;

    /// warm_generation_retirement: retiring `code:<gen>` deletes exactly that
    /// generation's documents from the persistent warm index; after two
    /// reopen cycles the retired tokens are unreachable, and the ordinary
    /// documents survive untouched.
    #[test]
    fn warm_generation_retirement() {
        let dir = TempDir::new().expect("tempdir");
        let lexical_dir = dir.path().join("lexical");
        std::fs::create_dir_all(&lexical_dir).expect("mkdir");

        // Process 1: feed code documents under gen-1 plus ordinary docs.
        let index = reopen(&lexical_dir);
        index
            .upsert_stamped(
                "chunk-code-1",
                "retireme alpha alpha",
                &code_contributor_key("gen-1"),
            )
            .expect("feed code 1");
        index
            .upsert_stamped(
                "chunk-code-2",
                "retireme beta",
                &code_contributor_key("gen-1"),
            )
            .expect("feed code 2");
        index
            .upsert_stamped("chunk-doc", "ordinary keepme", "doc")
            .expect("feed doc");
        drop(index);

        // Process 2 (warm reopen): the retired generation is still visible —
        // this is exactly the stale warm-index hazard retirement must fix.
        let warm = reopen(&lexical_dir);
        assert!(
            warm.search("retireme", 10)
                .unwrap()
                .iter()
                .any(|(id, _)| id.starts_with("chunk-code")),
            "warm index must initially serve the generation being retired"
        );
        let retired = warm
            .retire_contributor(&code_contributor_key("gen-1"))
            .expect("retire");
        assert_eq!(
            retired, 2,
            "the durable receipt must count the retired generation's documents"
        );
        // Idempotent: retiring an already-retired generation removes nothing.
        let again = warm
            .retire_contributor(&code_contributor_key("gen-1"))
            .expect("retire again");
        assert_eq!(again, 0, "retirement must be idempotent");
        drop(warm);

        // Process 3 (second reopen): retired tokens can NEVER return from the
        // warm index; ordinary documents are untouched.
        let after = reopen(&lexical_dir);
        let retired_hits = after.search("retireme", 10).unwrap();
        assert!(
            retired_hits.is_empty(),
            "deleted tokens must not return from a reopened warm index: {retired_hits:?}"
        );
        let kept = after.search("keepme", 10).unwrap();
        assert!(
            kept.iter().any(|(id, _)| id == "chunk-doc"),
            "ordinary documents survive a generation retirement: {kept:?}"
        );
    }

    /// mixed_lineage_recompute: a target with BOTH a bound-code contribution
    /// and an ordinary document contribution loses exactly the retired code
    /// text; the surviving non-code contribution keeps the target searchable —
    /// never dropped wholesale, never serving the stale code text.
    #[test]
    fn mixed_lineage_recompute() {
        let dir = TempDir::new().expect("tempdir");
        let lexical_dir = dir.path().join("lexical");
        std::fs::create_dir_all(&lexical_dir).expect("mkdir");

        let index = reopen(&lexical_dir);
        // One target, two contributor documents: the bound-code text (with
        // its own unique token) and the ordinary document text.
        index
            .upsert_stamped(
                "chunk-mixed",
                "mixedcodeonlytoken parse",
                &code_contributor_key("gen-1"),
            )
            .expect("feed mixed code contribution");
        index
            .upsert_stamped("chunk-mixed", "mixeddoctoken parse", "doc")
            .expect("feed mixed doc contribution");
        index
            .upsert_stamped(
                "chunk-codeonly",
                "codeonlytoken parse",
                &code_contributor_key("gen-1"),
            )
            .expect("feed code-only target");
        drop(index);

        // Before retirement both contributions of the mixed target serve.
        let warm = reopen(&lexical_dir);
        assert!(
            warm.search("mixedcodeonlytoken", 10)
                .unwrap()
                .iter()
                .any(|(id, _)| id == "chunk-mixed")
        );
        assert!(
            warm.search("mixeddoctoken", 10)
                .unwrap()
                .iter()
                .any(|(id, _)| id == "chunk-mixed")
        );

        // Retire the code generation: exactly one mixed-target document (the
        // code contribution) plus the code-only target are removed.
        let retired = warm
            .retire_contributor(&code_contributor_key("gen-1"))
            .expect("retire");
        assert_eq!(
            retired, 2,
            "receipt counts the mixed code doc + code-only doc"
        );
        drop(warm);

        let after = reopen(&lexical_dir);
        // The stale code text is gone — for BOTH targets.
        assert!(
            after.search("mixedcodeonlytoken", 10).unwrap().is_empty(),
            "the retired code text must never be served for a mixed target"
        );
        assert!(
            after.search("codeonlytoken", 10).unwrap().is_empty(),
            "a code-only target disappears with its generation"
        );
        // The surviving non-code contribution keeps the mixed target live.
        let surviving = after.search("mixeddoctoken", 10).unwrap();
        assert!(
            surviving.iter().any(|(id, _)| id == "chunk-mixed"),
            "the mixed target's surviving non-code input must remain searchable: {surviving:?}"
        );
        // And the shared token resolves the mixed target exactly once.
        let shared = after.search("parse", 10).unwrap();
        let mixed_hits = shared.iter().filter(|(id, _)| id == "chunk-mixed").count();
        assert_eq!(
            mixed_hits, 1,
            "one target, one hit — contributor docs dedupe"
        );
    }
}
