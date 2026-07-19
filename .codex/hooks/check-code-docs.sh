#!/usr/bin/env bash
set -euo pipefail

ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
cd "$ROOT"

if [[ -d crates ]]; then
  python3 - <<'PY'
from pathlib import Path
import re
import sys

errors = []

rust_files = sorted(Path("crates").glob("*/src/**/*.rs"))
for path in rust_files:
    text = path.read_text(encoding="utf-8")
    lines = text.splitlines()
    first = next((line.strip() for line in lines if line.strip()), "")
    if not first.startswith("//!"):
        errors.append(f"{path}: missing module docs at top of file")
    module_doc = " ".join(
        line.strip().removeprefix("//!").strip()
        for line in lines
        if line.strip().startswith("//!")
    ).strip()
    if len(module_doc) < 120:
        errors.append(f"{path}: module docs are too thin; explain ownership and boundaries")

    for i, line in enumerate(lines):
        stripped = line.strip()
        if not re.match(r"pub(\([^)]+\))?\s+(async\s+)?(trait|fn)\s+", stripped):
            continue

        j = i - 1
        while j >= 0 and (not lines[j].strip() or lines[j].strip().startswith("#[")):
            j -= 1
        doc_lines = []
        while j >= 0 and lines[j].strip().startswith("///"):
            doc_lines.append(lines[j].strip().removeprefix("///").strip())
            j -= 1
        doc_text = " ".join(reversed(doc_lines)).strip()
        if not doc_text:
            errors.append(f"{path}:{i + 1}: public trait/function missing doc comment")
        elif len(doc_text) < 80:
            errors.append(f"{path}:{i + 1}: public trait/function docs are too thin")

if errors:
    print("code documentation review failed:", file=sys.stderr)
    for error in errors:
        print(f"  {error}", file=sys.stderr)
    sys.exit(1)
PY

  RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --document-private-items >/dev/null
fi

if [[ -d packages ]]; then
  python3 - <<'PY'
from pathlib import Path
import re
import sys

errors = []
ts_files = [
    path for path in sorted(Path("packages").glob("*/src/**/*"))
    if path.suffix in {".ts", ".tsx"} and not path.name.endswith(".generated.ts")
]

for path in ts_files:
    lines = path.read_text(encoding="utf-8").splitlines()
    for i, line in enumerate(lines):
        stripped = line.strip()
        if not re.match(r"export\s+(async\s+)?(function|class|interface|type)\s+", stripped):
            continue
        j = i - 1
        while j >= 0 and (not lines[j].strip() or lines[j].strip().startswith("//")):
            j -= 1
        prev = lines[j].strip() if j >= 0 else ""
        if not (prev.startswith("/**") or prev.startswith("*")):
            errors.append(f"{path}:{i + 1}: exported TypeScript surface missing JSDoc")

if errors:
    print("TypeScript documentation review failed:", file=sys.stderr)
    for error in errors:
        print(f"  {error}", file=sys.stderr)
    sys.exit(1)
PY
fi

echo "code documentation checks passed"
