"""Run: python3 -m unittest discover -s evals -v. SYNTAX-ONLY conformance for
engram-frame and its factory wiring — deterministic, no LLM. Behavioral
coverage lives in scenarios.json (scripted owner turns + per-assertion
evidence; string presence here does NOT prove interview behavior)."""

import json
import re
import unittest
from pathlib import Path

SKILL = Path(__file__).resolve().parents[1]
FACTORY = SKILL.parent / "engram-factory"


class FrameSkillTests(unittest.TestCase):
    def setUp(self):
        self.skill = (SKILL / "SKILL.md").read_text()
        self.brief = (FACTORY / "assets" / "templates" / "brief.md").read_text()

    def test_frontmatter_and_routing(self):
        m = re.match(r"^---\n(.*?)\n---\n", self.skill, re.S)
        self.assertTrue(m, "frontmatter present")
        fm = m.group(1)
        self.assertIn("name: engram-frame", fm)
        desc = re.search(r"description: (.+)", fm).group(1)
        self.assertGreater(len(desc), 40)
        self.assertRegex(desc, r"[Tt]rigger")

    def test_required_sections(self):
        for section in ("## Contract", "## Question format", "## Workflow",
                        "## When it fails", "## Anti-patterns", "## Final check"):
            self.assertIn(section, self.skill, f"missing {section}")

    def test_stays_lean(self):
        self.assertLessEqual(len(self.skill.splitlines()), 160)

    def test_interview_discipline_markers(self):
        for marker in ("freeform", "MoSCoW", "strike", "Translate-and-confirm",
                       "anything else?"):
            self.assertIn(marker, self.skill, f"missing interview marker: {marker}")

    def test_caps_are_stated(self):
        self.assertRegex(self.skill, r"four substantive questions|four questions")
        self.assertRegex(self.skill, r"two rounds per area, eight rounds total")
        self.assertRegex(self.skill, r"transition, not a question")

    def test_authority_rules(self):
        self.assertIn("Silence is never consent", self.skill)
        self.assertRegex(self.skill, r"C and D require\s+an explicit selection|C and D require an explicit")
        self.assertRegex(self.skill, r"Converting an `\[OPEN\]` row|Without a pick: OPEN")
        self.assertIn("ratification", self.skill.lower())
        self.assertRegex(self.skill, r"assumes <X>|assumption behind")

    def test_functional_nature_rules(self):
        self.assertIn("The brief is functional", self.skill)
        self.assertIn("behavior and outcomes, not mechanisms", self.skill)
        self.assertRegex(self.skill, r"how instead of what|names\n  how")

    def test_handoff_names_controller_seam(self):
        self.assertIn("--brief-file", self.skill)
        self.assertNotIn("copy it into the job directory", self.skill)


class BriefTemplateTests(unittest.TestCase):
    def setUp(self):
        self.brief = (FACTORY / "assets" / "templates" / "brief.md").read_text()

    def test_required_sections(self):
        for section in ("## The ask", "## Problem", "## Outcome measures",
                        "## Non-goals", "## Priorities (MoSCoW)",
                        "## Known constraints", "## Decision ledger",
                        "## Delegated to analysis", "## Open", "## Owner ratification",
                        "## Owner suggestions", "## Handoff"):
            self.assertIn(section, self.brief, f"missing {section}")

    def test_authority_vocabulary(self):
        for term in ("HUMAN-DECIDED", "DELEGATED", "OPEN", "LED-1"):
            self.assertIn(term, self.brief)
        self.assertIn("undecided", self.brief, "OPEN must read as undecided")
        self.assertIn("material by default", self.brief)

    def test_open_blocks_analysis(self):
        self.assertRegex(self.brief, r"block analysis, full stop|blocks? analysis")

    def test_ask_is_verbatim_any_length(self):
        self.assertRegex(self.brief, r"verbatim, any length")
        self.assertRegex(self.brief, r"never shorten|never compressed")

    def test_unmeasurable_requires_signal(self):
        self.assertRegex(self.brief, r"UNMEASURABLE")
        self.assertRegex(self.brief, r"pass/fail signal")

    def test_suggestions_are_non_binding(self):
        self.assertIn("non-binding", self.brief)
        self.assertIn("NOT a requirement", self.brief)

    def test_consumption_and_supersession_contracts(self):
        self.assertIn("Consumption contract", self.brief)
        self.assertIn("Rework supersession", self.brief)
        self.assertIn("no inherited authority", self.brief)

    def test_owner_fixed_is_external_only(self):
        self.assertRegex(self.brief, r"externally imposed")
        self.assertRegex(self.brief, r"NOT constraints")

    def test_functional_banner(self):
        self.assertIn("Functional nature", self.brief)
        self.assertIn("owner-fixed", self.brief)

    def test_no_render_placeholders(self):
        # The brief is never rendered by the controller; machine-style
        # {{placeholders}} would leak into real briefs unresolved.
        self.assertIsNone(re.search(r"\{\{[a-z_][a-z0-9_-]*\}\}", self.brief),
                          "brief template must use <angle> placeholders only")

    def test_stays_lean(self):
        self.assertLessEqual(len(self.brief.splitlines()), 130)

    def test_scenarios_parse(self):
        data = json.loads((SKILL / "evals" / "scenarios.json").read_text())
        self.assertIn("judge_rules", data)
        ids = [s["id"] for s in data["scenarios"]]
        self.assertEqual(len(ids), len(set(ids)), "scenario ids unique")
        adversarial = {"owner-picks-a-not-recommendation", "owner-rejects-framing",
                       "owner-declines-ratification", "forging-attempt-owner-unavailable"}
        self.assertTrue(adversarial <= set(ids), "adversarial scenarios present")
        for s in data["scenarios"]:
            self.assertTrue(s["request"].strip())
            self.assertIn("owner_turns", s, f"{s['id']} lacks scripted owner turns")
            self.assertGreaterEqual(len(s["assertions"]), 3)
            for a in s["assertions"]:
                self.assertIn("evidence", a, f"{s['id']}: assertion without evidence rule")
                self.assertTrue(set(a["evidence"]) & {"transcript", "brief_field"},
                                f"{s['id']}: unknown evidence kind")


class WiringTests(unittest.TestCase):
    def test_controller_accepts_brief(self):
        src = (FACTORY / "scripts" / "factory.py").read_text()
        self.assertIn("--brief-file", src)
        self.assertIn('"brief"', src)
        self.assertIn("LED-", src, "controller checks ledger row coverage")
        self.assertIn("feature text verbatim", src, "controller checks brief frames the feature")

    def test_analysis_intent_binds_ledger(self):
        analysis = (FACTORY / "assets" / "templates" / "analysis.md").read_text()
        self.assertRegex(analysis, r"ledger is BINDING|BINDING")
        self.assertRegex(analysis, r"OPEN rows BLOCK|OPEN rows block")

    def test_factory_docs_name_the_seam(self):
        for path, needle in (
            (FACTORY / "SKILL.md", "--brief-file"),
            (FACTORY / "references" / "workflow.md", "--brief-file"),
            (FACTORY / "references" / "agents.md", "brief"),
            (FACTORY / "references" / "templates.md", "brief.md"),
        ):
            self.assertIn(needle, path.read_text(), f"{path.name} missing {needle}")


if __name__ == "__main__":
    unittest.main()
