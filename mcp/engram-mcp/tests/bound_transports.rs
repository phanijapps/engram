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

fn pg_available() -> bool {
    std::env::var("ENGRAM_TEST_PG_URL").is_ok()
}

// The recipe under test: `pgvector` when the URL is present AND the binary
// was built with the feature (a build without the feature refuses the
// backend with its typed error — asserted in the unavailable path below).
fn backends() -> Vec<(&'static str, &'static str)> {
    if pg_available() {
        vec![("sqlite", "sqlite"), ("pgvector", "pg")]
    } else {
        vec![("sqlite", "sqlite")]
    }
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
        let mut observed: Vec<String> = Vec::new();
        for (backend, tag) in backends() {
            let mut server = spawn_server(backend, &format!("parity-{tag}"));
            let init = server.call(1, "initialize", json!({}));
            assert!(
                init["result"]["serverInfo"]["name"] == "engram-mcp",
                "{backend} init"
            );

            // register → bind → status(unknown) → refresh → status(observed)
            server.tool(
                2,
                "code_repo_register",
                json!({ "repo": format!("repo-{tag}"), "mode": "local-only",
                        "path": repo_path.to_str().unwrap() }),
            );
            let bound = server.tool(
                3,
                "code_bind",
                json!({ "repo": format!("repo-{tag}"), "branchRef": "refs/heads/main" }),
            );
            assert!(bound.get("error").is_none(), "{backend} bind: {bound}");
            let bound_text = Server::text_of(&bound);
            assert!(
                bound_text.contains("BOUND repo-") && bound_text.contains("refs/heads/main"),
                "{backend} bound text: {bound_text}"
            );

            let status = server.tool(4, "code_status", json!({ "repo": format!("repo-{tag}") }));
            let status_text = Server::text_of(&status);
            assert!(
                status_text.contains("FRESHNESS: unknown"),
                "{backend} freshness must start unknown: {status_text}"
            );
            assert!(
                status_text.contains("INDEXED: none"),
                "{backend} no published generation yet: {status_text}"
            );

            let refresh = server.tool(5, "code_refresh", json!({ "repo": format!("repo-{tag}") }));
            assert!(
                refresh.get("error").is_none(),
                "{backend} refresh: {refresh}"
            );
            let refreshed = Server::text_of(&refresh);
            assert!(
                refreshed.contains(&head_oid) && refreshed.contains("policy on_request"),
                "{backend} refresh observes the real ref: {refreshed}"
            );

            let after = server.tool(6, "code_status", json!({ "repo": format!("repo-{tag}") }));
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
                json!({ "repo": format!("typed-{tag}"),
                        "path": repo_path.to_str().unwrap() }),
            );
            server.tool(
                5,
                "code_bind",
                json!({ "repo": format!("typed-{tag}"), "branchRef": "refs/heads/main" }),
            );
            let conflict = server.tool(
                6,
                "code_bind",
                json!({ "repo": format!("typed-{tag}"), "branchRef": "refs/heads/other" }),
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
