//! Explicit configured remote acquisition into adapter-owned, bounded pack caches.
use super::{CommittedFile, GitResourceLimits, ResolvedCommit, validate_branch_ref};
use engram_knowledge::{CoreError, CoreResult};
use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};
use url::Url;

#[derive(Clone, Debug)]
pub struct RemotePolicy {
    pub endpoint_allowlist: Vec<String>,
    pub ssh_config: Option<PathBuf>,
    pub ssh_known_hosts: Option<PathBuf>,
    pub ca_bundle: Option<PathBuf>,
    pub limits: GitResourceLimits,
}
#[derive(Clone, Debug)]
pub struct RemoteSnapshot {
    pub commit: ResolvedCommit,
    pub files: Vec<CommittedFile>,
    pub managed_repo: PathBuf,
}
fn validate_remote(locator: &str, allowlist: &[String]) -> CoreResult<Url> {
    let url = Url::parse(locator).map_err(|_| CoreError::InvalidRequest {
        reason: "remote locator is invalid".into(),
    })?;
    if !matches!(url.scheme(), "https" | "ssh")
        || url.password().is_some()
        || (url.scheme() == "https" && !url.username().is_empty())
        || url.host_str().is_none()
        || url.query().is_some()
        || url.fragment().is_some()
        || !url
            .username()
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
    {
        return Err(CoreError::InvalidRequest {
            reason:
                "only credential-free HTTPS or explicitly authenticated SSH remotes are supported"
                    .into(),
        });
    }
    let endpoint = format!(
        "{}://{}{}",
        url.scheme(),
        url.host_str().unwrap(),
        url.port().map(|p| format!(":{p}")).unwrap_or_default()
    );
    if !allowlist.iter().any(|allowed| allowed == &endpoint) {
        return Err(CoreError::PolicyDenied {
            reason: "remote endpoint is not allowlisted".into(),
        });
    }
    Ok(url)
}
fn validate_ssh_config(path: &Path) -> CoreResult<()> {
    let contents = std::fs::read_to_string(path).map_err(|_| CoreError::InvalidRequest {
        reason: "explicit SSH configuration is unavailable".into(),
    })?;
    const ALLOWED: &[&str] = &[
        "host",
        "hostname",
        "port",
        "user",
        "identityfile",
        "identitiesonly",
        "userknownhostsfile",
        "stricthostkeychecking",
        "batchmode",
        "loglevel",
    ];
    for line in contents.lines() {
        let directive = line
            .split('#')
            .next()
            .unwrap_or("")
            .split([' ', '\t', '='])
            .find(|part| !part.is_empty())
            .unwrap_or("")
            .to_ascii_lowercase();
        if !directive.is_empty() && !ALLOWED.contains(&directive.as_str()) {
            return Err(CoreError::InvalidRequest {
                reason: "SSH configuration contains an unsupported command or proxy directive"
                    .into(),
            });
        }
    }
    Ok(())
}
fn new_attempt_id() -> String {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let ticks = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!(
        "{}-{ticks}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )
}
pub fn acquire_remote_branch(
    remote: &str,
    branch_ref: &str,
    policy: &RemotePolicy,
    managed_root: &Path,
) -> CoreResult<RemoteSnapshot> {
    validate_branch_ref(branch_ref)?;
    let url = validate_remote(remote, &policy.endpoint_allowlist)?;
    if url.scheme() == "ssh" {
        let config = policy
            .ssh_config
            .as_deref()
            .ok_or(CoreError::InvalidRequest {
                reason: "configured SSH remote requires an explicit trusted SSH config".into(),
            })?;
        if !config.is_file() || policy.ssh_known_hosts.as_ref().is_none_or(|p| !p.is_file()) {
            return Err(CoreError::InvalidRequest {
                reason:
                    "configured SSH remote requires explicit configuration and host-key trust files"
                        .into(),
            });
        }
        validate_ssh_config(config)?;
    }
    if url.scheme() == "https" && policy.ca_bundle.as_ref().is_none_or(|p| !p.is_file()) {
        return Err(CoreError::InvalidRequest {
            reason: "configured HTTPS remote requires an explicit trusted CA bundle".into(),
        });
    }
    let managed = super::private_storage::private_directory(managed_root)?;
    let attempt = super::private_storage::OwnedAttempt::create(
        &managed,
        &format!("attempt-{}", new_attempt_id()),
        "remote-acquisition",
    )?;
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let snapshot = super::portable_remote::acquire(
            &url,
            branch_ref,
            policy,
            attempt.directory.as_ref().expect("owned directory"),
        )?;
        managed.sync().map_err(|_| CoreError::Adapter {
            adapter: "engram-source-storage".into(),
            message: "managed acquisition parent synchronization failed".into(),
        })?;
        Ok(snapshot)
    }))
    .unwrap_or_else(|_| {
        Err(CoreError::Adapter {
            adapter: "engram-source-storage".into(),
            message: "source acquisition unwound; cleanup retains exact custody".into(),
        })
    });
    attempt.finish(result)
}
