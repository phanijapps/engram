//! Actual MCP stdio and one-shot consumers use only the selected store.
use serde_json::{Value, json};
use std::{
    io::Write,
    path::Path,
    process::{Command, Stdio},
};

struct OwnedRoot(Option<tempfile::TempDir>);
impl OwnedRoot {
    fn new() -> Self {
        Self(Some(
            tempfile::Builder::new()
                .prefix("mcp-selection-")
                .tempdir()
                .unwrap(),
        ))
    }
    fn path(&self) -> &Path {
        self.0.as_ref().unwrap().path()
    }
}
impl Drop for OwnedRoot {
    fn drop(&mut self) {
        if std::thread::panicking() {
            if let Some(root) = self.0.take() {
                eprintln!("MCP_SELECTION_RETAIN {}", root.keep().display());
            }
        }
    }
}
fn command() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_engram-mcp"));
    let owned_credential = std::env::var("HELIX_TEST_CREDENTIAL_ENV").ok();
    // Fixture selection is explicit; unrelated caller profiles cannot alter it.
    for (name, _) in std::env::vars() {
        if owned_credential.as_deref() == Some(name.as_str()) {
            continue;
        }
        if name.starts_with("ENGRAM_EMBEDDING_")
            || name.starts_with("ENGRAM_HELIX_")
            || matches!(
                name.as_str(),
                "ENGRAM_BACKEND"
                    | "ENGRAM_STORAGE"
                    | "ENGRAM_PROJECT"
                    | "ENGRAM_TRUSTED_ROOT"
                    | "ENGRAM_PG_CONNECTION_STRING"
            )
        {
            command.env_remove(name);
        }
    }
    command
}
fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).into()).collect()
}
fn common(root: &Path) -> Vec<String> {
    vec![
        "--storage".into(),
        root.join("uncreated/provider/storage")
            .display()
            .to_string(),
        "--trusted-root".into(),
        root.display().to_string(),
        "--project".into(),
        "selection-project".into(),
    ]
}
fn one_shot(name: &str, args: &[String]) -> Value {
    let output = command()
        .arg(name)
        .args(args)
        .arg("--json")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "one-shot failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    serde_json::from_str(
        stdout
            .lines()
            .rev()
            .find(|line| line.trim().starts_with('{'))
            .expect("one-shot JSON result"),
    )
    .unwrap()
}
fn assert_no_sqlite(root: &Path) {
    for file in [
        "engram_data.db",
        "memory.db",
        "knowledge.db",
        "belief.db",
        "hierarchy.db",
        "vectors.db",
    ] {
        assert!(
            !root.join("uncreated/provider/storage").join(file).exists(),
            "unselected SQLite file was opened: {file}"
        );
    }
}

