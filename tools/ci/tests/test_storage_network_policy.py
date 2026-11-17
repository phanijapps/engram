import json
import os
from pathlib import Path
import sys
import tempfile
import unittest
import tomllib
import uuid
from contextlib import contextmanager
import base64

from tools.ci import check_storage_network_policy as policy

CONNECT = '''import socket
s=socket.socket()
try: s.connect(("127.0.0.1", 9))
except OSError: pass
s.close()
'''
DNS = '''import socket
s=socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
try: s.sendto(b"\\x00\\x01\\x01\\x00\\x00\\x01\\x00\\x00\\x00\\x00\\x00\\x00\\x01x\\x07invalid\\x00\\x00\\x01\\x00\\x01", ("127.0.0.1", 53))
except OSError: pass
s.close()
'''


@contextmanager
def controlled_directory():
    with tempfile.TemporaryDirectory(dir=os.environ.get("TMPDIR")) as directory:
        try:
            yield directory
        finally:
            destination = os.environ.get("ENGRAM_NETWORK_PROOF_LOG")
            if destination:
                root = Path(directory)
                files = [p for p in root.rglob("*") if p.is_file() and
                         (p.name in {"network.trace", "preferences.before", "preferences.after", "command.stdout", "command.stderr"} or p.name.startswith("model-"))]
                with Path(destination).open("a") as evidence:
                    evidence.write(json.dumps({"raw_control_files": {
                        str(p.relative_to(root)): base64.b64encode(p.read_bytes()).decode()
                        for p in sorted(files)}}) + "\n")


class NetworkPolicyTests(unittest.TestCase):
    def test_actual_denied_connect_and_dns_attempts_are_detected(self):
        with controlled_directory() as directory:
            root = Path(directory)
            for name, code in (("connect", CONNECT), ("dns", DNS)):
                with self.subTest(attempt=name):
                    observation = policy.trace_command([sys.executable, "-c", code], root / name,
                                                        deny_attempts=True)
                    self.assertIn("EACCES", observation.trace.read_text())
                    self.assertTrue(observation.attempts, f"denied {name} attempt must be observed")

    def test_model_resolver_and_initializer_positive_controls(self):
        with controlled_directory() as directory:
            root = Path(directory)
            for name in ("model-credential-resolver", "model-initializer"):
                events = root / (name + ".json")
                code = f"import json,pathlib;pathlib.Path({str(events)!r}).write_text(json.dumps([{name!r}]))"
                observation = policy.trace_command([sys.executable, "-c", code], root / name)
                with self.assertRaisesRegex(policy.ObservationError, "resolver/initializer"):
                    policy.assert_offline(observation, events)

    def test_saved_and_environment_metrics_preserve_preference_bytes(self):
        with controlled_directory() as directory:
            for variant in policy.AMBIENT_METRICS:
                with self.subTest(variant=variant):
                    root = Path(directory) / variant
                    with policy.telemetry_fixture(root, variant) as (env, preferences):
                        saved = tomllib.loads(preferences.read_text())
                        self.assertEqual(str(uuid.UUID(saved["installation_id"])), saved["installation_id"])
                        self.assertTrue(saved["user_id"].strip())
                        self.assertEqual(saved["level"], variant[6:] if variant.startswith("saved-") else "off")
                        events = root / "model-events.json"
                        events.write_text("[]")
                        observation = policy.trace_command([sys.executable, "-c", "pass"], root / "trace", env=env)
                        policy.assert_offline(observation, events)
                        self.assertEqual(env.get("HELIX_TELEMETRY_LEVEL"), variant[4:] if variant.startswith("env-") else None)
                    self.assertEqual((root / "preferences.before").read_bytes(), (root / "preferences.after").read_bytes())

    def test_trace_failures_never_mean_zero_attempts(self):
        with controlled_directory() as directory:
            root = Path(directory)
            with self.assertRaises(policy.ObservationError):
                policy.trace_command([sys.executable, "-c", "pass"], root, tracer="no-such-tracer")
            with self.assertRaises(policy.ObservationError):
                policy.trace_command([sys.executable, "-c", "pass"], root / "failed-tracer", tracer="false")
            with self.assertRaises(policy.ObservationError):
                policy.trace_command([sys.executable, "-c", "raise SystemExit(1)"], root / "failed-command")
            trace = root / "trace"
            for content, code in (("", 0), ("123 <... connect resumed>) = -1 EACCES (Permission denied)\n123 +++ exited with 0 +++", 0), ("123 connect(3, <unfinished ...>\n123 +++ exited with 0 +++", 0), ("garbage\n+++ exited with 0 +++", 0), ("+++ exited with 0 +++", 1), ("strace: denied\n+++ exited with 0 +++", 0)):
                trace.write_text(content)
                with self.assertRaises(policy.ObservationError):
                    policy.read_observation(trace, code)
            trace.unlink()
            with self.assertRaises(policy.ObservationError):
                policy.read_observation(trace, 0)


    def test_each_pid_and_syscall_record_requires_complete_observation(self):
        with controlled_directory() as directory:
            root = Path(directory)
            trace = root / "trace"
            invalid = [
                "123 socket(AF_INET, SOCK_STREAM, IPPROTO_IP) = 3\n124 +++ exited with 0 +++\n",
                "123 socket(\n123 +++ exited with 0 +++\n",
                "123 gibberish() = 0\n123 +++ exited with 0 +++\n",
                "123 +++ exited with 0 +++ trailing\n",
                "123 +++ exited with 0 +++\n123 socket(AF_INET, SOCK_STREAM, IPPROTO_IP) = 3\n",
            ]
            for content in invalid:
                trace.write_text(content)
                with self.subTest(trace=content), self.assertRaises(policy.ObservationError):
                    policy.read_observation(trace, 0)
            code = "import subprocess,sys;subprocess.run([sys.executable,'-c','pass'],check=True)"
            observed = policy.trace_command([sys.executable, "-c", code], root / "multiprocess")
            self.assertFalse(observed.attempts)
            self.assertGreaterEqual(observed.trace.read_text().count("+++ exited with 0 +++"), 2)

    def test_changed_preferences_and_missing_probe_fail_closed(self):
        with controlled_directory() as directory:
            root = Path(directory)
            with self.assertRaisesRegex(policy.ObservationError, "preferences changed"):
                with policy.telemetry_fixture(root, "saved-full") as (_, preferences):
                    preferences.write_text("changed")
            observation = policy.trace_command([sys.executable, "-c", "pass"], root / "trace")
            with self.assertRaisesRegex(policy.ObservationError, "probe is missing"):
                policy.assert_offline(observation, root / "missing-probe")


