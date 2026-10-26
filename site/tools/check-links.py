#!/usr/bin/env python3
"""Link, budget, subresource, anchor and claim gate for the built site tree.

Job 004 plan DEC-P03; job 008 TASK-004 extends it with anchor resolution
(plan step 4): any internal URL carrying a fragment — same-page (#frag) or
cross-page (/base/page/#frag) — must resolve to a real id on the target built
page. A fragment that names no id is a dead anchor (DEC-P03's anchor rail and
gallery links ride on this rule).

Walks a built Jekyll tree and asserts:
  1. every internal href/src/srcset target is rooted at the --base prefix
     (wrong-root hits are the getbootstrap.com custom-domain trap, RISK-002)
     and resolves to a real file in the tree (adding index.html for
     directory URLs);
  2. every fragment-bearing internal <a href> resolves to an id on the target
     page (same-page or cross-page) — dead anchors FAIL;
  3. no external subresource is referenced (scripts, stylesheets, images,
     frames — NFR-002 third-party-request zero; plain external <a> anchors
     and metadata <link rel="canonical"|"alternate"|…> references are
     allowed and reported);
  4. each page's HTML + CSS + font payload stays at or under the budget
     (NFR-002: 200 KiB before diagram assets; .svg images are excluded);
  5. CSS url(...) references resolve to files;
  6. every "<number> diagrams" claim is machine-checked: the claim must sit
     inside an element carrying data-diagrams-for="<page url>", and the
     claimed count must equal the number of diagram <img> embeds on that
     page; unbound count claims anywhere are violations (LED-2 no-fabrication
     rule, FIND-UX-001 — the home card once said "six diagrams" over a page
     that embeds seven).

--selftest builds a synthetic fixture containing one broken link, one
wrong-root link, one dead cross-page anchor, one dead same-page anchor, one
live anchor (negative control), one wrong diagram count and one unbound
diagram count, and FAILS unless the checker rejects the fixture while leaving
the live anchor unflagged — the negative cases are proven, not assumed.

Usage:
  python3 site/tools/check-links.py site/_site --base /engram/
  python3 site/tools/check-links.py --selftest
"""
from __future__ import annotations

import re
import shutil
import sys
import tempfile
from html.parser import HTMLParser
from pathlib import Path

DEFAULT_BUDGET = 200 * 1024  # NFR-002: 200 KiB per page before diagram assets
EXTERNAL_OK_ANCHOR_SCHEMES = ("http://", "https://", "mailto:")
SUBRESOURCE_TAGS = ("script", "img", "iframe", "source", "link", "video", "audio", "embed", "track")

ATTR_RE = re.compile(r"""(?:href|src)\s*=\s*("([^"]*)"|'([^']*)')""", re.I)
SRCSET_RE = re.compile(r"""srcset\s*=\s*("([^"]*)"|'([^']*)')""", re.I)
REL_RE = re.compile(r"""rel\s*=\s*("([^"]*)"|'([^']*)')""", re.I)
ID_RE = re.compile(r"""id\s*=\s*("([^"]*)"|'([^']*)')""", re.I)
TAG_RE = re.compile(r"<\s*(\w+)[^>]*?>", re.I | re.S)
URL_IN_CSS_RE = re.compile(r"url\(\s*(['\"]?)([^'\")]+)\1\s*\)", re.I)

# <link rel> values that reference metadata without fetching a subresource
# (a canonical URL points at the page's own preferred address — an absolute
# URL by design; NFR-002 forbids fetched third-party resources, not metadata).
SAFE_LINK_RELS = {"canonical", "alternate", "author", "license", "prev", "next", "shortlink"}

# Diagram-count claims (assertion 6): "seven diagrams", "7 diagrams", …
NUMBER_WORDS = {w: n for n, w in enumerate(
    "one two three four five six seven eight nine ten eleven twelve".split(), start=1)}
DIAGRAM_CLAIM_RE = re.compile(r"\b(" + "|".join(NUMBER_WORDS) + r"|\d+)\s+diagrams?\b", re.I)
VOID_TAGS = {"area", "base", "br", "col", "embed", "hr", "img", "input", "link",
             "meta", "param", "source", "track", "wbr"}


class Violations:
    def __init__(self) -> None:
        self.items: list[str] = []

    def add(self, kind: str, page: str, detail: str) -> None:
        self.items.append(f"{kind}: {page}: {detail}")

    def any(self) -> bool:
        return bool(self.items)


