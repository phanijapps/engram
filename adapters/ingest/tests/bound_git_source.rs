use engram_ingest::git_source::{
    GitResourceLimits, RemotePolicy, acquire_local_branch, acquire_local_branch_with_limits,
    acquire_remote_branch, read_resolved_commit, resolve_local_branch, retain_snapshot,
    validate_branch_ref,
};
use std::{
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
};
use tempfile::tempdir;

fn fixture_python() -> std::ffi::OsString {
    std::env::var_os("ENGRAM_TEST_PYTHON").unwrap_or_else(|| "python3".into())
}
fn run(root: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(args)
        .current_dir(root)
        .env("GIT_AUTHOR_NAME", "Factory Fixture")
        .env("GIT_AUTHOR_EMAIL", "factory@example.invalid")
        .env("GIT_COMMITTER_NAME", "Factory Fixture")
        .env("GIT_COMMITTER_EMAIL", "factory@example.invalid")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "fixture Git operation failed: {:?}",
        args
    );
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}
fn repo() -> (tempfile::TempDir, PathBuf) {
    repo_format("sha1")
}
fn repo_format(object_format: &str) -> (tempfile::TempDir, PathBuf) {
    let d = tempdir().unwrap();
    let r = d.path().join("repo");
    std::fs::create_dir(&r).unwrap();
    run(
        &r,
        &[
            "init",
            "-b",
            "main",
            &format!("--object-format={object_format}"),
        ],
    );
    // The fixture's committed bytes must not depend on the host's Git checkout policy.
    run(&r, &["config", "core.autocrlf", "false"]);
    run(&r, &["config", "user.name", "Factory Fixture"]);
    run(&r, &["config", "user.email", "factory@example.invalid"]);
    std::fs::create_dir(r.join("src")).unwrap();
    std::fs::write(r.join("src/lib.rs"), b"pub fn committed() {}\r\n").unwrap();
    run(&r, &["add", "."]);
    run(&r, &["commit", "-m", "fixture"]);
    (d, r)
}
fn bare_copy(source: &Path, target: &Path) {
    let output = Command::new("git")
        .args(["clone", "--bare"])
        .arg(source)
        .arg(target)
        .output()
        .unwrap();
    assert!(output.status.success(), "bare fixture clone failed");
}
fn keygen(kind: &str, path: &Path) {
    let output = Command::new("ssh-keygen")
        .args(["-q", "-t", kind, "-N", ""])
        .arg("-f")
        .arg(path)
        .output()
        .unwrap();
    assert!(output.status.success(), "test-only key generation failed");
}
#[cfg(windows)]
#[path = "../src/git_source/bounded_process.rs"]
mod bounded_process;
#[cfg(windows)]
#[path = "../src/git_source/windows_job.rs"]
mod windows_job;
struct OwnedFixtureProcess {
    child: Option<Child>,
    #[cfg(windows)]
    job: Option<windows_job::OwnedJob>,
}
impl OwnedFixtureProcess {
    fn spawn(command: &mut Command) -> Self {
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
            Self {
                child: Some(command.spawn().unwrap()),
            }
        }
        #[cfg(windows)]
        {
            let (child, job) = windows_job::spawn(command).unwrap();
            Self {
                child: Some(child),
                job: Some(job),
            }
        }
    }
    fn stop(&mut self) -> std::io::Result<()> {
        let Some(mut child) = self.child.take() else {
            return Ok(());
        };
        #[cfg(unix)]
        {
            unsafe {
                libc::kill(-(child.id() as libc::pid_t), libc::SIGKILL);
            }
            let _ = child.kill();
            child.wait()?;
        }
        #[cfg(windows)]
        {
            windows_job::finish(child, self.job.take().unwrap())?;
        }
        Ok(())
    }
}
impl std::ops::Deref for OwnedFixtureProcess {
    type Target = Child;
    fn deref(&self) -> &Child {
        self.child.as_ref().unwrap()
    }
}
impl std::ops::DerefMut for OwnedFixtureProcess {
    fn deref_mut(&mut self) -> &mut Child {
        self.child.as_mut().unwrap()
    }
}
impl Drop for OwnedFixtureProcess {
    fn drop(&mut self) {
        if let Err(error) = self.stop() {
            eprintln!("owned fixture cleanup_pending: {error}");
        }
    }
}
fn https_server(repo_root: &Path, cert: &Path, key: &Path) -> (OwnedFixtureProcess, u16) {
    const SERVER: &str = r#"
import http.server, os, ssl, subprocess, sys, urllib.parse
root, cert, key = sys.argv[1:]
class H(http.server.BaseHTTPRequestHandler):
    protocol_version = 'HTTP/1.0'
    def log_message(self, fmt, *args): pass
    def do_GET(self): self.dispatch()
    def do_POST(self): self.dispatch()
    def dispatch(self):
        parsed=urllib.parse.urlsplit(self.path)
        env=os.environ.copy(); env.update({'GIT_PROJECT_ROOT':root,'GIT_HTTP_EXPORT_ALL':'1','PATH_INFO':parsed.path,'QUERY_STRING':parsed.query,'REQUEST_METHOD':self.command,'CONTENT_TYPE':self.headers.get('Content-Type',''),'CONTENT_LENGTH':self.headers.get('Content-Length','0'),'REMOTE_USER':'','REMOTE_ADDR':self.client_address[0],'SERVER_PROTOCOL':self.protocol_version,'SERVER_NAME':'localhost','SERVER_PORT':str(self.server.server_port),'SCRIPT_NAME':''})
        if self.headers.get('Git-Protocol'): env['HTTP_GIT_PROTOCOL']=self.headers['Git-Protocol']
        body=self.rfile.read(int(env['CONTENT_LENGTH'])) if self.command=='POST' else b''
        proc=subprocess.run(['git','http-backend'],input=body,stdout=subprocess.PIPE,stderr=subprocess.DEVNULL,env=env)
        raw=proc.stdout
        sep=b'\r\n\r\n' if b'\r\n\r\n' in raw else b'\n\n'
        if sep not in raw: self.send_error(502); return
        head, data=raw.split(sep,1); lines=head.replace(b'\r\n',b'\n').split(b'\n'); status=200; headers=[]
        for line in lines:
            if line.lower().startswith(b'status:'): status=int(line.split()[1])
            elif b':' in line:
                k,v=line.split(b':',1)
                if k.lower() not in (b'connection',b'transfer-encoding'): headers.append((k.decode('latin1'),v.strip().decode('latin1')))
        self.send_response(status)
        for k,v in headers: self.send_header(k,v)
        if not any(k.lower()=='content-length' for k,v in headers): self.send_header('Content-Length',str(len(data)))
        self.end_headers(); self.wfile.write(data); self.close_connection=True
server=http.server.HTTPServer(('127.0.0.1',0),H)
ctx=ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER); ctx.load_cert_chain(cert,key); server.socket=ctx.wrap_socket(server.socket,server_side=True)
print('READY '+str(server.server_port),flush=True); server.serve_forever()
"#;
    let mut child = OwnedFixtureProcess::spawn(
        Command::new(fixture_python())
            .args(["-u", "-c", SERVER])
            .arg(repo_root)
            .arg(cert)
            .arg(key)
            .stdout(Stdio::piped())
            .stderr(Stdio::null()),
    );
    let mut line = String::new();
    BufReader::new(child.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    let port = line
        .trim()
        .strip_prefix("READY ")
        .expect("HTTPS fixture readiness")
        .parse()
        .unwrap();
    (child, port)
}
struct SshFixture {
    _child: OwnedFixtureProcess,
    config: PathBuf,
    known_hosts: PathBuf,
    endpoint: String,
}
fn ssh_server(root: &Path, scratch: &Path) -> SshFixture {
    const SERVER: &str = r#"
import asyncio, asyncssh, sys
repo, host, client=sys.argv[1:]
authorized=asyncssh.read_public_key(client)
class Server(asyncssh.SSHServer):
    def begin_auth(self,username): return True
    def public_key_auth_supported(self): return True
    def validate_public_key(self,username,key): return username=='fixture' and key==authorized
async def serve(process):
    modes={"git-upload-pack --advertise-refs '/remote.git'":"--advertise-refs", "git-upload-pack --stateless-rpc '/remote.git'":"--stateless-rpc"}
    mode=modes.get(process.command)
    if mode is None: process.exit(126); return
    child=await asyncio.create_subprocess_exec('git','upload-pack','--strict',mode,repo,stdin=asyncio.subprocess.PIPE,stdout=asyncio.subprocess.PIPE,stderr=asyncio.subprocess.PIPE)
    async def pump(reader,writer):
        try:
            while True:
                data=await reader.read(65536)
                if not data: break
                writer.write(data); await writer.drain()
            writer.write_eof()
        except (BrokenPipeError,ConnectionResetError,asyncssh.Error): pass
    try:
        await asyncio.gather(pump(process.stdin,child.stdin),pump(child.stdout,process.stdout),pump(child.stderr,process.stderr))
        process.exit(await child.wait())
    finally:
        if child.returncode is None: child.kill(); await child.wait()
async def main():
    server=await asyncssh.create_server(Server,'127.0.0.1',0,server_host_keys=[host],process_factory=serve,encoding=None)
    print('READY '+str(server.get_port()),flush=True)
    await server.wait_closed()
asyncio.run(main())
"#;
    let host = scratch.join("host_key");
    let client = scratch.join("client_key");
    keygen("ed25519", &host);
    keygen("ed25519", &client);
    let mut child = OwnedFixtureProcess::spawn(
        Command::new(fixture_python())
            .args(["-u", "-c", SERVER])
            .arg(root)
            .arg(&host)
            .arg(client.with_extension("pub"))
            .stdout(Stdio::piped())
            .stderr(Stdio::null()),
    );
    let mut line = String::new();
    BufReader::new(child.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    let port: u16 = line
        .trim()
        .strip_prefix("READY ")
        .expect(
            "owned AsyncSSH fixture readiness; install tests/requirements-portable-acquisition.txt",
        )
        .parse()
        .unwrap();
    let known = scratch.join("known hosts with spaces");
    let public = std::fs::read_to_string(host.with_extension("pub")).unwrap();
    let fields = public.split_whitespace().collect::<Vec<_>>();
    std::fs::write(
        &known,
        format!("[127.0.0.1]:{port} {} {}\n", fields[0], fields[1]),
    )
    .unwrap();
    let config = scratch.join("ssh config with spaces");
    // Quote configured absolute paths; forward separators work with native and Git-for-Windows OpenSSH.
    let key = client.to_str().unwrap().replace('\\', "/");
    let known_path = known.to_str().unwrap().replace('\\', "/");
    std::fs::write(&config,format!("Host 127.0.0.1\n HostName 127.0.0.1\n Port {port}\n User fixture\n IdentityFile \"{key}\"\n IdentitiesOnly yes\n UserKnownHostsFile \"{known_path}\"\n StrictHostKeyChecking yes\n BatchMode yes\n LogLevel ERROR\n")).unwrap();
    SshFixture {
        _child: child,
        config,
        known_hosts: known,
        endpoint: format!("ssh://127.0.0.1:{port}"),
    }
}

mod task_007 {
    use super::*;
    #[test]
    fn dirty_checkout_untouched() {
        let (d, r) = repo();
        std::fs::write(r.join("src/lib.rs"), b"dirty\n").unwrap();
        std::fs::write(r.join("untracked"), b"untracked").unwrap();
        let before = (
            std::fs::read(r.join("src/lib.rs")).unwrap(),
            std::fs::read(r.join("untracked")).unwrap(),
            std::fs::read(r.join(".git/index")).unwrap(),
            run(&r, &["rev-parse", "HEAD"]),
            run(&r, &["for-each-ref", "--format=%(refname) %(objectname)"]),
        );
        let (c, f) = acquire_local_branch(&r, "refs/heads/main").unwrap();
        let s = retain_snapshot(&d.path().join("snapshots"), c, &f).unwrap();
        assert_eq!(
            std::fs::read(s.root.join("src/lib.rs")).unwrap(),
            b"pub fn committed() {}\r\n"
        );
        let after = (
            std::fs::read(r.join("src/lib.rs")).unwrap(),
            std::fs::read(r.join("untracked")).unwrap(),
            std::fs::read(r.join(".git/index")).unwrap(),
            run(&r, &["rev-parse", "HEAD"]),
            run(&r, &["for-each-ref", "--format=%(refname) %(objectname)"]),
        );
        assert_eq!(before, after);
    }
    #[test]
    fn local_no_origin() {
        let (_guard, r) = repo();
        assert!(run(&r, &["remote"]).is_empty());
        let (c, f) = acquire_local_branch(&r, "refs/heads/main").unwrap();
        assert_eq!(c.branch_ref, "refs/heads/main");
        assert!(!f.is_empty());
    }
    #[test]
    fn exact_ref_validation() {
        for bad in [
            "main",
            "HEAD",
            "refs/tags/v1",
            "refs/heads/a..b",
            "refs/heads/a//b",
            "refs/heads/x.lock",
            "refs/heads/a b",
        ] {
            assert!(validate_branch_ref(bad).is_err(), "{bad}")
        }
    }
    #[test]
    fn case_distinct_refs() {
        let (_guard, r) = repo();
        let lower = run(&r, &["rev-parse", "HEAD"]);
        let tree = run(&r, &["rev-parse", "HEAD^{tree}"]);
        let upper = run(
            &r,
            &[
                "commit-tree",
                &tree,
                "-p",
                &lower,
                "-m",
                "case-distinct fixture",
            ],
        );
        run(&r, &["pack-refs", "--all", "--prune"]);
        // Git's packed namespace preserves exact case even when the host filesystem cannot.
        std::fs::write(r.join(".git/packed-refs"),format!("# pack-refs with: peeled fully-peeled sorted\n{upper} refs/heads/Main\n{lower} refs/heads/main\n")).unwrap();
        let (l, _) = acquire_local_branch(&r, "refs/heads/main").unwrap();
        let (u, _) = acquire_local_branch(&r, "refs/heads/Main").unwrap();
        assert_eq!(l.oid, lower);
        assert_eq!(u.oid, upper);
        assert_ne!(l.oid, u.oid);
        assert_ne!(l.branch_ref, u.branch_ref);
    }
    #[test]
    fn moved_ref_pinned_once() {
        let (_guard, r) = repo();
        let (first_oid, old) = {
            let c = resolve_local_branch(&r, "refs/heads/main").unwrap();
            let oid = c.oid.clone();
            (c, oid)
        };
        std::fs::write(r.join("src/lib.rs"), b"second\n").unwrap();
        run(&r, &["add", "."]);
        run(&r, &["commit", "-m", "move ref"]);
        let (second, _) = acquire_local_branch(&r, "refs/heads/main").unwrap();
        assert_ne!(first_oid.oid, second.oid);
        let files = read_resolved_commit(&r, &first_oid).unwrap();
        assert_eq!(
            files
                .iter()
                .find(|x| x.path == Path::new("src/lib.rs"))
                .unwrap()
                .bytes,
            b"pub fn committed() {}\r\n"
        );
        assert_eq!(old, first_oid.oid);
    }
    #[test]
    fn missing_object() {
        let (_guard, r) = repo();
        assert!(acquire_local_branch(&r, "refs/heads/main").is_ok());
        assert!(matches!(
            acquire_local_branch(&r, "refs/heads/missing").unwrap_err(),
            engram_knowledge::CoreError::InvalidRequest { .. }
        ));
    }
    #[test]
    fn literal_backslash_git_name_is_rejected_before_retention() {
        // Construct the tree purely through Git object plumbing (no checkout): the
        // literal filename byte sequence a\b.rs (97,92,98,46,114,115) coexists with
        // a genuinely nested a/b.rs. Every host must reject the backslash name from
        // the RAW Git path before any host-path interpretation or retention.
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("adversarial.git");
        assert!(
            std::process::Command::new("git")
                .args(["init", "--bare", "-b", "main", repo.to_str().unwrap()])
                .current_dir(dir.path())
                .output()
                .unwrap()
                .status
                .success()
        );
        let hash = |content: &str| -> String {
            use std::io::Write as _;
            use std::process::Stdio;
            let mut child = std::process::Command::new("git")
                .args(["hash-object", "-w", "--stdin"])
                .current_dir(&repo)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .spawn()
                .unwrap();
            child
                .stdin
                .as_mut()
                .unwrap()
                .write_all(content.as_bytes())
                .unwrap();
            String::from_utf8(child.wait_with_output().unwrap().stdout)
                .unwrap()
                .trim()
                .to_string()
        };
        let mktree = |input: String| -> String {
            use std::io::Write as _;
            use std::process::Stdio;
            let mut child = std::process::Command::new("git")
                .args(["mktree"])
                .current_dir(&repo)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .spawn()
                .unwrap();
            child
                .stdin
                .as_mut()
                .unwrap()
                .write_all(input.as_bytes())
                .unwrap();
            String::from_utf8(child.wait_with_output().unwrap().stdout)
                .unwrap()
                .trim()
                .to_string()
        };
        let backslash = hash("backslash-name\n");
        let nested = hash("nested-name\n");
        let subtree = mktree(format!("100644 blob {nested}\tb.rs\n"));
        let root_tree = mktree(format!(
            "100644 blob {backslash}\ta\\b.rs\n040000 tree {subtree}\ta\n"
        ));
        let commit = run(&repo, &["commit-tree", &root_tree, "-m", "adversarial"]);
        run(&repo, &["update-ref", "refs/heads/main", commit.trim()]);
        let listed = run(&repo, &["ls-tree", "-rz", "--full-tree", "refs/heads/main"]);
        assert!(
            listed.bytes().any(|b| b == 92),
            "fixture must contain the literal backslash filename byte"
        );
        match acquire_local_branch(&repo, "refs/heads/main") {
            Err(engram_knowledge::CoreError::InvalidRequest { reason }) => {
                assert!(
                    reason.contains("nonportable or reserved component"),
                    "sanitized reason: {reason}"
                );
            }
            other => panic!("literal backslash name must be rejected: {other:?}"),
        }
    }
}

mod task_008 {
    use super::*;
    fn lfs_or_subrepo(repo: &Path, path: &str, kind: &str) {
        if kind == "symlink" {
            let target = repo.join("symlink-object-fixture");
            std::fs::write(&target, b"missing-target").unwrap();
            let oid = run(repo, &["hash-object", "-w", target.to_str().unwrap()]);
            std::fs::remove_file(target).unwrap();
            run(
                repo,
                &[
                    "update-index",
                    "--add",
                    "--cacheinfo",
                    &format!("120000,{oid},{path}"),
                ],
            );
        } else if kind == "lfs" {
            std::fs::write(repo.join(path),b"version https://git-lfs.github.com/spec/v1\noid sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\nsize 3\n").unwrap();
            run(repo, &["add", "."]);
        } else {
            let oid = run(repo, &["rev-parse", "HEAD"]);
            let out = Command::new("git")
                .current_dir(repo)
                .arg("update-index")
                .arg("--add")
                .arg("--cacheinfo")
                .arg(format!("160000,{oid},submodule"))
                .output()
                .unwrap();
            assert!(out.status.success(), "gitlink fixture staging failed");
        }
        run(repo, &["commit", "-m", "unsupported feature"]);
        assert!(
            acquire_local_branch(repo, "refs/heads/main").is_err(),
            "{kind} must deny"
        );
    }
    #[test]
    fn git_execution_path_secret_denials() {
        let scratch = tempdir().unwrap();
        for remote in [
            "file:///tmp/secret",
            "ext::sh -c evil",
            "https://user:password@localhost/repo.git",
            "https://localhost/repo.git",
        ] {
            let policy = RemotePolicy {
                endpoint_allowlist: vec!["https://localhost".into()],
                ssh_config: None,
                ssh_known_hosts: None,
                ca_bundle: None,
                limits: Default::default(),
            };
            let err = acquire_remote_branch(
                remote,
                "refs/heads/main",
                &policy,
                &scratch.path().join("managed"),
            );
            assert!(err.is_err(), "{remote}");
        }
    }
    #[test]
    fn denied_credentials_no_fallback() {
        let (_guard, local) = repo();
        assert!(acquire_local_branch(&local, "refs/heads/main").is_ok());
        let scratch = tempdir().unwrap();
        let policy = RemotePolicy {
            endpoint_allowlist: vec!["https://localhost:9".into()],
            ssh_config: None,
            ssh_known_hosts: None,
            ca_bundle: Some(scratch.path().join("unused-ca")),
            limits: Default::default(),
        };
        let err = acquire_remote_branch(
            "https://fixture:credential@localhost:9/not-a-repo.git",
            "refs/heads/main",
            &policy,
            &scratch.path().join("managed"),
        )
        .unwrap_err();
        assert!(matches!(
            err,
            engram_knowledge::CoreError::InvalidRequest { .. }
        ));
    }
    #[test]
    fn hooks_filters_and_configured_commands_never_execute() {
        let (guard, local) = repo();
        let marker = guard.path().join("unexpected-execution");
        let command = format!("touch {}", marker.display());
        run(&local, &["config", "core.fsmonitor", &command]);
        run(&local, &["config", "filter.hostile.smudge", &command]);
        run(&local, &["config", "diff.hostile.textconv", &command]);
        run(
            &local,
            &["config", "credential.helper", &format!("!{command}")],
        );
        let hook = local.join(".git/hooks/post-checkout");
        std::fs::write(&hook, format!("#!/bin/sh\n{command}\n")).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        std::fs::write(
            local.join(".git/info/attributes"),
            "*.rs filter=hostile diff=hostile\n",
        )
        .unwrap();
        let (commit, files) = acquire_local_branch(&local, "refs/heads/main").unwrap();
        assert_eq!(commit.oid.len(), 40);
        assert_eq!(files[0].bytes, b"pub fn committed() {}\r\n");
        assert!(!marker.exists(), "repository-controlled command executed");
    }

    #[test]
    fn snapshot_inventory_is_confined_and_failed_attempts_clean_up() {
        let (guard, local) = repo();
        let (commit, files) = acquire_local_branch(&local, "refs/heads/main").unwrap();
        let storage = guard.path().join("retained");
        let mut forged = commit.clone();
        forged.oid = "../outside".into();
        assert!(
            matches!(retain_snapshot(&storage, forged, &files), Err(engram_knowledge::CoreError::InvalidRequest { reason }) if reason.contains("full object"))
        );
        assert!(!storage.exists());
        let mut escape = files.clone();
        escape[0].path = "../outside".into();
        assert!(
            matches!(retain_snapshot(&storage, commit.clone(), &escape), Err(engram_knowledge::CoreError::InvalidRequest { reason }) if reason.contains("inventory"))
        );
        assert!(!storage.exists());
        // A file/directory collision fails during writes, not just inventory validation.
        let mut collision = files.clone();
        let mut second = files[0].clone();
        second.path = "src/lib.rs/child".into();
        collision.push(second);
        assert!(matches!(
            retain_snapshot(&storage, commit.clone(), &collision),
            Err(engram_knowledge::CoreError::Adapter { .. })
        ));
        assert_eq!(std::fs::read_dir(&storage).unwrap().count(), 0);
        let retained = retain_snapshot(&storage, commit, &files).unwrap();
        std::fs::remove_dir_all(&local).unwrap();
        assert_eq!(
            std::fs::read(retained.root.join("src/lib.rs")).unwrap(),
            b"pub fn committed() {}\r\n"
        );
    }

    #[test]
    fn ssh_config_command_forms_are_rejected_before_network() {
        let scratch = tempdir().unwrap();
        let config = scratch.path().join("ssh-config");
        let trust = scratch.path().join("known-hosts");
        std::fs::write(&trust, "").unwrap();
        for directive in [
            "ProxyCommand=touch forbidden",
            "Include /tmp/unsafe",
            "LocalCommand touch forbidden",
            "Match exec true",
        ] {
            std::fs::write(&config, directive).unwrap();
            let policy = RemotePolicy {
                endpoint_allowlist: vec!["ssh://localhost:9".into()],
                ssh_config: Some(config.clone()),
                ssh_known_hosts: Some(trust.clone()),
                ca_bundle: None,
                limits: Default::default(),
            };
            let error = acquire_remote_branch(
                "ssh://localhost:9/repo",
                "refs/heads/main",
                &policy,
                &scratch.path().join("managed"),
            )
            .unwrap_err();
            assert!(
                matches!(error, engram_knowledge::CoreError::InvalidRequest { reason } if reason.contains("unsupported command"))
            );
            assert!(!scratch.path().join("managed").exists());
        }
    }

    #[test]
    fn resource_limits_fail_before_snapshot_staging() {
        let (_guard, local) = repo();
        let mut limits = GitResourceLimits::default();
        // clippy roll 2026-10: staged fixture defaults make only the exercised limit explicit.
        #[allow(clippy::field_reassign_with_default)]
        limits.max_snapshot_bytes = 1;
        assert!(acquire_local_branch_with_limits(&local, "refs/heads/main", &limits).is_err());
        // clippy roll 2026-10: staged fixture defaults make only the exercised limit explicit.
        #[allow(clippy::field_reassign_with_default)]
        limits.max_snapshot_bytes = usize::MAX;
        limits.max_source_files = 0;
        assert!(acquire_local_branch_with_limits(&local, "refs/heads/main", &limits).is_err());
    }
    #[test]
    fn lfs_gitlink_symlink_denied() {
        let (_d1, r1) = repo();
        lfs_or_subrepo(&r1, "src/lib.rs", "lfs");
        let (_d2, r2) = repo();
        lfs_or_subrepo(&r2, "link", "symlink");
        let (_d3, r3) = repo();
        lfs_or_subrepo(&r3, "submodule", "gitlink");
    }
    #[test]
    fn explicit_remote_on_request() {
        for object_format in ["sha1", "sha256"] {
            let (scratch, source) = repo_format(object_format);
            let mut payload = vec![0u8; 32 * 1024];
            let mut seed = 0x1234_5678u32;
            for byte in &mut payload {
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                *byte = (seed >> 24) as u8;
            }
            std::fs::write(source.join("bounded-fixture.bin"), payload).unwrap();
            run(&source, &["add", "."]);
            run(&source, &["commit", "-m", "bounded fixture data"]);
            let bare = scratch.path().join("remote.git");
            bare_copy(&source, &bare);
            let cert = scratch.path().join("cert.pem");
            let key = scratch.path().join("cert.key");
            let fixture = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
            std::fs::write(&cert, fixture.cert.pem()).unwrap();
            std::fs::write(&key, fixture.signing_key.serialize_pem()).unwrap();
            let (mut server, port) = https_server(scratch.path(), &cert, &key);
            let url = format!("https://localhost:{port}/remote.git");
            let policy = RemotePolicy {
                endpoint_allowlist: vec![format!("https://localhost:{port}")],
                ssh_config: None,
                ssh_known_hosts: None,
                ca_bundle: Some(cert.clone()),
                limits: Default::default(),
            };
            let remote = acquire_remote_branch(
                &url,
                "refs/heads/main",
                &policy,
                &scratch.path().join("https-managed"),
            )
            .unwrap();
            let cache_files = std::fs::read_dir(&remote.managed_repo)
                .unwrap()
                .map(|entry| entry.unwrap().file_name())
                .collect::<Vec<_>>();
            assert_eq!(cache_files, vec![std::ffi::OsString::from("objects.pack")]);
            assert!(
                std::fs::metadata(remote.managed_repo.join("objects.pack"))
                    .unwrap()
                    .len()
                    <= policy.limits.max_fetched_storage_bytes
            );
            assert_eq!(
                remote
                    .files
                    .iter()
                    .find(|f| f.path == Path::new("src/lib.rs"))
                    .unwrap()
                    .bytes,
                b"pub fn committed() {}\r\n"
            );
            let retained = retain_snapshot(
                &scratch.path().join("https-retained"),
                remote.commit.clone(),
                &remote.files,
            )
            .unwrap();
            for file in &remote.files {
                assert_eq!(
                    std::fs::read(retained.root.join(&file.path)).unwrap(),
                    file.bytes
                );
            }
            let repeat = acquire_remote_branch(
                &url,
                "refs/heads/main",
                &policy,
                &scratch.path().join("https-managed"),
            )
            .unwrap();
            assert_ne!(repeat.managed_repo, remote.managed_repo);
            assert_eq!(repeat.commit, remote.commit);
            let wrong_ca = scratch.path().join("wrong-ca.pem");
            let wrong_key = scratch.path().join("wrong-ca.key");
            let wrong = rcgen::generate_simple_self_signed(vec!["not-localhost".into()]).unwrap();
            std::fs::write(&wrong_ca, wrong.cert.pem()).unwrap();
            std::fs::write(&wrong_key, wrong.signing_key.serialize_pem()).unwrap();
            let mut wrong_policy = policy.clone();
            wrong_policy.ca_bundle = Some(wrong_ca);
            assert!(
                acquire_remote_branch(
                    &url,
                    "refs/heads/main",
                    &wrong_policy,
                    &scratch.path().join("untrusted-managed")
                )
                .is_err(),
                "untrusted TLS certificate must fail closed"
            );
            let mut quota_policy = policy.clone();
            quota_policy.limits.max_fetched_storage_bytes = 4096;
            let quota_root = scratch.path().join("quota-managed");
            assert!(
                acquire_remote_branch(&url, "refs/heads/main", &quota_policy, &quota_root).is_err(),
                "oversized fetched pack must fail closed"
            );
            assert_eq!(
                std::fs::read_dir(&quota_root).unwrap().count(),
                0,
                "failed attempt storage must be cleaned"
            );
            server.stop().unwrap();
            let ssh = ssh_server(&bare, scratch.path());
            let ssh_url = format!(
                "ssh://fixture@127.0.0.1:{}/remote.git",
                ssh.endpoint.rsplit(':').next().unwrap()
            );
            let ssh_policy = RemotePolicy {
                endpoint_allowlist: vec![ssh.endpoint.clone()],
                ssh_config: Some(ssh.config.clone()),
                ssh_known_hosts: Some(ssh.known_hosts.clone()),
                ca_bundle: None,
                limits: Default::default(),
            };
            let acquired = acquire_remote_branch(
                &ssh_url,
                "refs/heads/main",
                &ssh_policy,
                &scratch.path().join("ssh-managed"),
            )
            .unwrap();
            assert_eq!(acquired.commit.branch_ref, "refs/heads/main");
            assert!(!acquired.files.is_empty());
            assert_eq!(acquired.commit.object_format, object_format);
            let retained = retain_snapshot(
                &scratch.path().join("ssh-retained"),
                acquired.commit.clone(),
                &acquired.files,
            )
            .unwrap();
            for file in &acquired.files {
                assert_eq!(
                    std::fs::read(retained.root.join(&file.path)).unwrap(),
                    file.bytes
                );
            }
        }
    }
}
