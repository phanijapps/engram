//! TASK-021 / TASK-022 focused proof: the REAL MCP transports (stdio server
//! subprocess + one-shot CLI subcommands) expose the scoped code operations
//! with typed diagnostics over BOTH recipes (SQLite default build; pgvector
//! feature build against the marked disposable PostgreSQL when
//! `ENGRAM_TEST_PG_URL` is present).
//!
//! The harness drives the actual `engram-mcp` BINARY (never in-process
//! dispatch): JSON-RPC lines over stdin/stdout for stdio, argv + stdout +
//! exit code for one-shot. Non-JSON startup/checkpoint lines on stdout are
//! skipped; every response is parsed as a JSON-RPC envelope.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use serde_json::{Value, json};

// ---------- harness ---------------------------------------------------------

struct Server {
    child: Child,
    stdin: std::process::ChildStdin,
    responses: mpsc::Receiver<Value>,
    _reader: std::thread::JoinHandle<()>,
    storage: std::path::PathBuf,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn read_timeout(receiver: &mpsc::Receiver<Value>, seconds: u64) -> Value {
    receiver
        .recv_timeout(Duration::from_secs(seconds))
        .expect("a JSON-RPC response line within the timeout")
}

fn spawn_server(backend: &str, tag: &str) -> Server {
    let dir = tempfile::tempdir().expect("tempdir");
    let storage = dir.path().join("storage");
    std::fs::create_dir_all(&storage).expect("mkdir storage");
    // Keep the tempdir alive for the server's lifetime by leaking it (test
    // scope; the storage path is what matters).
    std::mem::forget(dir);

    let mut command = Command::new(env!("CARGO_BIN_EXE_engram-mcp"));
    command
        .args([
            "--storage",
            storage.to_str().unwrap(),
            "--org",
            "task-transports",
            "--project",
            tag,
            "--no-vector",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    if backend == "pgvector" {
        command.arg("--backend").arg("pgvector");
        // The URL reaches the child through its ENV (never argv/logs).
        let url =
            std::env::var("ENGRAM_TEST_PG_URL").expect("marked disposable PostgreSQL URL required");
        command.env("ENGRAM_PG_CONNECTION_STRING", url);
    }
    let mut child = command.spawn().expect("spawn engram-mcp");
    let stdin = child.stdin.take().expect("stdin");
    let stdout = child.stdout.take().expect("stdout");
    let (sender, receiver) = mpsc::channel();
    let reader = std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };
            if let Ok(value) = serde_json::from_str::<Value>(&line)
                && value.get("jsonrpc").is_some()
            {
                if sender.send(value).is_err() {
                    break;
                }
            }
            // Non-JSON startup/diagnostic lines are skipped, never parsed.
        }
    });
    Server {
        child,
        stdin,
        responses: receiver,
        _reader: reader,
        storage,
    }
}

impl Server {
    fn call(&mut self, id: u64, method: &str, params: Value) -> Value {
        let request = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        self.stdin
            .write_all(format!("{}\n", serde_json::to_string(&request).unwrap()).as_bytes())
            .expect("write request");
        self.stdin.flush().expect("flush");
        read_timeout(&self.responses, 60)
    }

    fn tool(&mut self, id: u64, name: &str, args: Value) -> Value {
        self.call(id, "tools/call", json!({ "name": name, "arguments": args }))
    }

    fn text_of(response: &Value) -> String {
        response["result"]["content"][0]["text"]
            .as_str()
            .unwrap_or_default()
            .to_owned()
    }
}

/// Creates a real local git repository with one commit on `main` and returns
/// (path, full commit oid).
fn git_fixture(tag: &str) -> (std::path::PathBuf, String) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.keep();
    let run = |args: &[&str]| {
        let status = Command::new("git")
            .args(args)
            .current_dir(&path)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .expect("git");
        assert!(status.success(), "git {args:?} failed");
    };
    run(&["init", "--initial-branch=main", "."]);
    run(&["config", "user.email", "task-transports@engram.test"]);
    run(&["config", "user.name", "task-transports"]);
    std::fs::write(path.join("lib.rs"), "pub fn fixture() -> u32 { 1 }\n").expect("write");
    run(&["add", "."]);
    run(&["commit", "--no-gpg-sign", "-m", "fixture"]);
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(&path)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .output()
        .expect("git rev-parse");
    assert!(output.status.success());
    let oid = String::from_utf8(output.stdout)
        .expect("utf8 oid")
        .trim()
        .to_owned();
    assert_eq!(oid.len(), 40, "full commit oid");
    (path, oid)
}

