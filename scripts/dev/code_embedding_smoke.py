#!/usr/bin/env python3
"""Verify real local code embeddings in a disposable code-only MCP store.

Build: cargo build --release -p engram-mcp
Uses the configured/cached FastEmbed model; never modifies a shared store.
"""
import json
from pathlib import Path
import queue
import re
import subprocess
import tempfile
import threading

from code_source_smoke import Server


class VectorServer(Server):
    def __init__(self, root):
        self.stderr = []
        self.responses = queue.Queue()
        self.seq = 0
        self.proc = subprocess.Popen(
            ["target/release/engram-mcp", "--storage", str(root / "store"),
             "--project", "code-embedding-smoke", "--vector", "--tools", "all"],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
            text=True, bufsize=1)
        threading.Thread(target=self._read, daemon=True).start()
        threading.Thread(target=self._errors, daemon=True).start()


def main():
    root = Path(tempfile.mkdtemp(prefix="engram-code-vector-"))
    repo = root / "repo"
    repo.mkdir()
    raw = b'''export function settleInvoice(daysOverdue: number, balance: number) {
  // Calculate the late payment penalty for an overdue invoice.
  const dailyPenaltyRate = 0.02;
  function isValid(amount: number) { return amount >= 0; }
  if (!isValid(balance)) throw new Error('negative invoice balance');
  return balance + balance * dailyPenaltyRate * daysOverdue;
}
'''
    (repo / "billing.ts").write_bytes(raw)
    server = VectorServer(root)
    try:
        caps = server.call("capability_report", {})
        assert "code_embeddings: supported" in caps, caps
        scan = server.call("scan_repo", {"path": str(repo)})
        assert "errors: 0" in scan and "embedded 0 chunks" not in scan, scan
        manifests = json.loads(server.call("code_sources", {}))
        assert len(manifests) == 1 and manifests[0]["state"] == "active"
        source = json.loads(server.call("source_read", {
            "repository": manifests[0]["file"]["repository"], "path": "billing.ts"}))
        assert source["text"].encode() == raw
        result = server.call("search", {
            "query": "calculate the monetary late charge for an overdue bill",
            "limit": 5, "diagnostics": True})
        assert "(Chunk)" in result and "dailyPenaltyRate" in result, result
        assert re.search(r"\[[^\]\n]*\bvector\b", result), result
        repeat = server.call("reindex", {"limit": 256})
        assert "embedded 0 chunks" in repeat and "backlog drained" in repeat, repeat
        (root / "search.txt").write_text(result)
        print("PASS: real vector-lane code chunks; exact source; idle repeat", root)
        print(result)
    finally:
        server.close()


if __name__ == "__main__":
    main()
