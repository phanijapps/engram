#!/usr/bin/env python3
"""Own a pinned real Helix server and a loopback-only authentication test gateway.

Use /usr/bin/python3 on Linux for both provision and cleanup; the interpreter
must expose working kernel pidfd_open and pidfd_send_signal APIs.
"""
from __future__ import annotations
import argparse
import hashlib
import hmac
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import http.client
import json
import os
from pathlib import Path
import secrets
import re
import tomllib
import select
import signal
import socket
import subprocess
import sys
import time
import urllib.request

PIN = "2690416a40177b480014b8b5397426efbd1dc7b8"
ROOT = Path(__file__).resolve().parents[2]

RECIPE = '[package]\nname = "engram-helix-test-server"\nversion = "0.0.0"\nedition = "2024"\nrust-version = "1.97.1"\n\n[workspace]\n\n[[bin]]\nname = "server"\npath = "src/main.rs"\n\n[features]\nasync-index-benchmark = ["server/async-index-benchmark"]\n\n[dependencies]\nserver = { git = "https://github.com/HelixDB/helix-db.git", rev = "2690416a40177b480014b8b5397426efbd1dc7b8", default-features = false }\ntokio = { version = "1", features = ["full"] }\n'
REVIEWED_TRANSPORT_LEAVES = {'encoding_rs': {'side': 'adapter', 'name': 'encoding_rs', 'version': '0.8.35', 'source': 'registry+https://github.com/rust-lang/crates.io-index', 'checksum': '75030f3c4f45dafd7586dd6780965a8c7e8e285a5ecb86713e63a79c5b2766f3'}, 'httpdate': {'side': 'server', 'name': 'httpdate', 'version': '1.0.3', 'source': 'registry+https://github.com/rust-lang/crates.io-index', 'checksum': 'df3b46402a9d5adb4c86a0cf463f42e19994e3ee891101b1841f30a545cb49a9'}, 'mime': {'side': 'adapter', 'name': 'mime', 'version': '0.3.17', 'source': 'registry+https://github.com/rust-lang/crates.io-index', 'checksum': '6877bb514081ee2a7ff5ef9de3281f14a4dd4bceac4c09388074a6b5df8a139a'}}


def sha(path):
    result = hashlib.sha256()
    with Path(path).open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            result.update(chunk)
    return result.hexdigest()


def output_root(value):
    path = Path(value)
    if not path.is_absolute():
        path = ROOT / path
    runs = ROOT / "runs"
    if not path.is_relative_to(runs) or ".." in path.parts:
        raise ValueError("fixture output must remain under this checkout's runs directory")
    current = path
    while current != ROOT:
        if current.is_symlink():
            raise ValueError("symlinked fixture output is forbidden")
        current = current.parent
    return path


def port():
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        return sock.getsockname()[1]


def process_identity(pid):
    # Fail closed outside the implemented native Linux process-identity backend.
    stat = Path(f"/proc/{pid}/stat").read_text()
    return {"pid": pid, "start_ticks": stat.rsplit(")", 1)[1].split()[19],
            "exe": str(Path(f"/proc/{pid}/exe").resolve()),
            "argv": Path(f"/proc/{pid}/cmdline").read_bytes().rstrip(b"\0").decode().split("\0")}


def owned_alive(identity):
    try:
        return all(process_identity(identity["pid"]).get(key) == value for key, value in identity.items() if key != "role")
    except (OSError, ValueError):
        return False


def wait_ready(endpoint, process, timeout=120):
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if process.poll() is not None:
            raise RuntimeError("owned server exited before readiness; inspect retained log")
        try:
            with opener.open(endpoint + "/readyz", timeout=1) as response:
                if response.status == 200:
                    return
        except OSError:
            time.sleep(0.1)
    raise RuntimeError("owned server readiness timed out")


