#!/usr/bin/env python3
"""Sequential real-MCP code evaluation. Retains raw responses and true timings.

Labels never enter MCP requests. Baseline search cannot expose the complete
candidate pool, so candidate/phase metrics are explicitly unavailable there.
Use a frozen corpus copy, never the working repository that contains these labels.
"""
import argparse
import fcntl
import json
from pathlib import Path
import queue
import subprocess
import threading
import time

from code_eval_corpus import load_frozen, sha256, validate_artifact_paths


class EvaluationServer:
    def __init__(self, binary, storage, project, tool_profile="all", extra_args=()):
        self.responses = queue.Queue()
        self.errors = []
        self.seq = 0
        self.proc = subprocess.Popen(
            [str(binary), "--storage", str(storage), "--project", project,
             "--vector", "--tools", tool_profile, *extra_args], stdin=subprocess.PIPE,
            stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, bufsize=1)
        threading.Thread(target=self._read, daemon=True).start()
        threading.Thread(target=self._errors, daemon=True).start()

    def _read(self):
        try:
            for line in self.proc.stdout:
                self.responses.put(json.loads(line))
        finally:
            self.responses.put(None)

    def _errors(self):
        for line in self.proc.stderr:
            self.errors.append(line.rstrip())

    def call(self, name, arguments, timeout=1200):
        self.seq += 1
        request = {"jsonrpc": "2.0", "id": self.seq, "method": "tools/call",
                   "params": {"name": name, "arguments": arguments}}
        start = time.monotonic()
        self.proc.stdin.write(json.dumps(request) + "\n")
        self.proc.stdin.flush()
        response = self.responses.get(timeout=timeout)
        elapsed = time.monotonic() - start
        if response is None or response.get("id") != self.seq:
            raise RuntimeError(f"server exited or response order violated: {response}")
        result = response.get("result", {})
        return {"seconds": elapsed, "failed": "error" in response or bool(result.get("isError")),
                "text": "\n".join(c.get("text", "") for c in result.get("content", [])),
                "response": response}

    def close(self):
        self.proc.stdin.close()
        try:
            self.proc.wait(timeout=10)
        except subprocess.TimeoutExpired:
            self.proc.kill()
            self.proc.wait()


def save(path, report):
    temp = path.with_suffix(".tmp")
    temp.write_text(json.dumps(report, indent=2) + "\n")
    temp.replace(path)


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--manifest", type=Path, required=True)
    p.add_argument("--corpus", required=True)
    p.add_argument("--corpus-root", type=Path, required=True)
    p.add_argument("--storage", type=Path, required=True)
    p.add_argument("--binary", type=Path, required=True)
    p.add_argument("--output", type=Path, required=True)
    p.add_argument("--split", choices=["development", "heldout", "original", "identifier"], required=True)
    p.add_argument("--mode", choices=["baseline", "evidence"], required=True)
    p.add_argument("--scan", action="store_true")
    args = p.parse_args()
    load_frozen(args.manifest, args.corpus, args.corpus_root, args.split)
    validate_artifact_paths(args.manifest, args.output, args.storage,
                            [args.corpus_root], [args.binary])
    args.storage.mkdir(parents=True, exist_ok=True)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    binary = args.binary.resolve(strict=True)
    # A store-local queue is insufficient for a single-machine latency baseline.
    # All sibling evaluation stores share this OS lock, including preparation.
    lock = (args.storage.parent / ".code-evaluation.lock").open("a")
    fcntl.flock(lock, fcntl.LOCK_EX)
    # Queued preparation may take minutes. Validate again after acquiring the
    # machine before allowing it to read any corpus or query input.
    manifest, corpus, queries, manifest_digest = load_frozen(
        args.manifest, args.corpus, args.corpus_root, args.split)
    binary_digest = sha256(binary)
    if args.mode == "baseline" and binary_digest != manifest["baselineBinarySha256"]:
        raise ValueError("baseline binary differs from frozen binary")
    report = {"manifestSha256": manifest_digest, "corpus": args.corpus,
              "revision": corpus["revision"], "mode": args.mode, "split": args.split,
              "binarySha256": binary_digest, "concurrency": 1, "cases": [],
              "candidateMetricsAvailable": False, "runState": "running"}
    server = EvaluationServer(binary, args.storage, "code-eval-" + args.corpus,
                              tool_profile="all" if args.mode == "baseline" else "full")
    try:
        report["startup"] = server.call("capability_report", {}) if args.mode == "baseline" else server.call("maintain", {"op":"capabilities"})
        if report["startup"]["failed"] or "vector: supported" not in report["startup"]["text"]:
            raise RuntimeError("evaluation requires actual vector readiness")
        save(args.output, report)
        if args.scan:
            report["scan"] = server.call("scan_repo", {"path": str(args.corpus_root.resolve())}) if args.mode == "baseline" else server.call("scan", {"target":"repo","path":str(args.corpus_root.resolve())})
            save(args.output, report)
            if report["scan"]["failed"] or "errors: 0" not in report["scan"]["text"]:
                raise RuntimeError("corpus scan failed")
            print("scan", round(report["scan"]["seconds"], 3), flush=True)
            while True:
                r = server.call("reindex", {"limit": 1024}) if args.mode == "baseline" else server.call("maintain", {"op":"reindex","limit":1024})
                report.setdefault("reindex", []).append(r)
                save(args.output, report)
                print("reindex", round(r["seconds"], 3), r["text"][:120], flush=True)
                if r["failed"]:
                    raise RuntimeError("reindex failed")
                if "backlog drained" in r["text"]:
                    break
        # One declared warm-up, never included in per-query quality/p95.
        if queries:
            name = "search" if args.mode == "baseline" else "code"
            base = {"limit": 10, "diagnostics": True} if args.mode == "baseline" else {"op": "evidence", "resultLimit": 10}
            report["warmup"] = server.call(name, {**base, "query": queries[0]["query"]})
            for entry in queries:
                r = server.call(name, {**base, "query": entry["query"]})
                report["cases"].append({"id": entry["id"], **r})
                save(args.output, report)
                print(entry["id"], round(r["seconds"], 3), "error" if r["failed"] else "ok", flush=True)
        report["runState"] = "complete"
    finally:
        server.close()
        report["stderr"] = server.errors
        if report["runState"] != "complete":
            report["runState"] = "incomplete"
        save(args.output, report)
        lock.close()


if __name__ == "__main__":
    main()
