#!/usr/bin/env python3
"""WCAG AA contrast gate for the built site stylesheet (job 008, DEC-P01, NFR-001).

D2 "Machine Shop" is DARK-ONLY: the parser reads the single :root token block
and FAILS if any [data-theme] selector survives in the built CSS (single
polarity is part of the design decision, not an assumption).

The pair matrix is re-derived from the plan's DEC-P01 table (values verified
by recomputation): every body pair asserts >= 4.5:1, every UI boundary >= 3:1.
The worst body pair is --block on --plate at 6.19:1 (the warn/block plate
title rendering pair the table's enumeration does not list separately — the
tasks agent identified it; it is asserted here with its own witness so the
worst-pair claim is never phantom). Decorative --line hairlines (1.3-1.5:1)
are REPORTED, never asserted (004 line-boundary precedent).

Every matrix row carries a WITNESS — the CSS rule(s) that actually render the
pair. A row whose witness is missing from the built stylesheet fails as a
phantom pair (the FIND-CODE-001 regression class from job 004: a matrix that
tests a pair the shipped CSS no longer renders).

--selftest proves the negative cases by mutating the REAL built stylesheet
against a real D2 row (.btn-primary, paper text on the amber accent):
  1. broken ratio  — --accent set to the rejected Direction B light accent
     #b3541e must FAIL the button pair with the computed 3.79:1;
  2. mutated rule  — .btn-primary text flipped to var(--ink) must FAIL the
     witness (the matrix row no longer matches the shipped rule);
  3. deleted rule  — the .btn-primary rule deleted must FAIL as a witness miss;
  4. polarity guard — a re-added [data-theme="light"] token block must FAIL
     the single-polarity assertion.
The selftest exits 0 only when all four defective fixtures are rejected.

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
DATA_THEME_RE = re.compile(r'\[data-theme[^\]]*\]\s*{')
BODY_MIN = 4.5
BOUNDARY_MIN = 3.0

# DEC-P01 pair matrix: (fg token, bg token, label, threshold, witnesses).
# A witness is (selector regex, required declaration substring); the selector
# regex is matched as `<sel>{ ... }` against the compressed built CSS and the
# declaration must appear inside that rule. Inherited colors are witnessed at
# the rule that sets them (body color covers card/plate text; the a rule
# covers .card h3 a via inheritance — both carry their own surface witness).
PAIRS = [
    ("--ink", "--paper", "body text", BODY_MIN, [
        ("body", "color:var(--ink)"),
        ("body", "background:var(--paper)"),
    ]),
    ("--ink", "--card", "body text on card", BODY_MIN, [
        (r"\.card", "background:var(--card)"),
        ("body", "color:var(--ink)"),
    ]),
    ("--ink", "--plate", "body text on callout plate", BODY_MIN, [
        (r"\.plate", "background:var(--plate)"),
        ("body", "color:var(--ink)"),
    ]),
    ("--muted", "--panel", "secondary text on panel (figcaption)", BODY_MIN, [
        (r"figure\.diagram figcaption", "color:var(--muted)"),
        (r"figure\.diagram", "background:var(--panel)"),
    ]),
    ("--accent", "--paper", "accent text (kicker)", BODY_MIN, [
        (r"\.kicker", "color:var(--accent)"),
        ("body", "background:var(--paper)"),
    ]),
    ("--accent", "--card", "accent text on card (card kicker)", BODY_MIN, [
        (r"\.kicker", "color:var(--accent)"),
        (r"\.card", "background:var(--card)"),
    ]),
    ("--link", "--paper", "link text", BODY_MIN, [
        ("a", "color:var(--link)"),
        ("body", "background:var(--paper)"),
    ]),
    ("--link", "--card", "link text on card (card titles)", BODY_MIN, [
        (r"\.card h3 a", "color:var(--link)"),
        (r"\.card", "background:var(--card)"),
    ]),
    ("--pass", "--paper", "ok mark on paper", BODY_MIN, [
        (r"\.mark-ok", "color:var(--pass)"),
        ("body", "background:var(--paper)"),
    ]),
    ("--block", "--card", "limit mark on card", BODY_MIN, [
        (r"\.mark-block", "color:var(--block)"),
        (r"\.card", "background:var(--card)"),
    ]),
    ("--paper", "--accent", "primary button text on accent", BODY_MIN, [
        (r"\.btn-primary", "color:var(--paper)"),
        (r"\.btn-primary", "background:var(--accent)"),
    ]),
    ("--paper", "--pass", "chip-pass text on pass", BODY_MIN, [
        (r"\.chip-pass", "color:var(--paper)"),
        (r"\.chip-pass", "background:var(--pass)"),
    ]),
    ("--paper", "--block", "chip-block text on block", BODY_MIN, [
        (r"\.chip-block", "color:var(--paper)"),
        (r"\.chip-block", "background:var(--block)"),
    ]),
    ("--accent", "--chip", "active chip / fig-id on chip", BODY_MIN, [
        (r"\.nav-links a\[aria-current=\"page\"\]", "color:var(--accent)"),
        (r"\.nav-links a\[aria-current=\"page\"\]", "background:var(--chip)"),
        (r"figure\.diagram \.fig-id", "color:var(--accent)"),
        (r"figure\.diagram \.fig-id", "background:var(--chip)"),
    ]),
    ("--code-ink", "--code-bg", "code block text", BODY_MIN, [
        ("pre", "color:var(--code-ink)"),
        ("pre", "background:var(--code-bg)"),
    ]),
    ("--block", "--plate", "block plate title on plate (worst body pair)", BODY_MIN, [
        (r"\.plate-block \.plate-title", "color:var(--block)"),
        (r"\.plate", "background:var(--plate)"),
    ]),
    ("--line-ui", "--paper", "ghost button border (UI boundary)", BOUNDARY_MIN, [
        (r"\.btn-ghost", "border:1.5px solid var(--line-ui)"),
        (r"\.btn-ghost", "color:var(--ink)"),
    ]),
    ("--line-ui", "--panel", "nav chip border on panel (UI boundary)", BOUNDARY_MIN, [
        (r"\.nav-links a", "border:1px solid var(--line-ui)"),
        (r"\.nav-links a", "background:var(--panel)"),
    ]),
    ("--link", "--paper", "focus outline (UI boundary)", BOUNDARY_MIN, [
        (":focus-visible", "outline:2px solid var(--link)"),
        ("body", "background:var(--paper)"),
    ]),
]

REQUIRED_TOKENS = ("--paper", "--panel", "--chip", "--card", "--plate", "--ink",
                   "--muted", "--accent", "--link", "--pass", "--block",
                   "--code-bg", "--code-ink", "--line", "--line-ui")

# Decorative hairlines: computed and reported, never asserted (they frame
# non-interactive surfaces; the 004 precedent).
REPORTED_ONLY = [("--line", "--paper"), ("--line", "--panel"), ("--line", "--card")]


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


def parse_tokens(css: str) -> dict[str, str]:
    """D2 is single-polarity: :root is the only token block, and any surviving
    [data-theme] block is a design-contract violation, not a second polarity."""
    if DATA_THEME_RE.search(css):
        raise SystemExit("check-contrast: FAIL a [data-theme] token block survives in the built CSS — "
                         "D2 is dark-only (DEC-P01); re-derivation to two polarities is out of contract")
    root_block = re.search(r":root\s*{([^}]*)}", css)
    if not root_block:
        raise SystemExit("check-contrast: FAIL could not find the :root token block in the built CSS")
    out: dict[str, str] = {}
    for name, value in re.findall(r"(--[\w-]+)\s*:\s*([^;]+);", root_block.group(1)):
        m = HEX_RE.search(value)
        if m:
            out[name] = "#" + m.group(1).lower()
    return out


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
    if DATA_THEME_RE.search(css):
        return ([f"{label}: a [data-theme] token block survives — D2 is dark-only"], lines)
    tokens = parse_tokens(css)
    for required in REQUIRED_TOKENS:
        if required not in tokens:
            return ([f"{label}: the :root block is missing token {required}"], lines)

    for fg_spec, bg_spec, name, threshold, witnesses in PAIRS:
        for selector, decl in witnesses:
            block = rule_block(css, selector)
            if block is None or decl not in block:
                failures.append(
                    f"{label}: {name} — phantom pair: no rule `{selector}{{{decl}…}}` in the CSS "
                    f"(matrix rows must match rules that exist; FIND-CODE-001)")
        fg, bg = resolve(fg_spec, tokens), resolve(bg_spec, tokens)
        ratio = contrast(fg, bg)
        status = "ok " if ratio >= threshold else "FAIL"
        lines.append(f"  {status} {name:42s} {fg} on {bg} = {ratio}:1 (min {threshold})")
        if ratio < threshold:
            failures.append(f"{label}: {name} {fg} on {bg} = {ratio}:1 < {threshold}:1")
    for fg_spec, bg_spec in REPORTED_ONLY:
        fg, bg = resolve(fg_spec, tokens), resolve(bg_spec, tokens)
        lines.append(f"  n/a decorative hairline {fg_spec} on {bg_spec} = {contrast(fg, bg)}:1 (reported, not asserted)")
    return failures, lines


def run(path: Path) -> int:
    failures, lines = evaluate(path.read_text(encoding="utf-8"), str(path))
    for line in lines:
        print(line)
    for f in failures:
        print(f"check-contrast: FAIL {f}")
    if failures:
        return 1
    print("check-contrast: OK — every witnessed D2 pair meets AA on the single dark polarity "
          "(4.5:1 body incl. the 6.19:1 block-on-plate worst pair, 3:1 boundaries); "
          "every matrix row matches a rule in the CSS; decorative --line hairlines reported only")
    return 0


def selftest(css_path: Path) -> int:
    """Prove the negative cases on the real built stylesheet (FIND-CODE-001)."""
    if not css_path.is_file():
        print(f"check-contrast selftest: FAIL built stylesheet {css_path} not found (run site/tools/build.sh first)")
        return 1
    css = css_path.read_text(encoding="utf-8")
    m = re.search(r"\.btn-primary\{[^}]*\}", css)
    if not m or "var(--paper)" not in m.group(0) or "var(--accent)" not in m.group(0):
        print("check-contrast selftest: FAIL could not locate the .btn-primary{color:var(--paper);background:var(--accent)} rule")
        return 1
    btn_primary = m.group(0)
    root = re.search(r":root\s*{([^}]*)}", css)
    if not root or "--accent: #e0af68" not in root.group(1):
        print("check-contrast selftest: FAIL could not locate the --accent: #e0af68 token")
        return 1

    tmp = Path(tempfile.mkdtemp(prefix="engram-check-contrast-selftest-"))
    try:
        # Fixture 1 — broken ratio: --accent set to the rejected Direction B
        # light accent; paper button text drops to 3.79:1.
        broken_ratio = css.replace("--accent: #e0af68", "--accent: #b3541e", 1)
        # Fixture 2 — mutated rule: .btn-primary text flipped to var(--ink).
        mutated_rule = css.replace(btn_primary, btn_primary.replace("var(--paper)", "var(--ink)"))
        # Fixture 3 — phantom rule: the .btn-primary rule deleted entirely.
        phantom_rule = css.replace(btn_primary, "")
        # Fixture 4 — polarity guard: a re-added [data-theme="light"] block.
        polarity_lapse = css.replace(
            "body{", '[data-theme="light"]{--paper:#f7f5f0;--ink:#101418}\nbody{', 1)

        cases = [
            ("broken ratio (--accent #b3541e)", broken_ratio, ["3.79"]),
            ("mutated .btn-primary rule", mutated_rule, ["phantom"]),
            ("deleted .btn-primary rule", phantom_rule, ["phantom"]),
            ("re-added [data-theme] block", polarity_lapse, ["dark-only"]),
        ]
        for description, fixture, expects in cases:
            path = tmp / (description.split()[0] + ".css")
            path.write_text(fixture, encoding="utf-8")
            try:
                failures, _ = evaluate(fixture, description)
            except SystemExit as e:
                failures = [str(e)]
            joined = " | ".join(failures)
            if not failures:
                print(f"check-contrast selftest: FAIL — {description}: checker passed a defective fixture")
                return 1
            for expect in expects:
                if expect not in joined:
                    print(f"check-contrast selftest: FAIL — {description}: expected `{expect}` in the failure, got: {joined}")
                    return 1
        print("check-contrast selftest: OK — rejected Direction B accent #b3541e fails the button pair at 3.79:1, "
              "mutated and deleted .btn-primary rules both rejected as phantom pairs, "
              "re-added [data-theme] block rejected by the dark-only polarity guard")
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