/// Run-unique suffix for RepoIds: the disposable PostgreSQL database
/// persists across test runs and durable registrations key by RepoId alone —
/// static ids would collide with an earlier run's different locator.
fn unique_suffix() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{}-{}", std::process::id(), nanos)
}

fn pg_available() -> bool {
    std::env::var("ENGRAM_TEST_PG_URL").is_ok()
}

fn pg_required() -> bool {
    std::env::var("ENGRAM_TEST_REQUIRE_PG").map_or(false, |value| value == "1")
}

/// Convert the real preview document into the exact owner-approved journal
/// accepted by the public reset handler.
fn approved_journal(preview: Value, reset_id: &str, decision: &str) -> Value {
    let targets = preview["targets"].as_array().expect("preview targets");
    json!({
        "resetId": reset_id,
        "preview": preview.clone(),
        "previewHash": preview["previewHash"],
        "targetFingerprints": targets.iter().map(|target| json!([target["recordId"], target["fingerprint"]])).collect::<Vec<_>>(),
        "authorizingDecisions": [decision],
        "checkpoints": [],
        "preservationDigests": preview["preservationDigests"],
    })
}

/// The pure recipe-selection rule (review r6 finding 8): `pgvector` when the
/// URL is present AND the binary was built with the feature (a build without
/// the feature refuses the backend with its typed error — asserted in the
/// unavailable path below). REQUIRED-Pg mode without a URL FAILS FAST —
/// never a silent SQLite-only fallback.
fn select_backends(pg_required: bool, pg_url: bool) -> Vec<(&'static str, &'static str)> {
    if pg_required && !pg_url {
        panic!(
            "ENGRAM_TEST_REQUIRE_PG=1 but ENGRAM_TEST_PG_URL is absent — a required-Pg run              must fail fast rather than silently testing SQLite alone"
        );
    }
    if pg_url {
        vec![("sqlite", "sqlite"), ("pgvector", "pg")]
    } else {
        vec![("sqlite", "sqlite")]
    }
}

// The recipe set under test.
fn backends() -> Vec<(&'static str, &'static str)> {
    select_backends(pg_required(), pg_available())
}

#[test]
fn require_pg_without_url_fails_fast() {
    let result = std::panic::catch_unwind(|| select_backends(true, false));
    assert!(
        result.is_err(),
        "required-Pg without a URL must fail fast, never a silent SQLite fallback"
    );
    assert_eq!(select_backends(false, false), vec![("sqlite", "sqlite")]);
    assert_eq!(
        select_backends(true, true),
        vec![("sqlite", "sqlite"), ("pgvector", "pg")]
    );
    assert_eq!(
        select_backends(false, true),
        vec![("sqlite", "sqlite"), ("pgvector", "pg")]
    );
}

// ---------- task_021 focused cases ------------------------------------------

mod task_021 {
    use super::*;

