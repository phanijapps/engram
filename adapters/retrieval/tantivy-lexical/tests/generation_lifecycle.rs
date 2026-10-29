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

    /// Corrective r4 (canonical 5): canonical target eligibility AND
    /// authorized contributor membership are enforced together on the REAL
    /// persistent Tantivy index, with DISTINCT-TARGET collection. A shared
    /// allowed target carrying EXCLUDED-generation-only tokens cannot match
    /// through them, and a flood of duplicate contributor documents for one
    /// allowed target cannot starve another allowed target of the bounded
    /// page (the budget counts TARGETS, not contribution documents).
    #[test]
    fn distinct_targets_and_contributor_isolation() {
        use std::collections::BTreeSet;
        let dir = TempDir::new().expect("tempdir");
        let lexical_dir = dir.path().join("lexical");
        std::fs::create_dir_all(&lexical_dir).expect("mkdir");
        let index = reopen(&lexical_dir);

        let current = code_contributor_key("gen-current");
        let excluded = code_contributor_key("gen-excluded");
        // shared-target: ALLOWED by target id, but its only documents for the
        // matching token came from the EXCLUDED generation.
        index
            .upsert_batch_stamped(&[
                (
                    "shared-target".to_string(),
                    "render excluded render excluded".to_string(),
                    excluded.clone(),
                ),
                // crowd-target: ALLOWED, with MANY duplicate contributor
                // documents (well past any small fetch bound) that all match.
                (
                    "crowd-target".to_string(),
                    "render crowd high rank".to_string(),
                    current.clone(),
                ),
                (
                    "crowd-target".to_string(),
                    "render crowd more one".to_string(),
                    current.clone(),
                ),
                (
                    "crowd-target".to_string(),
                    "render crowd more two".to_string(),
                    current.clone(),
                ),
                (
                    "crowd-target".to_string(),
                    "render crowd more three".to_string(),
                    current.clone(),
                ),
                (
                    "crowd-target".to_string(),
                    "render crowd more four".to_string(),
                    current.clone(),
                ),
                (
                    "crowd-target".to_string(),
                    "render crowd more five".to_string(),
                    current.clone(),
                ),
                (
                    "crowd-target".to_string(),
                    "render crowd more six".to_string(),
                    current.clone(),
                ),
                (
                    "crowd-target".to_string(),
                    "render crowd more seven".to_string(),
                    current.clone(),
                ),
                (
                    "crowd-target".to_string(),
                    "render crowd more eight".to_string(),
                    current.clone(),
                ),
                (
                    "crowd-target".to_string(),
                    "render crowd more nine".to_string(),
                    current.clone(),
                ),
                (
                    "crowd-target".to_string(),
                    "render crowd more ten".to_string(),
                    current.clone(),
                ),
                // valid-target: lower-ranked but eligible on its own.
                (
                    "valid-target".to_string(),
                    "render valid lower".to_string(),
                    current.clone(),
                ),
            ])
            .expect("seed stamped docs");

        let targets: BTreeSet<String> = ["shared-target", "crowd-target", "valid-target"]
            .iter()
            .map(|id| id.to_string())
            .collect();
        let contributors: BTreeSet<String> = [current.clone()]
            .iter()
            .cloned()
            .chain(std::iter::once("doc".to_string()))
            .collect();

        // Contributor isolation: shared-target's matching documents are all
        // excluded-contributor rows — it must not match AT ALL.
        let page = index
            .search_targets_within("render", 2, &targets, &contributors)
            .expect("distinct-target search");
        let ids: Vec<&str> = page.iter().map(|(id, _)| id.as_str()).collect();
        assert!(
            !ids.contains(&"shared-target"),
            "an allowed target must not match through excluded-contributor documents: {ids:?}"
        );
        // Distinct-target budget: with limit 2, crowd-target takes ONE slot
        // (its eleven duplicate documents are one target) and valid-target
        // fills the second — a document-budget TopDocs page would have
        // consumed every slot with crowd-target duplicates.
        assert_eq!(
            ids.len(),
            2,
            "the target budget counts distinct targets: {ids:?}"
        );
        assert!(
            ids.contains(&"crowd-target") && ids.contains(&"valid-target"),
            "both allowed targets fill the target-budgeted page: {ids:?}"
        );

        // Reopen the persistent index and repeat: the constraints hold on the
        // warm path too.
        drop(index);
        let warm = reopen(&lexical_dir);
        let warm_page = warm
            .search_targets_within("render", 2, &targets, &contributors)
            .expect("warm distinct-target search");
        let warm_ids: Vec<&str> = warm_page.iter().map(|(id, _)| id.as_str()).collect();
        assert_eq!(
            warm_ids, ids,
            "the warm persistent index behaves identically: {warm_ids:?}"
        );
    }

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
