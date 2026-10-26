#!/usr/bin/env python3
"""WCAG AA contrast gate for the built site stylesheet (plan DEC-P005, NFR-001).

Parses the Direction B custom properties out of the built main.css for BOTH
polarities (:root light and [data-theme="dark"]), computes WCAG 2.1 relative
luminance contrast ratios for the component pair matrix, and asserts:
  - every body-size text pair is at least 4.5:1,
  - every large-text/UI-boundary pair is at least 3:1,
on both polarities. Exit 0 = both polarities pass; any failing pair exits
nonzero with the computed ratio and the threshold it missed.

Every matrix row carries a WITNESS — the CSS rule(s) that actually render the
pair. A row whose witness is missing from the built stylesheet fails as a
phantom pair. That is the FIND-CODE-001 regression class: the round-0 matrix
tested a "#ffffff on --pass" pair after TASK-015 had moved .tag text to
var(--paper), so the shipped 4.27:1 light .tag.pass pair was never checked
while a dead pair passed green. Rows now name the rules they come from
(.tag, .tag.acc, .tag.pass, .fig-id, .kicker, pre, a, body, .card,
.site-header border, :focus-visible outline).

--selftest proves the negative cases by mutating the REAL built stylesheet:
  1. broken ratio  — the light --pass token lightened to the dark value
     (#8fb08f) must FAIL the .tag.pass pair with the computed 2.39:1;
  2. mutated rule — .tag.pass text flipped back to var(--paper) must FAIL
     the witness (the matrix row no longer matches the shipped rule);
  3. phantom rule — the .tag.pass rule deleted must FAIL as a witness miss.
The selftest exits 0 only when all three defective fixtures are rejected.

Usage:
  python3 site/tools/check-contrast.py [site/_site/assets/css/main.css]
  python3 site/tools/check-contrast.py --selftest [site/_site/assets/css/main.css]
"""
from __future__ import annotations

import re
import shutil
import sys
import tempfile
from pathlib import Path

DEFAULT_CSS = Path("site/_site/assets/css/main.css")
HEX_RE = re.compile(r"#([0-9a-fA-F]{6})\b")
BODY_MIN = 4.5
BOUNDARY_MIN = 3.0

# Component pair matrix (DEC-P005, one set for both polarities — tokens
# resolve per polarity, the rules are shared):
# (fg token, bg token, label, threshold, witnesses)
# A witness is (selector regex, required declaration substring); the selector
# regex is matched as `<sel>{ ... }` against the compressed built CSS and the
# declaration must appear inside that rule. Inherited colors are witnessed at
# the rule that sets them (e.g. .tag.acc inherits .tag's color).
PAIRS = [
    ("--ink", "--paper", "body text", BODY_MIN, [
        ("body", "color:var(--ink)"),
        ("body", "background:var(--paper)"),
    ]),
    ("--muted", "--paper", "secondary text (hero lead)", BODY_MIN, [
        (r"\.hero \.lead", "color:var(--muted)"),
    ]),
    ("--accent", "--paper", "accent text (kicker)", BODY_MIN, [
        (r"\.kicker", "color:var(--accent)"),
    ]),
    ("--link", "--paper", "link text", BODY_MIN, [
        ("a", "color:var(--link)"),
    ]),
    ("--paper", "--ink", ".tag text on ink", BODY_MIN, [
        (r"\.tag", "color:var(--paper)"),
        (r"\.tag", "background:var(--ink)"),
    ]),
    ("--paper", "--accent", ".tag.acc text", BODY_MIN, [
        (r"\.tag", "color:var(--paper)"),
        (r"\.tag\.acc", "background:var(--accent)"),
    ]),
    ("--card", "--pass", ".tag.pass text", BODY_MIN, [
        (r"\.tag\.pass", "color:var(--card)"),
        (r"\.tag\.pass", "background:var(--pass)"),
    ]),
    ("--paper", "--ink", ".fig-id text on ink", BODY_MIN, [
        (r"figure\.diagram \.fig-id", "color:var(--paper)"),
        (r"figure\.diagram \.fig-id", "background:var(--ink)"),
    ]),
    ("--ink", "--card", "body text on card", BODY_MIN, [
        (r"\.card", "background:var(--card)"),
    ]),
    ("--accent", "--card", "accent text on card", BODY_MIN, [
        (r"\.kicker", "color:var(--accent)"),
        (r"\.card", "background:var(--card)"),
    ]),
    ("--link", "--card", "link text on card", BODY_MIN, [
        ("a", "color:var(--link)"),
        (r"\.card", "background:var(--card)"),
    ]),
    ("--code-ink", "--code-bg", "code block text", BODY_MIN, [
        ("pre", "color:var(--code-ink)"),
        ("pre", "background:var(--code-bg)"),
    ]),
    ("--ink", "--paper", "header border (UI boundary)", BOUNDARY_MIN, [
        (r"\.site-header", "background:var(--paper)"),
        (r"\.site-header", "border-bottom:2px solid var(--ink)"),
    ]),
    ("--link", "--paper", "focus outline (UI boundary)", BOUNDARY_MIN, [
        (":focus-visible", "outline:2px solid var(--link)"),
    ]),
]

REQUIRED_TOKENS = ("--paper", "--ink", "--muted", "--accent", "--link", "--pass",
                   "--card", "--code-bg", "--code-ink")


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


