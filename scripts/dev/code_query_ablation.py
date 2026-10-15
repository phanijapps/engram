#!/usr/bin/env python3
"""Compare three code-query formats using frozen DEVELOPMENT labels only.

The SDK probe receives no labels. This vector-only experiment chooses a query
format; it is neither the final fused-pipeline evaluation nor beta readiness.
"""
import argparse
import fcntl
import json
import math
import queue
from pathlib import Path
import subprocess
import threading

from code_eval_corpus import load_frozen, sha256, validate_artifact_paths
from code_intelligence_eval import EvaluationServer, save

FORMATS = ["legacy-v1", "none", "bge-retrieval-v1"]


def matches(parent, unit):
    if parent["path"] != unit["path"]:
        return False
    anchor = (parent.get("anchor") or "").partition(" ")[2].replace("::", ".")
    wanted = unit["qualifiedAnchor"].replace("::", ".")
    if anchor != wanted:
        return False
    declaration = parent.get("declarationRange")
    return declaration is not None and all(
        declaration["start"] <= r["startByte"] and declaration["end"] >= r["endByte"]
        for r in unit["requiredRanges"])


def judge(parents, units):
    """Offline candidate ranking only; no runtime evidence validation is implied."""
    recovered = set()
    first = None
    dcg = 0.0
    for rank, parent in enumerate(parents[:10], 1):
        new = {i for i, unit in enumerate(units) if matches(parent, unit)} - recovered
        if new:
            first = first or rank
            dcg += 7.0 / math.log2(rank + 1)
            recovered.update(new)
    ideal = sum(7.0 / math.log2(rank + 1) for rank in range(1, min(len(units), 10) + 1))
    return {"offlineRequiredUnitRecall10": len(recovered) / len(units),
            "offlineMrr10": 1.0 / first if first else 0.0,
            "offlineNdcg10": dcg / ideal,
            "recoveredRequiredUnits": sorted(recovered)}


class QueryProbe:
    """Bounded line protocol; teardown finishes before releasing the machine."""
    def __init__(self, command):
        self.process = subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                        stderr=subprocess.PIPE, text=True, bufsize=1)
        self.responses = queue.Queue()
        self.errors = []
        def read_output():
            try:
                for line in self.process.stdout:
                    self.responses.put(line)
            finally:
                self.responses.put(None)
        def read_errors():
            for line in self.process.stderr:
                self.errors.append(line.rstrip())
        self.readers = [threading.Thread(target=read_output, daemon=True),
                        threading.Thread(target=read_errors, daemon=True)]
        for reader in self.readers:
            reader.start()

    def call(self, query, instruction, timeout=120):
        self.process.stdin.write(json.dumps({"query": query, "instruction": instruction}) + "\n")
        self.process.stdin.flush()
        try:
            raw = self.responses.get(timeout=timeout)
        except queue.Empty as error:
            raise TimeoutError(f"query probe exceeded {timeout} seconds") from error
        if raw is None:
            raise RuntimeError("query probe exited")
        response = json.loads(raw)
        if "error" in response:
            raise RuntimeError(response["error"])
        return response

    def close(self, grace=10):
        try:
            self.process.stdin.close()
        except OSError:
            pass
        try:
            self.process.wait(timeout=grace)
        except subprocess.TimeoutExpired:
            self.process.terminate()
            try:
                self.process.wait(timeout=grace)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.wait()
        for reader in self.readers:
            reader.join(timeout=5)
        self.process.stdout.close()
        self.process.stderr.close()


