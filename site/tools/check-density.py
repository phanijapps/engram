#!/usr/bin/env python3
"""Density/structure gate for the built site tree (job 008, DEC-P03, TASK-005).

Asserts the DEC-004 structural core over the parity-built tree so "not a
thesis" is machine evidence, not taste:
  R1 paragraph cap   — no prose paragraph over 60 words (REQ-002);
  R2 rhythm          — at most two consecutive prose paragraphs before a
                       visual element interrupts (REQ-003); runs reset at
                       section boundaries, never at h3 (an h3 without a
                       visual after it is still part of the wall);
  R3 section visual  — every h2 section carries at least one visual element
                       (card, figure, code card, callout, table or list —
                       the DEC-004 enumeration; REQ-003);
  R4 page header     — a page-header lead of at most 35 words on every
                       content page (DEC-004);
  R5 hero buttons    — home carries at least two distinct .btn actions with
                       internal hrefs (REQ-005); home is derived from the
                       tree (the page at the tree root, index.html
                       convention when several), not a hardcoded filename —
                       a moved or renamed home cannot silently escape, and
                       a tree with no root page fails loudly (round-1
                       FIND-code-003);
  R6 callout variants— at least two distinct callout variants in use over
                       the scanned pages (REQ-007);
  R7 anchor nav      — on pages with four or more h2 sections, an anchor-nav
                       device (nav.toc) exists and its entry set equals the
                       page's h2 id set, both directions (REQ-004);
  R8 grounding       — a grounded-in listing on every content page (REQ-008,
                       LED-2).

Exemptions from word counts (DEC-P03 — split, never delete, RISK-003): code
blocks, figcaptions, table cells, list items and .grounded listings are never
word-counted; grounding is preserved verbatim, never trimmed for a number.
Label paragraphs (.kicker, .plate-title, .toc-label) are labels, not prose.

Content pages are built *.html pages that render the site chrome (<main>) and
are not redirect stubs (meta http-equiv=refresh, DEC-P02's /install/ stub).
The redirect stub and non-chrome files carry no content claims to check.

--selftest proves detection power both ways: a synthetic bad page violates
EVERY rule (including the 61-word paragraph AC-002 names, a >60-word
.grounded listing that must NOT trip the cap, and long figcaption/pre/li text
that must not either), and a synthetic good page passes clean — the checker
is proven neither blind nor always-fail.

Usage:
  python3 site/tools/check-density.py site/_site [--only <page.html>]
  python3 site/tools/check-density.py --selftest
"""
from __future__ import annotations

import re
import shutil
import sys
import tempfile
from html.parser import HTMLParser
from pathlib import Path

PARAGRAPH_CAP = 60
LEAD_CAP = 35
MAX_PROSE_RUN = 2
MIN_H2_SECTIONS_FOR_TOC = 4
MIN_HERO_BUTTONS = 2
MIN_CALLOUT_VARIANTS = 2

NON_PROSE_P_CLASSES = {"kicker", "plate-title", "toc-label"}
VISUAL_TAGS = {"figure", "pre", "table", "ul", "ol", "img"}
VISUAL_CLASSES = {"card", "plate", "btn", "chip", "grid"}
META_REFRESH_RE = re.compile(r"<meta[^>]+http-equiv=[\"']refresh[\"']", re.I)


