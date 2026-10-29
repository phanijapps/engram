//! Engine-neutral bound-code eligibility classification for retrieval lanes.
//!
//! A retrieval lane that can surface code-derived records must constrain its
//! candidate set BEFORE ranking, limits or budgets are applied (REQ-005:
//! code reads are limited to authorized bound published generations). This
//! module owns the neutral classification rules shared by every lane:
//!
//! - **Bound code** — a record stamped `codeGenerationId` by the bound-code
//!   scanner is eligible only when that generation is in the scope's published
//!   set. A staged, retired or superseded generation is ineligible no matter
//!   how strongly it matches the query.
//! - **Legacy unbound code** — a code record with repository provenance but no
//!   generation stamp (pre-binding `scan_repo` output) is never eligible for
//!   ranked retrieval: unbound code lineage yields a denial, it does not infer
//!   a binding from a checkout or leak through a documents fallback.
//! - **Non-code** — ordinary memory / document / knowledge records remain
//!   eligible always; constraining code never removes them (AC-006).
//!
//! Requested narrowing (`QueryFilter::code_index_keys`) can only narrow: a
//! requested repository key selects a subset of the published set and can
//! never widen eligibility to an unbound or unpublished repository. The
//! wildcard `"*"` means "every eligible bound repository", nothing more.
//!
//! This module is pure: no SQL, no storage types, no async I/O. Lanes resolve
//! a [`BoundCodeView`] once per request (via a [`CodeEligibilityResolver`]
//! port implemented over the neutral `CodeBindingRepository`) and then apply
//! `allows`/`classify` while building candidates.

use async_trait::async_trait;
use engram_domain::{Metadata, Scope};
use engram_runtime::{CoreError, CoreResult};
use std::collections::BTreeSet;

/// Metadata key the bound-code scanner stamps on every code-derived record
/// (see `adapters/ingest/src/generation_stage.rs`).
pub const CODE_GENERATION_KEY: &str = "codeGenerationId";
/// Metadata key carrying the bound repository id (`binding.repo_id`).
pub const CODE_REPOSITORY_KEY: &str = "codeRepoId";
/// Metadata key carrying the bound branch ref.
pub const CODE_BRANCH_KEY: &str = "branch";

/// Wildcard requested-index key: all eligible bound repositories.
pub const CODE_INDEX_WILDCARD: &str = "*";

/// Published-generation eligibility for one scope, resolved once per request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoundCodeEligibility {
    /// Monotonic scope epoch; bumps on publish/reset. Used to key warm caches
    /// so a stale warm entry can never serve a post-retirement answer.
    pub epoch: u64,
    /// Published (authorized, bound) generation ids.
    pub generations: Vec<String>,
    /// Published repository ids (bare `RepoId` strings).
    pub repositories: Vec<String>,
}

/// Per-request resolved eligibility view: published set ∩ requested narrowing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoundCodeView {
    eligible_generations: BTreeSet<String>,
    eligible_repositories: BTreeSet<String>,
    requested_keys: Option<BTreeSet<String>>,
}

/// Classification of one record against a [`BoundCodeView`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecordEligibility {
    /// Ordinary memory / document / knowledge record — always eligible.
    NonCode,
    /// Code from a published, authorized, bound generation.
    EligibleCode,
    /// Staged / retired / superseded code, or legacy unbound code, or a
    /// repository the request's narrowing did not select.
    IneligibleCode,
}

impl BoundCodeView {
    /// A view with no published bindings: no code is eligible, non-code stays
    /// eligible. (Legacy unbound code is still classified ineligible.)
    pub fn none_bound() -> Self {
        Self {
            eligible_generations: BTreeSet::new(),
            eligible_repositories: BTreeSet::new(),
            requested_keys: None,
        }
    }

    /// Builds the view over a published eligibility set.
    pub fn from_eligibility(eligibility: &BoundCodeEligibility) -> Self {
        Self {
            eligible_generations: eligibility.generations.iter().cloned().collect(),
            eligible_repositories: eligibility.repositories.iter().cloned().collect(),
            requested_keys: None,
        }
    }

