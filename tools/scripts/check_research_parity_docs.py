#!/usr/bin/env python3
"""Check research docs for implementation-drift claims.

The registry is intentionally small and explicit. It catches claims that became
wrong as implementation moved ahead of the research notes while leaving
historical docs readable when they carry a supersession marker.
"""

from __future__ import annotations

import fnmatch
import json
import re
import sys
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[2]
REGISTRY = ROOT / "tools" / "research-parity" / "doc-drift-registry.json"
CAPABILITY_STATUSES = {"active", "inactive", "deferred"}


class RegistryError(ValueError):
    """Raised when the drift registry cannot be interpreted safely."""


def relative_path(root: Path, path: Path) -> str:
    return path.relative_to(root).as_posix()


def require_string(rule: dict[str, Any], key: str, rule_id: str) -> str:
    value = rule.get(key)
    if not isinstance(value, str) or not value:
        raise RegistryError(f"{rule_id}: {key} must be a non-empty string")
    return value


def require_string_list(rule: dict[str, Any], key: str, rule_id: str) -> list[str]:
    value = rule.get(key)
    if not isinstance(value, list) or not value or not all(isinstance(item, str) for item in value):
        raise RegistryError(f"{rule_id}: {key} must be a non-empty list of strings")
    return value


def compile_regex(rule_id: str, field: str, pattern: str) -> re.Pattern[str]:
    try:
        return re.compile(pattern, re.MULTILINE | re.DOTALL)
    except re.error as error:
        raise RegistryError(f"{rule_id}: invalid regex in {field}: {error}") from error


def load_registry(path: Path = REGISTRY) -> dict[str, Any]:
    try:
        registry = json.loads(path.read_text(encoding="utf-8"))
    except json.JSONDecodeError as error:
        raise RegistryError(f"{relative_path(path.parents[2], path)}: invalid JSON: {error}") from error
    if not isinstance(registry, dict):
        raise RegistryError("registry root must be a JSON object")
    return registry


def validate_registry(registry: dict[str, Any]) -> None:
    require_string_list(registry, "scan_paths", "registry")

    scan_files = registry.get("scan_files", [])
    if not isinstance(scan_files, list) or not all(isinstance(item, str) for item in scan_files):
        raise RegistryError("registry: scan_files must be a list of strings")

    for group in ("deny", "require", "capabilities"):
        value = registry.get(group, [])
        if not isinstance(value, list):
            raise RegistryError(f"registry: {group} must be a list")

    for rule in registry.get("deny", []):
        rule_id = require_string(rule, "id", "deny")
        require_string_list(rule, "path_globs", rule_id)
        for index, pattern in enumerate(require_string_list(rule, "patterns", rule_id)):
            compile_regex(rule_id, f"patterns[{index}]", pattern)
        require_string(rule, "reason", rule_id)

    for rule in registry.get("require", []):
        rule_id = require_string(rule, "id", "require")
        require_string(rule, "path", rule_id)
        section_regex = require_string(rule, "section_regex", rule_id)
        compile_regex(rule_id, "section_regex", section_regex)
        require_string(rule, "reason", rule_id)

    for rule in registry.get("capabilities", []):
        rule_id = require_string(rule, "id", "capabilities")
        status = require_string(rule, "status", rule_id)
        if status not in CAPABILITY_STATUSES:
            raise RegistryError(
                f"{rule_id}: status must be one of {', '.join(sorted(CAPABILITY_STATUSES))}"
            )
        require_string(rule, "path", rule_id)
        section_regex = require_string(rule, "section_regex", rule_id)
        compile_regex(rule_id, "section_regex", section_regex)
        require_string_list(rule, "must_contain", rule_id)
        require_string(rule, "reason", rule_id)


def discovered_files(root: Path, registry: dict[str, Any]) -> list[Path]:
    files: set[Path] = set()
    for scan_path in registry["scan_paths"]:
        path = root / scan_path
        if path.is_dir():
            files.update(candidate for candidate in path.rglob("*.md") if candidate.is_file())
        elif path.is_file() and path.suffix == ".md":
            files.add(path)

    for scan_file in registry.get("scan_files", []):
        path = root / scan_file
        if path.is_file():
            files.add(path)

    return sorted(files)


def matches_any(root: Path, path: Path, patterns: list[str]) -> bool:
    rel = relative_path(root, path)
    return any(fnmatch.fnmatch(rel, pattern) for pattern in patterns)


def section_match(text: str, rule: dict[str, Any]) -> re.Match[str] | None:
    return compile_regex(rule["id"], "section_regex", rule["section_regex"]).search(text)


def check_registry(root: Path, registry: dict[str, Any]) -> list[str]:
    validate_registry(registry)
    files = discovered_files(root, registry)
    failures: list[str] = []

    for rule in registry.get("deny", []):
        paths = [path for path in files if matches_any(root, path, rule["path_globs"])]
        for pattern in rule["patterns"]:
            compiled = compile_regex(rule["id"], "patterns", pattern)
            for path in paths:
                text = path.read_text(encoding="utf-8")
                for match in compiled.finditer(text):
                    line = text.count("\n", 0, match.start()) + 1
                    rel = relative_path(root, path)
                    failures.append(
                        f"{rule['id']}: {rel}:{line}: stale claim matched {pattern!r} ({rule['reason']})"
                    )

    for rule in registry.get("require", []):
        path = root / rule["path"]
        if not path.exists():
            failures.append(f"{rule['id']}: missing required file {rule['path']} ({rule['reason']})")
            continue
        text = path.read_text(encoding="utf-8")
        if section_match(text, rule) is None:
            failures.append(
                f"{rule['id']}: {rule['path']}: missing required section {rule['section_regex']!r} ({rule['reason']})"
            )

    for rule in registry.get("capabilities", []):
        path = root / rule["path"]
        if not path.exists():
            failures.append(f"{rule['id']}: missing capability evidence file {rule['path']} ({rule['reason']})")
            continue
        text = path.read_text(encoding="utf-8")
        match = section_match(text, rule)
        if match is None:
            failures.append(
                f"{rule['id']}: {rule['path']}: missing {rule['status']} capability evidence section ({rule['reason']})"
            )
            continue
        section = match.group(0)
        for expected in rule["must_contain"]:
            if expected not in section:
                failures.append(
                    f"{rule['id']}: {rule['path']}: {rule['status']} capability section missing {expected!r}"
                )

    return failures


def main() -> int:
    try:
        registry = load_registry()
        failures = check_registry(ROOT, registry)
    except RegistryError as error:
        print(f"research parity doc drift registry error: {error}", file=sys.stderr)
        return 2

    if failures:
        print("research parity doc drift check failed:", file=sys.stderr)
        for failure in failures:
            print(failure, file=sys.stderr)
        return 1

    print("research parity doc drift check passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
