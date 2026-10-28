//! Explicit configured-remote Git fetches into adapter-owned, attempt-private bare databases.
use super::{
    CommittedFile, GitResourceLimits, ResolvedCommit, acquire_local_branch_with_limits,
    bounded_process::run_limited_checked, validate_branch_ref,
};
use engram_knowledge::{CoreError, CoreResult};
use std::{
    path::{Path, PathBuf},
    process::Command,
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
fn shell_quote(path: &Path) -> String {
    format!("'{}'", path.to_string_lossy().replace('\'', "'\\''"))
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
fn controlled_git(
    repo: &Path,
    args: &[String],
    policy: &RemotePolicy,
    max_stdout: usize,
    endpoint: &Url,
) -> CoreResult<()> {
    let mut command = Command::new("git");
    command
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .current_dir(repo)
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_COUNT", "0")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_NO_REPLACE_OBJECTS", "1")
        .env("GIT_NO_LAZY_FETCH", "1")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_TEMPLATE_DIR", "/dev/null")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_COMMON_DIR")
        .env_remove("GIT_ALTERNATE_OBJECT_DIRECTORIES")
        .env_remove("GIT_OBJECT_DIRECTORY")
        .env_remove("GIT_CONFIG_PARAMETERS")
        .env_remove("GIT_SSH")
        .env_remove("GIT_SSH_COMMAND")
        .env_remove("GIT_ASKPASS")
        .env_remove("SSH_ASKPASS")
        .env_remove("SSH_AUTH_SOCK")
        .env_remove("SSH_AGENT_PID")
        .env_remove("GIT_SSL_NO_VERIFY")
        .env_remove("GIT_SSL_CERT")
        .env_remove("GIT_SSL_KEY")
        .env_remove("GIT_SSL_CAPATH")
        .env_remove("GIT_PROXY_COMMAND")
        .env_remove("GIT_CURL_VERBOSE")
        .env_remove("CURL_CA_BUNDLE")
        .env_remove("SSL_CERT_FILE")
        .env_remove("SSL_CERT_DIR");
    for proxy in [
        "HTTP_PROXY",
        "HTTPS_PROXY",
        "ALL_PROXY",
        "http_proxy",
        "https_proxy",
        "all_proxy",
    ] {
        command.env_remove(proxy);
    }
    if let Some(config) = &policy.ssh_config {
        let known = policy
            .ssh_known_hosts
            .as_deref()
            .ok_or(CoreError::InvalidRequest {
                reason: "SSH host-key trust file is required".into(),
            })?;
        command.env("GIT_SSH_COMMAND", format!("ssh -F {} -oHostName={} -oPort={} -oUserKnownHostsFile={} -oGlobalKnownHostsFile=/dev/null -oIdentityAgent=none -oIdentitiesOnly=yes -oBatchMode=yes -oStrictHostKeyChecking=yes -oPermitLocalCommand=no", shell_quote(config), shell_quote(Path::new(endpoint.host_str().unwrap())), endpoint.port().unwrap_or(22), shell_quote(known)));
    }
    if let Some(ca) = &policy.ca_bundle {
        command.env("GIT_SSL_CAINFO", ca);
    }
    let output = run_limited_checked(&mut command, &policy.limits, max_stdout, || {
        if object_store_size(repo)? > policy.limits.max_fetched_storage_bytes {
            return Err(CoreError::InvalidRequest {
                reason: "managed Git storage exceeded its configured quota".into(),
            });
        }
        Ok(())
    })?;
    if !output.status.success() {
        return Err(CoreError::InvalidRequest {
            reason:
                "configured remote Git operation failed; no local checkout fallback was attempted"
                    .into(),
        });
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
fn object_store_size(repo: &Path) -> CoreResult<u64> {
    // Include pack temporaries, indexes, refs and metadata, not only final objects.
    let mut pending = vec![repo.to_path_buf()];
    let mut total = 0u64;
    while let Some(path) = pending.pop() {
        for entry in std::fs::read_dir(path).map_err(|_| CoreError::InvalidRequest {
            reason: "managed Git object store could not be inspected".into(),
        })? {
            let entry = entry.map_err(|_| CoreError::InvalidRequest {
                reason: "managed Git object store could not be inspected".into(),
            })?;
            let meta =
                std::fs::symlink_metadata(entry.path()).map_err(|_| CoreError::InvalidRequest {
                    reason: "managed Git object store could not be inspected".into(),
                })?;
            if meta.file_type().is_symlink() {
                return Err(CoreError::InvalidRequest {
                    reason: "managed Git object store contains an unexpected symlink".into(),
                });
            }
            if meta.is_dir() {
                pending.push(entry.path());
            } else if meta.is_file() {
                total = total
                    .checked_add(meta.len())
                    .ok_or(CoreError::InvalidRequest {
                        reason: "managed Git object store size overflow".into(),
                    })?;
            } else {
                return Err(CoreError::InvalidRequest {
                    reason: "managed Git object store contains an unsupported file".into(),
                });
            }
        }
    }
    Ok(total)
}
fn acquire_attempt(
    remote: &str,
    branch_ref: &str,
    url: &Url,
    policy: &RemotePolicy,
    attempt_root: &Path,
) -> CoreResult<RemoteSnapshot> {
    let repo = attempt_root.join("objects.git");
    controlled_git(
        attempt_root,
        &["init".into(), "--bare".into(), repo.display().to_string()],
        policy,
        4096,
        url,
    )?;
    let refspec = format!("+{branch_ref}:{branch_ref}");
    let args = vec![
        "-c".into(),
        "core.hooksPath=/dev/null".into(),
        "-c".into(),
        "core.attributesFile=/dev/null".into(),
        "-c".into(),
        "core.pager=cat".into(),
        "-c".into(),
        "fetch.unpackLimit=0".into(),
        "-c".into(),
        "http.followRedirects=false".into(),
        "-c".into(),
        "protocol.allow=never".into(),
        "-c".into(),
        format!("protocol.{}.allow=always", url.scheme()),
        "--git-dir".into(),
        repo.display().to_string(),
        "fetch".into(),
        "--no-tags".into(),
        "--no-write-fetch-head".into(),
        "--no-recurse-submodules".into(),
        "--depth=1".into(),
        remote.into(),
        refspec,
    ];
    controlled_git(attempt_root, &args, policy, 4096, url)?;
    if object_store_size(&repo)? > policy.limits.max_fetched_storage_bytes {
        return Err(CoreError::InvalidRequest {
            reason: "fetched Git object store exceeded its configured storage quota".into(),
        });
    }
    let (commit, files) = acquire_local_branch_with_limits(&repo, branch_ref, &policy.limits)?;
    Ok(RemoteSnapshot {
        commit,
        files,
        managed_repo: repo,
    })
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
    let existed = managed_root.exists();
    std::fs::create_dir_all(managed_root).map_err(|_| CoreError::Adapter {
        adapter: "engram-ingest-git".into(),
        message: "managed Git storage is unavailable".into(),
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let metadata = std::fs::metadata(managed_root).map_err(|_| CoreError::InvalidRequest {
            reason: "managed Git storage is unavailable".into(),
        })?;
        if existed && metadata.mode() & 0o077 != 0 {
            return Err(CoreError::PolicyDenied {
                reason: "managed Git storage must be private to its owner".into(),
            });
        }
        if !existed {
            std::fs::set_permissions(managed_root, std::fs::Permissions::from_mode(0o700))
                .map_err(|_| CoreError::PolicyDenied {
                    reason: "managed Git storage permissions could not be secured".into(),
                })?;
        }
    }
    let attempt_root = managed_root.join(format!("attempt-{}", new_attempt_id()));
    std::fs::create_dir(&attempt_root).map_err(|_| CoreError::Adapter {
        adapter: "engram-ingest-git".into(),
        message: "attempt-private Git storage could not be created".into(),
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&attempt_root, std::fs::Permissions::from_mode(0o700)).map_err(
            |_| CoreError::PolicyDenied {
                reason: "attempt-private Git storage permissions could not be secured".into(),
            },
        )?;
    }
    let result = acquire_attempt(remote, branch_ref, &url, policy, &attempt_root);
    if result.is_err() {
        let _ = std::fs::remove_dir_all(&attempt_root);
    }
    result
}