    /// stdio_dispatch_parity: driving the REAL stdio transport, the scoped
    /// code operations return the same bound revision facts and the same
    /// denials on every recipe under test.
    #[test]
    fn stdio_dispatch_parity() {
        let (repo_path, head_oid) = git_fixture("parity");
        let run = unique_suffix();
        let mut observed: Vec<String> = Vec::new();
        for (backend, tag) in backends() {
            let mut server = spawn_server(backend, &format!("parity-{tag}"));
            let init = server.call(1, "initialize", json!({}));
            assert!(
                init["result"]["serverInfo"]["name"] == "engram-mcp",
                "{backend} init"
            );
            let listed = server.call(99, "tools/list", json!({}));
            let names = listed["result"]["tools"]
                .as_array()
                .expect("tool list")
                .iter()
                .filter_map(|tool| tool["name"].as_str())
                .collect::<Vec<_>>();
            for reset_tool in [
                "code_reset_preview",
                "code_reset_apply",
                "code_reset_resume",
            ] {
                assert!(
                    names.contains(&reset_tool),
                    "{backend} missing {reset_tool}"
                );
            }

            // register → bind → status(unknown) → refresh → status(observed)
            server.tool(
                2,
                "code_repo_register",
                json!({ "repo": format!("repo-{tag}-{run}"), "mode": "local-only",
                        "path": repo_path.to_str().unwrap() }),
            );
            let bound = server.tool(
                3,
                "code_bind",
                json!({ "repo": format!("repo-{tag}-{run}"), "branchRef": "refs/heads/main" }),
            );
            assert!(bound.get("error").is_none(), "{backend} bind: {bound}");
            let bound_text = Server::text_of(&bound);
            assert!(
                bound_text.contains("BOUND repo-") && bound_text.contains("refs/heads/main"),
                "{backend} bound text: {bound_text}"
            );

            let status = server.tool(
                4,
                "code_status",
                json!({ "repo": format!("repo-{tag}-{run}") }),
            );
            let status_text = Server::text_of(&status);
            assert!(
                status_text.contains("FRESHNESS: unknown"),
                "{backend} freshness must start unknown: {status_text}"
            );
            assert!(
                status_text.contains("INDEXED: none"),
                "{backend} no published generation yet: {status_text}"
            );

            let refresh = server.tool(
                5,
                "code_refresh",
                json!({ "repo": format!("repo-{tag}-{run}") }),
            );
            assert!(
                refresh.get("error").is_none(),
                "{backend} refresh: {refresh}"
            );
            let refreshed = Server::text_of(&refresh);
            assert!(
                refreshed.contains(&head_oid) && refreshed.contains("policy on_request"),
                "{backend} refresh observes the real ref: {refreshed}"
            );

            let rebuilt = server.tool(
                6,
                "code_rebuild",
                json!({ "repo": format!("repo-{tag}-{run}"), "requestId": format!("rebuild-{run}") }),
            );
            let rebuilt_text = Server::text_of(&rebuilt);
            assert!(
                rebuilt.get("error").is_none() && rebuilt_text.contains("generationId"),
                "{backend} registered-locator rebuild publishes: {rebuilt}"
            );
            let preview_response = server.tool(
                7,
                "code_reset_preview",
                json!({ "scope": { "tenant": "task-transports", "workspace": format!("parity-{tag}") }, "repo": format!("repo-{tag}-{run}") }),
            );
            let preview: Value = serde_json::from_str(&Server::text_of(&preview_response))
                .unwrap_or_else(|error| {
                    panic!("{backend} reset preview: {error}: {preview_response}")
                });
            assert!(
                preview["targets"]
                    .as_array()
                    .is_some_and(|targets| !targets.is_empty()),
                "{backend} preview targets: {preview}"
            );
            assert!(
                preview["ambiguity"].is_array() && preview["preservationDigests"].is_array(),
                "{backend} preview inventories: {preview}"
            );
            let decision = format!("owner-approved-{run}");
            let journal = approved_journal(preview, &format!("reset-{run}"), &decision);
            let applied = server.tool(
                8,
                "code_reset_apply",
                json!({ "journal": journal.clone(), "decision": decision }),
            );
            assert!(
                applied.get("error").is_none() && Server::text_of(&applied).contains("completed:"),
                "{backend} reset apply: {applied}"
            );
            let resumed = server.tool(9, "code_reset_resume", json!({ "journal": journal }));
            assert!(
                resumed.get("error").is_none() && Server::text_of(&resumed).contains("completed:"),
                "{backend} reset resume: {resumed}"
            );

            let after = server.tool(
                10,
                "code_status",
                json!({ "repo": format!("repo-{tag}-{run}") }),
            );
            let after_text = Server::text_of(&after);
            assert!(
                after_text.contains(&head_oid) && after_text.contains("FRESHNESS-STATE:"),
                "{backend} status reports the observation: {after_text}"
            );
            observed.push(after_text);
        }
        // Parity: every recipe's final status carries the same bound revision
        // facts (same branch, same observed HEAD, same policy semantics).
        for text in &observed {
            assert!(text.contains("refs/heads/main"), "parity branch: {text}");
            assert!(text.contains(&head_oid), "parity observed oid: {text}");
        }
    }

