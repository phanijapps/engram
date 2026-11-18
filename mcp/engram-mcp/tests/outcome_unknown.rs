//! Local fixture presentation only; actual transport-loss controls run later.
#[path = "../src/main.rs"]
mod host;
pub(crate) use host::*;

#[cfg(all(feature = "helixdb-service", feature = "helixdb-embedded"))]
fn actual_host_faults(mode: &str) {
    let manifest = std::env::var("ENGRAM_PROVIDER_E2E_ARTIFACTS")
        .expect("BLOCKED: actual instrumented MCP artifact and fault-plan manifest required");
    assert!(std::path::Path::new(&manifest).is_absolute());
    assert!(std::path::Path::new(&manifest).is_file());
    let expected = std::env::var("ENGRAM_PROVIDER_E2E_EXPECTED_BUILD_RECORD")
        .expect("BLOCKED: independent orchestration build record required");
    let pin = std::env::var("ENGRAM_PROVIDER_E2E_EXPECTED_BUILD_RECORD_SHA256")
        .expect("BLOCKED: independently pinned build record hash required");
    assert_eq!(pin.len(), 64);
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    for host in ["mcp-stdio", "mcp-oneshot"] {
        let output = std::process::Command::new("/usr/bin/python3")
            .current_dir(root)
            .args([
                "tools/ci/run_provider_e2e.py",
                "--provider",
                "helixdb",
                "--mode",
                mode,
                "--host",
                host,
                "--phase",
                "faults",
                "--artifacts",
                &manifest,
                "--expected-build-record",
                &expected,
                "--expected-build-record-sha256",
                &pin,
                "--output",
                "runs/e2e/gate022-mcp",
            ])
            .output()
            .expect("actual host fault runner unavailable");
        assert!(
            output.status.success(),
            "actual host fault proof non-PASS: {} {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let report: serde_json::Value = serde_json::from_slice(
            output
                .stdout
                .split(|byte| *byte == b'\n')
                .filter(|line| !line.is_empty())
                .last()
                .unwrap(),
        )
        .unwrap();
        assert_eq!(report["status"], "PASS");
    }
}
#[cfg(all(feature = "helixdb-service", feature = "helixdb-embedded"))]
#[test]
fn outcome_unknown_service_actual_stdio_and_applicable_oneshot() {
    actual_host_faults("service");
}
#[cfg(all(feature = "helixdb-service", feature = "helixdb-embedded"))]
#[test]
fn outcome_unknown_embedded_actual_stdio_and_applicable_oneshot() {
    actual_host_faults("embedded");
}

#[test]
fn presentation_batch_retains_statuses_original_id_and_redacts_private_fields() {
    use engram_integration::{BatchStep, StepOutcome, StepStatus};
    let id = "11111111-2222-4333-8444-555555555555";
    let succeeded =
        tools::presentation_step(&StepOutcome::ok(BatchStep::Episode, StepStatus::Succeeded));
    let failed = tools::presentation_step(&StepOutcome {
        step: BatchStep::Facts,
        status: StepStatus::Failed,
        supersession: None,
        error: Some(engram_runtime::CoreError::Adapter {
            adapter: "helixdb".into(),
            message: format!(
                "OutcomeUnknown operation_id={id} <private credential=secret-token record=secret-record>"
            ),
        }),
    });
    let skipped =
        tools::presentation_step(&StepOutcome::ok(BatchStep::Entities, StepStatus::Skipped));
    assert!(succeeded.contains("Succeeded"));
    assert!(failed.contains("Failed"));
    assert!(skipped.contains("Skipped"));
    assert!(failed.contains(id));
    assert!(failed.contains("OutcomeUnknown"));
    assert!(!failed.contains("secret-token"));
    assert!(!failed.contains("secret-record"));
}

