#!/usr/bin/env python3
"""Disposable-store stdio verification for runtime embedding modes.

--off-only is a cross-platform release smoke: cold cache, invalid inactive
embedding settings, actual scan/search, no embedding service. The default also
uses a deterministic loopback embedding fixture to test capped backfill,
failures, hashless legacy chunks, restart preservation and vector-only rebuild.
No user store or external embedding service is touched.
"""
import argparse
import contextlib
import http.server
import json
import os
from pathlib import Path
import queue
import sqlite3
import subprocess
import tempfile
import threading
import time


class Server:
    def __init__(self, binary, store, flags, env):
        self.stderr = []
        self.output = queue.Queue()
        self.next_id = 0
        self.proc = subprocess.Popen(
            [str(binary), "--storage", str(store), "--project", "mode-test", "--tools", "all", *flags],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
            text=True, encoding="utf-8", env={**os.environ, **env},
        )
        def read_output():
            for line in self.proc.stdout:
                self.output.put(json.loads(line))
            self.output.put(None)
        def read_errors():
            self.stderr.extend(self.proc.stderr)
        self.readers = [threading.Thread(target=read_output, daemon=True), threading.Thread(target=read_errors, daemon=True)]
        for reader in self.readers:
            reader.start()
        self.request("initialize", {"protocolVersion": "2024-11-05", "capabilities": {}, "clientInfo": {"name": "mode-smoke", "version": "1"}})

    def request(self, method, params):
        self.next_id += 1
        message = {"jsonrpc": "2.0", "id": self.next_id, "method": method, "params": params}
        self.proc.stdin.write(json.dumps(message) + "\n")
        self.proc.stdin.flush()
        deadline = time.monotonic() + 90
        while True:
            try:
                response = self.output.get(timeout=max(0.01, deadline - time.monotonic()))
            except queue.Empty:
                raise AssertionError(f"timeout: {method}; stderr={''.join(self.stderr)}") from None
            assert response is not None, f"server exited: {''.join(self.stderr)}"
            if response.get("id") == self.next_id:
                assert "error" not in response, response
                return response["result"]

    def call(self, tool, args):
        result = self.request("tools/call", {"name": tool, "arguments": args})
        assert not result.get("isError"), result
        return "\n".join(item.get("text", "") for item in result.get("content", []))

    def close(self):
        if self.proc.poll() is None:
            self.proc.stdin.close()
            try:
                self.proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                self.proc.kill()
                self.proc.wait(timeout=5)
        for reader in self.readers:
            reader.join(timeout=5)
        self.proc.stdout.close()
        self.proc.stderr.close()


@contextlib.contextmanager
def running(binary, store, flags, env):
    # Keep construction inside cleanup coverage: initialization itself can fail.
    server = Server.__new__(Server)
    try:
        server.__init__(binary, store, flags, env)
        yield server
    finally:
        if hasattr(server, "proc"):
            server.close()


def snapshot(db):
    with contextlib.closing(sqlite3.connect(db)) as conn:
        return {table: conn.execute(f"SELECT * FROM {table} ORDER BY id").fetchall()
                for table in ["knowledge_sources", "knowledge_documents", "knowledge_chunks", "knowledge_entities", "knowledge_relationships"]}


def vector_ids(db):
    with contextlib.closing(sqlite3.connect(db)) as conn:
        # vec0 itself needs the extension; its shadow id map is ordinary SQLite.
        return conn.execute("SELECT id FROM vectors_rowids ORDER BY id").fetchall()


