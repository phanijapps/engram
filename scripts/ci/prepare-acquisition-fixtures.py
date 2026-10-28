#!/usr/bin/env python3
"""Install loopback-only test transport dependencies into an owned CI venv."""
import os
from pathlib import Path
import subprocess
import tempfile
import venv

root = Path(os.environ.get("RUNNER_TEMP", Path(__file__).resolve().parents[2] / "target"))
root.mkdir(parents=True, exist_ok=True)
owned = Path(tempfile.mkdtemp(prefix="engram-acquisition-fixtures-", dir=root))
venv.EnvBuilder(with_pip=True, symlinks=(os.name != "nt")).create(owned)
python = owned / ("Scripts/python.exe" if os.name == "nt" else "bin/python")
requirements = Path(__file__).resolve().parents[2] / "adapters/ingest/tests/requirements-portable-acquisition.txt"
subprocess.run([str(python), "-m", "pip", "install", "--disable-pip-version-check", "--no-cache-dir", "-r", str(requirements)], check=True)
if "GITHUB_ENV" in os.environ:
    with open(os.environ["GITHUB_ENV"], "a", encoding="utf-8") as stream:
        stream.write(f"ENGRAM_TEST_PYTHON={python}\n")
print(f"ENGRAM_TEST_PYTHON={python}")
