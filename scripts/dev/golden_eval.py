#!/usr/bin/env python3
"""Golden eval runner — grade a built engram-mcp against examples/golden.

Drives ONE server process over stdio (scan, then every case in order —
single-process keeps hydration deterministic), grades each case from
examples/golden/cases.json, prints a scorecard, exits non-zero on any
failure. Cases are contracts: see examples/golden/README.md.

Usage:
  python3 scripts/dev/golden_eval.py [--binary target/release/engram-mcp]
      [--storage <scratch>] [--project golden] [--keep]
"""
import argparse
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import time

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
CASES_PATH = os.path.join(ROOT, "examples", "golden", "cases.json")
FIXTURE = os.path.join(ROOT, "examples", "golden", "zbot")


class Server:
    def __init__(self, binary, storage, project, no_vector=True):
        cmd = [binary, "--storage", storage, "--project", project]
        if no_vector:
            cmd.append("--no-vector")
        self.proc = subprocess.Popen(
            cmd, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL, text=True, bufsize=1)

    def call(self, tool, args, timeout=60):
        req = {"jsonrpc": "2.0", "id": 1, "method": "tools/call",
               "params": {"name": tool, "arguments": args}}
        self.proc.stdin.write(json.dumps(req) + "\n")
        self.proc.stdin.flush()
        line = self.proc.stdout.readline()
        if not line:
            raise RuntimeError("server closed the stream")
        msg = json.loads(line)
        if "error" in msg:
            return f"ERROR: {msg['error'].get('message')}"
        return msg["result"]["content"][0]["text"]

    def close(self):
        try:
            self.proc.terminate()
        except Exception:
            pass


def grade(expectation, text, parsed):
    t = expectation["type"]
    if t == "contains":
        return expectation["value"] in text, f"expected {expectation['value']!r} in output"
    if t == "not_contains":
        return expectation["value"] not in text, f"expected {expectation['value']!r} ABSENT"
    if t == "empty":
        stripped = text.strip()
        return ("No results" in stripped or stripped == ""), "expected empty result"
    if t == "not_empty":
        return len(text.strip()) > 0 and "No results" not in text, "expected non-empty"
    if t == "regex":
        return re.search(expectation["value"], text) is not None, f"regex {expectation['value']!r}"
    if t == "json_field":
        if parsed is None:
            return False, "output is not JSON"
        field = parsed
        for part in expectation["field"].split("."):
            if isinstance(field, dict) and part in field:
                field = field[part]
            else:
                return False, f"field {expectation['field']} missing"
        op, want = expectation["op"], expectation["value"]
        ok = {"eq": lambda: field == want, "gte": lambda: field >= want,
              "lte": lambda: field <= want}.get(op, lambda: False)()
        return ok, f"field {expectation['field']}={field!r} failed {op} {want!r}"
    if t == "set_equals_field":
        m = re.search(expectation["regex"], text)
        if not m:
            return False, f"regex {expectation['regex']!r} found no list"
        got = {s.strip().strip('"') for s in m.group(1).split(",") if s.strip()}
        want = set(expectation["value"])
        return got == want, f"set {sorted(got)} != {sorted(want)}"
    return False, f"unknown assertion type {t}"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--binary", default=os.path.join(ROOT, "target", "release", "engram-mcp"))
    ap.add_argument("--storage", default=None)
    ap.add_argument("--project", default="golden")
    ap.add_argument("--keep", action="store_true", help="keep the scratch store")
    args = ap.parse_args()

    cases = json.load(open(CASES_PATH))["cases"]
    storage = args.storage or tempfile.mkdtemp(prefix="engram-golden-")
    try:
        server = Server(args.binary, storage, args.project)
        time.sleep(4)
        scan = server.call("scan_repo", {"path": FIXTURE})
        if "errors: 0" not in scan:
            print(f"SCAN FAILED: {scan[:300]}")
            return 2
        # Deterministic hydration: one warm-up recall before grading.
        server.call("search", {"query": "warmup golden", "limit": 1})

        passed, failed = 0, []
        for case in cases:
            text = server.call(case["tool"], case["args"])
            parsed = None
            if case["tool"] == "fetch_content":
                try:
                    parsed = json.loads(text)
                except Exception:
                    parsed = None
            case_ok, reason = True, ""
            for exp in case["expect"]:
                ok, why = grade(exp, text, parsed)
                if not ok:
                    case_ok, reason = False, why
                    break
            status = "PASS" if case_ok else "FAIL"
            print(f"[{status}] {case['id']}: {case['guard']}")
            if not case_ok:
                print(f"       -> {reason}")
                failed.append((case["id"], reason))
            else:
                passed += 1
        server.close()
        print(f"\ngolden eval: {passed}/{len(cases)} passed")
        return 1 if failed else 0
    finally:
        if not args.keep and args.storage is None:
            shutil.rmtree(storage, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())
