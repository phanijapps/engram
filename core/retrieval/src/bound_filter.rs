//! Engine-neutral bound-code eligibility classification for retrieval lanes.
//!
//! A retrieval lane that can surface code-derived records must constrain its
//! candidate set BEFORE ranking, limits or budgets are applied (REQ-005:
//! code reads are limited to authorized bound published generations).
//!
//! Corrective r2 (review findings 4/9): classification is driven by DURABLE
//! TARGET LINEAGE — the repository-owned staged lineage records — never by
//! record metadata. A record is code exactly when its id has a durable
//! lineage entry; it is eligible code only when that lineage's repository and
//! generation match a PUBLISHED binding (repository + branch + generation
//! verified together) and the request's narrowing selects that repository.
//! Records with no lineage but a code kind are LEGACY UNBOUND CODE and are
//! never eligible. Ordinary non-code records stay eligible always (AC-006).
//!
//! Requested narrowing (`QueryFilter::code_index_keys`) intersects the FULL
//! immutable repository/branch identity: a qualified key `repo@refs/heads/x`
//! selects that repository only when its published binding branch is exactly
//! `refs/heads/x` (case-sensitive). Narrowing can never widen eligibility;
//! `"*"` means every eligible bound repository, nothing more.
//!
//! This module is pure: no SQL, no storage types, no async I/O.

use async_trait::async_trait;
use engram_domain::Scope;
use engram_runtime::{CoreError, CoreResult};
use std::collections::{BTreeMap, BTreeSet};

/// Metadata key the bound-code scanner stamps on records. NOT an authority:
/// only durable lineage entries establish code identity (finding 4).
pub const CODE_GENERATION_KEY: &str = "codeGenerationId";
/// Metadata key carrying the bound repository id. NOT an authority.
pub const CODE_REPOSITORY_KEY: &str = "codeRepoId";

/// Wildcard requested-index key: all eligible bound repositories.
pub const CODE_INDEX_WILDCARD: &str = "*";

/// One published (authorized, bound) repository binding as resolved from the
/// durable store: repository + branch + active generation verified together.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoundBinding {
    pub repo_id: String,
    pub branch_ref: String,
    pub generation_id: String,
}

/// One durable target-lineage association: `record_id` is code contributed by
/// `generation_id` of `repo_id` (the scanner's staged lineage record).
/// `retired` (review r5 finding 2) is the additive retirement fact: a retired
/// association stays POSITIVE code ownership for classification (the record
/// is never ordinary again) while being explicitly INELIGIBLE in every view.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DurableLineage {
    pub record_id: String,
    pub repo_id: String,
    pub generation_id: String,
    pub retired: bool,
}

/// Published-generation eligibility for one scope, resolved once per request
/// from durable authority (bindings + lineage), never from record metadata.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BoundCodeEligibility {
    /// Monotonic scope epoch; bumps on publish/reset. Used to key warm caches
    /// so a stale warm entry can never serve a post-retirement answer.
    pub epoch: u64,
    /// Published bindings (repository + branch + generation together).
    pub bindings: Vec<BoundBinding>,
    /// Durable target lineage for the scope (record → repo + generation).
    pub lineage: Vec<DurableLineage>,
}

impl BoundCodeEligibility {
    /// Published generation ids (derived from [`Self::bindings`]).
    pub fn generations(&self) -> Vec<String> {
        self.bindings
            .iter()
            .map(|b| b.generation_id.clone())
            .collect()
    }

    /// Published repository ids (derived from [`Self::bindings`]).
    pub fn repositories(&self) -> Vec<String> {
        self.bindings.iter().map(|b| b.repo_id.clone()).collect()
    }
}

/// Per-request resolved eligibility view: published bindings ∩ durable
/// lineage ∩ requested narrowing.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BoundCodeView {
    /// repo → (branch, generation) for every published binding.
    published: BTreeMap<String, (String, String)>,
    /// record → (repo, generation) from durable staged lineage, RETIRED
    /// records included (finding 2): retirement is an ownership fact, so a
    /// retired record never loses its code identity — only its eligibility.
    lineage: BTreeMap<String, (String, String)>,
    /// Retired durable-lineage records (finding 2): positive code ownership,
    /// explicitly denied eligibility.
    retired: BTreeSet<String>,
    requested_keys: Option<BTreeSet<String>>,
}

