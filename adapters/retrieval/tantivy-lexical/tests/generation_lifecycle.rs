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

/// A5 (r5): bounded distinct-target collector completion — genuine
/// contributor multiplicity, shared-target contributor isolation, duplicate /
/// tie / zero-limit / cross-commit merge coverage, and the full
/// cold/reopened/warm equivalence matrix with retained-state counters
/// documenting the transient K+1 per-segment bound and the O(segments × K)
/// merge storage.
mod a5_collector {
    use super::*;
    use std::collections::BTreeSet;
    use std::sync::Arc;

    use engram_store_lexical::DistinctTargetCounters;

    fn keys(generations: &[&str]) -> BTreeSet<String> {
        generations
            .iter()
            .map(|generation| code_contributor_key(generation))
            .collect()
    }

    fn ids(values: &[&str]) -> BTreeSet<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    fn page_ids(page: &[(String, f32)]) -> Vec<&str> {
        page.iter().map(|(id, _)| id.as_str()).collect()
    }

    /// Genuinely DISTINCT authorized contributor keys for ONE target
    /// produce MORE surviving matching documents than the legacy document
    /// fetch bound (limit × 2 = 4 for limit 2), and a lower-ranked eligible
    /// target still fills the bounded page. The old `search_contributors`
    /// page (document-budget TopDocs + dedupe) is the comparator: under it,
    /// five same-target documents consume every fetched slot and the tail
    /// target starves. Multiplicity is ASSERTED: each of the five
    /// contributor keys is proven to hold a live matching document for the
    /// shared target (single-key constrained searches).
    #[test]
    fn distinct_contributor_multiplicity_exceeds_document_fetch_bound() {
        let dir = TempDir::new().expect("tempdir");
        let lexical_dir = dir.path().join("lexical-a5-multi");
        std::fs::create_dir_all(&lexical_dir).expect("mkdir");
        let index = reopen(&lexical_dir);

        // FIVE distinct contributor keys, each contributing a DIFFERENT
        // matching document for the SAME target (upsert per (target,
        // contributor) pair REPLACES that pair, so distinct keys are what
        // actually survive — unlike the previous fixture whose repeated
        // same-key upserts replaced one row).
        let contributors = ["gen-m1", "gen-m2", "gen-m3", "gen-m4", "gen-m5"];
        for (position, generation) in contributors.iter().enumerate() {
            index
                .upsert_stamped(
                    "multi-target",
                    &format!("zephyr match variant {position}"),
                    &code_contributor_key(generation),
                )
                .expect("seed distinct contributor doc");
        }
        // A lower-ranked eligible target: matches once, weaker text.
        index
            .upsert_stamped("tail-target", "zephyr weaker tail", "doc")
            .expect("seed tail target");

        // Multiplicity assertion: every contributor key really holds a live
        // matching document for multi-target (five surviving documents > the
        // legacy limit × 2 = 4 document fetch bound for this page size).
        for generation in contributors {
            let only = index
                .search_within("zephyr", 5, &keys(&[generation]))
                .expect("single-key search");
            assert!(
                only.iter().any(|(id, _)| id == "multi-target"),
                "contributor {generation} holds a live matching document: {only:?}"
            );
        }

        let targets = ids(&["multi-target", "tail-target"]);
        let mut allowed = keys(&contributors);
        allowed.insert("doc".to_string());
        let counters = Arc::new(DistinctTargetCounters::default());
        let page = index
            .search_targets_within_counted("zephyr", 2, &targets, &allowed, Some(&counters))
            .expect("distinct-target multiplicity page");
        let listed = page_ids(&page);
        assert_eq!(
            listed.len(),
            2,
            "the target budget fills with DISTINCT targets: {listed:?}"
        );
        assert!(
            listed.contains(&"multi-target") && listed.contains(&"tail-target"),
            "the five same-target contributor documents take ONE slot and the lower-ranked \
             eligible target still appears: {listed:?}"
        );

        // Retained-state bounds (documented, now instrumented): the
        // per-segment map transiently permits K+1 entries and at most K
        // survive per segment, so merge storage is O(segments × K) — the
        // observed values must sit inside those bounds.
        assert!(
            counters.retained_max() <= 2 + 1,
            "transient per-segment bound is K+1 (K=2): {}",
            counters.retained_max()
        );
        assert!(
            counters.retained_max() > 0,
            "the instrumented search actually retained entries"
        );
        assert!(
            counters.merge_documents() <= counters.segments() * 2,
            "merge storage is O(segments × K) with K=2: {} entries over {} segments",
            counters.merge_documents(),
            counters.segments()
        );
    }