class AmendmentRecorderTests(unittest.TestCase):
    def test_provenance_rejects_unapplied_rule_and_never_emits_during_preparation(self):
        import subprocess
        from tools.dev import record_constitution_amendment as recorder

        with controlled_directory() as directory:
            repo = Path(directory)
            subprocess.run(["git", "init", "-q"], cwd=repo, check=True)
            rule = repo / recorder.RULE
            rule.parent.mkdir(parents=True)
            rule.write_text("Current strict offline rule.\n")
            registry = repo / recorder.REGISTRY
            registry.parent.mkdir(parents=True)
            registry.write_text(json.dumps({"families": [{"family": "constitution.amended", "fields": ["rail", "rule_ids", "supersession"]}]}))
            def commit():
                subprocess.run(["git", "add", "."], cwd=repo, check=True, capture_output=True)
                subprocess.run(["git", "-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid", "commit", "-qm", "fixture"], cwd=repo, check=True, capture_output=True)
                return subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=repo, text=True).strip()
            base = commit()
            job = repo / ".engram-factory/jobs/job-fixture"
            evidence = job / "runs/constitution"
            evidence.mkdir(parents=True)
            proposal = "## Exact proposed replacement normative text\n" + "\n".join(f"> Sentence {i}." for i in range(5)) + "\n## Scope definitions and limits\nComplete scope.\n## Observable boundary\n"
            (evidence / "PLT-001-amendment.md").write_text(proposal)
            ratification = "# Owner ratification:\n" + recorder.digest(proposal.encode()) + "\n" + recorder.digest(rule.read_bytes()) + "\n" + base
            receipt = evidence / "PLT-001-ratification.md"
            receipt.write_text(ratification)
            review = evidence / "PLT-001-amendment-review.md"
            review.write_text(ratification.replace("# Owner ratification:", "# Independent review:"))
            with self.assertRaisesRegex(ValueError, "unapplied"):
                recorder.prepare_event(repo, job, base_revision=base, applied_revision=base, actor="fixture-owner")
            rule.write_text("\n".join(f"Sentence {i}." for i in range(5)) + "\nComplete scope.\n")
            applied = commit()
            event = recorder.prepare_event(repo, job, base_revision=base, applied_revision=applied, actor="fixture-owner")
            self.assertEqual(event["source_revision"], applied)
            self.assertEqual(event["artifact_hashes"]["applied_rule"], recorder.digest(rule.read_bytes()))
            self.assertEqual(event["artifact_hashes"]["draft_review"], recorder.digest(review.read_bytes()))
            self.assertFalse((job / "events.jsonl").exists())
            receipt.write_text("unratified draft")
            with self.assertRaisesRegex(ValueError, "ratification"):
                recorder.prepare_event(repo, job, base_revision=base, applied_revision=applied, actor="fixture-owner")
            receipt.write_text(ratification)
            registry.write_text('{"families": []}')
            with self.assertRaisesRegex(ValueError, "not registered"):
                recorder.prepare_event(repo, job, base_revision=base, applied_revision=applied, actor="fixture-owner")

    def test_evidence_paths_reject_traversal_and_symlinks(self):
        from tools.dev import record_constitution_amendment as recorder
        with controlled_directory() as directory:
            root = Path(directory)
            for path in ("../escape", "/absolute"):
                with self.assertRaises(ValueError):
                    recorder.safe_path(root, path)
            (root / "events.jsonl").symlink_to(root / "outside")
            with self.assertRaises(ValueError):
                recorder.safe_path(root, "events.jsonl")