    /// typed_errors_capabilities: failures are typed, inspectable diagnoses
    /// (BindingRequired kinds in the JSON-RPC error envelope) and the
    /// capability report exposes the bound-code states.
    #[test]
    fn typed_errors_capabilities() {
        let run = unique_suffix();
        for (backend, tag) in backends() {
            let mut server = spawn_server(backend, &format!("typed-{tag}"));

            // Unbound status: the typed BindingRequired denial (never prose).
            let unbound = server.tool(2, "code_status", json!({ "repo": format!("never-{tag}") }));
            let message = unbound["error"]["message"]
                .as_str()
                .unwrap_or_else(|| panic!("{backend} expected a typed error: {unbound}"));
            assert!(
                message.contains("BindingRequired"),
                "{backend} typed denial: {message}"
            );

            let unbound_preview = server.tool(
                20,
                "code_reset_preview",
                json!({ "scope": { "tenant": "task-transports", "workspace": format!("typed-{tag}") }, "repo": format!("never-{tag}") }),
            );
            assert!(
                unbound_preview["error"]["message"]
                    .as_str()
                    .is_some_and(|message| message.contains("BindingRequired")),
                "{backend} unbound reset preview denial: {unbound_preview}"
            );

            let unapproved_apply = server.tool(
                21,
                "code_reset_apply",
                json!({ "journal": {}, "decision": "" }),
            );
            assert!(
                unapproved_apply["error"]["message"]
                    .as_str()
                    .is_some_and(|message| message.contains("InvalidResetPreview")
                        || message.contains("ResetApprovalRequired")
                        || message.contains("invalid reset request")),
                "{backend} unapproved reset denial: {unapproved_apply}"
            );

            // Refresh without a registration: typed, and distinct from the
            // unbound-binding denial.
            let unregistered =
                server.tool(3, "code_refresh", json!({ "repo": format!("never-{tag}") }));
            let message = unregistered["error"]["message"]
                .as_str()
                .unwrap_or_else(|| panic!("{backend} expected a typed error: {unregistered}"));
            assert!(
                message.contains("no registered repository"),
                "{backend} unregistered denial: {message}"
            );

            // Wrong branch on a bound repo: BindingConflict.
            let (repo_path, _oid) = git_fixture(&format!("typed-{tag}"));
            server.tool(
                4,
                "code_repo_register",
                json!({ "repo": format!("typed-{tag}-{run}"),
                        "path": repo_path.to_str().unwrap() }),
            );
            server.tool(
                5,
                "code_bind",
                json!({ "repo": format!("typed-{tag}-{run}"), "branchRef": "refs/heads/main" }),
            );
            let foreign_path = server.tool(
                60,
                "code_rebuild",
                json!({ "repo": format!("typed-{tag}-{run}"), "requestId": format!("foreign-{run}"), "path": "/foreign/repository" }),
            );
            assert!(
                foreign_path["error"]["message"]
                    .as_str()
                    .is_some_and(|message| message.contains("path is not an input")),
                "{backend} rebuild foreign path denial: {foreign_path}"
            );
            let conflict = server.tool(
                6,
                "code_bind",
                json!({ "repo": format!("typed-{tag}-{run}"), "branchRef": "refs/heads/other" }),
            );
            let message = conflict["error"]["message"]
                .as_str()
                .unwrap_or_else(|| panic!("{backend} expected a typed conflict: {conflict}"));
            assert!(
                message.contains("BindingConflict"),
                "{backend} conflict denial: {message}"
            );

            // Capability report: the bound-code states are inspectable.
            let report = server.tool(7, "capability_report", json!({}));
            let text = Server::text_of(&report);
            assert!(
                text.contains("code_binding: supported"),
                "{backend} capability code_binding: {text}"
            );
            assert!(
                text.contains("code_source: supported"),
                "{backend} capability code_source: {text}"
            );
            assert!(
                text.contains("code_reset: supported"),
                "{backend} capability code_reset: {text}"
            );
        }

        // A binary built WITHOUT the pgvector feature refuses the backend
        // with its typed error (verified only when the feature is absent —
        // in the pgvector-featured build this arm is skipped by design).
        #[cfg(not(feature = "pgvector"))]
        if pg_available() {
            let dir = tempfile::tempdir().unwrap();
            let storage = dir.path().join("storage");
            std::fs::create_dir_all(&storage).unwrap();
            let url = std::env::var("ENGRAM_TEST_PG_URL").unwrap();
            let output = Command::new(env!("CARGO_BIN_EXE_engram-mcp"))
                .args([
                    "--storage",
                    storage.to_str().unwrap(),
                    "--org",
                    "task-transports",
                    "--project",
                    "no-feature",
                    "--no-vector",
                    "--backend",
                    "pgvector",
                ])
                .env("ENGRAM_PG_CONNECTION_STRING", url)
                .output()
                .expect("spawn featureless binary");
            let text = String::from_utf8_lossy(&output.stderr).to_string()
                + &String::from_utf8_lossy(&output.stdout);
            assert!(
                text.contains("pgvector feature"),
                "featureless refusal must be typed: {text}"
            );
        }
    }
}

// ---------- task_022 focused cases ------------------------------------------

mod task_022 {
    use super::*;

    /// One one-shot invocation of the binary; returns (exit code, stdout).
    fn oneshot(args: &[&str], envs: &[(&str, &str)]) -> (i32, String) {
        let mut command = Command::new(env!("CARGO_BIN_EXE_engram-mcp"));
        command
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for (key, value) in envs {
            command.env(key, value);
        }
        let output = command.output().expect("spawn one-shot");
        let text = String::from_utf8_lossy(&output.stdout).to_string()
            + &String::from_utf8_lossy(&output.stderr);
        (output.status.code().unwrap_or(-1), text)
    }

