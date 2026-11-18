//! Isolated actual-port consumer. The external recorder attests consumed bytes;
//! packet identity is not a replacement for artifact/native-target verification.
use engram_domain::*;
use engram_integration::{EngramConfig, EngramProvider};
use engram_runtime::{CoreError, CoreResult, PolicyAuthorizer};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use crate::dispatch::*;
#[derive(Deserialize)]
struct Packet {
    config: EngramConfig,
    scope: Scope,
    invocation: String,
    source_revision: String,
    lock_sha256: String,
    target: String,
    artifact_sha256: String,
    calls: Vec<Call>,
    #[serde(default)]
    code_authority: Option<CodeAuthority>,
    #[serde(default)]
    fixture: Option<Value>,
    #[serde(default)]
    proof_obligations: Option<Value>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CodeAuthority {
    launch_scopes: Vec<Scope>,
}
#[derive(Deserialize)]
struct Call {
    id: String,
    operation: String,
    arguments: Value,
    expect: Expect,
    #[serde(default)]
    control: Option<Value>,
    #[serde(default)]
    after_unknown: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Expect {
    status: String,
    #[serde(default)]
    equals: BTreeMap<String, Value>,
    error_class: Option<String>,
}
struct Opened {
    provider: EngramProvider,
    fixture: Option<Box<dyn Fn(&Value) -> CoreResult<Value>>>,
}
fn open(packet: &Packet) -> CoreResult<Opened> {
    let policy: Arc<dyn PolicyAuthorizer> = Arc::new(ScopePolicy(packet.scope.clone()));
    let opened = if let Some(fixture) = &packet.fixture {
        #[cfg(feature = "test-controls")]
        {
            use engram_backend_provider::test_controls::{FixtureHost, FixtureIdentity};
            let identity = FixtureIdentity::new(
                &packet.invocation,
                FixtureHost::Rust,
                text(fixture, "sourceRoot")?.into(),
                packet.source_revision.clone(),
                packet.artifact_sha256.clone(),
                text(fixture, "ownedRoot")?.into(),
            )?;
            let opened = engram_backend_provider::test_controls::open(
                &packet.config,
                &packet.scope,
                policy,
                identity,
            )?;
            let binding = json!({"backend":opened.binding.backend,"mode":opened.binding.mode,
                "store":opened.binding.logical_store,"scope":opened.binding.scope});
            let control = opened.control;
            control.configure_trace_recorder(std::path::Path::new(text(fixture, "ownedRoot")?),
                json!({"host":"rust","invocation":packet.invocation,"sourceRevision":packet.source_revision,"artifactSha256":packet.artifact_sha256,"actual":binding}))
                .map_err(|_| invalid("fixture recorder unavailable"))?;
            let operate = Box::new(move |request: &Value| -> CoreResult<Value> {
                match text(request, "operation")? {
                    "snapshot" => {}
                    "loseAck" => control.lose_mutation_ack_after(
                        optional(request, "acknowledgedBefore")?.unwrap_or(0),
                    ),
                    "loseAckCleanup" => control.lose_mutation_ack_with_cleanup_failure_after(
                        optional(request, "acknowledgedBefore")?.unwrap_or(0),
                    ),
                    "definiteFailure" => control.reject_next_mutation_definitely(),
                    _ => return Err(invalid("invalid private fixture control")),
                }
                let snapshot = control.snapshot();
                Ok(
                    json!({"binding":binding,"events":snapshot.events_json(),"observations":snapshot.observations_json(),
                    "allSends":snapshot.all_sends,"reconciliationReads":snapshot.reconciliation_reads,
                    "mutationAttempts":snapshot.mutation_attempts,"landedCommits":snapshot.landed_commits,
                    "nativeReceipts":snapshot.native_receipts,"cleanupGuardRefusals":snapshot.cleanup_guard_refusals,"recorderFailed":control.recorder_failed()}),
                )
            });
            if let Some(n) = fixture.get("loseAckAfter") {
                operate(&json!({"operation":"loseAck","acknowledgedBefore":n}))?;
            }
            if fixture.get("definiteFailure").and_then(Value::as_bool) == Some(true) {
                operate(&json!({"operation":"definiteFailure"}))?;
            }
            Opened {
                provider: opened.provider,
                fixture: Some(operate),
            }
        }
        #[cfg(not(feature = "test-controls"))]
        {
            let _ = (fixture, policy);
            return Err(invalid(
                "private fixture requires the nondefault test-controls feature",
            ));
        }
    } else {
        Opened {
            provider: engram_backend_provider::open(&packet.config, &packet.scope, policy)?,
            fixture: None,
        }
    };
    let provider = if let Some(authority) = &packet.code_authority {
        for scope in &authority.launch_scopes {
            ScopePolicy(packet.scope.clone()).check(scope)?;
        }
        opened.provider.with_code_authorizer(Arc::new(
            engram_integration::code_binding::ScopeSetAuthorizer::new(
                authority.launch_scopes.clone(),
            ),
        ))?
    } else {
        opened.provider
    };
    Ok(Opened {
        provider,
        fixture: opened.fixture,
    })
}
fn snapshot(opened: &Opened) -> CoreResult<Value> {
    match &opened.fixture {
        Some(control) => control(&json!({"operation":"snapshot"})),
        None => Ok(Value::Null),
    }
}
fn delta(before: &Value, after: &Value, field: &str) -> CoreResult<Vec<Value>> {
    let old = before
        .get(field)
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    let current = after
        .get(field)
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if current.len() < old {
        return Err(invalid("fixture observation sequence regressed"));
    }
    Ok(current[old..].to_vec())
}
fn actual_target() -> CoreResult<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") if cfg!(target_env = "gnu") => Ok("linux-x64-gnu"),
        ("linux", "aarch64") if cfg!(target_env = "gnu") => Ok("linux-arm64-gnu"),
        ("macos", "aarch64") => Ok("darwin-arm64"),
        ("windows", "x86_64") => Ok("win32-x64"),
        _ => Err(invalid(
            "native consumer target is not in the supported inventory",
        )),
    }
}
fn executable_digest() -> CoreResult<String> {
    use std::io::Read;
    let path =
        std::env::current_exe().map_err(|_| invalid("consumer executable identity unavailable"))?;
    let mut file =
        std::fs::File::open(path).map_err(|_| invalid("consumer executable unavailable"))?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|_| invalid("consumer executable could not be hashed"))?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    Ok(hash
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}
fn validate_packet(packet: &Packet) -> CoreResult<()> {
    if packet.target != actual_target()? || packet.artifact_sha256 != executable_digest()? {
        return Err(invalid(
            "consumed executable hash or native target mismatch",
        ));
    }
    let hex = |s: &str, n| {
        s.len() == n
            && s.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    };
    if !hex(&packet.source_revision, 40)
        || !hex(&packet.lock_sha256, 64)
        || !hex(&packet.artifact_sha256, 64)
        || packet.invocation.is_empty()
        || packet.scope.tenant.is_empty()
        || packet.calls.is_empty()
        || ![
            "linux-x64-gnu",
            "linux-arm64-gnu",
            "darwin-arm64",
            "win32-x64",
        ]
        .contains(&packet.target.as_str())
    {
        return Err(invalid(
            "invalid invocation identity or empty operation plan",
        ));
    }
    let mut ids = BTreeSet::new();
    for call in &packet.calls {
        if call.id.is_empty()
            || call.id.len() > 128
            || !call
                .id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
            || !ids.insert(call.id.clone())
            || !["ok", "unsupported", "error"].contains(&call.expect.status.as_str())
            || call
                .after_unknown
                .as_deref()
                .is_some_and(|p| p != "blocked-admission")
        {
            return Err(invalid("invalid or duplicate call identity/expectation"));
        }
        for pointer in call.expect.equals.keys() {
            if !pointer.is_empty() && !pointer.starts_with('/') {
                return Err(invalid("invalid assertion JSON pointer"));
            }
        }
    }
    Ok(())
}
fn receipt(packet: &Packet, call: &Call, provider: &str, mode: &str) -> Value {
    let capability = [
        include_str!("../../../../adapters/integration/tests/fixtures/provider_operations.json"),
        include_str!(
            "../../../../adapters/integration/tests/fixtures/provider_code_operations.json"
        ),
    ]
    .into_iter()
    .filter_map(|text| serde_json::from_str::<Value>(text).ok())
    .flat_map(|document| {
        document["operations"]
            .as_array()
            .cloned()
            .unwrap_or_default()
    })
    .find(|row| row["operation"].as_str() == Some(call.operation.as_str()))
    .map(|row| row["capability"].clone())
    .unwrap_or(Value::Null);
    json!({"schema":"engram.provider-operation.v1","call_id":call.id,"operation":call.operation,
        "capability":capability,
        "provider":provider,"mode":mode,"host":"rust","target":packet.target,"scope":packet.scope,
        "store":packet.config.helixdb.as_ref().map(|helix|&helix.logical_store),
        "invocation":packet.invocation,"source_revision":packet.source_revision,"lock_sha256":packet.lock_sha256,
        "artifact_sha256":packet.artifact_sha256})
}
fn emit_blocked(
    packet: &Packet,
    call: &Call,
    provider: &str,
    mode: &str,
    reason: &str,
) -> CoreResult<()> {
    let mut row = receipt(packet, call, provider, mode);
    row["status"] = json!("BLOCKED");
    row["outcome"] = json!("error");
    row["reason"] = json!(reason);
    row["assertions"] = json!(["operation not executed"]);
    row["result_sha256"] = json!(digest(&json!({"blocked":reason}))?);
    println!("{row}");
    Ok(())
}
fn memory_sentinel(written: &Value, read: &Value, scope: &Scope) -> CoreResult<()> {
    let record = &written["record"];
    let id = record["id"]
        .as_str()
        .filter(|id| !id.is_empty())
        .ok_or_else(|| invalid("actual written memory ID missing"))?;
    let content = record["content"]["text"]
        .as_str()
        .filter(|text| !text.is_empty())
        .ok_or_else(|| invalid("actual written sentinel value missing"))?;
    let owner: Scope = serde_json::from_value(record["scope"].clone())
        .map_err(|_| invalid("written sentinel scope missing"))?;
    if &owner != scope {
        return Err(invalid("written sentinel full scope differs"));
    }
    if let Some(items) = read
        .get("items")
        .unwrap_or(&read["context"]["items"])
        .as_array()
    {
        if items.iter().any(|item| {
            item["targetId"] == id && item["targetType"] == "memory" && item["content"] == content
        }) {
            return Ok(());
        }
    }
    if let Some(items) = read["memories"].as_array() {
        for item in items {
            if item["id"] == id && item["content"] == content {
                let owner: Scope = serde_json::from_str(
                    item["scope"]
                        .as_str()
                        .ok_or_else(|| invalid("archive sentinel scope missing"))?,
                )
                .map_err(|_| invalid("archive sentinel scope invalid"))?;
                if &owner == scope {
                    return Ok(());
                }
            }
        }
    }
    Err(invalid(
        "empty or unrelated read cannot prove the written memory sentinel",
    ))
}
fn run(packet: Packet) -> CoreResult<bool> {
    validate_packet(&packet)?;
    let selected = engram_backend_provider::validate_selection(&packet.config)?;
    let provider = match selected {
        engram_backend_provider::StorageProvider::Sqlite => "sqlite",
        engram_backend_provider::StorageProvider::Pgvector => "pgvector",
        engram_backend_provider::StorageProvider::Helixdb => "helixdb",
    };
    let mode = match packet.config.helixdb.as_ref().and_then(|h| h.mode) {
        Some(engram_backend_provider::HelixMode::Service) => "service",
        Some(engram_backend_provider::HelixMode::Embedded) => "embedded",
        None => "not_applicable",
    };
    let opened = open(&packet)?;
    let mut results = BTreeMap::new();
    let mut passed = true;
    let mut stopped = false;
    let mut uncertain = false;
    let mut prior_unknown: Option<String> = None;
    for call in &packet.calls {
        if stopped && !(uncertain && call.after_unknown.as_deref() == Some("blocked-admission")) {
            emit_blocked(
                &packet,
                call,
                provider,
                mode,
                "prior failure or unknown stopped this plan; no replay",
            )?;
            passed = false;
            continue;
        }
        if !supported(&call.operation) {
            let reason = if call.operation == "code.backfill" {
                "Rust backfill is not applicable: no EngramProvider port; MCP codegraph owns that operation"
            } else {
                "required operation has no actual dispatch"
            };
            emit_blocked(&packet, call, provider, mode, reason)?;
            passed = false;
            stopped = true;
            continue;
        }
        if let Some(control) = &call.control {
            opened
                .fixture
                .as_ref()
                .ok_or_else(|| invalid("private control needs an instrumented provider"))?(
                control,
            )?;
        }
        let before = snapshot(&opened)?;
        let response = references(&call.arguments, &results)
            .and_then(|a| dispatch(&opened.provider, &packet.scope, &call.operation, &a));
        let after = snapshot(&opened)?;
        let events = delta(&before, &after, "events")?;
        let observations = delta(&before, &after, "observations")?;
        for observation in events.iter().chain(observations.iter()) {
            if observation["scope"] != json!(packet.scope)
                || observation["backend"] != provider
                || observation["mode"] != mode
                || packet
                    .config
                    .helixdb
                    .as_ref()
                    .is_some_and(|h| observation["store"] != h.logical_store)
            {
                return Err(invalid("typed native observation binding mismatch"));
            }
        }
        let (outcome, class, result) = match &response {
            Ok(value) => ("ok", None, value.clone()),
            Err(error) => (
                if matches!(error, CoreError::CapabilityUnsupported { .. }) {
                    "unsupported"
                } else {
                    "error"
                },
                Some(error_class(error)),
                json!({"errorClass":error_class(error),"message":error.to_redacted().to_string()}),
            ),
        };
        let mut assertions = vec![
            "operation handler invoked; typed argument validation precedes port execution"
                .to_string(),
            "selected provider and full scope bound".to_string(),
        ];
        let mut matches = outcome == call.expect.status;
        assertions.push(format!(
            "outcome:{}",
            if matches { "matched" } else { "mismatched" }
        ));
        if let Some(expected) = &call.expect.error_class {
            let yes = class == Some(expected.as_str());
            matches &= yes;
            assertions.push(format!(
                "errorClass:{}",
                if yes { "matched" } else { "mismatched" }
            ));
        }
        for (pointer, expected) in &call.expect.equals {
            let yes = result.pointer(pointer) == Some(expected);
            matches &= yes;
            assertions.push(format!(
                "equals:{pointer}:{}",
                if yes { "matched" } else { "mismatched" }
            ));
        }
        if uncertain && call.after_unknown.as_deref() == Some("blocked-admission") {
            let blocked = observations.iter().any(|v| {
                v["outcome"] == "BlockedByPriorUnknown"
                    && v["operationId"].as_str() == prior_unknown.as_deref()
                    && v["commitId"].is_null()
            });
            let no_new = events.iter().all(|v| v["phase"] != "BeforeSend");
            matches &= blocked && no_new;
            assertions.push(format!(
                "blocked admission without dispatch:{}",
                blocked && no_new
            ));
        }
        let original_unknown = observations
            .iter()
            .find(|v| v["outcome"] == "Unknown")
            .map(|v| {
                let id = v["operationId"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .ok_or_else(|| invalid("typed unknown has no original operation ID"))?;
                if v["commitId"].as_str() != Some(id) {
                    return Err(invalid("typed unknown operation ID mismatch"));
                }
                Ok(id.to_owned())
            })
            .transpose()?;
        let ambiguous_without_observer = opened.fixture.is_none()
            && response.as_ref().err().is_some_and(|e| {
                matches!(
                    e,
                    CoreError::Adapter { .. } | CoreError::BackendTransient { .. }
                )
            });
        let mut row = receipt(&packet, call, provider, mode);
        row["status"] = json!(if ambiguous_without_observer {
            "BLOCKED"
        } else if matches {
            "PASS"
        } else {
            "FAIL"
        });
        row["error_class"] = json!(class);
        row["outcome"] = json!(outcome);
        row["result_sha256"] = json!(digest(&result)?);
        if call.operation == "batch.ingest" {
            row["public_outcome"] = result.clone();
        }
        row["assertions"] = json!(assertions);
        row["events"] = json!(events);
        row["observations"] = json!(observations);
        if let Err(error) = &response {
            row["reason"] = json!(error.to_redacted().to_string());
        }
        if ambiguous_without_observer {
            row["reason"] =
                json!("adapter failure has no typed causal observation; no unknown inferred");
        } else if !matches {
            row["reason"] = json!("operation expectations were not met");
        }
        println!("{row}");
        results.insert(call.id.clone(), result);
        if original_unknown.is_some() {
            prior_unknown = original_unknown;
            uncertain = true;
            stopped = true;
            passed = false;
        }
        // Without typed evidence, an adapter error is ambiguous, not a proved
        // unknown. Stop conservatively instead of guessing from its display text.
        if ambiguous_without_observer {
            stopped = true;
            passed = false;
        }
        if !matches {
            passed = false;
            stopped = true;
        }
    }
    if let (Some(fixture), Some(control)) = (&packet.fixture, &opened.fixture) {
        use std::io::Write;
        let observed = control(&json!({"operation":"snapshot"}))?;
        let output =
            std::path::Path::new(text(fixture, "ownedRoot")?).join("rust-fixture-observation.json");
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(output)
            .map_err(|_| invalid("fixture snapshot unavailable"))?;
        writeln!(file, "{observed}").map_err(|_| invalid("fixture snapshot unavailable"))?;
    }
    if let Some(proof) = &packet.proof_obligations {
        if packet.fixture.is_some() {
            return Err(invalid(
                "production bootstrap proof cannot use a fault artifact",
            ));
        }
        let sentinel = text(proof, "sentinel_call_id")?;
        if !passed
            || !results.contains_key(sentinel)
            || !packet
                .calls
                .iter()
                .any(|call| call.id == sentinel && !call.expect.equals.is_empty())
        {
            return Err(invalid("actual sentinel readback assertion missing"));
        }
        let write = text(proof, "sentinel_write_call_id")?;
        if !packet.calls.iter().any(|call| {
            call.id == write && call.expect.status == "ok" && call.operation == "memory.write"
        }) {
            return Err(invalid("actual prior sentinel write missing"));
        }
        let call = packet
            .calls
            .iter()
            .find(|call| call.id == sentinel)
            .ok_or_else(|| invalid("sentinel call missing"))?;
        if !matches!(
            call.operation.as_str(),
            "memory.retrieve" | "archive.export"
        ) {
            return Err(invalid("sentinel must be nonmutating"));
        }
        memory_sentinel(
            results
                .get(write)
                .ok_or_else(|| invalid("written sentinel missing"))?,
            results
                .get(sentinel)
                .ok_or_else(|| invalid("read sentinel missing"))?,
            &packet.scope,
        )?;
        // Release the original owner before opening the same known-successful
        // fixture generation. This is never used after an uncertain call.
        drop(opened.provider);
        drop(opened.fixture);
        let reopened = open(&packet)?;
        let args = references(&call.arguments, &results)?;
        let actual = dispatch(&reopened.provider, &packet.scope, &call.operation, &args)?;
        memory_sentinel(
            results
                .get(write)
                .ok_or_else(|| invalid("written sentinel missing"))?,
            &actual,
            &packet.scope,
        )?;
        if call
            .expect
            .equals
            .iter()
            .any(|(pointer, expected)| actual.pointer(pointer) != Some(expected))
        {
            return Err(invalid("reopened sentinel readback differs"));
        }
        drop(reopened.provider);
        drop(reopened.fixture);
        drop(results);
        println!(
            "{}",
            json!({"schema":"engram.provider-bootstrap.v1","artifact_sha256":packet.artifact_sha256,"bootstrap":true,"sentinel_readback":true,"owned_close":true,"close_origin":"actual provider release, same successful namespace reopen and sentinel read; no held NativeClose attestation"})
        );
        return Ok(passed);
    }
    drop(results);
    drop(opened.provider);
    drop(opened.fixture);
    // A process reaper/fixture custodian independently verifies final closure.
    Ok(passed)
}
pub(crate) fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let result = (|| {
        if args.len() != 2 || args[0] != "--request" {
            return Err(invalid("expected --request FILE"));
        }
        let bytes = std::fs::read(&args[1]).map_err(|_| invalid("request file unavailable"))?;
        let packet: Packet =
            serde_json::from_slice(&bytes).map_err(|_| invalid("invalid request packet"))?;
        run(packet)
    })();
    match result {
        Ok(true) => {}
        Ok(false) => std::process::exit(1),
        Err(error) => {
            eprintln!("Rust provider consumer blocked: {}", error_class(&error));
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod controls {
    use super::*;
    #[test]
    fn sentinel_must_read_actual_written_id_value_and_complete_scope() {
        let scope: Scope =
            serde_json::from_value(json!({"tenant":"owned","workspace":"proof"})).unwrap();
        let written =
            json!({"record":{"id":"actual-id","scope":scope,"content":{"text":"actual-sentinel"}}});
        let read = json!({"context":{"items":[{"targetId":"actual-id","targetType":"memory","content":"actual-sentinel"}]}});
        assert!(memory_sentinel(&written, &read, &scope).is_ok());
        for bad in [
            json!({}),
            json!({"memories":[]}),
            json!({"context":{"items":[{"targetId":"unrelated","targetType":"memory","content":"actual-sentinel"}]}}),
            json!({"context":{"items":[{"targetId":"actual-id","targetType":"entity","content":"actual-sentinel"}]}}),
            json!({"context":{"items":[{"targetId":"actual-id","targetType":"memory","content":"different"}]}}),
        ] {
            assert!(memory_sentinel(&written, &bad, &scope).is_err());
        }
        let mut foreign = written;
        foreign["record"]["scope"]["session"] = json!("");
        assert!(memory_sentinel(&foreign, &read, &scope).is_err());
    }
    #[test]
    fn repeated_operation_results_are_referenced_by_unique_call_id() {
        let results = BTreeMap::from([
            ("first".into(), json!({"id":"one"})),
            ("second".into(), json!({"id":"two"})),
        ]);
        let request = json!({"old":{"$result":"first#/id"},"new":{"$result":"second#/id"}});
        assert_eq!(
            references(&request, &results).unwrap(),
            json!({"old":"one","new":"two"})
        );
        assert!(references(&json!({"$result":"memory.write#/id"}), &results).is_err());
        assert!(references(&json!({"$result":"first#/missing"}), &results).is_err());
    }
    #[test]
    fn full_scope_guard_distinguishes_none_from_empty_nested_scope() {
        let scope = Scope {
            tenant: "owned".into(),
            subject: None,
            workspace: Some("workspace".into()),
            session: None,
            environment: None,
        };
        let mut other = scope.clone();
        other.session = Some(String::new());
        assert!(scope_guard(&json!({"request":{"entity":{"scope":other}}}), &scope).is_err());
        assert!(scope_guard(&json!({"request":{"entity":{"scope":scope}}}), &scope).is_ok());
    }
    #[test]
    fn rust_backfill_is_not_a_fabricated_executed_operation() {
        assert!(!supported("code.backfill"));
        assert!(supported("identity.resolve_entity"));
        assert!(supported("archive.apply"));
        assert!(!supported("capability.getter"));
    }
}
