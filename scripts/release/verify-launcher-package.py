#!/usr/bin/env python3
"""Resolve and launch the actual packed launcher + canonical platform tarball.

No registry writes or installs. Extract only the named manifest/entrypoint/binary
into an isolated node_modules layout, then launch --version and status --slim.
"""
import argparse
import json
import os
from pathlib import Path
import subprocess
import tarfile
import tempfile


def member(archive, name):
    stream = archive.extractfile(f"package/{name}")
    if stream is None:
        raise ValueError(f"missing {name}")
    with stream:
        return stream.read()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--launcher", type=Path, required=True)
    parser.add_argument("--platform-package", type=Path, required=True)
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix="engram-packed-") as scratch:
        root = Path(scratch)
        with tarfile.open(args.launcher) as archive:
            manifest = json.loads(member(archive, "package.json"))
            assert manifest["name"] == "@engramlabs/mcp", manifest
            launcher = root / "node_modules" / "@engramlabs" / "mcp"
            launcher.mkdir(parents=True)
            for name in ["package.json", "bin.js"]:
                (launcher / name).write_bytes(member(archive, name))
        with tarfile.open(args.platform_package) as archive:
            platform = json.loads(member(archive, "package.json"))
            name = platform["name"]
            assert name.startswith("@engramlabs/mcp-") and not name.endswith("-full"), name
            assert manifest["optionalDependencies"][name] == platform["version"] == manifest["version"]
            binary_name = "engram-mcp.exe" if platform["os"] == ["win32"] else "engram-mcp"
            target = launcher / "node_modules" / "@engramlabs" / name.split("/")[1]
            target.mkdir(parents=True)
            (target / "package.json").write_bytes(member(archive, "package.json"))
            binary = target / binary_name
            binary.write_bytes(member(archive, binary_name))
            binary.chmod(0o755)
        env = {key: value for key, value in os.environ.items() if key not in ["ENGRAM_MCP_BIN", "ENGRAM_NO_FULL"]}
        env.update({"ENGRAM_EMBEDDING_DIMENSIONS": "invalid", "ENGRAM_EMBEDDING_CONFIG": str(root / "missing-profile")})
        command = ["node", str(launcher / "bin.js")]
        version = subprocess.run([*command, "--version"], capture_output=True, text=True, env=env, timeout=30, check=True)
        assert version.stdout.strip() == manifest["version"], version.stdout
        status = subprocess.run([*command, "status", "--slim", "--storage", str(root / "store"), "--json"],
                                capture_output=True, text=True, env=env, timeout=30, check=True)
        assert json.loads(status.stdout)["ok"], status.stdout
        print(f"PASS packed {manifest['name']}@{manifest['version']} resolves {name}; --slim startup works")


if __name__ == "__main__":
    main()