def gateway(directory):
    state = json.loads((directory / "state.json").read_text())
    token = (directory / "credential.secret").read_text().strip()
    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *_args):
            pass
        def do_GET(self):
            if self.path != "/readyz":
                self.send_error(404); return
            self.send_response(200); self.end_headers(); self.wfile.write(b"ready")
        def do_POST(self):
            if self.path != "/v2/query":
                self.send_error(404); return
            if not hmac.compare_digest(self.headers.get("Authorization", ""), "Bearer " + token):
                self.send_error(401); return
            try:
                length = int(self.headers.get("Content-Length", "0"))
                if not 0 < length <= 1024 * 1024:
                    self.send_error(413); return
                body = self.rfile.read(length)
                request = json.loads(body)
                headers = {"Content-Type": "application/json"}
                for name in ("x-helix-await-durable", "x-helix-require-writer"):
                    if name in self.headers:
                        headers[name] = self.headers[name]
                connection = http.client.HTTPConnection("127.0.0.1", state["engine_port"], timeout=30)
                connection.request("POST", "/v2/query", body, headers)
                response = connection.getresponse()
                result = response.read(8 * 1024 * 1024 + 1)
                status = response.status
                connection.close()
                write = request.get("request_type") == "write"
                control = directory / "drop-next-write-ack"
                dropped = write and control.exists() and control.read_text().strip().encode() in body
                if dropped:
                    (directory / "drop-next-write-ack").unlink()
                # No query payload/credential is logged. Actual upstream status and ack headers are retained.
                with (directory / "requests.jsonl").open("a") as journal:
                    journal.write(json.dumps({"write": write, "status": status,
                        "durable": headers.get("x-helix-await-durable"),
                        "require_writer": headers.get("x-helix-require-writer"),
                        "ack_dropped_after_actual_response": dropped}) + "\n")
                if dropped:
                    self.close_connection = True
                    self.connection.shutdown(socket.SHUT_RDWR)
                    return
                self.send_response(status)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(result)))
                self.end_headers(); self.wfile.write(result)
            except (OSError, ValueError, json.JSONDecodeError):
                # No storage request is retried; an incomplete forwarded write remains unknown.
                self.close_connection = True
    server = ThreadingHTTPServer(("127.0.0.1", state["gateway_port"]), Handler)
    server.daemon_threads = False
    server.serve_forever()


def engine_projection(root, package, toolchain, target, *, all_features=False, locked=True):
    command = ["cargo", "+" + toolchain, "tree", "--offline", "--package", package,
               "--target", target, "--edges", "normal,build", "--prefix", "depth",
               "--no-dedupe", "--format", "{p}|||{f}"]
    if locked:
        command.append("--locked")
    if all_features:
        command.append("--all-features")
    result = subprocess.run(command, cwd=root, stdout=subprocess.PIPE, stderr=sys.stderr, text=True, check=True)
    depth = None
    selected = set()
    features = {}
    for line in result.stdout.splitlines():
        match = re.fullmatch(r"(\d+)(\S+) v([^\s]+).*?\|\|\|(.*)", line)
        if match is None:
            raise ValueError("unknown dependency-tree record")
        level, name, version, flags = int(match[1]), match[2], match[3], match[4]
        if depth is None:
            if name != "db":
                continue
            depth = level
        elif level <= depth:
            break
        selected.add((name, version))
        features[name + "@" + version] = flags
    if depth is None:
        raise ValueError("actual selected production tree lacks Helix engine")
    lock = tomllib.loads((root / "Cargo.lock").read_text())["package"]
    packages = []
    for name, version in sorted(selected):
        matches = [entry for entry in lock if entry["name"] == name and entry["version"] == version]
        if len(matches) != 1 or not matches[0].get("source"):
            raise ValueError("ambiguous or unpinned engine dependency identity")
        entry = matches[0]
        packages.append({"name": name, "version": version, "source": entry["source"], "checksum": entry.get("checksum")})
    return {"packages": packages, "features": features}


def verify_engine_sources():
    root = Path(os.environ.get("CARGO_HOME", Path.home() / ".cargo")) / "git/checkouts"
    verified = []
    for prefix, revision in (("helix-db", PIN), ("slatedb", "50c063327b826c2c018c664171fc1ada5dc3096d")):
        found = False
        for candidate in root.glob(prefix + "-*/*"):
            if not (candidate / ".git").exists():
                continue
            head = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=candidate, text=True).strip()
            if head != revision:
                continue
            changes = subprocess.check_output(["git", "status", "--porcelain", "--untracked-files=all"], cwd=candidate, text=True).splitlines()
            if any(line[3:] not in {".cargo-ok", "Cargo.lock"} for line in changes):
                raise ValueError("actual engine dependency checkout has source changes")
            verified.append({"checkout": str(candidate), "revision": head})
            found = True
        if not found:
            raise ValueError("actual pinned engine dependency checkout is unavailable")
    return verified


