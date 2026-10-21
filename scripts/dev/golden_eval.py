#!/usr/bin/env python3
"""Golden eval runner — grade a built engram-mcp against examples/golden.

Drives ONE server process over stdio (scan, then every case in order —
single-process keeps hydration deterministic), grades each case from
examples/golden/cases.json, prints a scorecard, exits non-zero on any
failure.

Hardening (adversarial review, golden eval round 1):
- per-call deadline enforced via a reader thread (a wedged server fails
  fast instead of hanging CI);
- strict empty-response shape: the whole body must be the "No results"
  envelope (optionally plus the repository-filter note) — a "No results"
  line mixed with result rows is a FAILURE, not an empty pass;
- tool-error envelopes fail before grading;
- server terminated + reaped in `finally`; stderr captured to a file and
  surfaced on failure;
- the fixture is scanned from an ISOLATED COPY in a temp dir so displayed
  provenance is the fixture's own (`engram-mcp-scan`), not the surrounding
  git checkout.

Usage:
  python3 scripts/dev/golden_eval.py [--binary target/release/engram-mcp]
      [--storage <scratch>] [--project golden] [--keep]
"""
import argparse
import json
import os
import queue
import re
import shutil
import subprocess
import sys
import tempfile
import threading
import time

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
CASES_PATH = os.path.join(ROOT, "examples", "golden", "cases.json")
FIXTURE = os.path.join(ROOT, "examples", "golden", "zbot")
CALL_TIMEOUT = 60.0
RESULT_LINE = re.compile(r"^\S.* — .*\[", re.MULTILINE)  # "name — repo, path [score]"


class Server:
    def __init__(self, binary, storage, project, stderr_path, no_vector=True):
        cmd = [binary, "--storage", storage, "--project", project]
        if no_vector:
            cmd.append("--no-vector")
        self.stderr_file = open(stderr_path, "w")
        self.proc = subprocess.Popen(
            cmd, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
            stderr=self.stderr_file, text=True, bufsize=1)
        self._lines = queue.Queue()

        def reader():
            try:
                for line in self.proc.stdout:
                    self._lines.put(line)
            finally:
                self._lines.put(None)  # stream closed

        self._reader = threading.Thread(target=reader, daemon=True)
        self._reader.start()

    def call(self, tool, args, timeout=CALL_TIMEOUT):
        req = {"jsonrpc": "2.0", "id": 1, "method": "tools/call",
               "params": {"name": tool, "arguments": args}}
        self.proc.stdin.write(json.dumps(req) + "\n")
        self.proc.stdin.flush()
        try:
            line = self._lines.get(timeout=timeout)
        except queue.Empty:
            raise RuntimeError(f"tool {tool!r} timed out after {timeout}s")
        if line is None:
            raise RuntimeError("server closed the stream")
        msg = json.loads(line)
        if "error" in msg:
            return f"ERROR: {msg['error'].get('message')}"
        return msg["result"]["content"][0]["text"]

    def close(self):
        try:
            self.proc.terminate()
            self.proc.wait(timeout=10)
        except Exception:
            try:
                self.proc.kill()
                self.proc.wait(timeout=5)
            except Exception:
                pass
        finally:
            self.stderr_file.close()


def is_strictly_empty(text):
    """True only for the complete empty-response envelope. A 'No results'
    line mixed with result rows is NOT empty (the round-1 grader bug)."""
    body = text.strip()
    if body.startswith("ERROR:"):
        return False
    lines = [ln for ln in body.splitlines() if ln.strip()]
    if not lines:
        return True
    non_envelope = [ln for ln in lines
                    if not ln.startswith("No results")
                    and "filtered to repository" not in ln]
    return len(non_envelope) == 0


def grade(expectation, text, parsed):
    t = expectation["type"]
    if t == "contains":
        return expectation["value"] in text, f"expected {expectation['value']!r} in output"
    if t == "not_contains":
        return expectation["value"] not in text, f"expected {expectation['value']!r} ABSENT"
    if t == "empty":
        return is_strictly_empty(text), "expected the complete empty envelope"
    if t == "not_empty":
        return not is_strictly_empty(text), "expected a non-empty response"
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
    workdir = tempfile.mkdtemp(prefix="engram-golden-fixture-")
    stderr_path = os.path.join(workdir, "server.stderr")
    server = None
    try:
        # Isolated fixture copy: displayed provenance must be the fixture's
        # own (engram-mcp-scan), not the surrounding git checkout.
        fixture_copy = os.path.join(workdir, "zbot")
        shutil.copytree(FIXTURE, fixture_copy)

        server = Server(args.binary, storage, args.project, stderr_path)
        time.sleep(4)
        scan = server.call("scan_repo", {"path": fixture_copy})
        if "errors: 0" not in scan:
            print(f"SCAN FAILED: {scan[:300]}")
            return 2
        # Deterministic hydration: one warm-up recall before grading.
        server.call("search", {"query": "warmup golden", "limit": 1})

        # Advertised-schema assertion (not expressible as a case): the
        # suppression opt-in must be DISCOVERABLE by schema-driven clients,
        # not just accepted by the handler.
        list_req = {"jsonrpc": "2.0", "id": 1, "method": "tools/list"}
        server.proc.stdin.write(json.dumps(list_req) + "\n")
        server.proc.stdin.flush()
        line = server._lines.get(timeout=CALL_TIMEOUT)
        tools = json.loads(line)["result"]["tools"]
        search_tool = next((t for t in tools if t.get("name") == "search"), None)
        schema_ok = search_tool is not None and search_tool.get("inputSchema", {}).get(
            "properties", {}).get("include_code_artifacts", {}).get("type") == "boolean"
        print(f"[{'PASS' if schema_ok else 'FAIL'}] G-SCHEMA-01: search advertises include_code_artifacts in tools/list")
        if not schema_ok:
            return 1

        passed, failed = 0, []
        for case in cases:
            text = server.call(case["tool"], case["args"])
            if text.startswith("ERROR:"):
                print(f"[FAIL] {case['id']}: {case['guard']}")
                print(f"       -> tool error: {text[:200]}")
                failed.append((case["id"], text[:200]))
                continue
            parsed = None
            if case["tool"] == "fetch_content":
                try:
                    parsed = json.loads(text)
                except Exception:
                    parsed = None
            case_ok, reason, actual = True, "", text
            for exp in case["expect"]:
                ok, why = grade(exp, text, parsed)
                if not ok:
                    case_ok, reason = False, why
                    break
            status = "PASS" if case_ok else "FAIL"
            print(f"[{status}] {case['id']}: {case['guard']}")
            if not case_ok:
                print(f"       -> {reason}")
                print(f"       actual: {actual[:300]!r}")
                failed.append((case["id"], reason))
            else:
                passed += 1
        print(f"\ngolden eval: {passed}/{len(cases)} passed")
        if failed:
            tail = open(stderr_path).read()[-500:]
            if tail.strip():
                print(f"--- server stderr tail ---\n{tail}")
        return 1 if failed else 0
    finally:
        if server is not None:
            server.close()
        if not args.keep:
            shutil.rmtree(storage, ignore_errors=True)
            shutil.rmtree(workdir, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())