    fn oneshot_args(storage: &std::path::Path, tag: &str, rest: &[&str]) -> Vec<String> {
        let mut args: Vec<String> = vec![
            "--storage".into(),
            storage.to_str().unwrap().into(),
            "--org".into(),
            "task-transports".into(),
            "--project".into(),
            tag.into(),
            "--no-vector".into(),
        ];
        args.extend(rest.iter().map(|s| (*s).to_owned()));
        args
    }

    fn storage_for(tag: &str) -> std::path::PathBuf {
        let dir = tempfile::tempdir().expect("tempdir");
        let storage = dir.path().join("storage");
        std::fs::create_dir_all(&storage).expect("mkdir storage");
        std::mem::forget(dir);
        storage
    }

    fn oneshot_refs(args: &[String]) -> Vec<&str> {
        args.iter().map(String::as_str).collect()
    }

    /// oneshot_dispatch_parity: the one-shot `code` subcommands return the
    /// SAME bound-revision facts the stdio tools produce, on every recipe.
    #[test]
    fn oneshot_dispatch_parity() {
        let (repo_path, head_oid) = git_fixture("oneshot");
        let run = unique_suffix();
        for (backend, tag) in backends() {
            let storage = storage_for(&format!("oneshot-{tag}"));
            let mut envs: Vec<(&str, &str)> = Vec::new();
            let mut extra: Vec<&str> = Vec::new();
            if backend == "pgvector" {
                let url = std::env::var("ENGRAM_TEST_PG_URL").expect("pg url");
                envs.push(("ENGRAM_PG_CONNECTION_STRING", url.leak()));
                extra = vec!["--backend", "pgvector"];
            }
            let base = {
                let mut args = oneshot_args(&storage, &format!("oneshot-{tag}"), &extra);
                args.insert(0, "code".into());
                args
            };
            let mut args = base.clone();
            args.extend(
                [
                    "--op",
                    "register",
                    "--repo",
                    &format!("repo-{tag}-{run}"),
                    "--path",
                    repo_path.to_str().unwrap(),
                ]
                .iter()
                .map(|s| (*s).to_owned()),
            );
            let (code, text) = oneshot(&oneshot_refs(&args), &envs);
            assert_eq!(code, 0, "{backend} register: {text}");
            assert!(text.contains("REGISTERED"), "{backend} register: {text}");

            let mut args = base.clone();
            args.extend(
                [
                    "--op",
                    "bind",
                    "--repo",
                    &format!("repo-{tag}-{run}"),
                    "--branch-ref",
                    "refs/heads/main",
                ]
                .iter()
                .map(|s| (*s).to_owned()),
            );
            let (code, text) = oneshot(&oneshot_refs(&args), &envs);
            assert_eq!(code, 0, "{backend} bind: {text}");
            assert!(
                text.contains("BOUND repo-") && text.contains("refs/heads/main"),
                "{backend} one-shot bind parity: {text}"
            );

            let mut args = base.clone();
            args.extend(
                ["--op", "status", "--repo", &format!("repo-{tag}-{run}")]
                    .iter()
                    .map(|s| (*s).to_owned()),
            );
            let (code, text) = oneshot(&oneshot_refs(&args), &envs);
            assert_eq!(code, 0, "{backend} status: {text}");
            assert!(
                text.contains("FRESHNESS: unknown") && text.contains("INDEXED: none"),
                "{backend} one-shot status parity: {text}"
            );

            let mut args = base.clone();
            args.extend(
                ["--op", "refresh", "--repo", &format!("repo-{tag}-{run}")]
                    .iter()
                    .map(|s| (*s).to_owned()),
            );
            let (code, text) = oneshot(&oneshot_refs(&args), &envs);
            assert_eq!(code, 0, "{backend} refresh: {text}");
            assert!(
                text.contains(&head_oid),
                "{backend} one-shot refresh observes the real ref: {text}"
            );

            // Public registered-locator rebuild publishes before reset. The caller
            // supplies a retry correlation key, never a path or checkout.
            let mut args = base.clone();
            args.extend(
                [
                    "--op",
                    "rebuild",
                    "--repo",
                    &format!("repo-{tag}-{run}"),
                    "--request-id",
                    &format!("rebuild-{run}"),
                ]
                .iter()
                .map(|s| (*s).to_owned()),
            );
            let (code, text) = oneshot(&oneshot_refs(&args), &envs);
            assert_eq!(code, 0, "{backend} rebuild: {text}");
            assert!(
                text.contains("generationId"),
                "{backend} rebuild publishes: {text}"
            );

            let mut args = base.clone();
            args.extend(
                [
                    "--op",
                    "reset-preview",
                    "--repo",
                    &format!("repo-{tag}-{run}"),
                ]
                .iter()
                .map(|s| (*s).to_owned()),
            );
            let (code, text) = oneshot(&oneshot_refs(&args), &envs);
            assert_eq!(code, 0, "{backend} reset preview: {text}");
            let preview_line = text
                .lines()
                .find(|line| line.starts_with('{'))
                .expect("one-shot preview JSON line");
            let preview: Value = serde_json::from_str(preview_line)
                .unwrap_or_else(|error| panic!("{backend} reset preview JSON: {error}: {text}"));
            assert!(
                preview["targets"]
                    .as_array()
                    .is_some_and(|targets| !targets.is_empty()),
                "{backend} preview targets: {preview}"
            );
            assert!(
                preview["ambiguity"].is_array() && preview["preservationDigests"].is_array(),
                "{backend} preview inventories: {preview}"
            );
            let decision = format!("owner-approved-{run}");
            let journal = approved_journal(preview, &format!("oneshot-reset-{run}"), &decision);
            let journal_text = serde_json::to_string(&journal).expect("journal JSON");
            let mut args = base.clone();
            args.extend(
                [
                    "--op",
                    "reset-apply",
                    "--journal",
                    &journal_text,
                    "--decision",
                    &decision,
                ]
                .iter()
                .map(|s| (*s).to_owned()),
            );
            let (code, text) = oneshot(&oneshot_refs(&args), &envs);
            assert_eq!(code, 0, "{backend} reset apply: {text}");
            assert!(
                text.contains("completed:"),
                "{backend} reset apply lanes: {text}"
            );
            let mut args = base.clone();
            args.extend(
                ["--op", "reset-resume", "--journal", &journal_text]
                    .iter()
                    .map(|s| (*s).to_owned()),
            );
            let (code, text) = oneshot(&oneshot_refs(&args), &envs);
            assert_eq!(code, 0, "{backend} reset resume: {text}");
            assert!(
                text.contains("completed:"),
                "{backend} reset resume idempotency: {text}"
            );

            // Unbound preview and unapproved apply remain nonzero typed denials.
            let mut args = base.clone();
            args.extend(
                ["--op", "reset-preview", "--repo", "never-bound"]
                    .iter()
                    .map(|s| (*s).to_owned()),
            );
            let (code, text) = oneshot(&oneshot_refs(&args), &envs);
            assert_ne!(code, 0, "{backend} unbound preview must fail: {text}");
            assert!(
                text.contains("BindingRequired"),
                "{backend} typed unbound preview: {text}"
            );
            let mut args = base.clone();
            args.extend(
                ["--op", "reset-apply", "--journal", "{}", "--decision", ""]
                    .iter()
                    .map(|s| (*s).to_owned()),
            );
            let (code, text) = oneshot(&oneshot_refs(&args), &envs);
            assert_ne!(code, 0, "{backend} unapproved apply must fail: {text}");
            assert!(
                text.contains("invalid reset request") || text.contains("ResetApprovalRequired"),
                "{backend} unapproved apply denial: {text}"
            );

            // Same status facts as stdio afterward (observed at HEAD).
            let mut args = base;
            args.extend(
                ["--op", "status", "--repo", &format!("repo-{tag}-{run}")]
                    .iter()
                    .map(|s| (*s).to_owned()),
            );
            let (code, text) = oneshot(&oneshot_refs(&args), &envs);
            assert_eq!(code, 0);
            assert!(
                text.contains(&head_oid) && text.contains("refs/heads/main"),
                "{backend} one-shot/stdio dispatch parity: {text}"
            );
        }
    }

