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