    /// Applies the request's `code_index_keys` narrowing. Keys are qualified
    /// (`repo@refs/heads/main`), bare repository ids, or `"*"` (all eligible).
    /// Requesting an unbound repository selects nothing — narrowing can never
    /// widen eligibility.
    pub fn with_requested_keys(mut self, keys: Option<&[String]>) -> Self {
        self.requested_keys = keys.map(|keys| keys.iter().cloned().collect());
        self
    }

    /// Published generation ids visible to this view (after narrowing by
    /// repository, when the mapping is expressible from stamps alone).
    pub fn eligible_generations(&self) -> Vec<String> {
        self.eligible_generations.iter().cloned().collect()
    }

    /// Published repository ids visible to this view.
    pub fn eligible_repositories(&self) -> Vec<String> {
        self.eligible_repositories.iter().cloned().collect()
    }

    /// True when a requested-key filter is active (only eligible keys apply).
    pub fn narrows_by_request(&self) -> bool {
        self.requested_keys
            .as_ref()
            .is_some_and(|keys| !keys.is_empty())
    }

    /// Whether a code record from `repository` survives request narrowing.
    fn repository_selected(&self, repository: Option<&str>) -> bool {
        let Some(requested) = self.requested_keys.as_ref() else {
            return true;
        };
        if requested.is_empty() || requested.contains(CODE_INDEX_WILDCARD) {
            return true;
        }
        // Accept the bare repository id or any qualified `repo@ref` spelling
        // of it; the ref part can only further narrow, never widen.
        repository.is_some_and(|repo| {
            requested.contains(repo)
                || requested.iter().any(|key| {
                    key.rsplit_once('@').is_some_and(|(bare, _)| {
                        bare == repo && self.eligible_repositories.contains(repo)
                    })
                })
        })
    }

    /// Classifies one code-derived record from its scanner stamps.
    ///
    /// `metadata` is the record's metadata map; `legacy_unbound_code` marks a
    /// code record with repository provenance but no generation stamp.
    pub fn classify(
        &self,
        metadata: Option<&Metadata>,
        legacy_unbound_code: bool,
    ) -> RecordEligibility {
        let generation = metadata
            .and_then(|meta| meta.get(CODE_GENERATION_KEY))
            .and_then(|value| value.as_str())
            .map(str::to_owned);
        let repository = metadata
            .and_then(|meta| meta.get(CODE_REPOSITORY_KEY))
            .and_then(|value| value.as_str())
            .map(str::to_owned);
        match generation {
            Some(generation) => {
                if !self.eligible_generations.contains(&generation) {
                    RecordEligibility::IneligibleCode
                } else if !self.repository_selected(repository.as_deref()) {
                    RecordEligibility::IneligibleCode
                } else {
                    RecordEligibility::EligibleCode
                }
            }
            None if legacy_unbound_code => RecordEligibility::IneligibleCode,
            None => RecordEligibility::NonCode,
        }
    }

    /// Convenience predicate over [`Self::classify`]: non-code and eligible
    /// code pass; ineligible or legacy unbound code fail.
    pub fn allows(&self, metadata: Option<&Metadata>, legacy_unbound_code: bool) -> bool {
        !matches!(
            self.classify(metadata, legacy_unbound_code),
            RecordEligibility::IneligibleCode
        )
    }
}

/// Rejects an unbound code read attempt with the typed contract error.
pub fn binding_required() -> CoreError {
    CoreError::InvalidRequest {
        reason: "BindingRequired: code retrieval requires a published bound generation".into(),
    }
}