/// Classification of one record against a [`BoundCodeView`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecordEligibility {
    /// Ordinary memory / document / knowledge record — always eligible.
    NonCode,
    /// Code whose durable lineage matches a published binding of a selected
    /// repository (repository + generation verified together).
    EligibleCode,
    /// Stale / retired / staged / cross-repo-mismatched code, or legacy
    /// unbound code (code kind without durable lineage), or a repository the
    /// request's narrowing did not select.
    IneligibleCode,
}

impl BoundCodeView {
    /// A view with no published bindings: no code is eligible, non-code stays
    /// eligible, and legacy unbound code is still denied.
    pub fn none_bound() -> Self {
        Self::default()
    }

    /// Builds the view over published bindings + durable lineage. Retired
    /// lineage entries STAY in the classification universe (finding 2): the
    /// record remains code for `is_code_record`/`code_records` while
    /// [`Self::classify`] denies it.
    pub fn from_eligibility(eligibility: &BoundCodeEligibility) -> Self {
        Self {
            published: eligibility
                .bindings
                .iter()
                .map(|binding| {
                    (
                        binding.repo_id.clone(),
                        (binding.branch_ref.clone(), binding.generation_id.clone()),
                    )
                })
                .collect(),
            lineage: eligibility
                .lineage
                .iter()
                .map(|entry| {
                    (
                        entry.record_id.clone(),
                        (entry.repo_id.clone(), entry.generation_id.clone()),
                    )
                })
                .collect(),
            retired: eligibility
                .lineage
                .iter()
                .filter(|entry| entry.retired)
                .map(|entry| entry.record_id.clone())
                .collect(),
            requested_keys: None,
        }
    }

    /// Applies the request's `code_index_keys` narrowing. Keys are the bare
    /// repository id, the fully-qualified `repo@refs/heads/branch` form, or
    /// `"*"` (all eligible). A qualified key selects the repository ONLY when
    /// its published branch matches EXACTLY (case-sensitive); requesting an
    /// unbound repository or a wrong/case-distinct ref selects nothing —
    /// narrowing can never widen eligibility (finding 9).
    pub fn with_requested_keys(mut self, keys: Option<&[String]>) -> Self {
        self.requested_keys = keys.map(|keys| keys.iter().cloned().collect());
        self
    }

    /// Published bindings visible to this view (after narrowing).
    pub fn eligible_bindings(&self) -> Vec<BoundBinding> {
        self.published
            .iter()
            .filter(|(repo, _)| self.repository_selected(Some(repo)))
            .map(|(repo, (branch, generation))| BoundBinding {
                repo_id: repo.clone(),
                branch_ref: branch.clone(),
                generation_id: generation.clone(),
            })
            .collect()
    }

    /// Published generation ids of repo-selected bindings (for lexical
    /// contributor keys and other pre-budget lane constraints).
    pub fn eligible_generations(&self) -> Vec<String> {
        self.eligible_bindings()
            .iter()
            .map(|binding| binding.generation_id.clone())
            .collect()
    }

    /// Whether a requested-key filter is active.
    pub fn narrows_by_request(&self) -> bool {
        self.requested_keys
            .as_ref()
            .is_some_and(|keys| !keys.is_empty())
    }

    /// True when `repository` survives request narrowing: no narrowing, an
    /// empty narrowing, the wildcard, the bare repository id, or a fully
    /// qualified `repo@branch` whose branch equals the PUBLISHED branch
    /// exactly (case-sensitive) — the immutable identity is intersected, not
    /// truncated at `@`.
    fn repository_selected(&self, repository: Option<&str>) -> bool {
        let Some(requested) = self.requested_keys.as_ref() else {
            return true;
        };
        if requested.is_empty() || requested.contains(CODE_INDEX_WILDCARD) {
            return true;
        }
        let Some(repo) = repository else {
            return false;
        };
        let published_branch = self.published.get(repo).map(|(branch, _)| branch.clone());
        requested.iter().any(|key| match key.split_once('@') {
            None => key == repo,
            Some((bare, ref_part)) => {
                bare == repo
                    && published_branch
                        .as_deref()
                        .is_some_and(|branch| branch == ref_part)
            }
        })
    }

