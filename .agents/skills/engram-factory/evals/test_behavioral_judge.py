import json
import unittest
from pathlib import Path

from behavioral_judge import evaluate


class BehavioralJudgeTests(unittest.TestCase):
    def setUp(self):
        self.cases = json.loads(Path(__file__).with_name("behavioral_cases.json").read_text())
        self.case = self.cases["cases"][0]
        self.result = {"case_id": self.case["id"], "judgments": {
            key: {"verdict": "PASS", "reason": "reviewed", "evidence": [{"artifact": "a", "quote": "example evidence"}]}
            for key in self.case["required"]}}

    def test_complete_evidence_is_accepted_without_claiming_semantic_proof(self):
        report = evaluate(self.cases, self.result, {"a": "example evidence"})
        self.assertEqual(report["verdict"], "PASS")
        self.assertIn("not independently verified", report["semantic_review"])

    def test_missing_and_fabricated_evidence_fail(self):
        self.assertEqual(evaluate(self.cases, self.result, {"a": "different text"})["verdict"], "FAIL")
        del self.result["judgments"][self.case["required"][0]]
        self.assertEqual(evaluate(self.cases, self.result, {"a": "example evidence"})["verdict"], "FAIL")

    def test_unrun_or_failed_judgment_fails(self):
        for verdict in ("NOT_RUN", "FAIL"):
            self.result["judgments"][self.case["required"][0]]["verdict"] = verdict
            self.assertEqual(evaluate(self.cases, self.result, {"a": "example evidence"})["verdict"], "FAIL")

    def test_autonomy_case_cannot_pass_with_unrun_or_missing_evidence(self):
        case = next(item for item in self.cases["cases"] if item["id"] == "blocked-fallback-authorized")
        result = {"case_id": case["id"], "judgments": {
            key: {"verdict": "PASS", "reason": "reviewed", "evidence": [{"artifact": "transcript", "quote": "actual trace"}]}
            for key in case["required"]}}
        self.assertEqual(evaluate(self.cases, result, {"transcript": "actual trace"})["verdict"], "PASS")
        result["judgments"]["FALLBACK"]["verdict"] = "NOT_RUN"
        self.assertEqual(evaluate(self.cases, result, {"transcript": "actual trace"})["verdict"], "FAIL")
        result["judgments"]["FALLBACK"]["verdict"] = "PASS"
        self.assertEqual(evaluate(self.cases, result, {"transcript": "different trace"})["verdict"], "FAIL")

    def test_fallback_fixtures_distinguish_actual_owner_authority(self):
        cases = {item["id"]: item for item in self.cases["cases"]}
        for name in ("blocked-fallback-unauthorized", "blocked-fallback-authorized"):
            self.assertTrue({"AUTONOMY", "FALLBACK", "GATE_INTEGRITY"} <= set(cases[name]["required"]))
        self.assertIn("no owner instruction", cases["blocked-fallback-unauthorized"]["context"])
        self.assertIn("explicitly authorized", cases["blocked-fallback-authorized"]["context"])

    def test_cases_cover_different_shapes_and_unique_ids(self):
        ids = [case["id"] for case in self.cases["cases"]]
        self.assertEqual(len(ids), len(set(ids)))
        self.assertGreaterEqual(len(ids), 6)
        for case in self.cases["cases"]:
            self.assertTrue(set(case["critical"]) <= set(case["required"]))
            self.assertTrue(set(case["required"]) <= set(self.cases["rubric"]))


if __name__ == "__main__":
    unittest.main()