class PageFacts(HTMLParser):
    """Per-page facts: element ids, diagram <img> embeds, bound count claims
    and unbound claim text."""

    def __init__(self) -> None:
        super().__init__(convert_charrefs=True)
        self.ids: set[str] = set()
        self.diagram_imgs = 0
        self.claims: list[tuple[str, int | None, str]] = []  # (target, claimed, phrase)
        self.unbound_claims: list[str] = []
        self._depth = 0
        self._bound: list[dict] = []
        self._unbound: list[str] = []

    def handle_starttag(self, tag, attrs):
        if tag not in VOID_TAGS:
            self._depth += 1
        self._see(tag, attrs)

    def handle_startendtag(self, tag, attrs):
        self._see(tag, attrs)

    def _see(self, tag, attrs):
        a = dict(attrs)
        if a.get("id"):
            self.ids.add(a["id"])
        if tag == "img" and "/assets/diagrams/" in (a.get("src") or ""):
            self.diagram_imgs += 1
        target = a.get("data-diagrams-for")
        if target is not None:
            self._bound.append({"depth": self._depth, "target": target, "buf": []})

    def handle_endtag(self, tag):
        while self._bound and self._bound[-1]["depth"] == self._depth:
            bound = self._bound.pop()
            text = " ".join(bound["buf"])
            m = DIAGRAM_CLAIM_RE.search(text)
            if not m:
                self.claims.append((bound["target"], None, text.strip()[:60]))
            else:
                word = m.group(1).lower()
                claimed = NUMBER_WORDS[word] if word in NUMBER_WORDS else int(word)
                self.claims.append((bound["target"], claimed, m.group(0)))
        if tag not in VOID_TAGS:
            self._depth -= 1

    def handle_data(self, data):
        if self._bound:
            self._bound[-1]["buf"].append(data)
        else:
            self._unbound.append(data)

    def close(self) -> None:
        super().close()
        text = " ".join(self._unbound)
        self.unbound_claims = [m.group(0) for m in DIAGRAM_CLAIM_RE.finditer(text)]


def parse_page(page: Path) -> PageFacts:
    f = PageFacts()
    f.feed(page.read_text(encoding="utf-8", errors="replace"))
    f.close()
    return f


def strip_frag_query(url: str) -> str:
    return url.split("#", 1)[0].split("?", 1)[0]


def fragment_of(url: str) -> str:
    return url.split("#", 1)[1].split("?", 1)[0] if "#" in url else ""


def resolve_internal(base: str, tree: Path, url: str) -> Path | None:
    """Map an internal absolute URL to a file in the tree, or None."""
    if not url.startswith(base):
        return None
    rel = url[len(base):].lstrip("/")
    if rel == "":
        return tree / "index.html"
    candidate = tree / rel
    if candidate.suffix == "":
        return candidate / "index.html"
    return candidate


def css_urls(css_path: Path):
    urls: list[str] = []
    for match in URL_IN_CSS_RE.finditer(css_path.read_text(encoding="utf-8", errors="replace")):
        urls.append(match.group(2))
    return urls


def check_anchor(url: str, tag: str, page: Path, rel: str, base: str, tree: Path,
                 facts: dict[Path, PageFacts], v: Violations) -> None:
    """Assertion 2: fragment-bearing internal <a href> must name a real id on
    the target page (same-page or cross-page)."""
    frag = fragment_of(url)
    if not frag or tag != "a":
        return
    if url.startswith("#"):
        target_page = page
        shown = url
    elif url.startswith(base):
        resolved = resolve_internal(base, tree, strip_frag_query(url))
        if resolved is None or not resolved.is_file():
            return  # the path check below reports unresolved targets
        if resolved not in facts:
            facts[resolved] = parse_page(resolved)
        target_page = resolved
        shown = url
    else:
        return  # external or wrong-root URLs are handled by their own rules
    if frag not in facts[target_page].ids:
        v.add("dead-anchor", rel,
              f'<a href="{shown}"> — no element id "{frag}" on '
              f'{target_page.relative_to(tree)}')


def check_diagram_claims(tree: Path, base: str, pages: list[Path], v: Violations) -> None:
    """Assertion 6: every '<n> diagrams' claim matches the target page's real embed count."""
    facts: dict[Path, PageFacts] = {}
    for page in pages:
        f = parse_page(page)
        facts[page] = f
        rel = str(page.relative_to(tree))
        for phrase in f.unbound_claims:
            v.add("diagram-count-unbound", rel,
                  f'"{phrase}" — count claims must sit inside an element carrying '
                  f'data-diagrams-for="<page url>" that matches the target page')
    for page, f in facts.items():
        rel = str(page.relative_to(tree))
        for target, claimed, phrase in f.claims:
            if claimed is None:
                v.add("diagram-count-no-claim", rel,
                      f'data-diagrams-for="{target}" carries no "<n> diagrams" claim')
                continue
            if not target.startswith(base):
                v.add("diagram-count-wrong-root", rel,
                      f'data-diagrams-for="{target}" — not rooted at {base}')
                continue
            t = resolve_internal(base, tree, target)
            if t is None or not t.is_file() or t not in facts:
                v.add("diagram-count-unresolved", rel,
                      f'data-diagrams-for="{target}" does not resolve to a built page')
                continue
            actual = facts[t].diagram_imgs
            if claimed != actual:
                v.add("diagram-count", rel,
                      f'claims "{phrase}" via data-diagrams-for="{target}" but that page '
                      f'embeds {actual} diagram(s)')


