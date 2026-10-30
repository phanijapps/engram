//! Git v0 smart transport with application-owned pack storage, not `git fetch`/`index-pack`.
// clippy roll 2026-10: fixture setup mutates only the exercised default limit.
#![allow(clippy::field_reassign_with_default)]
use super::{
    CommittedFile, ResolvedCommit,
    acquire::{RemotePolicy, RemoteSnapshot},
    bounded_process::run_limited_input,
    portable_pack::{GitObject, PackLimits, decode_pack},
};
use engram_knowledge::{CoreError, CoreResult};
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::Command,
    time::Instant,
};
use url::Url;
fn invalid() -> CoreError {
    CoreError::InvalidRequest {
        reason: "remote Git protocol or acquisition resource limit rejected".into(),
    }
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn unhex(s: &[u8]) -> CoreResult<Vec<u8>> {
    if !matches!(s.len(), 40 | 64) {
        return Err(invalid());
    }
    s.as_chunks::<2>()
        .0
        .iter()
        .map(|p| {
            std::str::from_utf8(p)
                .ok()
                .and_then(|s| u8::from_str_radix(s, 16).ok())
                .ok_or_else(invalid)
        })
        .collect()
}
fn packet<'a>(bytes: &'a [u8], at: &mut usize) -> CoreResult<Option<&'a [u8]>> {
    let end = at.checked_add(4).ok_or_else(invalid)?;
    let size = std::str::from_utf8(bytes.get(*at..end).ok_or_else(invalid)?)
        .ok()
        .and_then(|s| usize::from_str_radix(s, 16).ok())
        .ok_or_else(invalid)?;
    *at = end;
    if size == 0 {
        return Ok(None);
    }
    if !(4..=65520).contains(&size) {
        return Err(invalid());
    }
    let end = at.checked_add(size - 4).ok_or_else(invalid)?;
    let out = bytes.get(*at..end).ok_or_else(invalid)?;
    *at = end;
    Ok(Some(out))
}
fn pkt(out: &mut Vec<u8>, bytes: &[u8]) {
    out.extend(format!("{:04x}", bytes.len() + 4).as_bytes());
    out.extend(bytes);
}
fn select_ref(
    advertisement: &[u8],
    branch: &str,
    policy: &RemotePolicy,
    started: Instant,
) -> CoreResult<(Vec<u8>, Vec<String>)> {
    let (mut at, mut count) = (0, 0usize);
    let mut selected = None;
    let mut capabilities = Vec::new();
    let mut names = std::collections::BTreeSet::new();
    let mut width = None;
    let (mut first, mut service_flush, mut final_flush) = (true, false, false);
    while at < advertisement.len() {
        if started.elapsed() >= policy.limits.subprocess_timeout {
            return Err(https_deadline());
        }
        let line = packet(advertisement, &mut at)?;
        if service_flush {
            if line.is_some() {
                return Err(invalid());
            }
            service_flush = false;
            continue;
        }
        let Some(line) = line else {
            final_flush = true;
            break;
        };
        if first && line == b"# service=git-upload-pack\n" {
            first = false;
            service_flush = true;
            continue;
        }
        first = false;
        count = count.checked_add(1).ok_or_else(invalid)?;
        if count > policy.limits.max_advertised_refs {
            return Err(invalid());
        }
        let line = line.strip_suffix(b"\n").unwrap_or(line);
        let mut parts = line.splitn(2, |b| *b == 0);
        let reference = parts.next().ok_or_else(invalid)?;
        if let Some(caps) = parts.next() {
            if count != 1 || caps.iter().any(|b| *b != b' ' && !(33..=126).contains(b)) {
                return Err(invalid());
            }
            for cap in std::str::from_utf8(caps)
                .map_err(|_| invalid())?
                .split_whitespace()
            {
                if capabilities.len() >= 128
                    || cap.len() > 1024
                    || capabilities.iter().any(|c| c == cap)
                {
                    return Err(invalid());
                }
                capabilities.push(cap.to_owned());
            }
        }
        let space = reference
            .iter()
            .position(|b| *b == b' ')
            .ok_or_else(invalid)?;
        let oid = unhex(&reference[..space])?;
        let name = &reference[space + 1..];
        if name.is_empty()
            || name.len() > 4096
            || name.iter().any(|b| *b <= 32 || *b == 127)
            || !names.insert(name.to_vec())
        {
            return Err(invalid());
        }
        if width.is_some_and(|w| w != oid.len()) {
            return Err(invalid());
        }
        width = Some(oid.len());
        if name == branch.as_bytes()
            && (oid.iter().all(|b| *b == 0) || selected.replace(oid).is_some())
        {
            return Err(invalid());
        }
    }
    if !final_flush || service_flush || at != advertisement.len() {
        return Err(invalid());
    }
    let mut format = None;
    for cap in &capabilities {
        if let Some(value) = cap.strip_prefix("object-format=")
            && (!matches!(value, "sha1" | "sha256") || format.replace(value).is_some())
        {
            return Err(invalid());
        }
    }
    if matches!(
        (width, format),
        (Some(20), Some("sha256")) | (Some(32), Some("sha1")) | (Some(32), None)
    ) {
        return Err(invalid());
    }
    Ok((selected.ok_or_else(invalid)?, capabilities))
}
fn response_pack<'a>(
    response: &'a [u8],
    oid: &[u8],
    shallow_requested: bool,
    policy: &RemotePolicy,
    started: Instant,
) -> CoreResult<&'a [u8]> {
    let mut at = 0;
    let (mut shallow, mut flushed, mut nak) = (false, false, false);
    while !response.get(at..).is_some_and(|b| b.starts_with(b"PACK")) {
        if started.elapsed() >= policy.limits.subprocess_timeout {
            return Err(https_deadline());
        }
        match packet(response, &mut at)? {
            None => {
                if !shallow || flushed || nak {
                    return Err(invalid());
                }
                flushed = true;
            }
            Some(b"NAK\n") => {
                if nak || (shallow && !flushed) {
                    return Err(invalid());
                }
                nak = true;
            }
            Some(line) => {
                if !shallow_requested || shallow || flushed || nak {
                    return Err(invalid());
                }
                let value = line.strip_prefix(b"shallow ").ok_or_else(invalid)?;
                if unhex(value.strip_suffix(b"\n").unwrap_or(value))? != oid {
                    return Err(invalid());
                }
                shallow = true;
            }
        }
    }
    if !nak {
        return Err(invalid());
    }
    Ok(&response[at..])
}
fn bounded_read(reader: impl Read, limit: usize) -> CoreResult<Vec<u8>> {
    let mut out = Vec::new();
    reader
        .take((limit as u64).checked_add(1).ok_or_else(invalid)?)
        .read_to_end(&mut out)
        .map_err(|_| invalid())?;
    if out.len() > limit {
        return Err(invalid());
    }
    Ok(out)
}
fn https(
    remote: &Url,
    policy: &RemotePolicy,
    body: Option<Vec<u8>>,
    limit: usize,
) -> CoreResult<Vec<u8>> {
    https_configured(remote, policy, body, limit, |builder| builder)
}
fn https_configured(
    remote: &Url,
    policy: &RemotePolicy,
    body: Option<Vec<u8>>,
    limit: usize,
    configure: impl FnOnce(reqwest::ClientBuilder) -> reqwest::ClientBuilder + Send + 'static,
) -> CoreResult<Vec<u8>> {
    let remote = remote.clone();
    let policy = policy.clone();
    // Every network/DNS task belongs to this current-thread runtime; no getaddrinfo or detached worker.
    std::thread::spawn(move || {
        let ca = bounded_read(
            std::fs::File::open(policy.ca_bundle.as_ref().ok_or_else(invalid)?)
                .map_err(|_| invalid())?,
            1024 * 1024,
        )?;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|_| invalid())?;
        let outcome = runtime.block_on(async move {
            let timeout = policy.limits.subprocess_timeout;
            let operation = async move {
                let mut builder = reqwest::Client::builder()
                    .https_only(true)
                    .no_proxy()
                    .redirect(reqwest::redirect::Policy::none())
                    .hickory_dns(true)
                    .timeout(policy.limits.subprocess_timeout)
                    .tls_built_in_root_certs(false);
                let certs = reqwest::Certificate::from_pem_bundle(&ca).map_err(|_| invalid())?;
                if certs.is_empty() {
                    return Err(invalid());
                }
                for cert in certs {
                    builder = builder.add_root_certificate(cert);
                }
                let client = configure(builder).build().map_err(|_| invalid())?;
                let base = remote.as_str().trim_end_matches('/');
                let request = if let Some(body) = body {
                    client
                        .post(format!("{base}/git-upload-pack"))
                        .header("Content-Type", "application/x-git-upload-pack-request")
                        .header("Accept", "application/x-git-upload-pack-result")
                        .body(body)
                } else {
                    client
                        .get(format!("{base}/info/refs?service=git-upload-pack"))
                        .header("Accept", "application/x-git-upload-pack-advertisement")
                };
                let mut response = request
                    .header("Git-Protocol", "version=0")
                    .send()
                    .await
                    .map_err(network_error)?;
                if !response.status().is_success()
                    || response
                        .content_length()
                        .is_some_and(|size| size > limit as u64)
                {
                    return Err(invalid());
                }
                let mut out = Vec::new();
                while let Some(chunk) = response.chunk().await.map_err(network_error)? {
                    if chunk.len() > limit.saturating_sub(out.len()) {
                        return Err(invalid());
                    }
                    out.extend_from_slice(&chunk);
                }
                Ok(out)
            };
            tokio::time::timeout(timeout, operation)
                .await
                .map_err(|_| https_deadline())?
        });
        // Current-thread runtime drop cancels resolver and connection tasks and closes their sockets.
        drop(runtime);
        outcome
    })
    .join()
    .map_err(|_| invalid())?
}
fn https_deadline() -> CoreError {
    CoreError::InvalidRequest {
        reason: "remote Git HTTPS deadline exceeded".into(),
    }
}
fn network_error(error: reqwest::Error) -> CoreError {
    if error.is_timeout() {
        https_deadline()
    } else {
        invalid()
    }
}
fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}
fn remote_path(url: &Url) -> CoreResult<String> {
    let mut out = Vec::new();
    let bytes = url.path().as_bytes();
    let mut at = 0;
    while at < bytes.len() {
        let b = if bytes[at] == b'%' {
            let s = std::str::from_utf8(bytes.get(at + 1..at + 3).ok_or_else(invalid)?)
                .map_err(|_| invalid())?;
            at += 3;
            u8::from_str_radix(s, 16).map_err(|_| invalid())?
        } else {
            let b = bytes[at];
            at += 1;
            b
        };
        if b == 0 || b == b'\n' || b == b'\r' {
            return Err(invalid());
        }
        out.push(b);
    }
    String::from_utf8(out).map_err(|_| invalid())
}
fn known_hosts_option(path: &Path) -> CoreResult<String> {
    let absolute = std::path::absolute(path).map_err(|_| invalid())?;
    let value = absolute
        .to_str()
        .ok_or_else(invalid)?
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('%', "%%");
    Ok(format!("-oUserKnownHostsFile=\"{value}\""))
}
fn ssh(
    remote: &Url,
    policy: &RemotePolicy,
    body: Option<Vec<u8>>,
    limit: usize,
    cwd: &Path,
) -> CoreResult<Vec<u8>> {
    let mut command = Command::new("ssh");
    command
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .current_dir(cwd)
        .arg("-F")
        .arg(policy.ssh_config.as_ref().ok_or_else(invalid)?)
        .args([
            "-oBatchMode=yes",
            "-oStrictHostKeyChecking=yes",
            "-oUpdateHostKeys=no",
            "-oIdentityAgent=none",
            "-oIdentitiesOnly=yes",
            "-oIdentityFile=none",
            "-oPermitLocalCommand=no",
            "-oControlMaster=no",
            "-oControlPath=none",
            "-oProxyCommand=none",
            "-oProxyJump=none",
            "-oRequestTTY=no",
        ])
        .arg(known_hosts_option(
            policy.ssh_known_hosts.as_ref().ok_or_else(invalid)?,
        )?)
        .arg(if cfg!(windows) {
            "-oGlobalKnownHostsFile=NUL"
        } else {
            "-oGlobalKnownHostsFile=/dev/null"
        })
        .arg("-p")
        .arg(remote.port().unwrap_or(22).to_string());
    if !remote.username().is_empty() {
        command.arg("-l").arg(remote.username());
    }
    let mode = if body.is_some() {
        "--stateless-rpc"
    } else {
        "--advertise-refs"
    };
    command
        .arg("--")
        .arg(remote.host_str().ok_or_else(invalid)?)
        .arg(format!(
            "git-upload-pack {mode} {}",
            shell_quote(&remote_path(remote)?)
        ));
    #[cfg(windows)]
    for variable in ["SYSTEMROOT", "WINDIR"] {
        if let Some(value) = std::env::var_os(variable) {
            command.env(variable, value);
        }
    }
    let output = run_limited_input(&mut command, &policy.limits, limit, body, || Ok(()))?;
    if !output.status.success() {
        return Err(invalid());
    }
    Ok(output.stdout)
}
fn exchange(
    remote: &Url,
    policy: &RemotePolicy,
    body: Option<Vec<u8>>,
    limit: usize,
    cwd: &Path,
) -> CoreResult<Vec<u8>> {
    match remote.scheme() {
        "https" => https(remote, policy, body, limit),
        "ssh" => ssh(remote, policy, body, limit, cwd),
        _ => Err(invalid()),
    }
}
fn source_files(
    objects: &BTreeMap<Vec<u8>, GitObject>,
    oid: &[u8],
    policy: &RemotePolicy,
    started: Instant,
) -> CoreResult<Vec<CommittedFile>> {
    let commit = objects
        .get(oid)
        .filter(|o| o.kind == 1)
        .ok_or_else(invalid)?;
    // Git's tree identity is the first terminated header, never a message line.
    // Other commit bytes need not be UTF-8 to identify its exact retained tree.
    let end = commit
        .bytes
        .iter()
        .position(|b| *b == b'\n')
        .ok_or_else(invalid)?;
    let tree_line = commit.bytes[..end]
        .strip_prefix(b"tree ")
        .ok_or_else(invalid)?;
    if policy.limits.max_git_objects == 0 {
        return Err(CoreError::InvalidRequest {
            reason: "Git tree traversal budget exceeded".into(),
        });
    }
    let tree = unhex(tree_line)?;
    let mut pending = vec![(tree, PathBuf::new(), 0usize)];
    let mut total = 0usize;
    let mut files = Vec::new();
    let mut scheduled = 1usize;
    let mut entries = 0usize;
    if scheduled > policy.limits.max_git_objects {
        return Err(invalid());
    }
    while let Some((oid, prefix, depth)) = pending.pop() {
        if depth > policy.limits.max_tree_depth {
            return Err(CoreError::InvalidRequest {
                reason: "Git tree depth exceeded".into(),
            });
        }
        let tree = objects
            .get(&oid)
            .filter(|o| o.kind == 2)
            .ok_or_else(invalid)?;
        let mut at = 0;
        let mut names = std::collections::BTreeSet::new();
        while at < tree.bytes.len() {
            entries = entries.checked_add(1).ok_or_else(invalid)?;
            if entries > policy.limits.max_tree_entries {
                return Err(CoreError::InvalidRequest {
                    reason: "Git tree-entry work budget exceeded".into(),
                });
            }
            if started.elapsed() >= policy.limits.subprocess_timeout {
                return Err(invalid());
            }
            let rest = &tree.bytes[at..];
            let space = rest.iter().position(|b| *b == b' ').ok_or_else(invalid)?;
            let nul = rest.iter().position(|b| *b == 0).ok_or_else(invalid)?;
            if nul <= space {
                return Err(invalid());
            }
            let mode = std::str::from_utf8(&rest[..space]).map_err(|_| invalid())?;
            let name = std::str::from_utf8(&rest[space + 1..nul]).map_err(|_| invalid())?;
            super::safety::validate_retained_component(name)?;
            if !names.insert(name) {
                return Err(invalid());
            }
            let end = nul
                .checked_add(1)
                .and_then(|x| x.checked_add(oid.len()))
                .ok_or_else(invalid)?;
            let child = rest.get(nul + 1..end).ok_or_else(invalid)?;
            at = at.checked_add(end).ok_or_else(invalid)?;
            if mode == "40000" {
                if depth >= policy.limits.max_tree_depth {
                    return Err(CoreError::InvalidRequest {
                        reason: "Git tree depth exceeded".into(),
                    });
                }
                if scheduled >= policy.limits.max_git_objects {
                    return Err(CoreError::InvalidRequest {
                        reason: "Git tree traversal budget exceeded".into(),
                    });
                }
                objects
                    .get(child)
                    .filter(|o| o.kind == 2)
                    .ok_or_else(invalid)?;
                scheduled += 1;
            }
            // Admission and referenced-directory validation precede path/OID allocation.
            let path = prefix.join(name);
            if path.as_os_str().len() > 4096 {
                return Err(invalid());
            }
            if mode == "40000" {
                pending.push((child.to_vec(), path, depth + 1));
                continue;
            }
            if !matches!(mode, "100644" | "100755") {
                return Err(invalid());
            }
            let blob = objects
                .get(child)
                .filter(|o| o.kind == 3)
                .ok_or_else(invalid)?;
            total = total.checked_add(blob.bytes.len()).ok_or_else(invalid)?;
            if files.len() >= policy.limits.max_source_files
                || blob.bytes.len() > policy.limits.max_file_bytes
                || total > policy.limits.max_snapshot_bytes
                || blob
                    .bytes
                    .starts_with(b"version https://git-lfs.github.com/spec/v1\n")
                || blob
                    .bytes
                    .starts_with(b"version https://git-lfs.github.com/spec/v1\r\n")
            {
                return Err(invalid());
            }
            files.push(CommittedFile {
                path,
                oid: hex(child),
                mode: mode.into(),
                bytes: blob.bytes.clone(),
            });
        }
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    if files.windows(2).any(|p| p[0].path == p[1].path) {
        return Err(invalid());
    }
    Ok(files)
}

pub(super) fn acquire(
    remote: &Url,
    branch: &str,
    policy: &RemotePolicy,
    root: &super::private_storage::PrivateDirectory,
) -> CoreResult<RemoteSnapshot> {
    let started = Instant::now();
    let advertisement = exchange(
        remote,
        policy,
        None,
        policy.limits.max_stdout_bytes,
        &root.path,
    )?;
    let (oid, caps) = select_ref(&advertisement, branch, policy, started)?;
    drop(advertisement);
    let mut requested = Vec::new();
    if caps.iter().any(|c| c == "ofs-delta") {
        requested.push("ofs-delta");
    }
    if caps.iter().any(|c| c == "no-progress") {
        requested.push("no-progress");
    }
    let format = if oid.len() == 20 { "sha1" } else { "sha256" };
    let format_cap = format!("object-format={format}");
    if oid.len() == 32 && !caps.contains(&format_cap) {
        return Err(invalid());
    }
    if caps.contains(&format_cap) {
        requested.push(&format_cap);
    }
    let mut request = Vec::new();
    pkt(
        &mut request,
        format!("want {} {}\n", hex(&oid), requested.join(" ")).as_bytes(),
    );
    if caps.iter().any(|c| c == "shallow") {
        pkt(&mut request, b"deepen 1\n");
    }
    request.extend(b"0000");
    pkt(&mut request, b"done\n");
    let mut remaining_policy = policy.clone();
    remaining_policy.limits.subprocess_timeout = policy
        .limits
        .subprocess_timeout
        .checked_sub(started.elapsed())
        .filter(|remaining| !remaining.is_zero())
        .ok_or_else(invalid)?;
    let response = exchange(
        remote,
        &remaining_policy,
        Some(request),
        policy.limits.max_remote_transport_bytes,
        &root.path,
    )?;
    let pack = response_pack(
        &response,
        &oid,
        caps.iter().any(|c| c == "shallow"),
        policy,
        started,
    )?;
    let budget = usize::try_from(policy.limits.max_fetched_storage_bytes).map_err(|_| invalid())?;
    // A complete owned write-set: one pack, no bare-repository files, indexes, alternates or Git subprocess import.
    if pack.len() > budget {
        return Err(invalid());
    }
    let objects = decode_pack(
        pack,
        oid.len(),
        &PackLimits {
            wire_bytes: policy.limits.max_remote_transport_bytes,
            inflated_bytes: policy.limits.max_inflated_git_bytes,
            objects: policy.limits.max_git_objects,
            delta_depth: policy.limits.max_delta_depth,
            timeout: policy
                .limits
                .subprocess_timeout
                .checked_sub(started.elapsed())
                .ok_or_else(invalid)?,
        },
    )?;
    let files = source_files(&objects, &oid, policy, started)?;
    if started.elapsed() >= policy.limits.subprocess_timeout {
        return Err(invalid());
    }
    let mut out = root
        .create_file(Path::new("objects.pack"))
        .map_err(|_| invalid())?;
    out.write_all(pack).map_err(|_| invalid())?;
    out.sync_all().map_err(|_| invalid())?;
    drop(out);
    root.sync().map_err(|_| invalid())?;
    Ok(RemoteSnapshot {
        commit: ResolvedCommit {
            branch_ref: branch.into(),
            oid: hex(&oid),
            object_format: format.into(),
        },
        files,
        managed_repo: root.path.clone(),
    })
}

#[cfg(test)]
mod recovery_tests {
    use super::*;
    use reqwest::dns::{Name, Resolve, Resolving};
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    use std::time::Duration;
    fn test_policy(path: &Path) -> RemotePolicy {
        let mut limits = super::super::GitResourceLimits::default();
        limits.subprocess_timeout = Duration::from_millis(200);
        RemotePolicy {
            endpoint_allowlist: vec![],
            ssh_config: None,
            ssh_known_hosts: None,
            ca_bundle: Some(path.to_path_buf()),
            limits,
        }
    }
    struct PendingDns {
        active: Arc<AtomicUsize>,
        started: Arc<AtomicUsize>,
    }
    struct Active(Arc<AtomicUsize>);
    impl Drop for Active {
        fn drop(&mut self) {
            self.0.fetch_sub(1, Ordering::SeqCst);
        }
    }
    impl Resolve for PendingDns {
        fn resolve(&self, name: Name) -> Resolving {
            assert_eq!(name.as_str(), "stalled.invalid");
            let active = self.active.clone();
            let started = self.started.clone();
            Box::pin(async move {
                active.fetch_add(1, Ordering::SeqCst);
                started.fetch_add(1, Ordering::SeqCst);
                let _active = Active(active);
                futures::future::pending().await
            })
        }
    }
    #[test]
    fn task_008_recovery_dns_timeout_cancels_owned_future_and_runtime() {
        let dir = tempfile::tempdir().unwrap();
        let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
        let ca = dir.path().join("ca.pem");
        std::fs::write(&ca, cert.cert.pem()).unwrap();
        let active = Arc::new(AtomicUsize::new(0));
        let started = Arc::new(AtomicUsize::new(0));
        for _ in 0..4 {
            let resolver = Arc::new(PendingDns {
                active: active.clone(),
                started: started.clone(),
            });
            let began = Instant::now();
            let error = https_configured(
                &Url::parse("https://stalled.invalid/repo").unwrap(),
                &test_policy(&ca),
                None,
                64,
                move |b| b.dns_resolver(resolver),
            )
            .unwrap_err();
            assert!(
                matches!(error,CoreError::InvalidRequest {reason} if reason=="remote Git HTTPS deadline exceeded")
            );
            assert!(began.elapsed() < Duration::from_secs(3));
            assert_eq!(
                active.load(Ordering::SeqCst),
                0,
                "request returns only after resolver future is dropped"
            );
        }
        assert_eq!(started.load(Ordering::SeqCst), 4);
    }
    fn stall(body: bool, host: &str) -> (CoreError, bool) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio_rustls::{TlsAcceptor, rustls};
        let dir = tempfile::tempdir().unwrap();
        let key = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
        let ca = dir.path().join("ca.pem");
        std::fs::write(&ca, key.cert.pem()).unwrap();
        let config = rustls::ServerConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(
            vec![key.cert.der().clone()],
            rustls::pki_types::PrivatePkcs8KeyDer::from(key.signing_key.serialize_der()).into(),
        )
        .unwrap();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        listener.set_nonblocking(true).unwrap();
        let (stop, stopped) = tokio::sync::oneshot::channel();
        let (ready, received) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            runtime.block_on(async move {
                let listener=tokio::net::TcpListener::from_std(listener).unwrap();
                tokio::select! {
                    _=async {
                        let (socket,_)=listener.accept().await.unwrap();
                        if body {
                            let Ok(mut tls)=TlsAcceptor::from(Arc::new(config)).accept(socket).await else {return;};
                            let mut header=Vec::new();let mut b=[0u8;1];
                            while !header.ends_with(b"\r\n\r\n") {if tls.read(&mut b).await.unwrap()==0{return;}header.push(b[0]);assert!(header.len()<8192);}
                            tls.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 10\r\n\r\nxyz").await.unwrap();tls.flush().await.unwrap();
                            ready.send(()).unwrap();std::future::pending::<()>().await;
                        } else {let _socket=socket;ready.send(()).unwrap();std::future::pending::<()>().await;}
                    }=>{},
                    _=stopped=>{},
                }
            });
        });
        let host = host.to_owned();
        let remote = Url::parse(&format!("https://{host}:{}/repo", address.port())).unwrap();
        let began = Instant::now();
        let error = https_configured(&remote, &test_policy(&ca), None, 64, move |b| {
            b.resolve_to_addrs(&host, &[address])
        })
        .unwrap_err();
        assert!(began.elapsed() < Duration::from_secs(3));
        let reached = received.try_recv().is_ok();
        let _ = stop.send(());
        worker.join().unwrap();
        (error, reached)
    }
    #[test]
    fn task_008_recovery_tls_and_body_stalls_close_owned_io() {
        for body in [false, true] {
            let (error, reached) = stall(body, "localhost");
            assert!(reached, "fixture must reach actual TLS/body stall");
            assert!(
                matches!(error,CoreError::InvalidRequest {reason} if reason=="remote Git HTTPS deadline exceeded")
            );
        }
    }
    #[test]
    fn task_008_recovery_address_override_preserves_hostname_validation() {
        let (error, _) = stall(true, "wrong-host.invalid");
        assert!(
            matches!(error,CoreError::InvalidRequest {reason} if reason=="remote Git protocol or acquisition resource limit rejected")
        );
    }
}