def prepare_recipe(primary, source):
    source_head = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=source, text=True).strip()
    changes = subprocess.check_output(["git", "status", "--porcelain", "--untracked-files=all"], cwd=source, text=True).splitlines()
    if source_head != PIN or any(line[3:] != "Cargo.lock" for line in changes):
        raise ValueError("source checkout is not clean pinned code")
    declared = tomllib.loads((source / "rust-toolchain.toml").read_text())["toolchain"]["channel"]
    if declared != "1.97.1" or os.environ.get("HELIX_SERVER_TOOLCHAIN", declared) != declared:
        raise ValueError("server must use the exact declared pinned upstream toolchain")
    main = subprocess.check_output(["git", "show", PIN + ":crates/server/src/main.rs"], cwd=source)
    if main != (source / "crates/server/src/main.rs").read_bytes():
        raise ValueError("server main differs from pinned source")
    input_hash = sha(ROOT / "Cargo.lock")
    recipe = primary / ".context/helix-test-server-cache" / PIN / input_hash / "standalone"
    for candidate in (recipe, *recipe.parents):
        if candidate.is_symlink():
            raise ValueError("private build cache must not contain symlinks")
        if candidate == primary:
            break
    if not recipe.exists():
        (recipe / "src").mkdir(parents=True, mode=0o700)
        (recipe / "Cargo.toml").write_text(RECIPE)
        (recipe / "src/main.rs").write_bytes(main)
        (recipe / "Cargo.lock").write_bytes((ROOT / "Cargo.lock").read_bytes())
        target = subprocess.check_output(["rustc", "+1.88.0", "-vV"], text=True).split("host: ", 1)[1].splitlines()[0]
        engine_projection(recipe, "engram-helix-test-server", declared, target, locked=False)
    if (recipe / "Cargo.toml").read_text() != RECIPE or (recipe / "src/main.rs").read_bytes() != main:
        raise ValueError("private recipe/bootstrap bytes changed")
    target = subprocess.check_output(["rustc", "+1.88.0", "-vV"], text=True).split("host: ", 1)[1].splitlines()[0]
    adapter = engine_projection(ROOT, "engram-store-helixdb", "1.88.0", target, all_features=True)
    server = engine_projection(recipe, "engram-helix-test-server", declared, target)
    deltas = []
    for name in sorted({p["name"] for graph in (adapter, server) for p in graph["packages"]}):
        left = [p for p in adapter["packages"] if p["name"] == name]
        right = [p for p in server["packages"] if p["name"] == name]
        if left == right:
            continue
        if left and right:
            raise ValueError("shared engine dependency projection differs")
        side, values = ("adapter", left) if left else ("server", right)
        if len(values) != 1 or REVIEWED_TRANSPORT_LEAVES.get(name) != {"side": side, **values[0]}:
            raise ValueError("unreviewed production graph delta")
        deltas.append({"side": side, **values[0]})
    for key in set(adapter["features"]) | set(server["features"]):
        if key.startswith(("db@", "slatedb@", "slatedb-common@", "slatedb-txn-obj@")):
            if adapter["features"].get(key) != server["features"].get(key):
                raise ValueError("storage engine feature set differs")
    if sha(ROOT / "Cargo.lock") != input_hash:
        raise ValueError("Engram lock changed during fixture preparation")
    feature_deltas = {key: {"adapter": adapter["features"].get(key), "server": server["features"].get(key)}
                      for key in sorted(set(adapter["features"]) & set(server["features"]))
                      if adapter["features"].get(key) != server["features"].get(key)}
    (recipe.parent / "engine-features.json").write_text(json.dumps({"adapter": adapter["features"], "server": server["features"],
                                                                  "shared_feature_deltas": feature_deltas}, indent=2) + "\n")
    source_checkouts = verify_engine_sources()
    summary = {"source_checkouts": source_checkouts, "all_shared_packages_equal": True, "whole_projection_equal": not deltas and not feature_deltas,
               "shared_feature_maps_equal": not feature_deltas, "observed_shared_feature_deltas": feature_deltas,
               "feature_maps_sha256": sha(recipe.parent / "engine-features.json"),
               "reviewed_transport_deltas": deltas,
               "adapter_projection_sha256": hashlib.sha256(json.dumps(adapter, sort_keys=True).encode()).hexdigest(),
               "server_projection_sha256": hashlib.sha256(json.dumps(server, sort_keys=True).encode()).hexdigest(),
               "engine_features": {k: v for k, v in server["features"].items() if k.startswith(("db@", "slatedb"))},
               "input_engram_lock_sha256": input_hash, "private_server_lock_sha256": sha(recipe / "Cargo.lock"),
               "recipe_sha256": sha(recipe / "Cargo.toml"), "pinned_main_sha256": sha(recipe / "src/main.rs"),
               "server_toolchain": declared, "native_target": target}
    (recipe.parent / "engine-identity.json").write_text(json.dumps(summary, indent=2) + "\n")
    return recipe, declared, summary


