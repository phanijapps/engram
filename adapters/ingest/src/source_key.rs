//! Derivation of a SHA-free, commit-stable source identity key.
//!
//! The stable-source-key normalizes a git remote to `host/org/repo` (scheme,
//! credentials, and a trailing `.git` stripped, lowercased) so it does not
//! change across commits — unlike `source_id`/`document_id`, which embed the
//! commit SHA / content hash (see ADR-0017/0018). Non-git sources fall back to
//! a caller-supplied key (the un-enriched source name / repo root).

/// Metadata key under which the stable-source-key is carried on a
/// `KnowledgeGraph`'s metadata map.
pub const STABLE_SOURCE_KEY: &str = "stableSourceKey";

/// Metadata key under which a document's source-relative path is carried on a
/// `KnowledgeGraph`'s metadata map.
pub const SOURCE_PATH_KEY: &str = "path";

/// Metadata key under which the source document id is carried on a
/// `KnowledgeGraph`'s metadata map. The reconcile path reads this from a prior
/// graph to cascade-delete the document's chunks + embeddings + the document
/// itself on re-ingest (knowledge-source-retraction), since `graph_id` is a
/// non-reversible hash of `document_id`.
pub const DOCUMENT_ID_KEY: &str = "documentId";

/// Metadata key for the git remote URL the source was indexed from (RFC-0020
/// §"Repository provenance"). Provenance only — never part of entity identity.
pub const REPOSITORY_KEY: &str = "repository";

/// Metadata key for the git branch the source was indexed from. Provenance
/// only — re-indexing from a different branch updates it; one logical entity
/// per function regardless of branch (RFC-0020 rev).
pub const BRANCH_KEY: &str = "branch";

/// Metadata key for the git commit SHA the source was indexed from
/// (RFC-0020 §"Repository provenance"). Provenance only — never identity.
pub const REVISION_KEY: &str = "revision";

/// Derives the stable-source-key from an optional git remote, falling back to
/// `fallback` (the un-enriched source name / repo root) for non-git sources or
/// remotes that cannot be normalized.
pub fn stable_source_key(remote: Option<&str>, fallback: &str) -> String {
    remote
        .map(str::trim)
        .filter(|r| !r.is_empty())
        .and_then(normalize_remote)
        .unwrap_or_else(|| normalize_fallback(fallback))
}

fn normalize_fallback(fallback: &str) -> String {
    fallback.trim().to_lowercase()
}

/// Normalizes a git remote URL to lowercase `host/org/repo`, stripping the
/// scheme, credentials, a `:port`, and a trailing `.git`. Handles both URL
/// forms (`https://…`, `ssh://…`) and scp-like syntax (`git@host:org/repo`).
/// Returns `None` when the remote cannot be shaped into `host/path`.
fn normalize_remote(remote: &str) -> Option<String> {
    let has_scheme = remote.contains("://");
    // Strip scheme (`scheme://`).
    let after_scheme = remote.splitn(2, "://").last().unwrap_or(remote);
    // Strip userinfo (`user[:pw]@`) up to the last '@' before the host.
    let after_user = match after_scheme.rfind('@') {
        Some(i) => &after_scheme[i + 1..],
        None => after_scheme,
    };
    // scp-like syntax (`host:org/repo`, no scheme) uses ':' as the host/path
    // separator; convert the first ':' to '/'. URL forms keep ':' for a port.
    let separated = if has_scheme {
        after_user.to_string()
    } else {
        after_user.replacen(':', "/", 1)
    };
    let mut parts = separated.splitn(2, '/');
    let host_raw = parts.next().unwrap_or("");
    let host = host_raw.split(':').next().unwrap_or(host_raw); // drop ':port'
    let path = parts.next().unwrap_or("").trim_end_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);
    if host.is_empty() || path.is_empty() {
        return None;
    }
    Some(format!("{host}/{path}").to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_common_remote_forms_to_host_org_repo() {
        for remote in [
            "git@github.com:Org/Repo.git",
            "https://user:pw@github.com/Org/Repo.git",
            "https://github.com/Org/Repo",
            "ssh://git@github.com/Org/Repo",
        ] {
            assert_eq!(
                stable_source_key(Some(remote), "fallback"),
                "github.com/org/repo",
                "remote: {remote}"
            );
        }
    }

    #[test]
    fn key_is_commit_stable_and_has_no_sha_or_branch() {
        // The remote never carries the SHA, so re-deriving from the same remote
        // (regardless of the commit checked out) yields an identical key.
        let a = stable_source_key(Some("https://github.com/Org/Repo.git"), "f");
        let b = stable_source_key(Some("https://github.com/Org/Repo.git"), "f");
        assert_eq!(a, b);
        assert_eq!(a, "github.com/org/repo");
        assert!(!a.contains('@') && !a.contains(':'));
    }

    #[test]
    fn strips_port_on_url_host() {
        assert_eq!(
            stable_source_key(Some("https://github.com:443/Org/Repo.git"), "f"),
            "github.com/org/repo"
        );
    }

    #[test]
    fn falls_back_for_non_git_or_unparseable_source() {
        assert_eq!(stable_source_key(None, "scan:MyRepo"), "scan:myrepo");
        assert_eq!(stable_source_key(Some("   "), "scan:MyRepo"), "scan:myrepo");
        // A bare token with no host/path shape falls back rather than panicking.
        assert_eq!(
            stable_source_key(Some("garbage"), "scan:MyRepo"),
            "scan:myrepo"
        );
    }
}

/// Derives the branch-qualified code-index partition key (ADR-0032).
///
/// When a git ref (branch) is known, the key becomes `{repo_key}@{ref}`
/// (e.g. `github.com/acme/payments@refs/heads/main`), so scans of different
/// branches of the same repo produce independent lifecycles — a dev scan
/// never retracts main's graphs. When no ref is known (non-git source, or
/// legacy single-branch scan), the bare repository key is returned unchanged
/// (backward-compatible with existing stores).
///
/// The ref is used AS-DETECTED — case-sensitive, not lowercased, not fed
/// through the remote normalizer — because git refs are case-sensitive.
pub fn qualified_source_key(repo_key: &str, git_ref: Option<&str>) -> String {
    match git_ref {
        Some(r) if !r.is_empty() => format!("{repo_key}@{r}"),
        _ => repo_key.to_owned(),
    }
}

#[cfg(test)]
mod qualified_tests {
    use super::*;

    #[test]
    fn qualified_key_includes_the_ref() {
        assert_eq!(
            qualified_source_key("github.com/acme/payments", Some("refs/heads/main")),
            "github.com/acme/payments@refs/heads/main"
        );
        assert_eq!(
            qualified_source_key("github.com/acme/payments", Some("refs/heads/dev")),
            "github.com/acme/payments@refs/heads/dev"
        );
    }

    #[test]
    fn qualified_key_preserves_ref_case_sensitivity() {
        assert_eq!(
            qualified_source_key("github.com/org/repo", Some("refs/heads/Feature-X")),
            "github.com/org/repo@refs/heads/Feature-X"
        );
    }

    #[test]
    fn no_ref_returns_bare_key_backward_compatible() {
        assert_eq!(
            qualified_source_key("github.com/acme/payments", None),
            "github.com/acme/payments"
        );
        assert_eq!(
            qualified_source_key("github.com/acme/payments", Some("")),
            "github.com/acme/payments"
        );
    }
}