    /// Classifies one record from DURABLE LINEAGE + record kind. Metadata
    /// stamps are never consulted (finding 4).
    ///
    /// - RETIRED lineage (finding 2) → always `IneligibleCode`, even when the
    ///   immutable historical generation still matches a published binding:
    ///   retirement preserves ownership while denying eligibility.
    /// - `record_id` with a durable lineage entry → code: eligible only when
    ///   the lineage repository's PUBLISHED generation equals the lineage
    ///   generation AND that repository survives narrowing.
    /// - No lineage + `kind_is_code` → legacy unbound code: ineligible.
    /// - No lineage + non-code kind → `NonCode`.
    pub fn classify(&self, record_id: &str, kind_is_code: bool) -> RecordEligibility {
        if self.retired.contains(record_id) {
            return RecordEligibility::IneligibleCode;
        }
        match self.lineage.get(record_id) {
            Some((repo, generation)) => {
                let published = self.published.get(repo).map(|(_, generation)| generation);
                match published {
                    Some(published) if published == generation => {
                        if self.repository_selected(Some(repo)) {
                            RecordEligibility::EligibleCode
                        } else {
                            RecordEligibility::IneligibleCode
                        }
                    }
                    _ => RecordEligibility::IneligibleCode,
                }
            }
            None if kind_is_code => RecordEligibility::IneligibleCode,
            None => RecordEligibility::NonCode,
        }
    }

    /// Convenience predicate over [`Self::classify`].
    pub fn allows(&self, record_id: &str, kind_is_code: bool) -> bool {
        !matches!(
            self.classify(record_id, kind_is_code),
            RecordEligibility::IneligibleCode
        )
    }

    /// True when `record_id` carries durable code lineage (regardless of
    /// eligibility — RETIRED records included, finding 2) — the authoritative
    /// code test for seams that must treat code and non-code differently
    /// (finding 4). Retirement never makes a record ordinary.
    pub fn is_code_record(&self, record_id: &str) -> bool {
        self.lineage.contains_key(record_id)
    }

    /// True when `record_id` is RETIRED durable code lineage (finding 2):
    /// positive ownership, denied eligibility.
    pub fn is_retired_record(&self, record_id: &str) -> bool {
        self.retired.contains(record_id)
    }

    /// The repository a durable-lineage record belongs to, when its lineage
    /// exists (display qualification and repo-scoped composition).
    pub fn code_repo_of(&self, record_id: &str) -> Option<&str> {
        self.lineage.get(record_id).map(|(repo, _)| repo.as_str())
    }

    /// Every durable-lineage record id in the scope (code records, eligible
    /// or not) — the authoritative enumeration for wildcard authorization.
    pub fn code_records(&self) -> Vec<&str> {
        self.lineage.keys().map(String::as_str).collect()
    }

    /// The durable lineage association (repository, generation) of one
    /// NON-RETIRED record, when it exists — the canonical origin association
    /// used to select PARTICIPATING chunks for exact reads (corrective r4,
    /// canonical 10: stamps are never authority). A retired association
    /// resolves to `None`: retired code never participates (finding 2).
    pub fn code_lineage_of(&self, record_id: &str) -> Option<(&str, &str)> {
        if self.retired.contains(record_id) {
            return None;
        }
        self.lineage
            .get(record_id)
            .map(|(repo, generation)| (repo.as_str(), generation.as_str()))
    }
}

/// Rejects an unbound code read attempt with the typed contract error.
pub fn binding_required() -> CoreError {
    CoreError::InvalidRequest {
        reason: "BindingRequired: code retrieval requires a published bound generation".into(),
    }
}

/// Port: resolve the scope's durable eligibility (published bindings + target
/// lineage) once per request. Implemented over the neutral
/// `CodeBindingRepository` port by the composition layer; lanes depend on
/// this trait, never on a store.
#[allow(clippy::double_must_use)]
// async_trait codegen stamps #[must_use] on the boxed-future helper; CoreResult is already must_use
#[async_trait]
pub trait CodeEligibilityResolver: Send + Sync {
    /// `Ok(default)` — no bindings exist in this scope (non-code still flows).
    async fn resolve(&self, scope: &Scope) -> CoreResult<BoundCodeEligibility>;
}