    /// backfill_not_rescan: the EXPLICIT embedding-backfill name carries
    /// distinct semantics — it never touches the binding (status byte-equal
    /// before/after), its help says it is NOT a code rebuild, and the
    /// ambiguous `reindex` alias is a deprecation diagnostic naming both.
    #[test]
    fn backfill_not_rescan() {
        // Review r6 finding 8: the backfill semantics run against BOTH
        // recipes in the Pg-featured target (not SQLite alone).
        for (backend, tag) in backends() {
            backfill_not_rescan_case(backend, &format!("backfill-{tag}"));
        }
    }

    fn backfill_not_rescan_case(backend: &str, tag: &str) {
        let run = unique_suffix();
        let repo_id = format!("repo-backfill-{tag}-{run}");
        let (repo_path, _oid) = git_fixture(tag);
        let storage = storage_for(tag);
        let mut envs: Vec<(&str, &str)> = Vec::new();
        let mut extra: Vec<&str> = Vec::new();
        if backend == "pgvector" {
            let url = std::env::var("ENGRAM_TEST_PG_URL").expect("pg url");
            envs.push(("ENGRAM_PG_CONNECTION_STRING", url.leak()));
            extra = vec!["--backend", "pgvector"];
        }
        for op_args in [
            &mut base_args(
                &storage,
                tag,
                &extra,
                &repo_id,
                repo_path.to_str().unwrap(),
                "register",
            ),
            &mut base_args(&storage, tag, &extra, &repo_id, "", "bind"),
        ] {
            let (code, text) = oneshot(&oneshot_refs(op_args), &envs);
            assert_eq!(code, 0, "{backend} setup: {text}");
        }
        fn base_args(
            storage: &std::path::Path,
            tag: &str,
            extra: &[&str],
            repo_id: &str,
            path: &str,
            op: &str,
        ) -> Vec<String> {
            let mut args = oneshot_args(storage, tag, extra);
            args.insert(0, "code".into());
            let mut tail = vec![
                "--op".to_owned(),
                op.to_owned(),
                "--repo".to_owned(),
                repo_id.to_owned(),
            ];
            if op == "register" {
                tail.push("--path".to_owned());
                tail.push(path.to_owned());
            } else if op == "bind" {
                tail.push("--branch-ref".to_owned());
                tail.push("refs/heads/main".to_owned());
            }
            args.extend(tail);
            args
        }

        let mut status_args = oneshot_args(&storage, tag, &extra);
        status_args.insert(0, "code".into());
        status_args.extend(
            ["--op", "status", "--repo", repo_id.as_str()]
                .iter()
                .map(|s| (*s).to_owned()),
        );
        let (_, before) = oneshot(&oneshot_refs(&status_args), &envs);

        // Help: the explicit distinction, in the operator surface.
        let (code, help) = oneshot(&["backfill-embeddings", "--help"], &[]);
        assert_eq!(code, 0);
        assert!(
            help.contains("NOT a code rescan") && help.contains("NOT a branch rebuild"),
            "help must distinguish semantics: {help}"
        );

        // The run itself (no vector lane in this build): a typed NONZERO
        // failure about the embedding lane — never a binding mutation.
        let mut backfill = oneshot_args(&storage, tag, &extra);
        backfill.insert(0, "backfill-embeddings".into());
        let (code, text) = oneshot(&oneshot_refs(&backfill), &envs);
        assert_ne!(code, 0, "a failed mutation must exit nonzero: {text}");
        assert!(
            text.contains("no embedding provider wired"),
            "typed embedding-lane diagnosis: {text}"
        );

        // The binding is byte-equal: backfill NEVER rescans/rebuilds code.
        let (_, after) = oneshot(&oneshot_refs(&status_args), &envs);
        assert_eq!(
            before, after,
            "embedding backfill must not touch the binding"
        );

        // The ambiguous alias is deprecated with the distinction spelled out.
        let (code, diagnostic) = oneshot(&["reindex"], &[]);
        assert_eq!(code, 2, "deprecated alias exits 2: {diagnostic}");
        assert!(
            diagnostic.contains("deprecated")
                && diagnostic.contains("backfill-embeddings")
                && diagnostic.contains("NEVER a branch rebuild"),
            "alias diagnostic names both semantics: {diagnostic}"
        );
    }

