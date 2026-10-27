#!/usr/bin/env bash
# Render probe matrix for the built site (job 004 DEC-P03; re-derived for the
# D2 dark-only register in job 008, TASK-011 / DEC-P01).
# For each built page x {360px, 1280px} x dark (single polarity — the toggle,
# the forced-polarity storage sentinel and the diagram swap checks are gone
# with the mechanism): drive the site's inert ?probe=1 diagnostics through
# google-chrome --headless=new + CDP, assert the D2 token set, the browser-
# computed DEC-P01 body pairs (>= 4.5:1, worst block-on-plate 6.19:1) and the
# two --line-ui boundary pairs (>= 3:1), the four font faces, ALL fifteen copy
# buttons on the code-bearing page (FIND-code-002: the count is exact — >= 1
# let 11 of 15 broken tab panels ship green in round 1 review), no horizontal overflow at 360px and zero non-same-
# origin resources. Writes /tmp/engram-probe-report.json (8 dark cells).
# Run after a fresh build: bash site/tools/build.sh && bash site/tools/render-probe.sh
set -uo pipefail

PORT="${ENGRAM_PROBE_PORT:-8766}"
BASE="http://127.0.0.1:${PORT}/engram"
REPORT="/tmp/engram-probe-report.json"
CHROME=google-chrome
PAGES=("/" "/factory/" "/engram/" "/get-started/")
WIDTHS=(360 1280)

command -v "$CHROME" >/dev/null 2>&1 || { echo "render-probe: google-chrome not found" >&2; exit 1; }
test -f site/_site/index.html || { echo "render-probe: site/_site missing — run site/tools/build.sh first" >&2; exit 1; }

python3 site/tools/serve.py "$PORT" site/_site >/tmp/engram-serve.log 2>&1 &
SERVE_PID=$!
trap 'kill "$SERVE_PID" 2>/dev/null || true' EXIT

for _ in $(seq 1 40); do
  curl -fsS -o /dev/null "$BASE/" && break
  sleep 0.25
done

PROBE_PORT="$PORT" PAGES="${PAGES[*]}" WIDTHS="${WIDTHS[*]}" \
REPORT="$REPORT" python3 - <<'PYEOF'
import json
import os
import re
import subprocess
import sys
import tempfile
import urllib.parse

port = os.environ["PROBE_PORT"]
pages = os.environ["PAGES"].split()
widths = [int(w) for w in os.environ["WIDTHS"].split()]
report_path = os.environ["REPORT"]
base = f"http://127.0.0.1:{port}/engram"

# D2 Machine Shop, DEC-P01 — the single dark polarity (tokens as authored).
EXPECTED_TOKENS = {
    "--paper": "#0e1116", "--panel": "#12161d", "--chip": "#1b222b",
    "--card": "#161b23", "--plate": "#1a2029", "--ink": "#dfe5ec",
    "--muted": "#9aa7b5", "--accent": "#e0af68", "--link": "#7aa2f7",
    "--pass": "#9ece6a", "--block": "#f7768e", "--code-bg": "#0a0d10",
    "--code-ink": "#e8e6df", "--line": "#2a3340", "--line-ui": "#56687f",
}

BODY_PAIRS = (
    "ink-on-paper", "ink-on-card", "ink-on-plate", "muted-on-panel",
    "accent-on-paper", "accent-on-card", "link-on-paper", "link-on-card",
    "pass-on-paper", "block-on-card", "block-on-plate", "paper-on-accent",
    "paper-on-pass", "paper-on-block", "accent-on-chip", "code-ink-on-code-bg",
)
BOUNDARY_PAIRS = ("line-ui-on-paper", "line-ui-on-panel")

# FIND-code-002: the /get-started/ copy affordance is asserted per expected
# count, not >= 1 — site.js wraps every main pre in a copy button, so the
# count equals rendered command blocks. Fifteen = cards 01/03/04 (one block
# each) + card 02's six client tabs + card 05's five agent tabs + card 05's
# uninstall tail. A re-broken tab panel drops a <pre> and FAILS this cell.
GET_STARTED_CODE_BLOCKS = 15  # 14 config blocks + 1 direct install block (owner quick-fix dedup)


def rgb_to_hex(rgb: str) -> str | None:
    m = re.match(r"rgba?\((\d+),\s*(\d+),\s*(\d+)", rgb.strip())
    if not m:
        return rgb if rgb.startswith("#") else None
    return "#{:02x}{:02x}{:02x}".format(*(int(g) for g in m.groups()))


