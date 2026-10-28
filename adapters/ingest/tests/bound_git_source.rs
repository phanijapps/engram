use engram_ingest::git_source::{
    GitResourceLimits, RemotePolicy, acquire_local_branch, acquire_local_branch_with_limits,
    acquire_remote_branch, read_resolved_commit, resolve_local_branch, retain_snapshot,
    validate_branch_ref,
};
use std::{
    io::{BufRead, BufReader},
    net::TcpListener,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
};
use tempfile::tempdir;

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
    let d = tempdir().unwrap();
    let r = d.path().join("repo");
    std::fs::create_dir(&r).unwrap();
    run(&r, &["init", "-b", "main"]);
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
fn https_server(repo_root: &Path, cert: &Path, key: &Path) -> (Child, u16) {
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
        proc=subprocess.run(['/usr/lib/git-core/git-http-backend'],input=body,stdout=subprocess.PIPE,stderr=subprocess.DEVNULL,env=env)
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
    let mut child = Command::new("python3")
        .args(["-u", "-c", SERVER])
        .arg(repo_root)
        .arg(cert)
        .arg(key)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
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
    child: Child,
    config: PathBuf,
    known_hosts: PathBuf,
    endpoint: String,
}
impl Drop for SshFixture {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
fn ssh_server(_root: &Path, scratch: &Path) -> SshFixture {
    let host = scratch.join("host_key");
    let client = scratch.join("client_key");
    keygen("ed25519", &host);
    keygen("ed25519", &client);
    let user = std::env::var("USER").unwrap();
    let port = {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap().port()
    };
    let authorized = scratch.join("authorized_keys");
    std::fs::copy(client.with_extension("pub"), &authorized).unwrap();
    let known = scratch.join("known_hosts");
    let pubhost = std::fs::read_to_string(host.with_extension("pub")).unwrap();
    let fields = pubhost.split_whitespace().collect::<Vec<_>>();
    std::fs::write(
        &known,
        format!("[127.0.0.1]:{port} {} {}\n", fields[0], fields[1]),
    )
    .unwrap();
    let config = scratch.join("ssh_config");
    std::fs::write(&config,format!("Host 127.0.0.1\n HostName 127.0.0.1\n Port {port}\n User {user}\n IdentityFile {}\n IdentitiesOnly yes\n UserKnownHostsFile {}\n StrictHostKeyChecking yes\n BatchMode yes\n LogLevel ERROR\n",client.display(),known.display())).unwrap();
    let daemon = scratch.join("sshd_config");
    std::fs::write(&daemon,format!("Port {port}\nListenAddress 127.0.0.1\nHostKey {}\nPidFile {}\nAuthorizedKeysFile {}\nStrictModes no\nPasswordAuthentication no\nKbdInteractiveAuthentication no\nPubkeyAuthentication yes\nPermitRootLogin no\nUsePAM no\nAllowUsers {user}\nLogLevel VERBOSE\n",host.display(),scratch.join("pid").display(),authorized.display())).unwrap();
    let check = Command::new("/usr/sbin/sshd")
        .args(["-t", "-f"])
        .arg(&daemon)
        .output()
        .unwrap();
    assert!(check.status.success(), "sshd fixture config invalid");
    let mut child = Command::new("/usr/sbin/sshd")
        .args(["-D", "-e", "-f"])
        .arg(&daemon)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut ready = false;
    let mut logs = String::new();
    let stderr = child.stderr.take().unwrap();
    for line in BufReader::new(stderr).lines() {
        let line = line.unwrap();
        if line.contains("Server listening") {
            ready = true;
            break;
        }
        logs.push_str(&line);
        logs.push('\n');
        if !logs.is_empty() && logs.contains("error:") {
            break;
        }
    }
    assert!(ready, "isolated sshd did not start: {logs}");
    SshFixture {
        child,
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
        run(&r, &["branch", "Main"]);
        let (l, _) = acquire_local_branch(&r, "refs/heads/main").unwrap();
        let (u, _) = acquire_local_branch(&r, "refs/heads/Main").unwrap();
        assert_eq!(l.oid, u.oid);
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
}

mod task_008 {
    use super::*;
    fn lfs_or_subrepo(repo: &Path, path: &str, kind: &str) {
        if kind == "symlink" {
            std::os::unix::fs::symlink("missing-target", repo.join(path)).unwrap();
            run(repo, &["add", "-A"]);
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
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
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
        limits.max_snapshot_bytes = 1;
        assert!(acquire_local_branch_with_limits(&local, "refs/heads/main", &limits).is_err());
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
        let (scratch, source) = repo();
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
        let certgen = Command::new("openssl")
            .args(["req", "-x509", "-newkey", "rsa:2048", "-nodes", "-keyout"])
            .arg(&key)
            .arg("-out")
            .arg(&cert)
            .args([
                "-days",
                "1",
                "-subj",
                "/CN=localhost",
                "-addext",
                "subjectAltName=DNS:localhost",
                "-addext",
                "basicConstraints=critical,CA:TRUE",
            ])
            .output()
            .unwrap();
        assert!(certgen.status.success());
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
        assert_eq!(
            remote
                .files
                .iter()
                .find(|f| f.path == Path::new("src/lib.rs"))
                .unwrap()
                .bytes,
            b"pub fn committed() {}\r\n"
        );
        let wrong_ca = scratch.path().join("wrong-ca.pem");
        let wrong_key = scratch.path().join("wrong-ca.key");
        let wrong = Command::new("openssl")
            .args(["req", "-x509", "-newkey", "rsa:2048", "-nodes", "-keyout"])
            .arg(&wrong_key)
            .arg("-out")
            .arg(&wrong_ca)
            .args([
                "-days",
                "1",
                "-subj",
                "/CN=not-localhost",
                "-addext",
                "basicConstraints=critical,CA:TRUE",
            ])
            .output()
            .unwrap();
        assert!(wrong.status.success());
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
        let _ = server.kill();
        let _ = server.wait();
        let ssh = ssh_server(&bare, scratch.path());
        let ssh_url = format!(
            "ssh://{}@127.0.0.1:{}/{}",
            std::env::var("USER").unwrap(),
            ssh.endpoint.rsplit(':').next().unwrap(),
            bare.display().to_string().trim_start_matches('/')
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
    }
}