    /// read_error_envelope: the read-hook fail-open contract is preserved —
    /// `recall --json` / `status --json` report `{ok:false, error}` and exit
    /// ZERO even when the store cannot open.
    #[test]
    fn read_error_envelope() {
        // Review r6 finding 8: the read-hook fail-open contract is verified
        // on BOTH recipes — SQLite via an impossible storage path, pgvector
        // via a refused connection (the envelope contract, not the fixture).
        for (backend, tag) in backends() {
            read_error_envelope_case(backend, &format!("envelope-{tag}"));
        }
    }

    fn read_error_envelope_case(backend: &str, tag: &str) {
        // A storage path that cannot exist (its parent is a FILE).
        let dir = tempfile::tempdir().unwrap();
        let blocker = dir.path().join("blocker");
        std::fs::write(&blocker, b"not a directory").unwrap();
        let impossible = blocker.join("storage");
        let mut envs: Vec<(&str, &str)> = Vec::new();
        let mut extra: Vec<&str> = Vec::new();
        if backend == "pgvector" {
            // Refused connection: the open fails BEFORE the envelope check,
            // exercising the same read-hook fail-open on the Pg recipe.
            envs.push((
                "ENGRAM_PG_CONNECTION_STRING",
                "postgresql://engram:nobody@127.0.0.1:1/none",
            ));
            extra = vec!["--backend", "pgvector"];
        }

        let (code, text) = oneshot(
            &[
                "recall",
                "--json",
                "--query",
                "anything",
                "--storage",
                impossible.to_str().unwrap(),
                "--org",
                "task-transports",
                "--project",
                tag,
                "--no-vector",
            ]
            .iter()
            .chain(extra.iter())
            .copied()
            .collect::<Vec<_>>(),
            &envs,
        );
        assert_eq!(code, 0, "read-hook fail-open stays exit 0: {text}");
        let envelope: Value = serde_json::from_str(
            text.lines()
                .rev()
                .find(|line| line.starts_with('{'))
                .expect("a JSON envelope line"),
        )
        .expect("envelope parses");
        assert_eq!(envelope["ok"], false, "envelope: {envelope}");
        assert_eq!(envelope["command"], "recall");
        assert!(
            envelope["error"]
                .as_str()
                .is_some_and(|error| !error.is_empty()),
            "envelope carries the error: {envelope}"
        );

        let (code, text) = oneshot(
            &[
                "status",
                "--json",
                "--storage",
                impossible.to_str().unwrap(),
                "--org",
                "task-transports",
                "--project",
                tag,
                "--no-vector",
            ]
            .iter()
            .chain(extra.iter())
            .copied()
            .collect::<Vec<_>>(),
            &envs,
        );
        assert_eq!(code, 0, "status fail-open stays exit 0: {text}");
        let envelope: Value = serde_json::from_str(
            text.lines()
                .rev()
                .find(|line| line.starts_with('{'))
                .expect("a JSON envelope line"),
        )
        .expect("envelope parses");
        assert_eq!(envelope["ok"], false);
        assert_eq!(envelope["command"], "status");
    }