def execute(args, report, corpus, questions):
    if args.prepare:
        server = EvaluationServer(args.mcp, args.storage, "code-eval-" + args.corpus)
        try:
            startup = server.call("capability_report", {})
            report["startup"] = startup
            if startup["failed"] or "code_embeddings: supported" not in startup["text"]:
                raise RuntimeError("actual code embeddings unavailable")
            report["scan"] = server.call("scan_repo", {"path": str(args.corpus_root.resolve())})
            save(args.output, report)
            if report["scan"]["failed"] or "errors: 0" not in report["scan"]["text"]:
                raise RuntimeError("scan failed")
            print(args.corpus, "scan", round(report["scan"]["seconds"], 3), flush=True)
            for _ in range(1000):
                result = server.call("reindex", {"limit": 1024})
                report.setdefault("reindex", []).append(result)
                save(args.output, report)
                print(args.corpus, "reindex", round(result["seconds"], 3), result["text"][:90], flush=True)
                if result["failed"]:
                    raise RuntimeError("reindex failed")
                if "backlog drained" in result["text"]:
                    break
            else:
                raise RuntimeError("index backlog did not drain")
        finally:
            server.close()
            report["mcpStderr"] = server.errors
            save(args.output, report)
    probe = QueryProbe([str(args.probe.resolve()), str(args.storage.resolve()),
                        "code-eval-" + args.corpus])
    try:
        for instruction in FORMATS:
            for number, question in enumerate(questions):
                # One declared warm-up per format, excluded from all metrics.
                if number == 0:
                    probe.call(question["query"], instruction)
                observed = probe.call(question["query"], instruction)
                if observed["rawSegmentHits"]==0:
                    raise RuntimeError("query probe has no actual code vectors")
                if any(parent["path"] not in corpus["files"] for parent in observed["parents"]):
                    raise RuntimeError("query returned a parent outside frozen corpus")
                if any(parent["sourceHash"].removeprefix("sha256:") != corpus["files"][parent["path"]] for parent in observed["parents"]):
                    raise RuntimeError("query returned a non-frozen source generation")
                metrics = judge(observed["parents"], question["requiredUnits"]) if question["requiredUnits"] else None
                report["cases"].append({"id": question["id"], "instruction": instruction,
                                        "polarity": question["polarity"], "observed": observed,
                                        "candidateRankingMetrics": metrics})
                save(args.output, report)
                print(args.corpus, instruction, question["id"], round(observed["seconds"], 3), flush=True)
        metrics = ["offlineMrr10", "offlineNdcg10", "offlineRequiredUnitRecall10"]
        summary = {}
        for instruction in FORMATS:
            rows = [r["candidateRankingMetrics"] for r in report["cases"]
                    if r["instruction"] == instruction and r["candidateRankingMetrics"] is not None]
            summary[instruction] = {key: sum(row[key] for row in rows) / len(rows) for key in metrics}
        report["summary"] = summary
        report["selectedWithinCorpus"] = max(FORMATS, key=lambda name: tuple(summary[name][key] for key in metrics))
        report["runState"] = "complete"
    finally:
        probe.close()
        report["probeStderr"] = probe.errors
        save(args.output, report)


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--manifest", type=Path, required=True)
    p.add_argument("--corpus", choices=["engram", "flask", "vite"], required=True)
    p.add_argument("--corpus-root", type=Path, required=True)
    p.add_argument("--storage", type=Path, required=True)
    p.add_argument("--mcp", type=Path, required=True)
    p.add_argument("--probe", type=Path, required=True)
    p.add_argument("--output", type=Path, required=True)
    p.add_argument("--prepare", action="store_true")
    args = p.parse_args()
    load_frozen(args.manifest, args.corpus, args.corpus_root, "development")
    validate_artifact_paths(args.manifest, args.output, args.storage,
                            [args.corpus_root], [args.mcp, args.probe])
    args.storage.mkdir(parents=True, exist_ok=True)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    with (args.storage.parent / ".code-evaluation.lock").open("a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        _, corpus, questions, digest = load_frozen(
            args.manifest, args.corpus, args.corpus_root, "development")
        report = {"mode": "vector-query-format-ablation", "split": "development",
                  "corpus": args.corpus, "revision": corpus["revision"],
                  "manifestSha256": digest, "mcpSha256": sha256(args.mcp),
                  "probeSha256": sha256(args.probe), "concurrency": 1,
                  "rawSegmentDepth": 50, "rankDepth": 10,
                  "selectionRule": "mean answerable MRR10, then nDCG10, then recall10; ties prefer legacy-v1, none, bge-retrieval-v1",
                  "featureShipped": False, "runtimeEvidenceValidated": False,
                  "runState": "running", "cases": []}
        save(args.output, report)
        try:
            execute(args, report, corpus, questions)
        except BaseException as error:
            report["runState"] = "incomplete"
            report["error"] = f"{type(error).__name__}: {error}"
            raise
        finally:
            save(args.output, report)
    print(json.dumps({"corpus": args.corpus, "summary": report.get("summary"),
                      "runState": report["runState"]}), flush=True)


if __name__ == "__main__":
    main()
