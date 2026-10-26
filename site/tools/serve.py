#!/usr/bin/env python3
"""Base-path static server for the built site (plan DEC-P03 probe harness).

Serves site/_site under the /engram/ base path exactly as GitHub Pages will
(http://127.0.0.1:<port>/engram/... -> site/_site/...), so probe runs and the
link gate resolve the same absolute URLs production will serve.

Usage: python3 site/tools/serve.py 8766 [site/_site]
"""
import sys
from functools import partial
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

BASE = "/engram"


class BasePathHandler(SimpleHTTPRequestHandler):
    def translate_path(self, path: str) -> str:
        if path == BASE or path.startswith(BASE + "/"):
            path = path[len(BASE):] or "/"
        else:
            # Anything outside the base path is a wrong-root request; report it
            # as the probe sees it (404) instead of leaking worktree files.
            return str(Path(self.server.root) / "__wrong_root__" / path.lstrip("/"))
        return super().translate_path(path)

    def log_message(self, fmt, *args):  # quiet: probe output stays readable
        pass


def main() -> int:
    port = int(sys.argv[1]) if len(sys.argv) > 1 else 8766
    root = sys.argv[2] if len(sys.argv) > 2 else "site/_site"
    handler = partial(BasePathHandler, directory=root)
    server = ThreadingHTTPServer(("127.0.0.1", port), handler)
    server.root = root
    print(f"serve.py: {BASE}/ -> {root}/ on http://127.0.0.1:{port}", flush=True)
    server.serve_forever()
    return 0


if __name__ == "__main__":
    sys.exit(main())
