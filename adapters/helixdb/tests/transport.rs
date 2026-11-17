use engram_store_helixdb::{HelixStore, codec::Partition, config::ServiceConfig, error::Error};
use std::{path::PathBuf, sync::Mutex, time::Duration};
static SERVICE: Mutex<()> = Mutex::new(());
fn partition() -> Partition {
    Partition {
        logical_store: format!("primitive-{}", uuid::Uuid::new_v4()),
        embedding_space: None,
        scope: engram_domain::Scope {
            tenant: "tenant".into(),
            subject: None,
            workspace: Some("workspace".into()),
            session: None,
            environment: None,
        },
    }
}
fn config() -> ServiceConfig {
    ServiceConfig {
        endpoint: std::env::var("HELIX_TEST_ENDPOINT")
            .expect("provision the actual pinned Helix server fixture"),
        credential_env: std::env::var("HELIX_TEST_CREDENTIAL_ENV")
            .expect("explicit fixture credential environment name required"),
        vectors_enabled: true,
        read_only: false,
        timeout: Duration::from_secs(30),
    }
}

#[cfg(feature = "service")]
#[test]
fn primitive_service_durable_data_scope_owner_and_client_only_close() {
    let _serial = SERVICE.lock().unwrap();
    let identity = partition();
    let writer = HelixStore::open_service(config(), identity.clone()).unwrap();
    writer.put_sentinel("public-id", "durable-service").unwrap();
    assert!(matches!(
        HelixStore::open_service(config(), identity.clone()),
        Err(Error::Contention)
    ));
    let mut read_config = config();
    read_config.read_only = true;
    let reader = HelixStore::open_service(read_config, identity.clone()).unwrap();
    writer.close().unwrap();
    drop(writer);
    assert_eq!(
        reader.get_sentinel("public-id").unwrap().as_deref(),
        Some("durable-service")
    );
    assert!(matches!(
        reader.put_sentinel("blocked", "value"),
        Err(Error::ReadOnly)
    ));
    let reopened = HelixStore::open_service(config(), identity).unwrap();
    assert_eq!(
        reopened.get_sentinel("public-id").unwrap().as_deref(),
        Some("durable-service")
    );
    reopened.close().unwrap();
    reader.close().unwrap();
    // A new independent client still opens the same running external server.
    let independent = HelixStore::open_service(config(), partition()).unwrap();
    independent.close().unwrap();
}

#[cfg(feature = "service")]
#[test]
fn primitive_service_lost_actual_ack_is_unknown_and_never_replayed() {
    let _serial = SERVICE.lock().unwrap();
    let control = PathBuf::from(
        std::env::var("HELIX_TEST_CONTROL_DIR")
            .expect("owned real-server gateway controls required"),
    );
    let store = HelixStore::open_service(config(), partition()).unwrap();
    let id = format!("lost-ack-{}", uuid::Uuid::new_v4());
    let baseline = std::fs::read_to_string(control.join("requests.jsonl"))
        .unwrap()
        .lines()
        .count();
    std::fs::write(control.join("drop-next-write-ack"), &id).unwrap();
    let operation_id = match store.put_sentinel(&id, "committed-before-ack-loss") {
        Err(Error::OutcomeUnknown { operation_id }) => operation_id,
        other => panic!("actual forwarded mutation must report unknown after ack loss: {other:?}"),
    };
    let journal = std::fs::read_to_string(control.join("requests.jsonl")).unwrap();
    let dropped = journal.lines().skip(baseline).filter(|line| serde_json::from_str::<serde_json::Value>(line).unwrap()["ack_dropped_after_actual_response"] == true).count();
    assert_eq!(dropped, 1);
    let before = journal.lines().count();
    assert!(store.put_sentinel("must-not-execute", "forbidden").is_err());
    assert_eq!(
        std::fs::read_to_string(control.join("requests.jsonl"))
            .unwrap()
            .lines()
            .count(),
        before
    );
    assert!(
        store.close().is_err(),
        "ownership must not release before reconciliation"
    );
    store.reconcile_sentinel(&id, &operation_id).unwrap();
    assert!(
        store
            .put_sentinel("still-cleanup-only", "forbidden")
            .is_err()
    );
    store.close().unwrap();
    assert!(
        std::fs::read_to_string(control.join("requests.jsonl"))
            .unwrap()
            .lines()
            .all(|line| {
                let row: serde_json::Value = serde_json::from_str(line).unwrap();
                !row["write"].as_bool().unwrap() || row["durable"] == "true"
            })
    );
}

