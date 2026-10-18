#!/usr/bin/env python3
"""Stamp a platform npm package from the template + a built binary.

Usage: stamp-platform-package.py <binary-path> <target-slug> <version> [--full]

Creates a ready-to-publish npm tarball next to the binary:
  <binary-dir>/pkg/<name>-<version>.tgz

The tarball contains exactly one file: the binary (plus package.json).
No install scripts, no extra files — the trust contract from RFC-0021.
"""
import hashlib
import json
import os
import subprocess
import sys
import tarfile
import tempfile

TEMPLATE_PATH = os.path.join(os.path.dirname(__file__), "..", "platform-package.template.json")

TARGET_MAP = {
    "linux-x64-gnu": {"os": "linux", "cpu": "x64"},
    "linux-arm64-gnu": {"os": "linux", "cpu": "arm64"},
    "darwin-arm64": {"os": "darwin", "cpu": "arm64"},
    "darwin-x64": {"os": "darwin", "cpu": "x64"},
    "win32-x64": {"os": "win32", "cpu": "x64"},
}


def sha256(path: str) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 16), b""):
            h.update(chunk)
    return h.hexdigest()


def main() -> int:
    if len(sys.argv) < 4:
        print(__doc__, file=sys.stderr)
        return 1
    binary_path = os.path.abspath(sys.argv[1])
    target = sys.argv[2]
    version = sys.argv[3]
    is_full = "--full" in sys.argv

    if target not in TARGET_MAP:
        print(f"error: unknown target {target}", file=sys.stderr)
        return 1
    if not os.path.isfile(binary_path):
        print(f"error: binary not found: {binary_path}", file=sys.stderr)
        return 1

    os_info = TARGET_MAP[target]
    name = f"@engram/mcp-{target}" + ("-full" if is_full else "")
    bin_name = os.path.basename(binary_path)

    # package.json from template
    pkg = {
        "name": name,
        "version": version,
        "description": f"engram-mcp server binary for {target}"
        + (" (fastembed/local-models variant)" if is_full else ""),
        "license": "MIT",
        "repository": {"type": "git", "url": "git+https://github.com/phanijapps/engram.git"},
        "os": [os_info["os"]],
        "cpu": [os_info["cpu"]],
        "files": [bin_name],
    }

    # Build the tarball: package/{package.json, binary} — npm's expected layout
    pkg_dir = os.path.join(os.path.dirname(binary_path), "pkg")
    os.makedirs(pkg_dir, exist_ok=True)
    tarball = os.path.join(pkg_dir, f"mcp-{target}" + ("-full" if is_full else "") + f"-{version}.tgz")

    with tempfile.TemporaryDirectory() as staging:
        stage = os.path.join(staging, "package")
        os.makedirs(stage)
        with open(os.path.join(stage, "package.json"), "w") as f:
            json.dump(pkg, f, indent=2)
            f.write("\n")
        # copy + chmod the binary
        import shutil
        dst = os.path.join(stage, bin_name)
        shutil.copy2(binary_path, dst)
        os.chmod(dst, 0o755)

        with tarfile.open(tarball, "w:gz") as tf:
            tf.add(os.path.join(stage, "package.json"), arcname="package/package.json")
            tf.add(dst, arcname=f"package/{bin_name}")

    checksum = sha256(binary_path)
    tarball_checksum = sha256(tarball)
    size_mb = os.path.getsize(tarball) // (1024 * 1024)

    print(f"stamped: {tarball}")
    print(f"  name:      {name}@{version}")
    print(f"  binary:    {bin_name} (sha256 {checksum[:16]}…)")
    print(f"  tarball:   {size_mb} MB (sha256 {tarball_checksum[:16]}…)")
    print(f"  os/cpu:    {os_info['os']}/{os_info['cpu']}")

    return 0


if __name__ == "__main__":
    sys.exit(main())
