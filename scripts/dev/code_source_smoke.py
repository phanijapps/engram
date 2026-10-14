#!/usr/bin/env python3
"""Assert source preservation through a real MCP binary in disposable stores.

Run after cargo build --release -p engram-mcp. No embedding/model calls.
The temporary root is printed and retained for inspection; no shared store is used.
"""
import json
from pathlib import Path
import queue
import sqlite3
import subprocess
import tempfile
import threading
import time


class Server:
    def __init__(self, root):
        self.stderr = []
        self.responses = queue.Queue()
        self.proc = subprocess.Popen(
            ["target/release/engram-mcp", "--storage", str(root / "store"),
             "--project", "source-smoke", "--no-vector"],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
            text=True, bufsize=1)
        threading.Thread(target=self._read, daemon=True).start()
        threading.Thread(target=self._errors, daemon=True).start()
        self.seq = 0

    def _read(self):
        for line in self.proc.stdout:
            self.responses.put(json.loads(line))

    def _errors(self):
        for line in self.proc.stderr:
            self.stderr.append(line.strip())

    def call(self, name, args, failure=False):
        self.seq += 1
        start = time.monotonic()
        self.proc.stdin.write(json.dumps({"jsonrpc": "2.0", "id": self.seq,
            "method": "tools/call", "params": {"name": name, "arguments": args}}) + "\n")
        self.proc.stdin.flush()
        response = self.responses.get(timeout=60)
        assert response.get("id") == self.seq, response
        result = response.get("result", {})
        failed = "error" in response or result.get("isError", False)
        text = "\n".join(c.get("text", "") for c in result.get("content", []))
        if failure:
            assert failed or "error" in text.lower() or "unavailable" in text.lower(), response
        else:
            assert not failed, response
        print(f"{name} {args.get('op', '')}: {time.monotonic()-start:.3f}s")
        return text

    def close(self):
        self.proc.stdin.close()
        try:
            self.proc.wait(timeout=10)
        except subprocess.TimeoutExpired:
            self.proc.kill()
            self.proc.wait()
        print("\n".join(self.stderr[-8:]))


def verify(root, existing=False):
    repo = root / "repo"
    server = Server(root)
    try:
        scan = {"target": "repo", "path": str(repo)}
        summary = server.call("scan", scan)
        assert "errors: 0" in summary, summary
        if existing:
            assert "unchanged: 4" in summary, summary
        manifests = json.loads(server.call("code", {"op": "sources"}))
        assert len(manifests) == 4
        for manifest in manifests:
            read = json.loads(server.call("code", {"op": "source_read",
                "repository": manifest["file"]["repository"], "path": manifest["file"]["path"]}))
            raw = (repo / manifest["file"]["path"]).read_bytes()
            assert read["text"].encode("utf-8") == raw
            assert all(c["role"] != "source_snapshot" for c in read["coverage"])
            records = [(r["kind"], r["id"]) for r in read["manifest"]["records"]]
            assert len(records) == len(set(records))
        search = server.call("code", {"op": "search", "query": "checkout", "limit": 3})
        assert "checkout" in search, search
        with sqlite3.connect(root / "store" / "engram_data.db") as conn:
            assert conn.execute("SELECT count(*) FROM knowledge_chunks WHERE json_extract(record_json,'$.code.role')='source_snapshot'").fetchone()[0] == 0
            units = conn.execute("SELECT record_json FROM knowledge_chunks WHERE json_extract(record_json,'$.code.role')='symbol'").fetchall()
            assert any(all(token in json.loads(row[0])["text"] for token in
                ["const fee", "function helper", "throw new Error", "return fee"]) for row in units)
        second = server.call("scan", scan)
        assert "unchanged: 4" in second and "ingested: 0" in second, second
        server.call("code", {"op": "source_read", "repository": "engram-mcp-scan",
            "path": "user.ts", "scope": {"tenant": "other"}}, failure=True)
        if existing:
            (repo / "user.ts").write_bytes(b"\xff")
            failed = server.call("scan", scan)
            assert "UnsupportedEncoding" in failed, failed
            read = json.loads(server.call("code", {"op": "source_read", "repository": "engram-mcp-scan", "path": "user.ts"}))
            assert "checkout" in read["text"]
            (repo / "user.ts").unlink()
            removed = server.call("scan", scan)
            assert "errors: 0" in removed, removed
            server.call("code", {"op": "source_read", "repository": "engram-mcp-scan", "path": "user.ts"}, failure=True)
            sibling = json.loads(server.call("code", {"op": "source_read", "repository": "engram-mcp-scan", "path": "app.py"}))
            assert sibling["text"].encode() == (repo / "app.py").read_bytes()
    finally:
        server.close()


def main():
    root = Path(tempfile.mkdtemp(prefix="engram-source-smoke-"))
    (root / "repo").mkdir()
    cases = {
        "user.ts": "\ufeffimport { Database } from './db';\r\nexport class UserService {\r\n private db: Database;\r\n async checkout(id: string) {\r\n const fee=1; function helper() {}\r\n if (!id) throw new Error('id'); return fee;\r\n }\r\n}\r\n",
        "main.rs": 'fn outer() { let fee=1; fn nested() {} println!("{fee}"); }\n',
        "app.py": "def outer():\n    fee=1\n    def nested():\n        pass\n    return fee\n",
        "empty.ts": "",
    }
    for path, text in cases.items():
        (root / "repo" / path).write_bytes(text.encode())
    print(f"Temporary root: {root}")
    verify(root)
    verify(root, existing=True)
    print("PASS: fresh/reopened stores, exact polyglot source, complete nested body, exclusions, rescan, invalid bytes, removal. Vector disabled.")


if __name__ == "__main__":
    main()