#[cfg(feature = "service")]
#[test]
fn primitive_disabled_service_rejects_before_credentials_or_network() {
    let config = ServiceConfig {
        endpoint: "http://127.0.0.1:1".into(),
        credential_env: "MISSING_STORAGE_CREDENTIAL_FOR_DISABLED_PRIMITIVE".into(),
        vectors_enabled: false,
        read_only: false,
        timeout: Duration::from_secs(1),
    };
    let error = match HelixStore::open_service(config, partition()) {
        Err(error) => error,
        Ok(_) => panic!("strict current policy must reject"),
    };
    assert!(error.to_string().contains("PLT-001"));
    assert!(!error.to_string().contains("MISSING_STORAGE"));
}

#[cfg(feature = "service")]
#[test]
fn primitive_wrong_auth_is_rejected_and_redacted() {
    let _serial = SERVICE.lock().unwrap();
    let mut invalid = config();
    invalid.credential_env = "ENGRAM_HELIX_BAD_PRIMITIVE_TOKEN".into();
    let error = match HelixStore::open_service(invalid, partition()) {
        Err(error) => error,
        Ok(_) => panic!("bad auth must reject"),
    };
    let safe = error.to_string();
    assert!(
        !safe.contains(
            &std::env::var("ENGRAM_HELIX_BAD_PRIMITIVE_TOKEN")
                .expect("explicit bad-credential fixture required")
        )
    );
    let valid = HelixStore::open_service(config(), partition()).unwrap();
    valid.close().unwrap();
}

#[test]
fn primitive_helper_refuses_foreign_process_cleanup() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    std::fs::create_dir_all(root.join("runs")).unwrap();
    let code = r#"
import importlib.util,pathlib,subprocess,tempfile,json,sys
root=pathlib.Path(sys.argv[1]); spec=importlib.util.spec_from_file_location('owned_helper',root/'tools/ci/helixdb_server.py');h=importlib.util.module_from_spec(spec);spec.loader.exec_module(h)
p=subprocess.Popen([sys.executable,'-c','import time;time.sleep(30)'])
try:
 with tempfile.TemporaryDirectory(dir=root/'runs') as d:
  d=pathlib.Path(d);identity={**h.process_identity(p.pid),'role':'engine'}
  (d/'state.json').write_text(json.dumps({'root':str(d),'source_revision':h.PIN,'processes':[identity]}))
  try:h.cleanup(d);raise AssertionError('foreign process cleanup accepted')
  except ValueError as e:assert 'foreign process' in str(e)
  assert p.poll() is None,'foreign probe was signalled';print('foreign process rejected; owned probe survives')
finally:p.terminate();p.wait(timeout=5)
"#;
    let status = std::process::Command::new("python3")
        .args(["-c", code])
        .arg(root)
        .status()
        .unwrap();
    assert!(status.success());
}

#[cfg(feature = "service")]
#[test]
fn primitive_invalid_service_origins_are_rejected_and_redacted_before_credentials() {
    let _serial = SERVICE.lock().unwrap();
    for endpoint in [
        "https://user:do-not-leak@127.0.0.1",
        "https://127.0.0.1/?do-not-leak",
        "https://127.0.0.1/arbitrary-route",
        "http://unselected.invalid",
        "s3://bucket/path",
    ] {
        let invalid = ServiceConfig {
            endpoint: endpoint.into(),
            credential_env: "MISSING_INVALID_URI_CONTROL_CREDENTIAL".into(),
            vectors_enabled: true,
            read_only: false,
            timeout: Duration::from_secs(1),
        };
        let error = match HelixStore::open_service(invalid, partition()) {
            Err(error) => error,
            Ok(_) => panic!("invalid service origin accepted"),
        };
        assert!(matches!(error, Error::InvalidConfig(_)));
        assert!(!error.to_string().contains("do-not-leak"));
        assert!(
            !error.to_string().contains("credential is unavailable"),
            "URI validation must precede credential lookup"
        );
    }
}