def require_pidfd_capability():
    if sys.platform != "linux" or not callable(getattr(os, "pidfd_open", None)) or not callable(getattr(signal, "pidfd_send_signal", None)):
        raise RuntimeError("supported Linux Python with pidfd APIs required; use /usr/bin/python3 for provision and cleanup")
    descriptor = os.pidfd_open(os.getpid())
    try:
        signal.pidfd_send_signal(descriptor, 0, None, 0)
    finally:
        os.close(descriptor)


def provision(directory):
    # Prove safe cleanup is possible before creating credentials or processes.
    require_pidfd_capability()
    if directory.exists():
        raise ValueError("refusing to replace an existing fixture directory")
    common = Path(subprocess.check_output(["git", "rev-parse", "--path-format=absolute", "--git-common-dir"], cwd=ROOT, text=True).strip())
    primary = common.parent
    cached = Path(os.environ.get("HELIX_TEST_SOURCE", primary / ".context/helix-plan-feasibility/helix-v3.4.4"))
    directory.mkdir(parents=True, mode=0o700)
    recipe, declared, engine_identity = prepare_recipe(primary, cached)
    target = Path(os.environ.get("HELIX_SERVER_TARGET_DIR", primary / ".context/helix-test-server-target" / declared))
    env = os.environ.copy()
    env.update(RUSTUP_TOOLCHAIN=declared, CARGO_TARGET_DIR=str(target), CARGO_BUILD_JOBS="2", CARGO_PROFILE_DEV_DEBUG="0")
    with (directory / "build.log").open("wb") as log:
        subprocess.run(["cargo", "+" + declared, "build", "--locked", "--offline", "--package", "engram-helix-test-server", "--bin", "server"],
                       cwd=recipe, env=env, check=True, stdout=log, stderr=subprocess.STDOUT)
    executable = target / "debug/server"
    # Pin a private copy so another build cannot replace the running fixture binary.
    import shutil
    binary = directory / "helix-server"
    shutil.copy2(executable, binary)
    engine_port, gateway_port, grpc_port = port(), port(), port()
    credential = secrets.token_urlsafe(32)
    secret = directory / "credential.secret"
    descriptor = os.open(secret, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(descriptor, "w") as stream:
        stream.write(credential)
    state = {"source_revision": PIN, "upstream_lock_sha256": sha(recipe / "Cargo.lock"), "engine_identity": engine_identity,
             "recipe": str(recipe), "server_compiler": subprocess.check_output(["rustc", "+" + declared, "--version"], text=True).strip(),
             "server_binary_sha256": sha(binary), "gateway_source_sha256": sha(__file__),
             "engine_port": engine_port, "gateway_port": gateway_port,
             "endpoint": f"http://127.0.0.1:{gateway_port}", "credential_env": "ENGRAM_HELIX_PRIMITIVE_TOKEN",
             "root": str(directory), "processes": [], "closed": False}
    state_path = directory / "state.json"
    state_path.write_text(json.dumps(state, indent=2) + "\n")
    for key in tuple(env):
        if key.startswith(("HELIX_", "AWS_", "S3_")) or key in {"HTTP_ADDR", "GRPC_ADDR", "DB_PATH", "HTTP_PROXY", "HTTPS_PROXY", "ALL_PROXY", "http_proxy", "https_proxy", "all_proxy"}:
            env.pop(key)
    env.update(HELIX_HTTP_ADDR=f"127.0.0.1:{engine_port}", HELIX_GRPC_ADDR=f"127.0.0.1:{grpc_port}",
               HELIX_DATA_DIR=str(directory / "data"), DB_PATH="test-server", HELIX_HOME=str(directory / "preferences"), HELIX_TELEMETRY_LEVEL="off", TOKIO_WORKER_THREADS="2")
    for name in ("data", "preferences", "cache"):
        owned_root = directory / name
        if owned_root.exists() or owned_root.is_symlink():
            raise ValueError("owned runtime roots must be new nonsymlink directories")
        owned_root.mkdir(mode=0o700)
    with (directory / "server.log").open("wb") as log:
        engine = subprocess.Popen([str(binary)], env=env, stdin=subprocess.DEVNULL, stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
    state["processes"].append({**process_identity(engine.pid), "role": "engine"}); state_path.write_text(json.dumps(state, indent=2) + "\n")
    wait_ready(f"http://127.0.0.1:{engine_port}", engine)
    with (directory / "gateway.log").open("wb") as log:
        front = subprocess.Popen([sys.executable, str(Path(__file__).resolve()), "_gateway", "--output", str(directory)], env=env,
                                 stdin=subprocess.DEVNULL, stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
    state["processes"].append({**process_identity(front.pid), "role": "gateway"}); state_path.write_text(json.dumps(state, indent=2) + "\n")
    wait_ready(state["endpoint"], front)
    print(json.dumps({"HELIX_TEST_ENDPOINT": state["endpoint"], "HELIX_TEST_CREDENTIAL_ENV": state["credential_env"],
                      "credential_file": str(secret), "HELIX_TEST_CONTROL_DIR": str(directory), "state": str(state_path)}))


def cleanup(directory):
    state_path = directory / "state.json"
    if state_path.is_symlink():
        raise ValueError("symlinked fixture state is forbidden")
    state = json.loads(state_path.read_text())
    if state.get("root") != str(directory) or state.get("source_revision") != PIN:
        raise ValueError("fixture identity does not match owned cleanup target")
    roles = set()
    for identity in state["processes"]:
        role = identity.get("role")
        if role in roles or role not in {"engine", "gateway"}:
            raise ValueError("invalid owned process role")
        roles.add(role)
        binary = directory / "helix-server"
        expected_exe = str(binary) if role == "engine" else str(Path(sys.executable).resolve())
        expected_argv = [str(binary)] if role == "engine" else [sys.executable, str(Path(__file__).resolve()), "_gateway", "--output", str(directory)]
        if identity.get("exe") != expected_exe or identity.get("argv") != expected_argv:
            raise ValueError("foreign process is not an owned fixture role")
    if (directory / "helix-server").is_symlink() or sha(directory / "helix-server") != state["server_binary_sha256"] or sha(__file__) != state["gateway_source_sha256"]:
        raise ValueError("fixture binary/helper hash changed; refusing cleanup")
    for identity in reversed(state["processes"]):
        stat_path = Path(f'/proc/{identity["pid"]}/stat')
        try:
            fields = stat_path.read_text().rsplit(")", 1)[1].split()
        except FileNotFoundError:
            continue
        if fields[19] != identity["start_ticks"]:
            raise RuntimeError("process identity changed; refusing to signal it")
        if fields[0] == "Z":
            continue
        if not owned_alive(identity):
            raise RuntimeError("process executable/arguments changed; refusing to signal it")
        # A PID descriptor binds the signal to this exact process across PID reuse races.
        descriptor = os.pidfd_open(identity["pid"])
        try:
            if not owned_alive(identity):
                raise RuntimeError("process identity changed while acquiring owned handle")
            signal.pidfd_send_signal(descriptor, signal.SIGTERM)
            poll = select.poll()
            poll.register(descriptor, select.POLLIN)
            if not poll.poll(30_000):
                raise RuntimeError("owned process shutdown unconfirmed; no forced replacement")
        finally:
            os.close(descriptor)
    secret = directory / "credential.secret"
    if secret.is_symlink():
        raise ValueError("refusing symlinked credential cleanup")
    secret.unlink(missing_ok=True)
    state["closed"] = True
    state_path.write_text(json.dumps(state, indent=2) + "\n")
    print("owned fixture closed; evidence retained; no external service was stopped")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=("provision", "cleanup", "_gateway"))
    parser.add_argument("--output", required=True)
    args = parser.parse_args()
    directory = output_root(args.output)
    {"provision": provision, "cleanup": cleanup, "_gateway": gateway}[args.action](directory)


if __name__ == "__main__":
    main()