def verify(binary, off_only):
    with tempfile.TemporaryDirectory(prefix="engram-modes-") as temp:
        root = Path(temp)
        store, repo, cache = root / "store", root / "zbot", root / "cache"
        repo.mkdir()
        (repo / "payments.py").write_text("def retry_payment():\n    return 'retry payment timeout'\n", encoding="utf-8")
        (repo / "readiness.py").write_text("def readiness_check():\n    return True\n", encoding="utf-8")
        env = {"FASTEMBED_CACHE_DIR": str(cache), "ENGRAM_EMBEDDING_CONFIG": str(root / "missing-profile"),
               "ENGRAM_EMBEDDING_DIMENSIONS": "invalid", "ENGRAM_NO_FULL": "0"}
        with running(binary, store, ["--slim"], env) as server:
            scanned = server.call("scan_repo", {"path": str(repo)})
            assert "embedded 0 chunks" in scanned, scanned
            found = server.call("search", {"query": "retry_payment"})
            assert "retry_payment" in found, found
            unavailable = server.call("reindex", {})
            assert "Backlog not assessed" in unavailable and "backlog drained" not in unavailable, unavailable
            caps = server.call("capability_report", {})
            assert 'vector: unsupported' in caps and 'Embedding space: unavailable' in caps, caps
        assert not cache.exists() or not any(cache.rglob("*")), "off mode populated model cache"
        db = store / "engram_data.db"
        before = snapshot(db)
        assert before["knowledge_chunks"], "scan produced no chunks"
        with contextlib.closing(sqlite3.connect(db)) as conn:
            assert not conn.execute("SELECT name FROM sqlite_master WHERE name='vectors'").fetchall(), "off mode created vector table"
        launcher = Path(__file__).resolve().parents[2] / "packages" / "launcher" / "bin.js"
        compatibility_env = {**os.environ, **env, "ENGRAM_MCP_BIN": str(binary), "ENGRAM_NO_FULL": "1"}
        for command in ["status", "recall"]:
            args = ["node", str(launcher), command, "--storage", str(store), "--project", "mode-test", "--json"]
            if command == "recall":
                args.extend(["--query", "retry payment"])
            result = subprocess.run(args, capture_output=True, text=True, encoding="utf-8", env=compatibility_env, timeout=30)
            assert result.returncode == 0, (command, result.stderr)
            assert json.loads(result.stdout)["ok"], result.stdout
            # Last CLI --vector must override the compatibility default, so
            # deliberately invalid active provider config becomes an error.
            enabled = subprocess.run([*args, "--vector"], capture_output=True, text=True, encoding="utf-8", env=compatibility_env, timeout=30)
            assert enabled.returncode == 2 and "DIMENSIONS" in enabled.stderr, enabled.stderr
        print("PASS launcher one-shot status/recall: deprecated env mapping and explicit --vector precedence")
        print("PASS cold-cache --slim: invalid inactive env ignored; scan/search available; no vector table/cache")
        if off_only:
            return

        # Exercise the pre-hash legacy branch, without modifying canonical text.
        with contextlib.closing(sqlite3.connect(db)) as conn, conn:
            for chunk_id, record in conn.execute("SELECT id, record_json FROM knowledge_chunks").fetchall():
                chunk = json.loads(record)
                chunk["contentHash"] = ""
                conn.execute("UPDATE knowledge_chunks SET record_json=? WHERE id=?", (json.dumps(chunk), chunk_id))
        before = snapshot(db)
        state = {"fail": False, "calls": 0}
        class Embedder(http.server.BaseHTTPRequestHandler):
            def log_message(self, *_):
                pass

            def do_POST(self):
                data = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
                state["calls"] += 1
                if state["fail"]:
                    self.send_error(500, "injected embedding failure")
                    return
                dimensions = 3 if data["model"] == "three" else 2
                body = json.dumps({"embeddings": [[1.0] * dimensions for _ in data["input"]]}).encode()
                self.send_response(200)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)

        endpoint = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Embedder)
        worker = threading.Thread(target=endpoint.serve_forever, daemon=True)
        worker.start()
        flags = ["--embedding-provider", "ollama", "--embedding-model", "two", "--embedding-endpoint", f"http://127.0.0.1:{endpoint.server_port}"]
        # Override ambient model settings so the smoke is deterministic.
        clean_env = os.environ.copy()
        for key in list(clean_env):
            if key.startswith("ENGRAM_EMBEDDING_"):
                del clean_env[key]
        enabled_env = {**clean_env, "FASTEMBED_CACHE_DIR": str(cache), "ENGRAM_EMBEDDING_DIMENSIONS": "2", "ENGRAM_NO_FULL": "0"}
        # Server's merge would reintroduce removed inherited keys. Normal smoke
        # environments have none; supply an explicit valid profile to mask CONFIG.
        profile = root / "embedding.toml"
        profile.write_text('[embedding_provider]\nprovider_type = "ollama"\nmodel = "two"\n', encoding="utf-8")
        enabled_env["ENGRAM_EMBEDDING_CONFIG"] = str(profile)
        try:
            total = len(before["knowledge_chunks"])
            with running(binary, store, flags, enabled_env) as server:
                state["fail"] = True
                failed = server.call("reindex", {"limit": 1024})
                assert f"{total} more pending" in failed and "backlog drained" not in failed, failed
                state["fail"] = False
                for i in range(total):
                    result = server.call("reindex", {"limit": 1})
                    assert "embedded 1 chunks" in result, result
                    assert ("backlog drained" in result) == (i == total - 1), result
            ids = vector_ids(db)
            assert len(ids) == total, (ids, total)
            count = state["calls"]
            with running(binary, store, [*flags, "--slim"], env) as server:
                assert "retry_payment" in server.call("search", {"query": "retry_payment"})
            assert state["calls"] == count, "off mode contacted embedding endpoint"
            assert vector_ids(db) == ids
            with running(binary, store, flags, enabled_env) as server:
                # Served providers probe their dimensions during enabled startup;
                # only backfill itself must avoid embedding already-covered data.
                count = state["calls"]
                assert "backlog drained" in server.call("reindex", {})
                assert state["calls"] == count, "same-space reopen re-embedded existing vectors"
            print(f"PASS {total} hashless chunks: failed batch remains pending; cap=1 backfill; on/off/on preserves vectors")
            changed = ["three" if arg == "two" else arg for arg in flags]
            enabled_env["ENGRAM_EMBEDDING_DIMENSIONS"] = "3"
            with running(binary, store, changed, enabled_env) as server:
                assert "RequiresReindex" in server.call("capability_report", {})
            assert vector_ids(db) == ids, "mismatch reset vectors without consent"
            with running(binary, store, [*changed, "--rebuild-vectors"], enabled_env) as server:
                assert f"embedded {total} chunks" in server.call("reindex", {"limit": 1024})
            assert snapshot(db) == before, "vector rebuild changed source records"
            with running(binary, store, changed, enabled_env) as server:
                assert "backlog drained" in server.call("reindex", {})
            # Legacy no-model identity takes the same explicit safe rebuild path.
            with contextlib.closing(sqlite3.connect(db)) as conn, conn:
                conn.execute("UPDATE vector_meta SET value='none' WHERE key IN ('provider', 'model')")
            with running(binary, store, [*changed, "--rebuild-vectors"], enabled_env) as server:
                assert f"embedded {total} chunks" in server.call("reindex", {"limit": 1024})
            assert snapshot(db) == before
            print("PASS changed dimensions/model + legacy none identity: explicit vector-only rebuild; canonical records byte-identical")
        finally:
            endpoint.shutdown()
            endpoint.server_close()
            worker.join(timeout=5)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=Path("target/release/engram-mcp"))
    parser.add_argument("--off-only", action="store_true")
    args = parser.parse_args()
    verify(args.binary.resolve(), args.off_only)


if __name__ == "__main__":
    main()