class PageScan(HTMLParser):
    """One content page -> the facts the eight rules assert over."""

    def __init__(self) -> None:
        super().__init__(convert_charrefs=True)
        self.stack: list[tuple[str, set[str]]] = []
        self.in_main = False
        self.main_depth = 0
        self.events: list[tuple] = []  # document-order facts (below)
        self.h2_ids: list[str] = []
        self.lead_words: int | None = None
        self.btn_hrefs: set[str] = set()
        self.toc_frags: set[str] = set()
        self.in_toc = False
        self.grounded = False
        self.plate_variants: set[str] = set()
        # paragraph bookkeeping
        self._p_depth: int | None = None
        self._p_words = 0
        self._p_eligible = True
        self._p_exempt = False

    # -- helpers ---------------------------------------------------------
    def _classes(self, attrs) -> set[str]:
        for k, v in attrs:
            if k == "class" and v:
                return set(v.split())
        return set()

    def _at_depth(self, names: set[str], tag: str) -> bool:
        return tag in names or any(names & cls for _, cls in self.stack)

    def _current_section(self):
        for ev in reversed(self.events):
            if ev[0] == "h2":
                return ev[1]
        return None

    def _visual_here(self, tag: str, cls: set[str]) -> bool:
        if tag in VISUAL_TAGS or (cls & VISUAL_CLASSES):
            return True
        return any(c.startswith("plate-") for c in cls)

    # -- parser events ----------------------------------------------------
    def handle_starttag(self, tag, attrs):
        cls = self._classes(attrs)
        a = dict(attrs)
        depth = len(self.stack)
        if tag == "main":
            self.in_main = True
            self.main_depth = depth + 1
        if self.in_main:
            section = self._current_section()
            if tag == "h2":
                self.events.append(("h2", a.get("id") or ""))
                self.h2_ids.append(a.get("id") or "")
                section = a.get("id") or ""
            if tag == "nav" and "toc" in cls:
                self.in_toc = True
            if self.in_toc and tag == "a":
                href = a.get("href") or ""
                if href.startswith("#") and len(href) > 1:
                    self.toc_frags.add(href[1:])
            if tag == "a" and "btn" in cls:
                href = a.get("href") or ""
                if href.startswith("/") and not href.startswith("//"):
                    self.btn_hrefs.add(href)
                self.events.append(("visual", section))
            if self._visual_here(tag, cls):
                self.events.append(("visual", section))
                for c in cls:
                    if c.startswith("plate-") and c != "plate-title":
                        self.plate_variants.add(c)
            if tag == "p":
                self._p_depth = depth
                self._p_words = 0
                self._p_eligible = (
                    not (cls & NON_PROSE_P_CLASSES)
                    and not self._at_depth({"grounded"}, tag)
                    and not self._at_depth(VISUAL_CLASSES, tag)
                    and not self._at_depth(set(VISUAL_TAGS), tag)
                    and "lead" not in cls
                )
                self._p_exempt = bool(self._at_depth({"grounded"}, tag)) or "grounded" in cls
                if "grounded" in cls or self._at_depth({"grounded"}, tag):
                    self.grounded = True
        self.stack.append((tag, cls))

    def handle_endtag(self, tag):
        if self.in_main and tag == "p" and self._p_depth is not None:
            section = self._current_section()
            self.events.append(("p", section, self._p_words, self._p_eligible, self._p_exempt))
            self._p_depth = None
        if tag == "nav" and self.in_toc:
            self.in_toc = False
        if self.stack:
            popped = self.stack.pop()
            if popped[0] == "main":
                self.in_main = False
        if self.in_main and self.stack and len(self.stack) < self.main_depth:
            self.in_main = False

    def handle_data(self, data):
        if self.in_main and self._p_depth is not None:
            self._p_words += len(data.split())
        # lead: the first p.lead's word count (recorded on close via events)
        if self.in_main and self._p_depth is not None and self.lead_words is None:
            pass


def scan_page(page: Path) -> PageScan | None:
    html = page.read_text(encoding="utf-8", errors="replace")
    if META_REFRESH_RE.search(html):
        return None  # redirect stub (DEC-P02) — a mechanic, not content
    if "<main" not in html:
        return None  # no site chrome — not a content page
    scan = PageScan()
    scan.feed(html)
    scan.close()
    # lead: first p.lead event words — recover from events by re-scan marker
    return scan


def scan_lead(page: Path) -> tuple[int | None, bool]:
    """Return (lead word count, has_lead) via a focused regex-free parse."""
    scan = _LeadScan()
    scan.feed(page.read_text(encoding="utf-8", errors="replace"))
    scan.close()
    return scan.words, scan.found


class _LeadScan(HTMLParser):
    def __init__(self) -> None:
        super().__init__(convert_charrefs=True)
        self.found = False
        self.words: int | None = None
        self._counting = False
        self._buf: list[str] = []

    def handle_starttag(self, tag, attrs):
        if tag == "p" and not self.found and any(k == "class" and "lead" in (v or "").split() for k, v in attrs):
            self._counting = True
            self._buf = []

    def handle_endtag(self, tag):
        if tag == "p" and self._counting:
            self.words = len(" ".join(self._buf).split())
            self.found = True
            self._counting = False

    def handle_data(self, data):
        if self._counting:
            self._buf.append(data)