#[cfg(not(feature = "pgvector"))]
#[test]
fn unpackaged_pgvector_does_not_create_storage() {
    let root = OwnedRoot::new();
    let storage = root.path().join("uncreated/provider/storage");
    let mut args = common(root.path());
    args.extend(strings(&[
        "--backend",
        "pgvector",
        "--pg-connection-string",
        "postgres://fixture:fixture@127.0.0.1:9/fixture",
        "--no-vector",
    ]));
    assert!(!storage.exists());
    let result = one_shot("status", &args);
    assert_eq!(result["ok"], false);
    assert!(
        result["error"]
            .as_str()
            .unwrap()
            .contains("BuildFeatureMissing")
    );
    assert!(
        !storage.exists(),
        "unpackaged PG selection created storage before rejection: {}",
        storage.display()
    );
}
#[cfg(not(feature = "helixdb-service"))]
#[test]
fn unpackaged_service_rejects_before_active_embedding_profile_resolution() {
    let root = OwnedRoot::new();
    let mut args = common(root.path());
    args.extend(strings(&[
        "--backend",
        "helixdb",
        "--helix-mode",
        "service",
        "--helix-store",
        "store",
        "--helix-endpoint",
        "http://127.0.0.1:9",
        "--helix-credential-env",
        "UNUSED_CREDENTIAL",
        "--embedding-config",
    ]));
    args.push(
        root.path()
            .join("must-not-be-read.toml")
            .display()
            .to_string(),
    );
    let result = one_shot("status", &args);
    assert_eq!(result["ok"], false);
    assert!(
        result["error"]
            .as_str()
            .unwrap()
            .contains("BuildFeatureMissing"),
        "provider check must precede active embedding configuration: {result}"
    );
    assert!(!root.path().join("uncreated").exists());
}
#[test]
fn conflicting_provider_flags_reject_before_storage_or_embedding_profiles() {
    let root = OwnedRoot::new();
    let mut args = common(root.path());
    args.extend(strings(&[
        "--backend",
        "sqlite",
        "--helix-mode",
        "embedded",
        "--embedding-config",
    ]));
    args.push(
        root.path()
            .join("must-not-be-read.toml")
            .display()
            .to_string(),
    );
    let result = one_shot("status", &args);
    assert_eq!(result["ok"], false);
    assert!(result["error"].as_str().unwrap().contains("conflict"));
    assert!(!root.path().join("uncreated").exists());
}
#[test]
fn helix_requires_an_explicit_mode_before_any_storage_access() {
    let root = OwnedRoot::new();
    let mut args = common(root.path());
    args.extend(strings(&[
        "--backend",
        "helixdb",
        "--helix-store",
        "store",
        "--no-vector",
    ]));
    let result = one_shot("status", &args);
    assert_eq!(result["ok"], false);
    assert!(
        result["error"]
            .as_str()
            .unwrap()
            .contains("explicit Helix mode")
    );
    assert!(!root.path().join("uncreated").exists());
}
#[cfg(feature = "sqlite")]
#[test]
fn legacy_default_sqlite_creates_a_valid_deep_storage_tree() {
    let root = OwnedRoot::new();
    let mut args = common(root.path());
    args.drain(2..4); // legacy launch infers nearest existing ancestor
    args.extend(strings(&["--no-vector"]));
    let result = one_shot("status", &args);
    assert_eq!(result["ok"], true);
    assert!(
        root.path()
            .join("uncreated/provider/storage/engram_data.db")
            .is_file()
    );
}
#[cfg(feature = "sqlite")]
#[test]
fn escaping_storage_rejects_before_active_embedding_profile_read() {
    let root = OwnedRoot::new();
    let trusted = root.path().join("trusted");
    std::fs::create_dir(&trusted).unwrap();
    let storage = root.path().join("outside/store");
    let mut args = strings(&[
        "--storage",
        storage.to_str().unwrap(),
        "--trusted-root",
        trusted.to_str().unwrap(),
        "--backend",
        "sqlite",
        "--embedding-config",
    ]);
    args.push(
        root.path()
            .join("must-not-be-read.toml")
            .display()
            .to_string(),
    );
    let result = one_shot("status", &args);
    assert_eq!(result["ok"], false);
    assert!(result["error"].as_str().unwrap().contains("validation"));
    assert!(!storage.exists());
}

