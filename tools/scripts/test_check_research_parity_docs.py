#!/usr/bin/env python3
"""Regression tests for the research parity doc drift checker."""

from __future__ import annotations

import importlib.util
import tempfile
import unittest
from pathlib import Path


SCRIPT = Path(__file__).with_name("check_research_parity_docs.py")
SPEC = importlib.util.spec_from_file_location("check_research_parity_docs", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
checker = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(checker)


def write(root: Path, relative: str, text: str) -> None:
    path = root / relative
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8")


def minimal_registry() -> dict:
    return {
        "scan_paths": ["docs/research", "docs/architecture"],
        "scan_files": ["docs/arch_divergence.md"],
        "deny": [
            {
                "id": "stale-belief-claim",
                "path_globs": ["docs/research/*.md", "docs/research/**/*.md"],
                "patterns": ["does not implement as-of reads"],
                "reason": "fixture stale claim",
            }
        ],
        "require": [
            {
                "id": "current-note",
                "path": "docs/research/current.md",
                "section_regex": "## Current implementation note\\n.*?valid-time support",
                "reason": "fixture current marker",
            }
        ],
        "capabilities": [
            {
                "id": "valid-time-as-of",
                "status": "active",
                "path": "docs/research/current.md",
                "section_regex": "## Current implementation note\\n.*?valid-time support",
                "must_contain": ["valid-time support"],
                "reason": "fixture capability",
            }
        ],
    }


class ResearchParityDocsTest(unittest.TestCase):
    def test_clean_fixture_passes(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            write(root, "docs/research/current.md", "## Current implementation note\nvalid-time support\n")

            failures = checker.check_registry(root, minimal_registry())

            self.assertEqual([], failures)

    def test_nested_research_docs_are_scanned(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            write(root, "docs/research/current.md", "## Current implementation note\nvalid-time support\n")
            write(root, "docs/research/nested/stale.md", "Engram does not implement as-of reads.\n")

            failures = checker.check_registry(root, minimal_registry())

            self.assertTrue(any("docs/research/nested/stale.md" in failure for failure in failures))

    def test_required_marker_must_match_scoped_section(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            write(
                root,
                "docs/research/current.md",
                "This file mentions Current implementation note incidentally.\n",
            )
            registry = minimal_registry()
            registry["capabilities"] = []

            failures = checker.check_registry(root, registry)

            self.assertTrue(any("missing required section" in failure for failure in failures))

    def test_invalid_registry_regex_names_rule_and_field(self) -> None:
        registry = minimal_registry()
        registry["deny"][0]["patterns"] = ["["]

        with self.assertRaisesRegex(checker.RegistryError, "stale-belief-claim: invalid regex in patterns\\[0\\]"):
            checker.validate_registry(registry)


if __name__ == "__main__":
    unittest.main()
