#!/usr/bin/env python3
"""CI wrapper: derive_catalogue.py (delegates to engram-constitution skill)."""
import subprocess, sys
from pathlib import Path
r = subprocess.run([sys.executable, str(Path(__file__).parents[2] / ".agents/skills/engram-constitution/scripts/derive_catalogue.py"), "."],
                   capture_output=False)
sys.exit(r.returncode)
