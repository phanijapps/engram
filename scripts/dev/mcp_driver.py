#!/usr/bin/env python3
"""MCP stdio driver — spawn engram-mcp, drive tool calls over JSON-RPC, time
each response. The reusable form of the session's throwaway drivers.

Usage:
  python3 scripts/dev/mcp_driver.py --storage ~/.engram/mem-alpha-self \\
      --project mem-alpha-self --no-vector \\
      call scan_repo '{"path": "/home/videogamer/projects/mem-alpha"}' \\
      call code_health '{}' \\
      call architecture '{"limit": 5}'

  # Quick timing of one call (model load is timed separately when vector is on):
  python3 scripts/dev/mcp_driver.py --storage ~/.engram/agentzero --project agentzero \\
      call reindex '{"limit": 256}'

Notes born of scars:
  - stderr is CAPTURED and printed after responses (boot warnings live there).
  - requests are sent AFTER --wait seconds (default 3; use 12+ with vector on
    for the FastEmbed model load) and each response is timed individually.
  - the server is killed on exit; stores with an open BFF (:3001) cannot be
    wiped while the BFF holds the DB — kill the BFF first.
"""
import argparse
import json
import subprocess
import sys
import time


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--storage", required=True)
    ap.add_argument("--project", default="default")
    ap.add_argument("--no-vector", action="store_true")
    ap.add_argument("--wait", type=float, default=3.0,
                    help="seconds to wait for boot before sending (12+ with vector on)")
    ap.add_argument("calls", nargs=argparse.REMAINDER,
                    help="pairs: call <tool> <json-args> [...]")
    args = ap.parse_args()

    requests: list[tuple[str, dict]] = []
    rest = list(args.calls)
    while rest:
        if rest.pop(0) != "call":
            print("expected 'call'", file=sys.stderr)
            return 2
        if len(rest) < 2:
            print("call needs <tool> <json-args>", file=sys.stderr)
            return 2
        tool = rest.pop(0)
        try:
            requests.append((tool, json.loads(rest.pop(0))))
        except json.JSONDecodeError as e:
            print(f"bad json args for {tool}: {e}", file=sys.stderr)
            return 2

    cmd = ["target/release/engram-mcp", "--storage", args.storage,
           "--project", args.project]
    if args.no_vector:
        cmd.append("--no-vector")
    proc = subprocess.Popen(cmd, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                            stderr=subprocess.PIPE, text=True, bufsize=1)
    time.sleep(args.wait)
    t0 = time.time()
    for i, (tool, tool_args) in enumerate(requests, start=1):
        proc.stdin.write(json.dumps(
            {"jsonrpc": "2.0", "id": i, "method": "tools/call",
             "params": {"name": tool, "arguments": tool_args}}) + "\n")
    proc.stdin.flush()

    got: dict[int, str] = {}
    deadline = time.time() + 300
    while time.time() < deadline and len(got) < len(requests):
        line = proc.stdout.readline()
        if not line:
            time.sleep(0.3)
            continue
        try:
            d = json.loads(line)
        except json.JSONDecodeError:
            continue
        if d.get("id") in set(range(1, len(requests) + 1)):
            got[d["id"]] = (d.get("result", {}).get("content") or [{}])[0].get("text", "")

    marks = [t0] * (len(requests) + 1)
    for i in sorted(got):
        marks[i] = time.time()
    for i, (tool, _) in enumerate(requests, start=1):
        print(f"===== {tool} ({marks[i] - marks[i - 1]:.2f}s) =====")
        print(got.get(i, "MISSING/TIMEOUT"))

    proc.stdin.close()
    try:
        proc.wait(timeout=10)
    except subprocess.TimeoutExpired:
        proc.kill()
    err = proc.stderr.read().strip()
    if err:
        print("=== STDERR ===")
        print(err[-3000:])
    return 0


if __name__ == "__main__":
    sys.exit(main())
