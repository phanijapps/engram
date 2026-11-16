"""Read-only pre-build contracts shared by freeze, preflight and completion.

No board, evidence or Git writes. The controller supplies its canonical artifact
and readiness validators; semantic correctness still requires independent review.
"""
import hashlib
import json
import re
from types import SimpleNamespace


def validate(namespace, jobdir, job, stage, artifacts):
    api = SimpleNamespace(**namespace)
    require = api.require
    require(job.get("layout_version") == 3, "Unsupported job format: current factory requires layout_version 3")
    require(stage in {"analyze", "plan", "tasks"}, "Preflight supports pre-build stages only")
    require(job["stage"] == stage and job["status"] == "running", "Handoff does not match current stage")
    for item in job["artifacts"].values():
        api.intact(jobdir, item)
    if stage == "plan":
        return api.plan_document.validate(namespace, jobdir, job, artifacts)
    if stage == "tasks":
        return _tasks(api, jobdir, job, artifacts)
    require(artifacts == ["spec.md"], "Canonical analysis output must be spec.md")
    upstream = [key for key in ("feature", "brief", "rework") if key in job["artifacts"]]
    require("brief" in upstream, "Current jobs require an accepted brief")
    required = [job["artifacts"][key]["path"] for key in upstream]
    brief = api.inside(jobdir, job["artifacts"]["brief"]["path"]).read_text()
    for led in sorted(set(re.findall(r"LED-\d+", brief))):
        require(led in api.inside(jobdir, "spec.md").read_text(), "Analysis does not address brief ledger row " + led)
    fields = api.validate_artifact(jobdir, "spec.md", job, "spec", {"engram-analyzer"}, required, None)
    declared = fields.get("required_reviewers")
    require(isinstance(declared, list) and all(isinstance(r, str) for r in declared), "Analysis must declare required_reviewers")
    require(len(declared) == len(set(declared)) and api.MANDATORY <= set(declared) <= set(api.REVIEWERS), "Invalid analysis reviewer set")
    require(fields.get("mode") is None or fields["mode"] in api.MODES, "Invalid analysis mode declaration")
    return {"expected": ["spec"], "required": required, "fields": {"spec": fields}}


def _tasks(api, jobdir, job, artifacts):
    api.require(artifacts == ["tasks/manifest.json", "tasks/task.json", "release.md"],
                "v3 tasks handoff requires manifest, task index and release in order")
    manifest_path = api.inside(jobdir, "tasks/manifest.json")
    data = manifest_path.read_bytes()
    manifest = json.loads(data)
    edges = api.task_index.validate_manifest(jobdir, manifest)
    api.require(manifest["job_id"] == job["job_id"] and manifest["cycle"] == job["cycle"], "Wrong task manifest identity")
    index_path = api.inside(jobdir, "tasks/task.json")
    index = json.loads(index_path.read_text())
    api.require(index.get("manifest_sha256") == hashlib.sha256(data).hexdigest()
                and index.get("job_id") == job["job_id"] and index.get("cycle") == job["cycle"]
                and index.get("revision") == 0 and set(index.get("tasks", {})) == set(edges)
                and index.get("phases") == [{"id": p["id"], "task_ids": [t["id"] for t in p["tasks"]]} for p in manifest["phases"]]
                and all(row == {"status": "pending", "evidence": []} for row in index["tasks"].values()),
                "Task index is not the initial state for this manifest")
    required = [job["artifacts"][n]["path"] for n in ("spec", "plan")]
    required.extend(item["path"] for key, item in job["artifacts"].items()
                    if key in {"architecture", "data-models", "interfaces", "contracts"})
    api.validate_artifact(jobdir, "release.md", job, "release", {"engram-tasks"}, required, None)
    return {"manifest": manifest, "index_path": index_path}


def artifacts_for(namespace, jobdir, job, stage):
    api = SimpleNamespace(**namespace)
    if stage == "analyze":
        return ["spec.md"]
    if stage == "plan":
        fields, body = api.metadata(api.inside(jobdir, "plan.md"))
        authored, _ = api.plan_document.header(api.inside(jobdir, "plan.md").read_text())
        return ["plan.md"] + api.plan_document.derive(authored, body, api.inside(jobdir, "spec.md").read_text())["attachments"]
    api.require(stage == "tasks", "Unknown pre-build stage")
    return ["tasks/manifest.json", "tasks/task.json", "release.md"]