    /// mutation_nonzero: the NEW mutating one-shot operations exit NONZERO on
    /// failure — conflicting bind, unregistered refresh, missing arguments.
    #[test]
    fn mutation_nonzero() {
        let run = unique_suffix();
        for (backend, tag) in backends() {
            let storage = storage_for(&format!("mut-{tag}"));
            let mut envs: Vec<(&str, &str)> = Vec::new();
            let mut extra: Vec<&str> = Vec::new();
            if backend == "pgvector" {
                let url = std::env::var("ENGRAM_TEST_PG_URL").expect("pg url");
                envs.push(("ENGRAM_PG_CONNECTION_STRING", url.leak()));
                extra = vec!["--backend", "pgvector"];
            }
            let (repo_path, _oid) = git_fixture(&format!("mut-{tag}"));
            let mut base = oneshot_args(&storage, &format!("mut-{tag}"), &extra);
            base.insert(0, "code".into());

            // register + bind main
            let mut reg = base.clone();
            reg.extend(
                [
                    "--op",
                    "register",
                    "--repo",
                    &format!("repo-{tag}-{run}"),
                    "--path",
                ]
                .iter()
                .map(|s| (*s).to_owned()),
            );
            reg.push(repo_path.to_str().unwrap().to_owned());
            assert_eq!(oneshot(&oneshot_refs(&reg), &envs).0, 0);
            let mut bind = base.clone();
            bind.extend(
                [
                    "--op",
                    "bind",
                    "--repo",
                    &format!("repo-{tag}-{run}"),
                    "--branch-ref",
                    "refs/heads/main",
                ]
                .iter()
                .map(|s| (*s).to_owned()),
            );
            let (code, text) = oneshot(&oneshot_refs(&bind), &envs);
            assert_eq!(code, 0, "{backend} bind: {text}");

            // Conflicting branch: nonzero, typed BindingConflict.
            let mut conflict = base.clone();
            conflict.extend(
                [
                    "--op",
                    "bind",
                    "--repo",
                    &format!("repo-{tag}-{run}"),
                    "--branch-ref",
                    "refs/heads/other",
                ]
                .iter()
                .map(|s| (*s).to_owned()),
            );
            let (code, text) = oneshot(&oneshot_refs(&conflict), &envs);
            assert_ne!(code, 0, "{backend} conflict must be nonzero: {text}");
            assert!(text.contains("BindingConflict"), "{backend}: {text}");

            // Unregistered refresh: nonzero, typed.
            let mut refresh = base.clone();
            refresh.extend(
                ["--op", "refresh", "--repo", "never-registered"]
                    .iter()
                    .map(|s| (*s).to_owned()),
            );
            let (code, text) = oneshot(&oneshot_refs(&refresh), &envs);
            assert_ne!(code, 0, "{backend} unregistered refresh: {text}");
            assert!(
                text.contains("no registered repository"),
                "{backend}: {text}"
            );

            // Missing required arguments: nonzero usage failure.
            let (code, text) = oneshot(&oneshot_refs(&base), &envs);
            assert_ne!(code, 0, "{backend} missing op: {text}");
        }
    }
}