/// Port: resolve the scope's published code eligibility once per request.
///
/// Implemented over the neutral `CodeBindingRepository::eligibility` port by
/// the composition layer; lanes depend on this trait, never on a store.
#[allow(clippy::double_must_use)]
// async_trait codegen stamps #[must_use] on the boxed-future helper; CoreResult is already must_use
#[async_trait]
pub trait CodeEligibilityResolver: Send + Sync {
    /// `Ok(None)` — no bindings exist in this scope (non-code still flows).
    async fn resolve(&self, scope: &Scope) -> CoreResult<Option<BoundCodeEligibility>>;
}

/// Resolves a request's [`BoundCodeView`] through a resolver, treating a
/// resolver failure as a failed-closed lane error (never as "no bindings").
pub async fn resolve_view(
    resolver: &dyn CodeEligibilityResolver,
    scope: &Scope,
    requested_keys: Option<&[String]>,
) -> CoreResult<BoundCodeView> {
    let view = match resolver.resolve(scope).await? {
        Some(eligibility) => BoundCodeView::from_eligibility(&eligibility),
        None => BoundCodeView::none_bound(),
    };
    Ok(view.with_requested_keys(requested_keys))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eligibility(generations: &[&str], repositories: &[&str]) -> BoundCodeEligibility {
        BoundCodeEligibility {
            epoch: 1,
            generations: generations.iter().map(|s| s.to_string()).collect(),
            repositories: repositories.iter().map(|s| s.to_string()).collect(),
        }
    }

    fn stamps(generation: Option<&str>, repository: Option<&str>) -> Option<Metadata> {
        let mut meta = Metadata::new();
        if let Some(generation) = generation {
            meta.insert(CODE_GENERATION_KEY.into(), generation.into());
        }
        if let Some(repository) = repository {
            meta.insert(CODE_REPOSITORY_KEY.into(), repository.into());
        }
        Some(meta)
    }

    #[test]
    fn published_generation_is_eligible_and_stale_is_not() {
        let view = BoundCodeView::from_eligibility(&eligibility(&["gen-1"], &["repo"]));
        assert_eq!(
            view.classify(stamps(Some("gen-1"), Some("repo")).as_ref(), false),
            RecordEligibility::EligibleCode
        );
        assert_eq!(
            view.classify(stamps(Some("gen-stale"), Some("repo")).as_ref(), false),
            RecordEligibility::IneligibleCode
        );
    }

    #[test]
    fn unbound_code_and_non_code_classify_independently() {
        let view = BoundCodeView::none_bound();
        assert_eq!(
            view.classify(stamps(None, Some("repo")).as_ref(), true),
            RecordEligibility::IneligibleCode,
            "legacy unbound code is never eligible"
        );
        assert_eq!(
            view.classify(stamps(None, None).as_ref(), false),
            RecordEligibility::NonCode,
            "ordinary documents stay eligible"
        );
    }

    #[test]
    fn requested_keys_only_narrow() {
        let view = BoundCodeView::from_eligibility(&eligibility(&["gen-1"], &["repo-a", "repo-b"]))
            .with_requested_keys(Some(&["repo-a@refs/heads/main".to_string()]));
        assert_eq!(
            view.classify(stamps(Some("gen-1"), Some("repo-a")).as_ref(), false),
            RecordEligibility::EligibleCode
        );
        assert_eq!(
            view.classify(stamps(Some("gen-1"), Some("repo-b")).as_ref(), false),
            RecordEligibility::IneligibleCode
        );
        // An unbound repository cannot be selected by requesting it.
        let widened = BoundCodeView::from_eligibility(&eligibility(&["gen-1"], &["repo-a"]))
            .with_requested_keys(Some(&["repo-unbound".to_string()]));
        assert_eq!(
            widened.classify(stamps(Some("gen-1"), Some("repo-a")).as_ref(), false),
            RecordEligibility::IneligibleCode
        );
        // Wildcard keeps every eligible repository, and nothing more.
        let wildcard = BoundCodeView::from_eligibility(&eligibility(&["gen-1"], &["repo-a"]))
            .with_requested_keys(Some(&["*".to_string()]));
        assert!(wildcard.allows(stamps(Some("gen-1"), Some("repo-a")).as_ref(), false));
    }
}