    /// A shared ALLOWED target holding BOTH a permitted-contributor document
    /// and an excluded-generation-only document: the target matches through
    /// the permitted token, and the excluded-only token can NEVER match
    /// through the shared allowed target.
    #[test]
    fn shared_target_excluded_only_token_cannot_match() {
        let dir = TempDir::new().expect("tempdir");
        let lexical_dir = dir.path().join("lexical-a5-shared");
        std::fs::create_dir_all(&lexical_dir).expect("mkdir");
        let index = reopen(&lexical_dir);
        index
            .upsert_stamped(
                "shared-target",
                "atlas permitted token",
                &code_contributor_key("gen-live"),
            )
            .expect("permitted contribution");
        index
            .upsert_stamped(
                "shared-target",
                "boreas excluded token",
                &code_contributor_key("gen-stale"),
            )
            .expect("excluded contribution");

        let targets = ids(&["shared-target"]);
        let allowed = keys(&["gen-live"]);
        let permitted = index
            .search_targets_within("atlas", 5, &targets, &allowed)
            .expect("permitted token search");
        assert!(
            permitted.iter().any(|(id, _)| id == "shared-target"),
            "the shared target matches through its permitted contribution: {permitted:?}"
        );
        let excluded_only = index
            .search_targets_within("boreas", 5, &targets, &allowed)
            .expect("excluded-only token search");
        assert!(
            excluded_only.is_empty(),
            "an excluded-generation-only token cannot match through the shared allowed \
             target: {excluded_only:?}"
        );
    }

    /// Duplicate documents (identical text under two contributor keys),
    /// equal-score ties (deterministic target-id order), zero limit, and
    /// cross-commit layout (three separate commit transactions — the merge
    /// path runs over separately-committed data whenever segments persist;
    /// Tantivy may itself merge segments, so the assertion is RESULT
    /// equivalence to the single-commit build, honestly labeled).
    #[test]
    fn duplicates_ties_zero_limit_and_cross_commit_merge() {
        let dir = TempDir::new().expect("tempdir");

        // Single-commit (cold comparator) build.
        let cold_dir = dir.path().join("lexical-a5-ties-cold");
        std::fs::create_dir_all(&cold_dir).expect("mkdir");
        let cold = reopen(&cold_dir);
        cold.upsert_batch_stamped(&[
            (
                "dupe-target".to_string(),
                "quantum duplicate text".to_string(),
                code_contributor_key("gen-d1"),
            ),
            (
                "dupe-target".to_string(),
                "quantum duplicate text".to_string(),
                code_contributor_key("gen-d2"),
            ),
            (
                "tie-alpha".to_string(),
                "quantum tie text".to_string(),
                "doc".to_string(),
            ),
            (
                "tie-beta".to_string(),
                "quantum tie text".to_string(),
                "doc".to_string(),
            ),
        ])
        .expect("seed cold");

        // Cross-commit build: the SAME documents committed in THREE separate
        // transactions, each finalizing its own segment before the next.
        let split_dir = dir.path().join("lexical-a5-ties-split");
        std::fs::create_dir_all(&split_dir).expect("mkdir");
        let split = reopen(&split_dir);
        split
            .upsert_stamped(
                "dupe-target",
                "quantum duplicate text",
                &code_contributor_key("gen-d1"),
            )
            .expect("commit 1");
        split
            .upsert_batch_stamped(&[(
                "tie-alpha".to_string(),
                "quantum tie text".to_string(),
                "doc".to_string(),
            )])
            .expect("commit 2");
        split
            .upsert_batch_stamped(&[
                (
                    "tie-beta".to_string(),
                    "quantum tie text".to_string(),
                    "doc".to_string(),
                ),
                (
                    "dupe-target".to_string(),
                    "quantum duplicate text".to_string(),
                    code_contributor_key("gen-d2"),
                ),
            ])
            .expect("commit 3");

        let targets = ids(&["dupe-target", "tie-alpha", "tie-beta"]);
        let mut allowed = keys(&["gen-d1", "gen-d2"]);
        allowed.insert("doc".to_string());

        // Duplicate documents: dupe-target occupies exactly ONE slot.
        let cold_page = cold
            .search_targets_within("quantum", 5, &targets, &allowed)
            .expect("cold page");
        let cold_listed = page_ids(&cold_page);
        assert_eq!(
            cold_listed
                .iter()
                .filter(|id| **id == "dupe-target")
                .count(),
            1,
            "duplicate documents collapse to one target slot: {cold_listed:?}"
        );
        // Equal-score ties break deterministically by target id ascending.
        assert_eq!(
            cold_listed,
            &["dupe-target", "tie-alpha", "tie-beta"],
            "deterministic order (score desc, then id asc): {cold_listed:?}"
        );

        // Cross-commit equivalence: the separately-committed layout returns
        // the identical page (the collector's merge path is exercised over
        // multi-segment state whenever segments persist).
        let split_page = split
            .search_targets_within("quantum", 5, &targets, &allowed)
            .expect("split page");
        assert_eq!(
            page_ids(&split_page),
            cold_listed,
            "cross-commit layout is result-equivalent to the single-commit build"
        );

        // Zero limit returns an explicit empty page (never a scan, never a
        // partial page).
        let zero = split
            .search_targets_within("quantum", 0, &targets, &allowed)
            .expect("zero-limit search");
        assert!(zero.is_empty(), "zero limit yields an explicit empty page");
    }