struct PresentationBatch(bool);
#[async_trait::async_trait]
impl engram_integration::BatchIngest for PresentationBatch {
    fn transaction_guarantee(&self) -> engram_integration::TransactionGuarantee {
        engram_integration::TransactionGuarantee::BestEffort
    }
    async fn ingest(
        &self,
        _: engram_integration::BatchIngestRequest,
    ) -> engram_runtime::CoreResult<engram_integration::BatchOutcome> {
        use engram_integration::*;
        let error = engram_runtime::CoreError::Adapter {adapter:"helixdb".into(),message:"OutcomeUnknown operation_id=11111111-2222-4333-8444-555555555555 <private credential=secret-token record=secret-record>".into()};
        if self.0 {
            return Err(error);
        }
        Ok(BatchOutcome {
            guarantee: TransactionGuarantee::BestEffort,
            status: BatchStatus::Partial,
            steps: vec![
                StepOutcome::ok(BatchStep::Episode, StepStatus::Succeeded),
                StepOutcome {
                    step: BatchStep::Facts,
                    status: StepStatus::Failed,
                    error: Some(error),
                    supersession: None,
                },
                StepOutcome::ok(BatchStep::Entities, StepStatus::Skipped),
            ],
        })
    }
}
#[test]
fn presentation_store_knowledge_actual_handler_preserves_envelope_and_prior_success() {
    for terminal_error in [false, true] {
        let provider = engram_integration::EngramProviderBuilder::new(
            engram_integration::CapabilityReport::new(engram_domain::CapabilityState::Supported),
        )
        .batch(std::sync::Arc::new(PresentationBatch(terminal_error)))
        .build();
        let app = app::App {
            provider,
            scope: engram_domain::Scope {
                tenant: "presentation".into(),
                subject: None,
                workspace: None,
                session: None,
                environment: None,
            },
            ontology: Default::default(),
            taxonomy: Default::default(),
            storage_dir: std::path::PathBuf::new(),
        };
        let result = tools::store_knowledge(
            &app,
            &serde_json::json!({"idempotency_key":"presentation","facts":[],"entities":[],"relationships":[]}),
        );
        let text = if terminal_error {
            let error = result.unwrap_err();
            assert_eq!(error.code, -32603);
            error.message
        } else {
            let value = result.unwrap();
            assert_eq!(value["content"][0]["type"], "text");
            let text = value["content"][0]["text"].as_str().unwrap().to_owned();
            assert!(text.contains("Episode: Succeeded"));
            assert!(text.contains("Facts: Failed"));
            assert!(text.contains("Entities: Skipped"));
            text
        };
        assert!(text.contains("11111111-2222-4333-8444-555555555555"));
        assert!(text.contains("OutcomeUnknown"));
        assert!(!text.contains("secret-token"));
        assert!(!text.contains("secret-record"));
    }
}

#[cfg(feature = "test-controls")]
#[test]
fn presentation_instrumented_stdio_initializes_exact_native_binding() {
    use std::{
        io::Write,
        process::{Command, Stdio},
    };
    let temp = tempfile::Builder::new()
        .prefix("mcp-instrumented-init-")
        .tempdir()
        .unwrap();
    let root = temp.keep(); // Retain on any failed assertion; remove only after reaped success.
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let revision = String::from_utf8(
        Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(&source)
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap();
    use sha2::{Digest, Sha256};
    let binary = env!("CARGO_BIN_EXE_engram-mcp");
    let artifact_digest = || {
        Sha256::digest(std::fs::read(binary).unwrap())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    };
    let artifact = artifact_digest();
    let declaration = root.join("fixture.json");
    std::fs::write(&declaration,serde_json::to_vec(&serde_json::json!({"invocation":"11111111-2222-4333-8444-555555555555","host":"mcp-stdio","sourceRoot":source,"sourceRevision":revision.trim(),"artifactSha256":artifact,"ownedRoot":root})).unwrap()).unwrap();
    let mut child = Command::new(binary)
        .args([
            "__fixture",
            declaration.to_str().unwrap(),
            "--backend",
            "helixdb",
            "--helix-mode",
            "embedded",
            "--helix-store",
            "fixture-init",
            "--storage",
            root.join("store").to_str().unwrap(),
            "--trusted-root",
            root.to_str().unwrap(),
            "--project",
            "fixture-init",
            "--no-vector",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    writeln!(stdin,"{}",serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"fixture","version":"1"}}})).unwrap();
    drop(stdin);
    let result = child.wait_with_output().unwrap();
    assert!(
        result.status.success(),
        "instrumented host startup failed; retained {}",
        root.display()
    );
    let response = String::from_utf8(result.stdout).unwrap();
    assert!(response.contains("serverInfo"));
    let observed: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("mcp-fixture-observation.json")).unwrap())
            .unwrap();
    assert_eq!(observed["binding"]["mode"], "embedded");
    assert_eq!(observed["binding"]["store"], "fixture-init");
    assert_eq!(observed["binding"]["host"], "mcp-stdio");
    assert_eq!(observed["binding"]["scope"]["workspace"], "fixture-init");
    assert_eq!(artifact_digest(), artifact);
    std::fs::remove_dir_all(root).unwrap();
}