#[cfg(test)]
mod tree_recovery_tests {
    use super::*;
    fn row(mode: &str, name: &str, id: u8) -> Vec<u8> {
        let mut v = format!("{mode} {name}\0").into_bytes();
        v.extend([id; 20]);
        v
    }
    fn records(tree: Vec<u8>) -> BTreeMap<Vec<u8>, GitObject> {
        BTreeMap::from([
            (
                vec![1; 20],
                GitObject {
                    kind: 1,
                    bytes: format!("tree {}\n", hex(&[2; 20])).into_bytes(),
                },
            ),
            (
                vec![2; 20],
                GitObject {
                    kind: 2,
                    bytes: tree,
                },
            ),
            (
                vec![3; 20],
                GitObject {
                    kind: 2,
                    bytes: vec![],
                },
            ),
            (
                vec![4; 20],
                GitObject {
                    kind: 3,
                    bytes: b"data".to_vec(),
                },
            ),
        ])
    }
    fn policy() -> RemotePolicy {
        RemotePolicy {
            endpoint_allowlist: vec![],
            ssh_config: None,
            ssh_known_hosts: None,
            ca_bundle: None,
            limits: Default::default(),
        }
    }
    fn check(
        objects: &BTreeMap<Vec<u8>, GitObject>,
        policy: &RemotePolicy,
    ) -> CoreResult<Vec<CommittedFile>> {
        source_files(objects, &[1; 20], policy, Instant::now())
    }
    #[test]
    fn task_008_recovery_tree_queue_and_entry_admission_precedes_allocation() {
        let mut wide = Vec::new();
        for n in 0..10000 {
            wide.extend(row("40000", &format!("d{n}"), 3));
        }
        let objects = records(wide);
        let mut policy = policy();
        policy.limits.max_git_objects = 2;
        assert!(
            matches!(check(&objects,&policy),Err(CoreError::InvalidRequest {reason}) if reason=="Git tree traversal budget exceeded")
        );
        policy.limits.max_git_objects = 10001;
        policy.limits.max_tree_entries = 10000;
        assert!(check(&objects, &policy).unwrap().is_empty());
        policy.limits.max_tree_entries = 9999;
        assert!(
            matches!(check(&objects,&policy),Err(CoreError::InvalidRequest {reason}) if reason=="Git tree-entry work budget exceeded")
        );
    }
    #[test]
    fn task_008_recovery_tree_missing_wrong_type_duplicates_and_depth() {
        for tree in [
            row("40000", "missing", 99),
            row("40000", "wrong", 4),
            [row("100644", "same", 4), row("100644", "same", 4)].concat(),
        ] {
            assert!(check(&records(tree), &policy()).is_err());
        }
        let objects = records([row("100644", "x", 4), row("100644", "y", 4)].concat());
        assert_eq!(check(&objects, &policy()).unwrap().len(), 2);
        let mut objects = records(row("40000", "a", 3));
        objects.get_mut(&vec![3; 20]).unwrap().bytes = row("40000", "b", 5);
        objects.insert(
            vec![5; 20],
            GitObject {
                kind: 2,
                bytes: row("100644", "leaf", 4),
            },
        );
        let mut p = policy();
        p.limits.max_tree_depth = 1;
        assert!(
            matches!(check(&objects,&p),Err(CoreError::InvalidRequest {reason}) if reason=="Git tree depth exceeded")
        );
        p.limits.max_tree_depth = 2;
        assert_eq!(check(&objects, &p).unwrap()[0].bytes, b"data");
    }
    #[test]
    fn task_008_commit_tree_header_cannot_be_forged_in_message_text() {
        let mut objects = records(row("100644", "file", 4));
        objects.get_mut(&vec![1; 20]).unwrap().bytes =
            format!("author fixture\n\ntree {}\n", hex(&[2; 20])).into_bytes();
        assert!(check(&objects, &policy()).is_err());
        objects.get_mut(&vec![1; 20]).unwrap().bytes =
            format!("tree {}", hex(&[2; 20])).into_bytes();
        assert!(check(&objects, &policy()).is_err());
        objects.get_mut(&vec![1; 20]).unwrap().bytes =
            format!("tree {}\n\n", hex(&[2; 20])).into_bytes();
        objects.get_mut(&vec![1; 20]).unwrap().bytes.push(255);
        assert_eq!(check(&objects, &policy()).unwrap()[0].bytes, b"data");
    }
    #[test]
    fn task_008_recovery_reserved_components_are_not_silently_renamed() {
        for name in [
            "CON",
            "con.txt",
            "LPT¹.txt",
            "NUL",
            "tail.",
            "tail ",
            "x:y",
            "a\\b",
            "a?b",
            ".git",
            "..",
        ] {
            assert!(
                check(&records(row("100644", name, 4)), &policy()).is_err(),
                "{name}"
            );
        }
        assert_eq!(
            check(&records(row("100644", "éxact.rs", 4)), &policy()).unwrap()[0].bytes,
            b"data"
        );
    }
}