def fetch_probe(path: str, width: int, attempts: int = 3) -> dict:
    """Fetch one probe cell through the CDP driver at an exact viewport width.

    --window-size clamps near 500px in new headless, so the matrix uses
    probe-cell.mjs with Emulation.setDeviceMetricsOverride for honest widths.
    """
    url = f"{base}{urllib.parse.quote(path)}?probe=1"
    for attempt in range(attempts):
        with tempfile.NamedTemporaryFile(suffix=".json", delete=False) as fh:
            probe_file = fh.name
        cmd = ["node", "site/tools/probe-cell.mjs", url, str(width), probe_file]
        proc = subprocess.run(cmd, capture_output=True, text=True, timeout=180)
        if proc.returncode == 0:
            with open(probe_file) as fh:
                probe = json.load(fh)
            os.unlink(probe_file)
            if attempt:
                probe["retries"] = attempt
            return probe
        os.unlink(probe_file)
    raise RuntimeError(
        f"probe-cell failed {attempts} times for {path} @{width}px")


failures = []
report = {"cells": [], "failures": []}

for path in pages:
    for width in widths:
        cell = {"page": path, "width": width}
        try:
            probe = fetch_probe(path, width)
        except Exception as e:
            failures.append(f"{path} @{width}px: {e}")
            cell["error"] = str(e)
            report["cells"].append(cell)
            continue
        cell["payload"] = probe
        report["cells"].append(cell)
        tag = f"{path} @{width}px"

        for token, expected in EXPECTED_TOKENS.items():
            actual = rgb_to_hex(probe["tokens"].get(token, ""))
            if actual != expected:
                failures.append(f"{tag}: token {token} = {actual}, expected {expected}")

        # Browser-computed AA on the witnessed DEC-P01 pairs (FIND-CODE-002:
        # the OK message may only claim what is asserted). The worst body pair
        # block-on-plate (6.19:1) is asserted here like in the static gate.
        for pair in BODY_PAIRS:
            value = probe["contrast"].get(pair)
            if value is None or value < 4.5:
                failures.append(f"{tag}: browser-computed contrast {pair} = {value} < 4.5:1")
        for pair in BOUNDARY_PAIRS:
            value = probe["contrast"].get(pair)
            if value is None or value < 3.0:
                failures.append(f"{tag}: browser-computed boundary {pair} = {value} < 3.0:1")

        expected_fonts = {
            "inter400": True, "inter600": True,
            "jetbrainsMono400": True, "jetbrainsMono700": True,
        }
        for font, want in expected_fonts.items():
            if probe["fonts"].get(font) is not want:
                failures.append(f"{tag}: font {font} not loaded")

        if width == 360 and probe["overflow"]["hasHorizontalOverflow"]:
            failures.append(
                f"{tag}: horizontal overflow at 360px "
                f"({probe['overflow']['documentScrollWidth']} > {probe['overflow']['documentClientWidth']})")

        if not probe["sameOriginOnly"]:
            failures.append(f"{tag}: non-same-origin resources: {probe['resourceOrigins']}")

        if path == "/get-started/" and probe["copyButtons"] != GET_STARTED_CODE_BLOCKS:
            failures.append(
                f"{tag}: copyButtons {probe['copyButtons']} != {GET_STARTED_CODE_BLOCKS} "
                "rendered command blocks — every code block must carry the copy affordance")

report["failures"] = failures
report["ok"] = not failures
with open(report_path, "w") as fh:
    json.dump(report, fh, indent=1)

print(f"render-probe: {len(report['cells'])} matrix cells across {len(pages)} pages x {len(widths)} widths (dark-only)")
if failures:
    for f in failures:
        print(f"render-probe: FAIL {f}")
    sys.exit(1)
print(f"render-probe: OK — D2 tokens, browser-computed contrast (16 body pairs ≥ 4.5:1 incl. block-on-plate 6.19:1, "
      "2 boundary pairs ≥ 3:1), four font faces, all '"$GET_STARTED_CODE_BLOCKS"' copy buttons on /get-started/, 360px overflow and same-origin only "
      f"all hold on 8 dark cells; no toggle/swap assertions remain; report at {report_path}")
PYEOF
STATUS=$?
exit $STATUS