/// Resolves a request's [`BoundCodeView`] through a resolver, treating a
/// resolver failure as a caller-visible error (the lane decides how to fail
/// closed — never by silently widening).
pub async fn resolve_view(
    resolver: &dyn CodeEligibilityResolver,
    scope: &Scope,
    requested_keys: Option<&[String]>,
) -> CoreResult<BoundCodeView> {
    let eligibility = resolver.resolve(scope).await?;
    Ok(BoundCodeView::from_eligibility(&eligibility).with_requested_keys(requested_keys))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bindings(entries: &[(&str, &str, &str)]) -> BoundCodeEligibility {
        BoundCodeEligibility {
            epoch: 1,
            bindings: entries
                .iter()
                .map(|(repo, branch, generation)| BoundBinding {
                    repo_id: (*repo).to_owned(),
                    branch_ref: (*branch).to_owned(),
                    generation_id: (*generation).to_owned(),
                })
                .collect(),
            lineage: Vec::new(),
        }
    }

    fn eligibility_with_lineage(
        binding_entries: &[(&str, &str, &str)],
        lineage_entries: &[(&str, &str, &str)],
    ) -> BoundCodeEligibility {
        BoundCodeEligibility {
            epoch: 2,
            bindings: binding_entries
                .iter()
                .map(|(repo, branch, generation)| BoundBinding {
                    repo_id: (*repo).to_owned(),
                    branch_ref: (*branch).to_owned(),
                    generation_id: (*generation).to_owned(),
                })
                .collect(),
            lineage: lineage_entries
                .iter()
                .map(|(record, repo, generation)| DurableLineage {
                    record_id: (*record).to_owned(),
                    repo_id: (*repo).to_owned(),
                    generation_id: (*generation).to_owned(),
                    retired: false,
                })
                .collect(),
        }
    }

    #[test]
    fn retired_lineage_stays_code_ownership_with_denied_eligibility() {
        // Review r5 finding 2: a retired durable-lineage record never
        // resurrects as ordinary — it stays POSITIVE code ownership
        // (is_code_record / code_records) while being explicitly
        // ineligible, even when its immutable generation still matches the
        // published binding, and it never participates in exact reads.
        let mut eligibility = eligibility_with_lineage(
            &[("repo-a", "refs/heads/main", "gen-1")],
            &[
                ("rec-retired", "repo-a", "gen-1"),
                ("rec-live", "repo-a", "gen-1"),
            ],
        );
        eligibility.lineage[0].retired = true;
        let view = BoundCodeView::from_eligibility(&eligibility);
        assert!(
            view.is_code_record("rec-retired"),
            "retired stays code-owned"
        );
        assert!(view.is_retired_record("rec-retired"));
        assert!(!view.is_retired_record("rec-live"));
        assert_eq!(
            view.classify("rec-retired", false),
            RecordEligibility::IneligibleCode,
            "a retired ordinary-KIND record with retained lineage denies (never NonCode)"
        );
        assert_eq!(
            view.classify("rec-retired", true),
            RecordEligibility::IneligibleCode
        );
        assert!(!view.allows("rec-retired", false));
        assert!(view.allows("rec-live", false));
        assert!(
            view.code_records().contains(&"rec-retired"),
            "wildcard/classification universes keep the retired record"
        );
        assert_eq!(
            view.code_lineage_of("rec-retired"),
            None,
            "retired code never participates in exact reads"
        );
        assert!(view.code_lineage_of("rec-live").is_some());
    }

    #[test]
    fn durable_lineage_is_the_code_authority_not_metadata() {
        // Finding 4: a lineage record on a published binding is eligible; the
        // SAME repository bound to a DIFFERENT generation is not; a forged
        // metadata stamp on a record without lineage does nothing.
        let eligibility = eligibility_with_lineage(
            &[("repo-a", "refs/heads/main", "gen-1")],
            &[
                ("rec-eligible", "repo-a", "gen-1"),
                ("rec-stale", "repo-a", "gen-staged"),
            ],
        );
        let view = BoundCodeView::from_eligibility(&eligibility);
        assert_eq!(
            view.classify("rec-eligible", false),
            RecordEligibility::EligibleCode
        );
        assert_eq!(
            view.classify("rec-stale", false),
            RecordEligibility::IneligibleCode
        );
        // No lineage + code kind ⇒ legacy unbound code ⇒ ineligible.
        assert_eq!(
            view.classify("rec-forged-stamp", true),
            RecordEligibility::IneligibleCode,
            "a code-kind record without durable lineage never becomes eligible"
        );
        // No lineage + non-code kind ⇒ NonCode (ordinary records flow).
        assert_eq!(
            view.classify("ordinary-doc", false),
            RecordEligibility::NonCode
        );
        assert!(view.is_code_record("rec-eligible"));
        assert!(!view.is_code_record("ordinary-doc"));
    }

    #[test]
    fn unbound_code_and_non_code_classify_independently() {
        let view = BoundCodeView::none_bound();
        assert_eq!(
            view.classify("legacy-symbol", true),
            RecordEligibility::IneligibleCode,
            "legacy unbound code is never eligible"
        );
        assert_eq!(
            view.classify("ordinary", false),
            RecordEligibility::NonCode,
            "ordinary documents stay eligible"
        );
    }

    #[test]
    fn qualified_keys_intersect_the_full_immutable_identity() {
        // Finding 9: `repo@refs/heads/develop` must NOT select repo-a's
        // published MAIN generation; case-distinct refs are distinct.
        let eligibility = eligibility_with_lineage(
            &[
                ("repo-a", "refs/heads/main", "gen-main"),
                ("repo-b", "refs/heads/develop", "gen-dev"),
            ],
            &[
                ("rec-a", "repo-a", "gen-main"),
                ("rec-b", "repo-b", "gen-dev"),
            ],
        );
        let view = BoundCodeView::from_eligibility(&eligibility)
            .with_requested_keys(Some(&["repo-a@refs/heads/develop".to_string()]));
        // Wrong ref for repo-a: repo-a is NOT selected.
        assert_eq!(
            view.classify("rec-a", false),
            RecordEligibility::IneligibleCode,
            "requesting repo-a@develop must not select repo-a's main generation"
        );
        // Correct qualified key selects exactly that binding.
        let correct = BoundCodeView::from_eligibility(&eligibility)
            .with_requested_keys(Some(&["repo-a@refs/heads/main".to_string()]));
        assert_eq!(
            correct.classify("rec-a", false),
            RecordEligibility::EligibleCode
        );
        assert_eq!(
            correct.classify("rec-b", false),
            RecordEligibility::IneligibleCode
        );
        // Case-distinct ref is a mismatch, not a fold.
        let case_view = BoundCodeView::from_eligibility(&eligibility)
            .with_requested_keys(Some(&["repo-a@refs/heads/Main".to_string()]));
        assert_eq!(
            case_view.classify("rec-a", false),
            RecordEligibility::IneligibleCode
        );
        // Bare repository id selects the published binding of that repo.
        let bare = BoundCodeView::from_eligibility(&eligibility)
            .with_requested_keys(Some(&["repo-b".to_string()]));
        assert_eq!(
            bare.classify("rec-b", false),
            RecordEligibility::EligibleCode
        );
        assert_eq!(
            bare.classify("rec-a", false),
            RecordEligibility::IneligibleCode
        );
        // Wildcard keeps every eligible repository, and nothing more.
        let wildcard = BoundCodeView::from_eligibility(&eligibility)
            .with_requested_keys(Some(&["*".to_string()]));
        assert!(wildcard.allows("rec-a", false) && wildcard.allows("rec-b", false));
        // Requesting an unbound repository selects nothing.
        let unbound = BoundCodeView::from_eligibility(&eligibility)
            .with_requested_keys(Some(&["repo-unbound".to_string()]));
        assert_eq!(
            unbound.classify("rec-a", false),
            RecordEligibility::IneligibleCode
        );
        // Narrowed bindings/generations reflect the full-key intersection.
        assert_eq!(correct.eligible_generations(), vec!["gen-main".to_owned()]);
    }
}