#[cfg(test)]
mod protocol_recovery_tests {
    use super::*;
    fn policy() -> RemotePolicy {
        RemotePolicy {
            endpoint_allowlist: vec![],
            ssh_config: None,
            ssh_known_hosts: None,
            ca_bundle: None,
            limits: Default::default(),
        }
    }
    fn ad(oid: &str, caps: &str) -> Vec<u8> {
        let mut out = Vec::new();
        pkt(
            &mut out,
            format!("{oid} refs/heads/main\0{caps}\n").as_bytes(),
        );
        out.extend(b"0000");
        out
    }
    #[test]
    fn task_008_advertisement_truncation_lengths_and_duplicate_refs_fail_closed() {
        let valid = ad(&"1".repeat(40), "shallow object-format=sha1");
        let p = policy();
        assert!(select_ref(&valid, "refs/heads/main", &p, Instant::now()).is_ok());
        for end in 0..valid.len() {
            assert!(
                select_ref(&valid[..end], "refs/heads/main", &p, Instant::now()).is_err(),
                "truncation {end}"
            );
        }
        for bad in [
            b"zzzz".as_slice(),
            b"0001",
            b"0002",
            b"0003",
            b"ffff",
            b"00040000",
        ] {
            assert!(select_ref(bad, "refs/heads/main", &p, Instant::now()).is_err());
        }
        let mut duplicate = valid[..valid.len() - 4].to_vec();
        pkt(
            &mut duplicate,
            format!("{} refs/heads/main\n", "2".repeat(40)).as_bytes(),
        );
        duplicate.extend(b"0000");
        assert!(select_ref(&duplicate, "refs/heads/main", &p, Instant::now()).is_err());
        let mut over = p;
        over.limits.max_advertised_refs = 0;
        assert!(select_ref(&valid, "refs/heads/main", &over, Instant::now()).is_err());
    }
    #[test]
    fn task_008_advertised_object_format_and_capability_admission_are_consistent() {
        for (len, caps) in [
            (40, "object-format=sha256"),
            (64, "object-format=sha1"),
            (64, ""),
            (40, "object-format=sha1 object-format=sha256"),
            (40, "shallow shallow"),
            (40, "object-format=sha512"),
        ] {
            assert!(
                select_ref(
                    &ad(&"1".repeat(len), caps),
                    "refs/heads/main",
                    &policy(),
                    Instant::now()
                )
                .is_err()
            );
        }
        assert!(
            select_ref(
                &ad(&"1".repeat(64), "object-format=sha256"),
                "refs/heads/main",
                &policy(),
                Instant::now()
            )
            .is_ok()
        );
        let caps = (0..129)
            .map(|n| format!("x{n}"))
            .collect::<Vec<_>>()
            .join(" ");
        assert!(
            select_ref(
                &ad(&"1".repeat(40), &caps),
                "refs/heads/main",
                &policy(),
                Instant::now()
            )
            .is_err()
        );
        let mut mixed = ad(&"1".repeat(40), "");
        mixed.truncate(mixed.len() - 4);
        pkt(
            &mut mixed,
            format!("{} refs/heads/other\n", "2".repeat(64)).as_bytes(),
        );
        mixed.extend(b"0000");
        assert!(select_ref(&mixed, "refs/heads/main", &policy(), Instant::now()).is_err());
    }
    #[test]
    fn task_008_http_service_and_response_framing_are_not_ambiguous() {
        let mut http = Vec::new();
        pkt(&mut http, b"# service=git-upload-pack\n");
        http.extend(b"0000");
        http.extend(ad(&"1".repeat(40), ""));
        assert!(select_ref(&http, "refs/heads/main", &policy(), Instant::now()).is_ok());
        http.extend(b"0000");
        assert!(select_ref(&http, "refs/heads/main", &policy(), Instant::now()).is_err());
        let oid = [1; 20];
        let mut good = Vec::new();
        pkt(&mut good, b"NAK\n");
        good.extend(b"PACKtest");
        assert_eq!(
            response_pack(&good, &oid, false, &policy(), Instant::now()).unwrap(),
            b"PACKtest"
        );
        for bytes in [
            b"PACKtest".as_slice(),
            b"0000PACKtest",
            b"0008ACK\nPACKtest",
            b"0008NAK\n0008NAK\nPACKtest",
            b"0008NAK\n0000PACKtest",
        ] {
            assert!(response_pack(bytes, &oid, false, &policy(), Instant::now()).is_err());
        }
        let mut shallow = Vec::new();
        pkt(&mut shallow, format!("shallow {}\n", hex(&oid)).as_bytes());
        shallow.extend(b"0000");
        shallow.extend(good);
        assert!(response_pack(&shallow, &oid, true, &policy(), Instant::now()).is_ok());
        assert!(response_pack(&shallow, &[2; 20], true, &policy(), Instant::now()).is_err());
        assert!(response_pack(&shallow, &oid, false, &policy(), Instant::now()).is_err());
    }
}
