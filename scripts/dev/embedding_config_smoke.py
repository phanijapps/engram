#!/usr/bin/env python3
"""Real MCP model-selection smoke. Uses disposable stores; no shared store writes.
Build: cargo build --release -p engram-mcp
BGE assets must be cached or this explicitly requested run downloads them.
The served-model case uses a deterministic loopback fixture, not a quality benchmark.
"""
import json
import os
from pathlib import Path
import queue
import sqlite3
import subprocess
import tempfile
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

BINARY = Path("target/release/engram-mcp").resolve()
ROOT = Path(tempfile.mkdtemp(prefix="engram-embedding-smoke-"))
ENV = {k: v for k, v in os.environ.items() if not k.startswith("ENGRAM_EMBEDDING_")}
REPORT = {}

class Server:
    def __init__(self, name, args, storage=None):
        self.storage = storage or ROOT / name
        self.responses = queue.Queue()
        self.errors = []
        self.seq = 0
        self.proc = subprocess.Popen([str(BINARY), "--tools", "all", "--storage", str(self.storage), "--project", "embedding-smoke", *args], env=ENV,
                                     stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        threading.Thread(target=self.read, daemon=True).start()
        self.stderr_thread = threading.Thread(target=lambda: self.errors.extend(self.proc.stderr.read().splitlines()), daemon=True)
        self.stderr_thread.start()
        self.name = name
    def read(self):
        for line in self.proc.stdout:
            self.responses.put(json.loads(line))
        self.responses.put(None)
    def call(self, name, arguments):
        self.seq += 1
        start = time.monotonic()
        self.proc.stdin.write(json.dumps({"jsonrpc":"2.0", "id":self.seq, "method":"tools/call", "params":{"name":name,"arguments":arguments}})+"\n")
        self.proc.stdin.flush()
        response = self.responses.get(timeout=180)
        assert response and response.get("id") == self.seq, (response,self.errors)
        result = response.get("result", {})
        assert "error" not in response and not result.get("isError"), response
        text = "\n".join(c.get("text","") for c in result.get("content",[]))
        REPORT.setdefault(self.name, []).append({"tool":name,"seconds":time.monotonic()-start,"text":text})
        print(self.name, name, round(time.monotonic()-start,3),flush=True)
        return text
    def close(self):
        self.proc.stdin.close()
        try: self.proc.wait(timeout=10)
        except subprocess.TimeoutExpired: self.proc.kill(); self.proc.wait()
        self.stderr_thread.join(timeout=2)
        assert self.proc.returncode == 0,self.errors
        (ROOT/f"{self.name}.stderr.log").write_text("\n".join(self.errors))

def metadata(storage):
    with sqlite3.connect(storage/"engram_data.db") as db:
        meta = dict(db.execute("SELECT key,value FROM vector_meta"))
        count = db.execute("SELECT count(*) FROM vectors_rowids").fetchone()[0]
    return meta,count

def model(name,args,dimensions):
    server = Server(name,args)
    try:
        assert "vector: supported" in server.call("capability_report",{})
        server.call("index_docs",{"path":"invoice.md","content":"# Invoice calculation\nOverdue invoice payment charges are calculated from the unpaid balance and days overdue.\n"})
        progress = server.call("reindex",{"limit":32})
        assert "backlog drained" in progress and "embedded 0 chunks" not in progress,progress
        result = server.call("search",{"query":"invoice payment charges", "diagnostics":True,"limit":5})
        assert "invoice" in result.lower() and "vector" in result.lower(),result
        repeat = server.call("reindex",{"limit":32})
        assert "embedded 0 chunks" in repeat and "backlog drained" in repeat,repeat
    finally: server.close()
    meta,count = metadata(server.storage)
    assert int(meta["dimensions"])==dimensions and count>0,(meta,count)
    REPORT[name+"_metadata"]={"metadata":meta,"vectors":count}
    return server.storage,meta,count

class Fixture(BaseHTTPRequestHandler):
    calls = 0
    def log_message(self,*args): pass
    def do_POST(self):
        Fixture.calls += 1
        body=json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        assert self.path == "/v1/embeddings" and body["model"] == "fixture-model",body
        vectors={"data":[{"index":i,"embedding":[1.0,0.25,0.5]} for i,_ in enumerate(body["input"])]}
        raw=json.dumps(vectors).encode()
        self.send_response(200); self.send_header("Content-Type","application/json");self.send_header("Content-Length",str(len(raw))); self.end_headers();self.wfile.write(raw)


def main():
    print("Artifacts:",ROOT,flush=True)
    storage,original,count=model("default",["--embedding-preset","default"],384)
    model("local-alternative",["--embedding-preset","local-alternative"],768)
    changed=Server("fenced",["--embedding-preset","local-alternative"],storage)
    try:
        assert "vector: unsupported" in changed.call("capability_report",{})
        assert "no embedding provider wired" in changed.call("reindex",{})
    finally: changed.close()
    assert metadata(storage)==(original,count),"model switch changed stored vectors"
    restore=Server("restored",["--embedding-preset","default"],storage)
    try:
        assert "vector: supported" in restore.call("capability_report",{})
        assert "embedded 0 chunks" in restore.call("reindex",{})
    finally: restore.close()
    fixture=ThreadingHTTPServer(("127.0.0.1",0),Fixture)
    threading.Thread(target=fixture.serve_forever,daemon=True).start()
    endpoint=f"http://127.0.0.1:{fixture.server_port}"
    try:
        model("served-fixture",["--embedding-provider","openai-compatible","--embedding-model","fixture-model","--embedding-endpoint",endpoint],3)
        previous=Fixture.calls
        disabled=Server("disabled",["--no-vector","--embedding-provider","openai-compatible","--embedding-model","fixture-model","--embedding-endpoint",endpoint,"--embedding-api-key-env","MISSING_EMBEDDING_SMOKE_KEY"])
        try: assert "vector: unsupported" in disabled.call("capability_report",{})
        finally: disabled.close()
        assert Fixture.calls==previous,"disabled vectors contacted endpoint"
    finally: fixture.shutdown();fixture.server_close()
    (ROOT/"receipt.json").write_text(json.dumps(REPORT,indent=2))
    print("PASS: actual BGE384/BGEBase768, served auto-dimensions, vector retrieval, repeat reindex, model fence and disabled no-egress",ROOT,flush=True)

if __name__=="__main__": main()
