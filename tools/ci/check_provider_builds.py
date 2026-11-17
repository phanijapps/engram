#!/usr/bin/env python3
"""Inspect isolated production dependency closures, separately from test graphs."""
from __future__ import annotations
import argparse
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SQLITE = {"engram-store-sqlite", "rusqlite", "sqlite-vec", "libsqlite3-sys"}


def package_names(output: str) -> set[str]:
    return {line.split()[0] for line in output.splitlines() if line.strip()}


def violations(production: set[str], forbidden: set[str]) -> list[str]:
    return [f"forbidden production dependency: {name}" for name in sorted(production & forbidden)]


def dependency_graph(package: str, features: str, edges: str) -> set[str]:
    command = ["cargo", "tree", "--locked", "--package", package,
               "--no-default-features", "--edges", edges, "--prefix", "none",
               "--format", "{p}"]
    if features:
        command += ["--features", features]
    result = subprocess.run(command, cwd=ROOT, check=True, text=True, capture_output=True)
    return package_names(result.stdout)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--package", default="engram-integration")
    parser.add_argument("--features", default="fastembed,http-embeddings")
    args = parser.parse_args()
    production = dependency_graph(args.package, args.features, "normal,build")
    tests = dependency_graph(args.package, args.features, "normal,build,dev")
    errors = violations(production, SQLITE)
    print("production (normal/build):", ", ".join(sorted(production)))
    print("test-only graph additions:", ", ".join(sorted(tests - production)))
    for error in errors:
        print(error)
    return int(bool(errors))


if __name__ == "__main__":
    raise SystemExit(main())