fn reply_server(
    response: impl Into<String>,
) -> (
    String,
    std::sync::Arc<std::sync::atomic::AtomicUsize>,
    std::sync::Arc<std::sync::atomic::AtomicBool>,
    std::thread::JoinHandle<()>,
) {
    use std::{
        io::{Read, Write},
        sync::{
            Arc,
            atomic::{AtomicBool, AtomicUsize, Ordering},
        },
    };
    let response = response.into();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    listener.set_nonblocking(true).unwrap();
    let count = Arc::new(AtomicUsize::new(0));
    let seen = count.clone();
    let stop = Arc::new(AtomicBool::new(false));
    let stopping = stop.clone();
    let handle = std::thread::spawn(move || {
        while !stopping.load(Ordering::SeqCst) {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    count.fetch_add(1, Ordering::SeqCst);
                    stream
                        .set_read_timeout(Some(Duration::from_secs(2)))
                        .unwrap();
                    let mut request = Vec::new();
                    let mut buffer = [0; 4096];
                    loop {
                        let size = stream.read(&mut buffer).unwrap_or(0);
                        if size == 0 {
                            break;
                        }
                        request.extend_from_slice(&buffer[..size]);
                        if let Some(end) = request.windows(4).position(|v| v == b"\r\n\r\n") {
                            let header = String::from_utf8_lossy(&request[..end]);
                            let length = header
                                .lines()
                                .find_map(|line| {
                                    line.to_ascii_lowercase()
                                        .strip_prefix("content-length:")
                                        .map(|v| v.trim().parse::<usize>().unwrap())
                                })
                                .unwrap_or(0);
                            if request.len() >= end + 4 + length {
                                break;
                            }
                        }
                        if request.len() > 1024 * 1024 {
                            break;
                        }
                    }
                    let _ = stream.write_all(response.as_bytes());
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(2))
                }
                Err(error) => panic!("owned negative-control listener failed: {error}"),
            }
        }
    });
    (endpoint, seen, stop, handle)
}

#[cfg(feature = "service")]
#[test]
fn primitive_redirect_is_not_followed_to_unconfigured_destination() {
    use std::sync::atomic::Ordering;
    let _serial = SERVICE.lock().unwrap();
    let (trap, trap_count, stop_trap, trap_thread) =
        reply_server("HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
    let response = format!(
        "HTTP/1.1 302 Found\r\nLocation: {trap}/unconfigured\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
    );
    let (endpoint, count, stop, handle) = reply_server(response);
    let mut selected = config();
    selected.endpoint = endpoint;
    let result = HelixStore::open_service(selected, partition());
    stop.store(true, Ordering::SeqCst);
    stop_trap.store(true, Ordering::SeqCst);
    handle.join().unwrap();
    trap_thread.join().unwrap();
    assert!(result.is_err());
    assert_eq!(count.load(Ordering::SeqCst), 1);
    assert_eq!(
        trap_count.load(Ordering::SeqCst),
        0,
        "redirect reached an unconfigured destination"
    );
}

#[cfg(feature = "service")]
#[test]
fn primitive_ambient_proxy_cannot_intercept_configured_service() {
    if std::env::var_os("ENGRAM_HELIX_PROXY_CONTROL").is_some() {
        let actual = HelixStore::open_service(config(), partition()).unwrap();
        actual
            .put_sentinel("proxy-control", "real-service")
            .unwrap();
        actual.close().unwrap();
        return;
    }
    use std::sync::atomic::Ordering;
    let _serial = SERVICE.lock().unwrap();
    let (proxy, seen, stop, handle) =
        reply_server("HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "primitive_ambient_proxy_cannot_intercept_configured_service",
            "--nocapture",
        ])
        .env("ENGRAM_HELIX_PROXY_CONTROL", "1")
        .env("HTTP_PROXY", &proxy)
        .env("http_proxy", &proxy)
        .env("HTTPS_PROXY", &proxy)
        .env("https_proxy", &proxy)
        .env("ALL_PROXY", &proxy)
        .env("all_proxy", &proxy)
        .env("NO_PROXY", "")
        .env("no_proxy", "")
        .status()
        .unwrap();
    stop.store(true, Ordering::SeqCst);
    handle.join().unwrap();
    assert!(status.success());
    assert_eq!(seen.load(Ordering::SeqCst), 0);
}