def check_tree(tree: Path, base: str, budget: int) -> tuple[Violations, dict[str, int], set[str]]:
    v = Violations()
    payloads: dict[str, int] = {}
    externals: set[str] = set()
    pages = sorted(tree.rglob("*.html"))
    facts: dict[Path, PageFacts] = {p: parse_page(p) for p in pages}

    css_assets: set[Path] = set()
    for css in sorted(tree.rglob("*.css")):
        css_assets.add(css)
        for url in css_urls(css):
            url = strip_frag_query(url.strip())
            if not url or url.startswith(("data:", "#")) or url.startswith(EXTERNAL_OK_ANCHOR_SCHEMES):
                if url.startswith(EXTERNAL_OK_ANCHOR_SCHEMES):
                    v.add("external-css-subresource", str(css), url)
                continue
            target = (css.parent / url).resolve()
            if not target.exists():
                v.add("broken-css-asset", str(css), url)
            elif target.suffix in (".woff", ".woff2", ".otf", ".ttf"):
                css_assets.add(target)

    for page in pages:
        rel = str(page.relative_to(tree))
        html = page.read_text(encoding="utf-8", errors="replace")
        payload = page.stat().st_size
        page_css: set[Path] = set()

        for tag_match in TAG_RE.finditer(html):
            tag = tag_match.group(1).lower()
            blob = tag_match.group(0)
            rel_match = REL_RE.search(blob)
            rels = set(((rel_match.group(2) or rel_match.group(3)) if rel_match else "").lower().split())
            for attr in ATTR_RE.finditer(blob):
                raw_url = attr.group(2) or attr.group(3) or ""
                url = strip_frag_query(raw_url)
                check_anchor(raw_url, tag, page, rel, base, tree, facts, v)
                if not url or url.startswith(("data:", "javascript:")):
                    continue
                if url.startswith(EXTERNAL_OK_ANCHOR_SCHEMES):
                    if tag == "a" or (tag == "link" and rels & SAFE_LINK_RELS):
                        externals.add(url)  # outbound anchor or metadata link — allowed
                    elif tag in SUBRESOURCE_TAGS:
                        v.add("external-subresource", rel, f"<{tag}> {url} (NFR-002 forbids third-party subresources)")
                    continue
                if not url.startswith("/"):
                    if not raw_url.startswith("#"):
                        v.add("relative-url", rel, f"<{tag}> {url} — must be absolute under {base}")
                    continue
                if not url.startswith(base):
                    v.add("wrong-root", rel, f"<{tag}> {url} — not rooted at {base}")
                    continue
                target = resolve_internal(base, tree, url)
                if target is None or not target.is_file():
                    v.add("broken-link", rel, f"<{tag}> {url}")
                    continue
                if tag == "link" and target.suffix == ".css":
                    page_css.add(target)
                elif tag in ("img", "source") and target.suffix in (".woff2", ".woff", ".css"):
                    page_css.add(target)  # font/style loaded via media elements

            for attr in SRCSET_RE.finditer(blob):
                for candidate in attr.group(2).split(","):
                    url = strip_frag_query(candidate.strip().split(" ")[0])
                    if not url:
                        continue
                    if url.startswith(EXTERNAL_OK_ANCHOR_SCHEMES):
                        v.add("external-subresource", rel, f"srcset {url}")
                        continue
                    if not url.startswith(base):
                        v.add("wrong-root", rel, f"srcset {url} — not rooted at {base}")
                        continue
                    target = resolve_internal(base, tree, url)
                    if target is None or not target.is_file():
                        v.add("broken-link", rel, f"srcset {url}")

        for css in page_css:
            payload += css.stat().st_size
            for url in css_urls(css):
                url = strip_frag_query(url.strip())
                if url and not url.startswith(("data:", "#")) and not url.startswith(EXTERNAL_OK_ANCHOR_SCHEMES):
                    target = (css.parent / url).resolve()
                    if target.suffix in (".woff2", ".woff", ".otf", ".ttf") and target.is_file():
                        payload += target.stat().st_size
        payloads[rel] = payload
        if payload > budget:
            v.add("payload-over-budget", rel, f"{payload} bytes > {budget} bytes HTML+CSS+fonts")

    check_diagram_claims(tree, base, pages, v)
    return v, payloads, externals


