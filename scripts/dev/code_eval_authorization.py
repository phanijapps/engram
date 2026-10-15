#!/usr/bin/env python3
"""Run controlled source denials against an already seeded isolated store."""
import argparse
import fcntl
import json
from pathlib import Path

from code_eval_corpus import sha256, validate_artifact_paths
from code_intelligence_eval import EvaluationServer, save


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--binary", type=Path, required=True)
    p.add_argument("--fixture", type=Path, required=True)
    p.add_argument("--manifest", type=Path, required=True)
    p.add_argument("--root", type=Path, required=True)
    p.add_argument("--output", type=Path, required=True)
    p.add_argument("--mode", choices=["baseline", "evidence"], default="baseline")
    args = p.parse_args()
    validate_artifact_paths(args.manifest, args.output, args.root / "store",
                            [args.root / "private", args.root / "expired"], [args.binary, args.fixture])
    control = json.loads(args.fixture.read_text())
    frozen = json.loads(args.manifest.read_text())["authorizationBattery"]
    if sha256(args.fixture) != frozen["sha256"]:
        raise ValueError("authorization fixture differs from frozen manifest")
    report = {"fixtureSha256": sha256(args.fixture), "binarySha256": sha256(args.binary),
              "mode": args.mode, "cases": [], "runState": "running"}
    # Same machine lock as corpus runs. Fixture is prepared before server launch.
    with (args.root.parent / "stores/.code-evaluation.lock").open("a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        server = EvaluationServer(args.binary, args.root / "store", "code-eval-denial")
        try:
            report["startup"] = server.call("capability_report", {})
            for c in control["sourceReadControls"]:
                r = server.call("source_read", {"repository": c["repository"], "path": c["path"]})
                report["cases"].append({"id": c["id"], **r})
                error = json.dumps(r["response"]).lower()
                if not r["failed"] or not ("policy" in error or "denied" in error):
                    raise AssertionError(f"source ownership denial failed: {c['id']}")
                if any(s in json.dumps(r["response"]) for s in control["forbiddenSourceSentinels"]):
                    raise AssertionError("denial response leaked forbidden source")
            for c in control["retrievalControls"]:
                name = "search" if args.mode == "baseline" else "code"
                extra = {"limit": 10, "diagnostics": True} if args.mode == "baseline" else {"op": "evidence", "resultLimit": 10}
                r = server.call(name, {"query": c["query"], **extra})
                report["cases"].append({"id": c["id"], **r})
                if r["failed"] or any(s in json.dumps(r["response"]) for s in control["forbiddenSourceSentinels"]):
                    raise AssertionError(f"unauthorized source returned: {c['id']}")
                if args.mode == "evidence":
                    packet = json.loads(r["text"])
                    if packet["evidenceStatus"] != "insufficient_evidence" or packet["items"] or packet["supportingUnits"]:
                        raise AssertionError("unauthorized inspection treated as evidence")
            report["runState"] = "complete"
            report["passed"] = True
        finally:
            server.close()
            report["stderr"] = server.errors
            if report["runState"] != "complete":
                report.update(runState="incomplete", passed=False)
            save(args.output, report)
    print("PASS: retained private/expired canonical sources deny outsider reads and retrieval")


if __name__ == "__main__":
    main()