struct OwnedChild(std::process::Child);
impl Drop for OwnedChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[cfg(feature = "service")]
#[test]
fn primitive_tls_rejects_untrusted_certificate_before_http() {
    let _serial = SERVICE.lock().unwrap();
    let temporary = tempfile::tempdir().unwrap();
    let certificate = temporary.path().join("certificate.pem");
    let key = temporary.path().join("key.pem");
    let generated = std::process::Command::new("openssl")
        .args([
            "req",
            "-x509",
            "-newkey",
            "rsa:2048",
            "-nodes",
            "-subj",
            "/CN=localhost",
            "-days",
            "1",
            "-keyout",
        ])
        .arg(&key)
        .arg("-out")
        .arg(&certificate)
        .output()
        .expect("openssl is required for the actual TLS negative control");
    assert!(
        generated.status.success(),
        "certificate fixture failed: {}",
        String::from_utf8_lossy(&generated.stderr)
    );
    let port_file = temporary.path().join("port");
    let observed = temporary.path().join("http-observed");
    let code = r#"
import http.server,ssl,pathlib,sys
class Handler(http.server.BaseHTTPRequestHandler):
 def log_message(self,*a):pass
 def do_POST(self):
  pathlib.Path(sys.argv[4]).write_text('unexpected HTTP after untrusted TLS');self.send_response(400);self.end_headers()
s=http.server.HTTPServer(('127.0.0.1',0),Handler);c=ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER);c.load_cert_chain(sys.argv[1],sys.argv[2]);s.socket=c.wrap_socket(s.socket,server_side=True);pathlib.Path(sys.argv[3]).write_text(str(s.server_port));s.serve_forever()
"#;
    let child = std::process::Command::new("python3")
        .args(["-c", code])
        .arg(&certificate)
        .arg(&key)
        .arg(&port_file)
        .arg(&observed)
        .spawn()
        .unwrap();
    let mut child = OwnedChild(child);
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while !port_file.exists() {
        assert!(child.0.try_wait().unwrap().is_none(), "TLS fixture exited");
        assert!(
            std::time::Instant::now() < deadline,
            "TLS fixture readiness timeout"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    let mut selected = config();
    selected.endpoint = format!(
        "https://127.0.0.1:{}",
        std::fs::read_to_string(port_file).unwrap()
    );
    assert!(HelixStore::open_service(selected, partition()).is_err());
    assert!(!observed.exists(), "untrusted TLS reached HTTP application");
}

#[cfg(feature = "service")]
#[test]
fn primitive_simultaneous_scope_bootstrap_has_one_owner_without_unknown_outcome() {
    let _serial = SERVICE.lock().unwrap();
    let identity = partition();
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let mut threads = Vec::new();
    for _ in 0..2 {
        let barrier = barrier.clone();
        let identity = identity.clone();
        let selected = config();
        threads.push(std::thread::spawn(move || {
            barrier.wait();
            HelixStore::open_service(selected, identity)
        }));
    }
    let mut winners = Vec::new();
    let mut contentions = 0;
    for thread in threads {
        match thread.join().unwrap() {
            Ok(store) => winners.push(store),
            Err(Error::Contention) => contentions += 1,
            Err(error) => panic!(
                "actual concurrent bootstrap must reject safely without unknown effect: {error}"
            ),
        }
    }
    assert_eq!(winners.len(), 1);
    assert_eq!(contentions, 1);
    winners.pop().unwrap().close().unwrap();
}
