"""Validate evidence-backed human/independent-agent behavioral evaluations.

This does not run an LLM or determine whether an excerpt supports a claim.
The reviewer must make that semantic judgment independently. See behavioral.md.
"""
import argparse
import json
from pathlib import Path

CASES = Path(__file__).with_name("behavioral_cases.json")


def evaluate(cases, result, artifacts):
    by_id = {case["id"]: case for case in cases["cases"]}
    case = by_id.get(result.get("case_id"))
    if case is None:
        raise ValueError("unknown case_id")
    judgments = result.get("judgments", {})
    errors = []
    for key in case["required"]:
        item = judgments.get(key)
        if not isinstance(item, dict) or item.get("verdict") not in ("PASS", "FAIL", "NOT_RUN"):
            errors.append(f"{key}: missing valid verdict")
            continue
        if item["verdict"] == "NOT_RUN":
            errors.append(f"{key}: NOT_RUN")
            continue
        evidence = item.get("evidence", [])
        if not evidence:
            errors.append(f"{key}: missing evidence")
            continue
        for ref in evidence:
            name, quote = ref.get("artifact"), ref.get("quote", "")
            if name not in artifacts or not quote.strip() or quote not in artifacts[name]:
                errors.append(f"{key}: evidence quote not found in supplied artifact {name!r}")
        if not str(item.get("reason", "")).strip():
            errors.append(f"{key}: missing reviewer reasoning")
        if item["verdict"] == "FAIL":
            errors.append(f"{key}: reviewer marked FAIL")
    extras = set(judgments) - set(case["required"])
    if extras:
        errors.append("unexpected judgments: " + ", ".join(sorted(extras)))
    return {"case_id": case["id"], "verdict": "FAIL" if errors else "PASS", "errors": errors,
            "semantic_review": "Reviewer verdicts are not independently verified by this program."}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("result", type=Path, help="reviewer JSON with case_id, artifacts and judgments")
    args = parser.parse_args()
    result = json.loads(args.result.read_text())
    base = args.result.resolve().parent
    artifacts = {name: (base / path).read_text() for name, path in result.get("artifacts", {}).items()}
    report = evaluate(json.loads(CASES.read_text()), result, artifacts)
    print(json.dumps(report, indent=2))
    return 0 if report["verdict"] == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())