#[cfg(any(feature = "helixdb-service", feature = "helixdb-embedded"))]
fn selected_helix(mode: &str) {
    let root = OwnedRoot::new();
    let mut args = common(root.path());
    let unique = format!(
        "mcp-selection-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    args.extend(strings(&[
        "--backend",
        "helixdb",
        "--helix-mode",
        mode,
        "--helix-store",
        &unique,
        "--org",
        "selection-org",
        "--domain",
        "selection-domain",
        "--subdomain",
        "selection-sub",
        "--embedding-provider",
        "none",
    ]));
    if mode == "embedded" {
        args.extend(strings(&[
            "--no-vector",
            "--helix-path",
            root.path().join("selected-embedded").to_str().unwrap(),
        ]));
    } else {
        let endpoint = std::env::var("HELIX_TEST_ENDPOINT")
            .expect("actual isolated service endpoint required");
        let credential = std::env::var("HELIX_TEST_CREDENTIAL_ENV")
            .expect("actual owned service credential reference required");
        args.extend(strings(&[
            "--vector",
            "--helix-endpoint",
            &endpoint,
            "--helix-credential-env",
            &credential,
        ]));
    }
    let sentinel = format!("selectedstorememory{unique}");
    let mut child = command()
        .args(&args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    {
        let mut input = child.stdin.take().unwrap();
        for request in [
            json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}),
            json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"write_memory","arguments":{"content":sentinel}}}),
        ] {
            writeln!(input, "{request}").unwrap();
        }
    }
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "selected stdio startup/write failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let responses: Vec<Value> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect();
    let write = responses
        .iter()
        .find(|response| response["id"] == 2)
        .expect("stdio memory write response");
    assert!(write.get("error").is_none(), "write error: {write}");
    assert_ne!(
        write["result"]["isError"], true,
        "tool write failed: {write}"
    );
    let mut recall = args.clone();
    recall.extend(strings(&["--query", &sentinel]));
    let result = one_shot("recall", &recall);
    assert_eq!(result["ok"], true, "{result}");
    assert!(
        result["hit_count"].as_u64().unwrap() > 0,
        "same selected store must reopen the stdio write: {result}"
    );
    let mut other = recall.clone();
    let store = other.iter().position(|arg| arg == "--helix-store").unwrap() + 1;
    other[store] = format!("{unique}-other");
    let wrong = one_shot("recall", &other);
    assert_eq!(wrong["ok"], true);
    assert_eq!(
        wrong["hit_count"], 0,
        "other logical store cannot recall selected-store sentinel"
    );
    assert_no_sqlite(root.path());
    if mode == "embedded" {
        assert!(root.path().join("selected-embedded").exists());
    }
}
#[cfg(feature = "helixdb-embedded")]
#[test]
fn embedded_stdio_and_oneshot_reopen_only_selected_store() {
    selected_helix("embedded")
}
#[cfg(feature = "helixdb-service")]
#[test]
fn service_stdio_and_oneshot_reopen_only_selected_store() {
    selected_helix("service")
}

#[cfg(feature = "helixdb-service")]
#[test]
fn missing_service_credentials_return_safe_selected_provider_error_without_fallback() {
    let root = OwnedRoot::new();
    let mut args = common(root.path());
    let reference = format!("MCP_SELECTION_MISSING_CREDENTIAL_{}", std::process::id());
    args.extend(strings(&[
        "--backend",
        "helixdb",
        "--helix-mode",
        "service",
        "--helix-store",
        "missing-credential",
        "--helix-endpoint",
        "http://127.0.0.1:9",
        "--helix-credential-env",
        &reference,
        "--vector",
        "--embedding-provider",
        "none",
    ]));
    let output = command()
        .env_remove(&reference)
        .arg("status")
        .args(&args)
        .arg("--json")
        .output()
        .unwrap();
    assert!(output.status.success());
    let result: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["ok"], false);
    assert!(result["error"].as_str().unwrap().contains("helix"));
    assert_no_sqlite(root.path());
}

#[test]
fn conflicting_repeated_provider_declarations_are_pure() {
    for flags in [
        vec!["--backend", "sqlite", "--backend", "helixdb"],
        vec![
            "--backend",
            "helixdb",
            "--helix-mode",
            "service",
            "--helix-mode",
            "embedded",
        ],
    ] {
        let root = OwnedRoot::new();
        let mut args = common(root.path());
        args.extend(strings(&flags));
        let result = one_shot("status", &args);
        assert_eq!(result["ok"], false);
        assert!(result["error"].as_str().unwrap().contains("conflicting"));
        assert!(!root.path().join("uncreated").exists());
    }
}

#[test]
fn invalid_provider_does_not_echo_supplied_secret_marker() {
    let root = OwnedRoot::new();
    let mut args = common(root.path());
    args.extend(strings(&[
        "--backend",
        "credential-sentinel-invalid-provider",
    ]));
    let result = one_shot("status", &args);
    assert_eq!(result["ok"], false);
    assert!(!result.to_string().contains("credential-sentinel"));
    assert!(!root.path().join("uncreated").exists());
}