def selftest() -> int:
    tmp = Path(tempfile.mkdtemp(prefix="engram-check-links-selftest-"))
    try:
        (tmp / "ok.html").write_text('<html><body id="live">ok</body></html>', encoding="utf-8")
        (tmp / "assets").mkdir()
        (tmp / "assets" / "x.css").write_text("body{color:red}", encoding="utf-8")
        diagrams = tmp / "assets" / "diagrams"
        diagrams.mkdir()
        for i in range(3):
            (diagrams / f"d{i}.svg").write_text("<svg xmlns='http://www.w3.org/2000/svg'/>", encoding="utf-8")
        (tmp / "gallery.html").write_text(
            '<html><body id="gallery-top">'
            '<img src="/engram/assets/diagrams/d0.svg">'
            '<img src="/engram/assets/diagrams/d1.svg">'
            '<img src="/engram/assets/diagrams/d2.svg">'
            "</body></html>",
            encoding="utf-8",
        )
        fixture = tmp / "index.html"
        fixture.write_text(
            '<html><head><link rel="stylesheet" href="/engram/assets/x.css"></head>'
            '<body id="home-top"><a href="/engram/ok.html">good</a>'
            '<a href="/engram/ok.html#live">live anchor</a>'
            '<a href="/engram/ok.html#dead">dead cross-page anchor</a>'
            '<a href="#home-top">live same-page anchor</a>'
            '<a href="#nowhere">dead same-page anchor</a>'
            '<a href="/engram/missing.html">broken</a>'
            '<a href="/assets/x.css">wrong root</a>'
            '<img src="/engram/assets/missing.png">'
            '<p data-diagrams-for="/engram/gallery.html">two diagrams live there</p>'
            "<p>plain text claiming five diagrams with no binding</p></body></html>",
            encoding="utf-8",
        )
        v, payloads, _ = check_tree(tmp, "/engram/", DEFAULT_BUDGET)
        kinds = {item.split(":")[0] for item in v.items}
        broken = any(item.startswith("broken-link:") and "/engram/missing.html" in item for item in v.items)
        wrong_root = any(item.startswith("wrong-root:") for item in v.items)
        dead_cross = any(item.startswith("dead-anchor:") and "ok.html#dead" in item for item in v.items)
        dead_same = any(item.startswith("dead-anchor:") and "#nowhere" in item for item in v.items)
        live_clean = not any("dead-anchor" in item and ("#live" in item or "#home-top" in item) for item in v.items)
        bad_count = any(item.startswith("diagram-count:") and "embeds 3" in item for item in v.items)
        unbound = any(item.startswith("diagram-count-unbound:") and "five diagrams" in item for item in v.items)
        for name, ok in (("broken link", broken), ("wrong-root", wrong_root),
                         ("dead cross-page anchor", dead_cross), ("dead same-page anchor", dead_same),
                         ("live anchors stayed clean", live_clean),
                         ("wrong diagram count", bad_count), ("unbound diagram count", unbound)):
            if not ok:
                print(f"check-links selftest: FAIL — fixture's {name} was not detected")
                return 1
        if not v.any():
            print("check-links selftest: FAIL — checker passed a defective fixture")
            return 1
        print(f"check-links selftest: OK — defective fixture rejected as expected ({sorted(kinds)}); "
              "broken + wrong-root + dead cross-page anchor + dead same-page anchor + wrong-count + "
              "unbound-count all detected while the live anchors stay clean")
        return 0
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


def main() -> int:
    args = sys.argv[1:]
    if args[:1] == ["--selftest"]:
        return selftest()
    if len(args) < 3 or args[1] != "--base":
        print(__doc__)
        return 2
    tree = Path(args[0])
    base = args[2]
    budget = DEFAULT_BUDGET
    if "--budget" in args:
        budget = int(args[args.index("--budget") + 1])
    if not tree.is_dir():
        print(f"check-links: FAIL built tree {tree} not found (run site/tools/build.sh first)")
        return 1
    if not base.startswith("/") or not base.endswith("/"):
        print(f"check-links: FAIL --base must be an absolute path with trailing slash, got {base!r}")
        return 2
    v, payloads, externals = check_tree(tree, base, budget)
    pages = sorted(payloads)
    print(f"check-links: {len(pages)} page(s), base {base}, budget {budget} bytes")
    for rel in pages:
        print(f"  payload {payloads[rel]:>7} bytes  {rel}")
    for item in v.items:
        print(f"check-links: FAIL {item}")
    if v.any():
        return 1
    print("check-links: OK — zero wrong-root hits, zero broken links, zero dead anchors, "
          "zero external subresources, budget holds, every diagram-count claim matches its target page")
    return 0


if __name__ == "__main__":
    sys.exit(main())