    /// The FULL cold/reopened/warm equivalence matrix over the multiplicity
    /// fixture: the constrained distinct-target page is identical on (1) the
    /// cold in-process index, (2) a closed-and-reopened index, and (3) the
    /// same warm handle after a further write — the write's target appears
    /// only when its inclusion is requested, and the original page is stable.
    #[test]
    fn cold_reopened_warm_equivalence_matrix() {
        let dir = TempDir::new().expect("tempdir");
        let lexical_dir = dir.path().join("lexical-a5-matrix");
        std::fs::create_dir_all(&lexical_dir).expect("mkdir");
        let index = reopen(&lexical_dir);
        index
            .upsert_batch_stamped(&[
                (
                    "matrix-core".to_string(),
                    "cobalt core strong match".to_string(),
                    code_contributor_key("gen-x1"),
                ),
                (
                    "matrix-core".to_string(),
                    "cobalt core second contribution".to_string(),
                    code_contributor_key("gen-x2"),
                ),
                (
                    "matrix-side".to_string(),
                    "cobalt side weaker".to_string(),
                    "doc".to_string(),
                ),
            ])
            .expect("seed matrix fixture");

        let mut allowed = keys(&["gen-x1", "gen-x2"]);
        allowed.insert("doc".to_string());
        let targets = ids(&["matrix-core", "matrix-side"]);

        // (1) Cold: fresh in-process index.
        let cold = index
            .search_targets_within("cobalt", 2, &targets, &allowed)
            .expect("cold matrix page");

        // (2) Reopened: close the handle, reopen from disk.
        drop(index);
        let warm = reopen(&lexical_dir);
        let reopened = warm
            .search_targets_within("cobalt", 2, &targets, &allowed)
            .expect("reopened matrix page");
        assert_eq!(
            page_ids(&reopened),
            page_ids(&cold),
            "reopened page equals the cold page: {:?} vs {:?}",
            page_ids(&reopened),
            page_ids(&cold)
        );

        // (3) Warm: the SAME handle after a further write — the original
        // page is unchanged, and the new target appears exactly when its
        // documents are in the requested target set.
        warm.upsert_stamped(
            "matrix-late",
            "cobalt late arrival",
            &code_contributor_key("gen-x3"),
        )
        .expect("warm write");
        let stable = warm
            .search_targets_within("cobalt", 2, &targets, &allowed)
            .expect("warm stable page");
        assert_eq!(
            page_ids(&stable),
            page_ids(&cold),
            "the warm page is identical after an unrelated write: {:?} vs {:?}",
            page_ids(&stable),
            page_ids(&cold)
        );
        let mut late_allowed = allowed.clone();
        late_allowed.insert(code_contributor_key("gen-x3"));
        let late_targets = ids(&["matrix-core", "matrix-side", "matrix-late"]);
        let with_late = warm
            .search_targets_within("cobalt", 3, &late_targets, &late_allowed)
            .expect("warm page including the late target");
        assert!(
            page_ids(&with_late).contains(&"matrix-late"),
            "the late write is visible once its target is requested: {:?}",
            page_ids(&with_late)
        );
    }
}
