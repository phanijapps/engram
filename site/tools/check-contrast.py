#!/usr/bin/env python3
"""WCAG AA contrast gate for the built site stylesheet (plan DEC-P005, NFR-001).

Parses the Direction B custom properties out of the built main.css for BOTH
polarities (:root light and [data-theme="dark"]), computes WCAG 2.1 relative
luminance contrast ratios for the DEC-P005 pair matrix, and asserts:
  - every body-size text pair is at least 4.5:1,
  - every large-text/UI-boundary pair is at least 3:1,
on both polarities. Exit 0 = both polarities pass; any failing pair exits
nonzero with the computed ratio and the threshold it missed.

Usage: python3 site/tools/check-contrast.py [site/_site/assets/css/main.css]
"""
from __future__ import annotations

import re
import sys
from pathlib import Path

DEFAULT_CSS = Path("site/_site/assets/css/main.css")
HEX_RE = re.compile(r"#([0-9a-fA-F]{6})\b")
BODY_MIN = 4.5
BOUNDARY_MIN = 3.0

# Pair matrix (DEC-P005): (token foreground, token background, class, threshold)
# body-size text uses 4.5:1; UI boundaries (borders, focus outline ink) use 3:1.
LIGHT_PAIRS = [
    ("--ink", "--paper", "body text", BODY_MIN),
    ("--muted", "--paper", "secondary text", BODY_MIN),
    ("--accent", "--paper", "accent text", BODY_MIN),
    ("--link", "--paper", "link text", BODY_MIN),
    ("--paper", "--ink", "tag/fig-id text on ink", BODY_MIN),
    ("#ffffff", "--pass", "white text on pass badge", BODY_MIN),
    ("--ink", "--card", "body text on card", BODY_MIN),
    ("--muted", "--card", "secondary text on card", BODY_MIN),
    ("--accent", "--card", "accent text on card", BODY_MIN),
    ("--link", "--card", "link text on card", BODY_MIN),
    ("--code-ink", "--code-bg", "code block text", BODY_MIN),
    ("--ink", "--paper", "interactive border ink (UI boundary)", BOUNDARY_MIN),
    ("--link", "--paper", "focus outline (UI boundary)", BOUNDARY_MIN),
]
DARK_PAIRS = [
    ("--ink", "--paper", "body text", BODY_MIN),
    ("--muted", "--paper", "secondary text", BODY_MIN),
    ("--accent", "--paper", "accent text", BODY_MIN),
    ("--link", "--paper", "link text", BODY_MIN),
    ("--pass", "--paper", "pass-tag text", BODY_MIN),
    ("--paper", "--ink", "tag/fig-id text on ink", BODY_MIN),
    ("--ink", "--card", "body text on card", BODY_MIN),
    ("--muted", "--card", "secondary text on card", BODY_MIN),
    ("--accent", "--card", "accent text on card", BODY_MIN),
    ("--link", "--card", "link text on card", BODY_MIN),
    ("--code-ink", "--code-bg", "code block text", BODY_MIN),
    ("--ink", "--paper", "interactive border ink (UI boundary)", BOUNDARY_MIN),
    ("--link", "--paper", "focus outline (UI boundary)", BOUNDARY_MIN),
]

# The light accent on dark paper is the recorded counter-example (3.70:1) —
# the gate fails if the dark polarity ever reuses a light-only token that
# drops below AA. Nothing here hardcodes values; ratios are computed.


def luminance(hexc: str) -> float:
    h = hexc.lstrip("#")
    r, g, b = (int(h[i:i + 2], 16) / 255 for i in (0, 2, 4))
    def f(c: float) -> float:
        return c / 12.92 if c <= 0.03928 else ((c + 0.055) / 1.055) ** 2.4
    return 0.2126 * f(r) + 0.7152 * f(g) + 0.0722 * f(b)


def contrast(fg: str, bg: str) -> float:
    a, b = luminance(fg), luminance(bg)
    hi, lo = max(a, b), min(a, b)
    return round((hi + 0.05) / (lo + 0.05), 2)


def parse_tokens(css: str) -> tuple[dict[str, str], dict[str, str]]:
    light_block = re.search(r":root\s*{([^}]*)}", css)
    dark_block = re.search(r'\[data-theme="dark"\]\s*{([^}]*)}', css)
    if not light_block or not dark_block:
        raise SystemExit("check-contrast: FAIL could not find :root and [data-theme=dark] token blocks in the built CSS")

    def tokens(block: str) -> dict[str, str]:
        out = {}
        for name, value in re.findall(r"(--[\w-]+)\s*:\s*([^;]+);", block):
            m = HEX_RE.search(value)
            if m:
                out[name] = "#" + m.group(1).lower()
        return out

    return tokens(light_block.group(1)), tokens(dark_block.group(1))


def resolve(spec: str, tokens: dict[str, str]) -> str:
    if spec.startswith("--"):
        if spec not in tokens:
            raise KeyError(spec)
        return tokens[spec]
    return spec.lower()


def run(pairs, tokens, label: str) -> list[str]:
    failures: list[str] = []
    print(f"check-contrast: {label} polarity")
    for fg_spec, bg_spec, name, threshold in pairs:
        fg, bg = resolve(fg_spec, tokens), resolve(bg_spec, tokens)
        ratio = contrast(fg, bg)
        status = "ok " if ratio >= threshold else "FAIL"
        print(f"  {status} {name:38s} {fg} on {bg} = {ratio}:1 (min {threshold})")
        if ratio < threshold:
            failures.append(f"{label}: {name} {fg} on {bg} = {ratio}:1 < {threshold}:1")
    return failures


def main() -> int:
    css_path = Path(sys.argv[1]) if len(sys.argv) > 1 else DEFAULT_CSS
    if not css_path.is_file():
        print(f"check-contrast: FAIL built stylesheet {css_path} not found (run site/tools/build.sh first)")
        return 1
    light, dark = parse_tokens(css_path.read_text(encoding="utf-8"))
    for required in ("--paper", "--ink", "--muted", "--accent", "--link", "--pass", "--card", "--code-bg", "--code-ink"):
        if required not in light:
            print(f"check-contrast: FAIL light polarity is missing token {required}")
            return 1
        if required not in dark:
            print(f"check-contrast: FAIL dark polarity is missing token {required}")
            return 1
    failures = run(LIGHT_PAIRS, light, "light") + run(DARK_PAIRS, dark, "dark")
    if failures:
        for f in failures:
            print(f"check-contrast: FAIL {f}")
        return 1
    print("check-contrast: OK — every pair meets AA on BOTH polarities (4.5:1 body, 3:1 boundaries)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