def rule_block(css: str, selector: str) -> str | None:
    """Return the declaration block of the first `<selector>{ ... }` rule."""
    m = re.search(r"(?:^|})" + selector + r"\{([^}]*)\}", css)
    return m.group(1) if m else None


def resolve(spec: str, tokens: dict[str, str]) -> str:
    if spec.startswith("--"):
        if spec not in tokens:
            raise KeyError(spec)
        return tokens[spec]
    return spec.lower()


def evaluate(css: str, label: str) -> tuple[list[str], list[str]]:
    """Run witness + ratio checks over one stylesheet. Returns (failures, lines)."""
    lines: list[str] = [f"check-contrast: {label}"]
    failures: list[str] = []
    light, dark = parse_tokens(css)
    for required in REQUIRED_TOKENS:
        if required not in light:
            return ([f"{label}: light polarity is missing token {required}"], lines)
        if required not in dark:
            return ([f"{label}: dark polarity is missing token {required}"], lines)

    for fg_spec, bg_spec, name, threshold, witnesses in PAIRS:
        for selector, decl in witnesses:
            block = rule_block(css, selector)
            if block is None or decl not in block:
                failures.append(
                    f"{label}: {name} — phantom pair: no rule `{selector}{{{decl}…}}` in the CSS "
                    f"(matrix rows must match rules that exist; FIND-CODE-001)")
        for polarity, tokens in (("light", light), ("dark", dark)):
            fg, bg = resolve(fg_spec, tokens), resolve(bg_spec, tokens)
            ratio = contrast(fg, bg)
            status = "ok " if ratio >= threshold else "FAIL"
            lines.append(f"  {status} {polarity} {name:30s} {fg} on {bg} = {ratio}:1 (min {threshold})")
            if ratio < threshold:
                failures.append(f"{label}: {polarity} {name} {fg} on {bg} = {ratio}:1 < {threshold}:1")
    return failures, lines


def run(path: Path) -> int:
    failures, lines = evaluate(path.read_text(encoding="utf-8"), str(path))
    for line in lines:
        print(line)
    for f in failures:
        print(f"check-contrast: FAIL {f}")
    if failures:
        return 1
    print("check-contrast: OK — every witnessed pair meets AA on BOTH polarities "
          "(4.5:1 body, 3:1 boundaries); every matrix row matches a rule in the CSS")
    return 0


def selftest(css_path: Path) -> int:
    """Prove the negative cases on the real built stylesheet (FIND-CODE-001)."""
    if not css_path.is_file():
        print(f"check-contrast selftest: FAIL built stylesheet {css_path} not found (run site/tools/build.sh first)")
        return 1
    css = css_path.read_text(encoding="utf-8")
    m = re.search(r"\.tag\.pass\{[^}]*\}", css)
    if not m or "var(--card)" not in m.group(0) or "var(--pass)" not in m.group(0):
        print("check-contrast selftest: FAIL could not locate the .tag.pass{color:var(--card);background:var(--pass)} rule")
        return 1
    tag_pass = m.group(0)
    root = re.search(r":root\s*{([^}]*)}", css)
    if not root or "--pass: #5a7d5a" not in root.group(1):
        print("check-contrast selftest: FAIL could not locate the light --pass: #5a7d5a token")
        return 1

    tmp = Path(tempfile.mkdtemp(prefix="engram-check-contrast-selftest-"))
    try:
        # Fixture 1 — broken ratio: light --pass lightened to the dark value; white text drops to 2.39:1.
        broken_ratio = css.replace("--pass: #5a7d5a", "--pass: #8fb08f", 1)
        # Fixture 2 — mutated rule: .tag.pass text back to --paper (the 4.27:1 class the round-0 gate missed).
        mutated_rule = css.replace(tag_pass, tag_pass.replace("var(--card)", "var(--paper)"))
        # Fixture 3 — phantom rule: the .tag.pass rule deleted entirely.
        phantom_rule = css.replace(tag_pass, "")

        cases = [
            ("broken ratio (light --pass lightened)", broken_ratio, [".tag.pass", "2.39"]),
            ("mutated .tag.pass rule", mutated_rule, ["phantom"]),
            ("deleted .tag.pass rule", phantom_rule, ["phantom"]),
        ]
        for description, fixture, expects in cases:
            path = tmp / (description.split()[0] + ".css")
            path.write_text(fixture, encoding="utf-8")
            failures, _ = evaluate(fixture, description)
            joined = " | ".join(failures)
            if not failures:
                print(f"check-contrast selftest: FAIL — {description}: checker passed a defective fixture")
                return 1
            for expect in expects:
                if expect not in joined:
                    print(f"check-contrast selftest: FAIL — {description}: expected `{expect}` in the failure, got: {joined}")
                    return 1
        print("check-contrast selftest: OK — lightened --pass rejected with .tag.pass at 2.39:1, "
              "mutated and deleted .tag.pass rules both rejected as phantom pairs")
        return 0
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


def main() -> int:
    args = [a for a in sys.argv[1:]]
    self_test = "--selftest" in args
    if self_test:
        args.remove("--selftest")
    css_path = Path(args[0]) if args else DEFAULT_CSS
    if not css_path.is_file():
        print(f"check-contrast: FAIL built stylesheet {css_path} not found (run site/tools/build.sh first)")
        return 1
    return selftest(css_path) if self_test else run(css_path)


if __name__ == "__main__":
    sys.exit(main())