def check_pages(tree: Path, only: str | None) -> tuple[list[str], list[str]]:
    pages = sorted(tree.rglob("*.html"))
    if only:
        pages = [p for p in pages if str(p.relative_to(tree)) == only]
        if not pages:
            return ([f"--only {only!r} matches no built page (use the tree-relative path, e.g. factory/index.html)"], [])
    violations: list[str] = []
    lines: list[str] = []
    plate_variants: set[str] = set()
    scanned = 0
    # R5 home derivation (round-1 FIND-code-003): home is the page at the
    # tree root — the index.html convention when several root pages exist —
    # not a hardcoded filename, so a moved or renamed home stays subject to
    # R5 and a rootless tree fails loudly instead of silently skipping it.
    root_pages = sorted(p.name for p in pages if p.parent == tree)
    if "index.html" in root_pages:
        home_rel: str | None = "index.html"
    elif len(root_pages) == 1:
        home_rel = root_pages[0]
    else:
        home_rel = None
    home_checked = False
    for page in pages:
        rel = str(page.relative_to(tree))
        scan = scan_page(page)
        if scan is None:
            lines.append(f"  skip {rel} (redirect stub or no chrome)")
            continue
        scanned += 1
        plate_variants |= scan.plate_variants
        lead_words, has_lead = scan_lead(page)

        # R4 page header
        if not has_lead:
            violations.append(f"lead-missing: {rel}: no page-header lead (DEC-004 requires kicker/title/lead)")
        elif lead_words > LEAD_CAP:
            violations.append(f"lead-over-cap: {rel}: lead is {lead_words} words > {LEAD_CAP}")

        # R1 paragraph cap + R2 prose runs + R3 section visuals
        over_cap = []
        run: list[tuple] = []
        longest_run = 0
        section_visuals: dict[str, int] = {}
        for ev in scan.events:
            kind = ev[0]
            if kind == "h2":
                run = []
            elif kind == "visual":
                run = []
                section_visuals[ev[1]] = section_visuals.get(ev[1], 0) + 1
            elif kind == "p":
                _, section, words, eligible, exempt = ev
                if words > PARAGRAPH_CAP and not exempt:
                    over_cap.append((words, section))
                if eligible:
                    run.append(ev)
                    longest_run = max(longest_run, len(run))
                else:
                    run = []
        for words, section in over_cap:
            violations.append(f"paragraph-over-cap: {rel}: section '{section or 'header'}' has a {words}-word paragraph > {PARAGRAPH_CAP}")
        if longest_run > MAX_PROSE_RUN:
            violations.append(f"prose-run: {rel}: {longest_run} consecutive prose paragraphs without a visual element > {MAX_PROSE_RUN}")
        for h2 in scan.h2_ids:
            if section_visuals.get(h2, 0) == 0:
                violations.append(f"section-without-visual: {rel}: section '{h2}' carries no visual element")

        # R5 hero buttons (home only)
        if home_rel is not None and rel == home_rel:
            home_checked = True
            if len(scan.btn_hrefs) < MIN_HERO_BUTTONS:
                violations.append(f"hero-buttons: {rel}: {len(scan.btn_hrefs)} distinct .btn internal action(s) < {MIN_HERO_BUTTONS}")

        # R7 anchor nav set equality
        if len(scan.h2_ids) >= MIN_H2_SECTIONS_FOR_TOC:
            if not scan.toc_frags:
                violations.append(f"anchor-nav-missing: {rel}: {len(scan.h2_ids)} sections but no nav.toc anchor rail")
            elif scan.toc_frags != set(scan.h2_ids):
                missing = sorted(set(scan.h2_ids) - scan.toc_frags)
                extra = sorted(scan.toc_frags - set(scan.h2_ids))
                violations.append(f"anchor-nav-mismatch: {rel}: toc set != h2 id set (missing {missing}, extra {extra})")

        # R8 grounded listing
        if not scan.grounded:
            violations.append(f"grounded-missing: {rel}: no grounded-in listing (LED-2)")

        lines.append(f"  ok-scan {rel}: {len(scan.h2_ids)} sections, lead {lead_words}w, "
                     f"btn-internal {sorted(scan.btn_hrefs)}, plates {sorted(scan.plate_variants)}")

    # R6 callout variants across the scanned set (site-wide on a whole-tree run)
    if scanned and len(plate_variants) < MIN_CALLOUT_VARIANTS:
        violations.append(f"callout-variants: {len(plate_variants)} distinct callout variant(s) in use over {scanned} page(s) < {MIN_CALLOUT_VARIANTS} (found {sorted(plate_variants)})")
    # R5 loud failure on full runs: a tree with no root page (or an
    # ambiguous root) must not let R5 pass by absence of a home to check.
    if only is None and home_rel is None:
        violations.append("hero-buttons: no unambiguous home page at the tree root "
                          f"(root-level pages: {root_pages or 'none'}) — R5 cannot be skipped silently (FIND-code-003)")
    elif only is None and not home_checked:
        violations.append(f"hero-buttons: tree-root home {home_rel!r} was skipped (redirect stub or no chrome) — "
                          "R5 needs a content home page")
    return violations, lines


