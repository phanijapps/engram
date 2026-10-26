#!/usr/bin/env bash
# Render probe matrix for the built site (plan DEC-P03, TASK-016).
# For each built page x {360px, 1280px} x {light, dark}: drive the site's
# inert ?probe=1 diagnostics through google-chrome --headless=new --dump-dom,
# assert LED-1/DEC-P005 tokens, no horizontal overflow at 360px, zero
# non-same-origin resources, copy buttons on code pages, and diagram src
# switching with polarity. Writes /tmp/engram-probe-report.json.
# Run after a fresh build: bash site/tools/build.sh && bash site/tools/render-probe.sh
set -uo pipefail

PORT="${ENGRAM_PROBE_PORT:-8766}"
BASE="http://127.0.0.1:${PORT}/engram"
REPORT="/tmp/engram-probe-report.json"
CHROME=google-chrome
PAGES=("/" "/factory/" "/engram/" "/install/")
WIDTHS=(360 1280)
POLARITIES=(light dark)

command -v "$CHROME" >/dev/null 2>&1 || { echo "render-probe: google-chrome not found" >&2; exit 1; }
test -f site/_site/index.html || { echo "render-probe: site/_site missing — run site/tools/build.sh first" >&2; exit 1; }

python3 site/tools/serve.py "$PORT" site/_site >/tmp/engram-serve.log 2>&1 &
SERVE_PID=$!
trap 'kill "$SERVE_PID" 2>/dev/null || true' EXIT

for _ in $(seq 1 40); do
  curl -fsS -o /dev/null "$BASE/" && break
  sleep 0.25
done

PROBE_PORT="$PORT" PAGES="${PAGES[*]}" WIDTHS="${WIDTHS[*]}" POLARITIES="${POLARITIES[*]}" \
REPORT="$REPORT" python3 - <<'PYEOF'
import html
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
polarities = os.environ["POLARITIES"].split()
report_path = os.environ["REPORT"]
base = f"http://127.0.0.1:{port}/engram"

EXPECTED_TOKENS = {
    "light": {
        "--paper": "#f7f5f0", "--ink": "#101418", "--muted": "#3c3a34",
        "--line": "#d8d2c6", "--accent": "#b3541e", "--link": "#33517a",
        "--pass": "#5a7d5a",
    },
    "dark": {
        "--paper": "#101418", "--ink": "#e8e6df", "--muted": "#b9b5aa",
        "--accent": "#e0af68", "--link": "#7e9cc4", "--pass": "#8fb08f",
    },
}

def rgb_to_hex(rgb: str) -> str | None:
    m = re.match(r"rgba?\((\d+),\s*(\d+),\s*(\d+)", rgb.strip())
    if not m:
        return rgb if rgb.startswith("#") else None
    return "#{:02x}{:02x}{:02x}".format(*(int(g) for g in m.groups()))

def fetch_probe(path: str, width: int, polarity: str, attempts: int = 3) -> dict:
    """Fetch one probe cell through the CDP driver at an exact viewport width.

    --window-size clamps near 500px in new headless, so the matrix uses
    probe-cell.mjs with Emulation.setDeviceMetricsOverride for honest widths.
    """
    url = f"{base}{urllib.parse.quote(path)}?probe=1&polarity={polarity}"
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
        f"probe-cell failed {attempts} times for {path} @{width}px/{polarity}")

failures = []
report = {"cells": [], "failures": []}
diagram_src = {}

for path in pages:
    for width in widths:
        for polarity in polarities:
            cell = {"page": path, "width": width, "polarity": polarity}
            try:
                probe = fetch_probe(path, width, polarity)
            except Exception as e:
                failures.append(f"{path} @{width}px/{polarity}: {e}")
                cell["error"] = str(e)
                report["cells"].append(cell)
                continue
            cell["payload"] = probe
            report["cells"].append(cell)
            tag = f"{path} @{width}px/{polarity}"

            if probe["polarity"] != polarity:
                failures.append(f"{tag}: page polarity {probe['polarity']} != requested {polarity}")

            # Forced polarity must apply WITHOUT wiping the stored theme
            # (FIND-CODE-003/FIND-UX-004): the probe seeds a stored 'dark'
            # sentinel before init; the old removeItem behavior fails here.
            if probe.get("forcedStoragePreserved") is not True:
                failures.append(
                    f"{tag}: forced polarity did not preserve the stored theme "
                    f"(forcedStoragePreserved={probe.get('forcedStoragePreserved')})")

            for token, expected in EXPECTED_TOKENS[polarity].items():
                actual = rgb_to_hex(probe["tokens"].get(token, ""))
                if actual != expected:
                    failures.append(f"{tag}: token {token} = {actual}, expected {expected}")

            if width == 360 and probe["overflow"]["hasHorizontalOverflow"]:
                failures.append(
                    f"{tag}: horizontal overflow at 360px "
                    f"({probe['overflow']['documentScrollWidth']} > {probe['overflow']['documentClientWidth']})")

            if not probe["sameOriginOnly"]:
                failures.append(f"{tag}: non-same-origin resources: {probe['resourceOrigins']}")

            # Browser-computed AA on the declared component pairs (FIND-CODE-002:
            # the OK message may only claim what is asserted). line-boundary is
            # reported, not asserted — a decorative non-interactive border, not
            # an NFR-001 UI boundary.
            for pair in ("body-ink-on-paper", "muted-on-paper", "accent-on-paper",
                         "link-on-paper", "tag-paper-on-ink", "tag-acc-paper-on-accent",
                         "tag-pass-card-on-pass"):
                value = probe["contrast"].get(pair)
                if value is None or value < 4.5:
                    failures.append(f"{tag}: browser-computed contrast {pair} = {value} < 4.5:1")

            expected_fonts = {
                "sourceSerif4": True, "inter400": True, "inter600": True,
                "jetbrainsMono400": True, "jetbrainsMono700": True,
            }
            for font, want in expected_fonts.items():
                if probe["fonts"].get(font) is not want:
                    failures.append(f"{tag}: font {font} not loaded")

            if path == "/install/" and probe["copyButtons"] < 1:
                failures.append(f"{tag}: no copy buttons on a code-bearing page")

            for d in probe["diagrams"]:
                diagram_src.setdefault((path, d["light"]), set()).add(d["current"])

for (path, light_src), currents in diagram_src.items():
    if len(currents) < 2:
        failures.append(f"{path}: diagram {light_src} did not switch src between polarities (saw {currents})")

report["failures"] = failures
report["ok"] = not failures
with open(report_path, "w") as fh:
    json.dump(report, fh, indent=1)

print(f"render-probe: {len(report['cells'])} matrix cells across {len(pages)} pages x {len(widths)} widths x {len(polarities)} polarities")
if failures:
    for f in failures:
        print(f"render-probe: FAIL {f}")
    sys.exit(1)
print(f"render-probe: OK — tokens, browser-computed contrast (7 body pairs ≥ 4.5:1), forced-polarity storage preservation, 360px overflow, same-origin only, fonts, copy buttons and diagram switching all hold; report at {report_path}")
PYEOF
STATUS=$?
exit $STATUS