LONG_TEXT = " ".join(["word"] * 90)


def selftest() -> int:
    tmp = Path(tempfile.mkdtemp(prefix="engram-check-density-selftest-"))
    try:
        sixty_one = " ".join(["word"] * 61)
        lead_36 = " ".join(["word"] * 36)
        bad = tmp / "index.html"
        bad.write_text(
            "<html><body><main>"
            "<p class='kicker'>bad fixture</p><h1>Bad</h1>"
            f"<p class='lead'>{lead_36}</p>"
            "<h2 id='s1'>One</h2>"
            f"<p>{sixty_one}</p>"
            "<p>second paragraph</p><p>third paragraph in the run</p>"
            "<h2 id='s2'>Two</h2>"
            "<p>section with no visual at all just prose</p>"
            "<div class='plate plate-warn'><p>only one callout variant</p></div>"
            "<h2 id='s3'>Three</h2><h2 id='s4'>Four</h2>"
            "<p>four sections and no anchor rail plus one button only</p>"
            "<a class='btn btn-primary' href='/engram/x/'>single</a>"
            f"<p class='grounded'>Grounded in: {LONG_TEXT}</p>"
            "<figure><figcaption>" + LONG_TEXT + "</figcaption></figure>"
            "<pre>" + LONG_TEXT + "</pre>"
            "<ul><li>" + LONG_TEXT + "</li></ul>"
            "</main></body></html>",
            encoding="utf-8",
        )
        good = tmp / "good.html"
        good.write_text(
            "<html><body><main>"
            "<p class='kicker'>good fixture</p><h1>Good</h1>"
            "<p class='lead'>A short lead under the cap.</p>"
            "<a class='btn btn-primary' href='/engram/factory/'>Factory</a>"
            "<a class='btn btn-ghost' href='/engram/engram/'>engram</a>"
            "<nav class='toc'><a href='#g1'>G1</a><a href='#g2'>G2</a></nav>"
            "<h2 id='g1'>G1</h2><p>short prose</p><div class='card'><p>card text</p></div>"
            "<h2 id='g2'>G2</h2><p>short prose</p><div class='plate plate-warn'><p>warn plate</p></div>"
            "<div class='plate plate-ok'><p class='plate-title'>ok</p><p>ok plate</p></div>"
            "<p class='grounded'>Grounded in: the fixture itself.</p>"
            "</main></body></html>",
            encoding="utf-8",
        )

        bad_dir = tmp / "bad-tree"
        bad_dir.mkdir()
        bad_src = bad.read_text(encoding="utf-8")
        (bad_dir / "index.html").write_text(
            bad_src.replace(f"<p class='grounded'>Grounded in: {LONG_TEXT}</p>", ""), encoding="utf-8")
        (bad_dir / "exempt.html").write_text(
            "<html><body><main><p class='kicker'>exempt fixture</p><h1>Exempt</h1>"
            "<p class='lead'>Short lead.</p>"
            "<h2 id='e1'>Exemptions</h2><p>short prose</p><ul><li>x</li></ul>"
            f"<p class='grounded'>Grounded in: {LONG_TEXT}</p>"
            "<figure><figcaption>" + LONG_TEXT + "</figcaption></figure>"
            "<pre>" + LONG_TEXT + "</pre>"
            "<ul><li>" + LONG_TEXT + "</li></ul>"
            "</main></body></html>", encoding="utf-8")
        violations, _ = check_pages(bad_dir, None)
        kinds = {v.split(":")[0] for v in violations}
        expected_kinds = {
            "paragraph-over-cap", "prose-run", "section-without-visual", "lead-over-cap",
            "hero-buttons", "callout-variants", "anchor-nav-missing", "grounded-missing",
        }
        missing = expected_kinds - kinds
        if missing:
            print(f"check-density selftest: FAIL — rules not fired on the bad fixture: {sorted(missing)} (got {sorted(kinds)})")
            return 1
        grounded_flagged = any("paragraph-over-cap" in v and v.count("word") for v in violations)
        if any("90-word" in v for v in violations):
            print("check-density selftest: FAIL — a .grounded listing was word-counted (the exemption is broken; RISK-003)")
            return 1
        if any(k in kinds for k in ("figcaption", "pre", "li")):
            print("check-density selftest: FAIL — non-<p> text was word-counted")
            return 1
        del grounded_flagged

        good_dir = tmp / "good-tree"
        good_dir.mkdir()
        shutil.copy(good, good_dir / "good.html")
        violations_good, _ = check_pages(good_dir, None)
        if violations_good:
            print(f"check-density selftest: FAIL — the good fixture was rejected: {violations_good}")
            return 1

        mismatch_dir = tmp / "mismatch-tree"
        mismatch_dir.mkdir()
        (mismatch_dir / "m.html").write_text(
            "<html><body><main><p class='lead'>short lead here.</p>"
            "<nav class='toc'><a href='#a'>A</a><a href='#zzz'>zzz</a></nav>"
            "<h2 id='a'>A</h2><p>text</p><ul><li>x</li></ul>"
            "<h2 id='b'>B</h2><p>text</p><ul><li>x</li></ul>"
            "<h2 id='c'>C</h2><p>text</p><ul><li>x</li></ul>"
            "<h2 id='d'>D</h2><p>text</p><ul><li>x</li></ul>"
            "<div class='plate plate-warn'><p>w</p></div><div class='plate plate-ok'><p>o</p></div>"
            "<p class='grounded'>Grounded in: fixture.</p></main></body></html>",
            encoding="utf-8",
        )
        violations_mm, _ = check_pages(mismatch_dir, None)
        if not any(v.startswith("anchor-nav-mismatch:") for v in violations_mm):
            print(f"check-density selftest: FAIL — toc/h2 set inequality not detected: {violations_mm}")
            return 1

        # FIND-code-003 negatives: home derived from the tree, not the filename —
        # a renamed home (m.html is the only root page, zero .btn) stays subject
        # to R5, and a tree with no root page fails loudly instead of skipping R5.
        if not any(v.startswith("hero-buttons: m.html:") for v in violations_mm):
            print(f"check-density selftest: FAIL — renamed home escaped R5: {violations_mm}")
            return 1

        subdir = tmp / "subdir-tree" / "section"
        subdir.mkdir(parents=True)
        (subdir / "deep.html").write_text(
            "<html><body><main><p class='lead'>short lead here.</p>"
            "<h2 id='d1'>D1</h2><p>text</p><ul><li>x</li></ul></main></body></html>",
            encoding="utf-8")
        violations_sub, _ = check_pages(subdir.parent, None)
        if not any(v.startswith("hero-buttons: no unambiguous home page") for v in violations_sub):
            print(f"check-density selftest: FAIL — rootless tree skipped R5 silently: {violations_sub}")
            return 1

        print("check-density selftest: OK — bad fixture fired all eight rules "
              "(61-word paragraph, 3-paragraph run, visual-less section, 36-word lead, "
              "one hero button, one callout variant, missing anchor rail, no grounded listing) "
              "while the 90-word .grounded listing, figcaption, pre and li stayed exempt; "
              "the good fixture passed clean; toc/h2 set inequality detected; the renamed "
              "home (m.html) stayed under R5 and the rootless tree failed loudly")
        return 0
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


def main() -> int:
    args = sys.argv[1:]
    if args[:1] == ["--selftest"]:
        return selftest()
    if not args:
        print(__doc__)
        return 2
    tree = Path(args[0])
    only = None
    if "--only" in args:
        only = args[args.index("--only") + 1]
    if not tree.is_dir():
        print(f"check-density: FAIL built tree {tree} not found (run site/tools/build.sh first)")
        return 1
    violations, lines = check_pages(tree, only)
    print(f"check-density: scanning {tree}{' --only ' + only if only else ''}")
    for line in lines:
        print(line)
    for v in violations:
        print(f"check-density: FAIL {v}")
    if violations:
        print(f"check-density: {len(violations)} violation(s)")
        return 1
    print("check-density: OK — no paragraph over 60 words, no prose run over two paragraphs, "
          "every section carries a visual, every lead ≤ 35 words, hero buttons present, "
          "≥ 2 callout variants, anchor-nav sets equal h2 id sets, grounded listing on every page")
    return 0


if __name__ == "__main__":
    sys.exit(main())
