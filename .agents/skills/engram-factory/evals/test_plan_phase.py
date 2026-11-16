"""Plan-phase evals: one design, generated identity, complete proof and exact review."""
import importlib
import json
import unittest
from pathlib import Path
import test_factory
import test_spec_phase
import test_current_workflow

PLAN = '''---
schema_version: 2
plan_format: "compact-v1"
size_tier: "S"
---

# Plan: make missing input visible

## Owner summary
Prove the current validation boundary first, then add a named error for missing inputs. Existing valid inputs retain their behavior.

## Approach
Change the existing validation seam in app.txt's fixture rather than replacing the export workflow. This keeps scope bounded and rejects a wholesale rewrite. The accepted spec is the behavior authority; the proof below distinguishes missing and valid paths.

## Steps
| ID | Action | Requirements | Depends on | Owned paths | Proof |
| --- | --- | --- | --- | --- | --- |
| STEP-001 | Add missing and valid input fixtures | REQ-001 | none | tests/test_inputs.py | GATE-001 |
| STEP-002 | Return the missing input name through the existing boundary | REQ-001 | STEP-001 | app.txt | GATE-001 |

## Proof
| ID | Acceptance | Command or protocol | Expected |
| --- | --- | --- | --- |
| GATE-001 | AC-001, AC-002 | python3 -m unittest discover -s tests | Missing-input case fails before fix; both cases pass afterward |

## Rollback and risks
Revert the behavior change on unexpected valid-input failures; no data migration or live deployment occurs. Revert the fixture separately if necessary. Risk: an overbroad missing-input check rejects valid paths, caught by AC-002.

## Decisions and evidence
LED-1 is preserved. The accepted spec and app.txt at the recorded base define the scope. No new authority or platform action is requested.
'''


class PlanContentTests(unittest.TestCase):
    def setUp(self):
        self.module = importlib.import_module('plan_document')

    def parse(self, text=PLAN):
        _, header, body = text.split('---\n', 2)
        fields = {line.split(':', 1)[0]: json.loads(line.split(':', 1)[1]) for line in header.splitlines()}
        return self.module.derive(fields, body, test_spec_phase.SINGLE)

    def test_single_plan_covers_acceptance_without_repeating_spec(self):
        result = self.parse()
        self.assertEqual(['STEP-001', 'STEP-002'], result['steps'])
        self.assertEqual([], result['attachments'])
        self.assertNotIn('sha256', PLAN)

    def test_trace_dag_and_ownership_errors_fail(self):
        for text in [PLAN.replace('REQ-001 | STEP-001', 'REQ-999 | STEP-001'),
                     PLAN.replace('AC-001, AC-002', 'AC-001'),
                     PLAN.replace('| none |', '| STEP-002 |'),
                     PLAN.replace('| STEP-001 | app.txt', '| STEP-999 | app.txt'),
                     PLAN.replace('| app.txt |', '| ../outside |'),
                     PLAN.replace('| app.txt |', '| - |'),
                     PLAN.replace('| app.txt |', '| a//b |'),
                     PLAN.replace('| app.txt |', '| C:escape |'),
                     PLAN.replace('| GATE-001 |\n', '| GATE-999 |\n'),
                     PLAN.replace('| STEP-002 | Return', '| STEP-001 | Return'),
                     PLAN.replace('## Rollback and risks', '## Notes')]:
            with self.subTest(text=text[-80:]), self.assertRaises(ValueError):
                self.parse(text)

    def test_optional_support_needs_distinct_purpose(self):
        text = PLAN + '\n## Supporting documents\n| Path | Purpose |\n| --- | --- |\n| context/interfaces.md | Reusable adapter contract for two host teams |\n'
        self.assertEqual(['context/interfaces.md'], self.parse(text)['attachments'])
        with self.assertRaises(ValueError):
            self.parse(text.replace('Reusable adapter contract for two host teams', '-'))
        with self.assertRaises(ValueError):
            self.parse(text.replace('context/interfaces.md', '../outside.md'))

    def test_incidental_spec_mentions_do_not_expand_scope_or_disable_links(self):
        _, header, body = PLAN.split('---\n', 2)
        fields = {line.split(':', 1)[0]: json.loads(line.split(':', 1)[1]) for line in header.splitlines()}
        result = self.module.derive(fields, body, test_spec_phase.SINGLE + '\nAC-999/REQ-999 are historical excluded references.\n')
        self.assertEqual(2, len(result['steps']))

    def test_shared_regression_gate_is_allowed_but_swapped_proofs_are_not(self):
        spec = test_spec_phase.SINGLE.replace('| REQ-001 | When', '| REQ-002 | Preserve existing behavior | EN-004 |\n| REQ-001 | When')
        spec = spec.replace('| AC-002 | REQ-001 |', '| AC-002 | REQ-002 |')
        shared = PLAN.replace('REQ-001 | STEP-001', 'REQ-002 | STEP-001')
        _, header, body = shared.split('---\n', 2)
        fields = {line.split(':', 1)[0]: json.loads(line.split(':', 1)[1]) for line in header.splitlines()}
        self.module.derive(fields, body, spec)
        swapped = body.replace('GATE-001 | AC-001, AC-002', 'GATE-001 | AC-002').replace('| STEP-001 | Add missing and valid input fixtures | REQ-001 | none | tests/test_inputs.py | GATE-001 |', '| STEP-001 | Add missing and valid input fixtures | REQ-001 | none | tests/test_inputs.py | GATE-002 |')
        swapped += '\n'  # Insert second gate inside the canonical proof table.
        swapped = swapped.replace('## Rollback and risks', '| GATE-002 | AC-002 | python3 -m unittest | Both pass as designed |\n\n## Rollback and risks')
        with self.assertRaisesRegex(ValueError, 'Step requirement/proof acceptance mismatch'):
            self.module.derive(fields, swapped, spec)

    def test_template_markers_fail_but_legitimate_angle_brackets_pass(self):
        self.parse(PLAN.replace('bounded', 'bounded List<T> and 0 < count and count > 2'))
        raw = (test_factory.SKILL / 'assets/templates/plan.md').read_text()
        with self.assertRaises(ValueError):
            self.parse(raw)
        self.assertLessEqual(len(raw.splitlines()), 35)
        for noise in ['sha256', 'input_artifacts', 'mandatory', '<!--', 'Worked example']:
            self.assertNotIn(noise, raw)



class PlanControllerTests(unittest.TestCase):
    setUp = test_factory.FactoryTests.setUp
    git = staticmethod(test_factory.FactoryTests.git)
    run_cli = test_factory.FactoryTests.run_cli
    job = test_factory.FactoryTests.job
    fill = test_factory.FactoryTests.fill
    setUpCurrent = test_current_workflow.CurrentWorkflowTests.setUpCurrent

    def start_plan(self):
        self.setUpCurrent()
        (self.jobdir / 'spec.md').write_text(test_spec_phase.SINGLE)
        self.run_cli('spec-prepare', '--job', self.job_id)
        self.review('spec')
        self.run_cli('complete', '--job', self.job_id, '--stage', 'analyze', '--artifacts', 'spec.md')
        (self.jobdir / 'plan.md').write_text(PLAN)

    def review(self, stage):
        self.run_cli('review-freeze', '--job', self.job_id, '--stage', stage, '--author', 'author-1',
                     '--reviewers-file', 'records/operations/reviewers.json')
        self.run_cli('review-record', '--job', self.job_id, '--stage', stage, '--candidate', '1',
                     '--role', 'red-forman', '--invocation', 'reviewer-1', '--verdict', 'PASS',
                     '--findings-file', 'records/operations/findings.json')

    def test_prepare_review_and_handoff_without_context_files(self):
        self.start_plan()
        authored = (self.jobdir / 'plan.md').read_bytes()
        self.run_cli('plan-prepare', '--job', self.job_id)
        receipt = self.jobdir / 'records/plan/receipt.json'
        first = receipt.read_bytes()
        self.run_cli('plan-prepare', '--job', self.job_id)
        self.assertEqual(first, receipt.read_bytes())
        self.assertEqual(authored, (self.jobdir / 'plan.md').read_bytes())
        self.run_cli('preflight', '--job', self.job_id, '--stage', 'plan')
        self.review('plan')
        self.run_cli('complete', '--job', self.job_id, '--stage', 'plan', '--artifacts', 'plan.md')
        self.assertEqual('tasks', self.job()['stage'])
        self.assertIn('plan-receipt', self.job()['artifacts'])
        self.assertIsNone(self.job()['approval'])
        self.run_cli('start-build', '--job', self.job_id, '--actor', 'owner', '--instruction-file', str(self.feature),
                     '--decision', 'accept-and-start', '--plan-sha256', self.job()['artifacts']['plan']['sha256'], fail=True)

    def test_plain_supporting_contract_is_hash_bound_without_frontmatter(self):
        self.start_plan()
        path = self.jobdir / 'context/interfaces.md'
        path.write_text('# Adapter contract\n\nOne implementation serves two hosts. Preserve request/response behavior and identity.\n')
        plan = self.jobdir / 'plan.md'
        plan.write_text(PLAN + '\n## Supporting documents\n| Path | Purpose |\n| --- | --- |\n| context/interfaces.md | Independently maintained adapter contract across hosts |\n')
        self.run_cli('plan-prepare', '--job', self.job_id)
        self.run_cli('preflight', '--job', self.job_id, '--stage', 'plan')
        self.review('plan')
        original = path.read_bytes()
        path.write_text(path.read_text() + '\nChanged after review.\n')
        self.run_cli('complete', '--job', self.job_id, '--stage', 'plan', '--artifacts', 'plan.md', 'context/interfaces.md', fail=True)
        path.write_bytes(original)
        self.run_cli('complete', '--job', self.job_id, '--stage', 'plan', '--artifacts', 'plan.md', 'context/interfaces.md')

    def test_stale_tampered_and_malformed_receipts_fail(self):
        self.start_plan()
        self.run_cli('plan-prepare', '--job', self.job_id)
        receipt = self.jobdir / 'records/plan/receipt.json'
        original = receipt.read_bytes()
        for value in [[], None, {}, {**json.loads(original), 'base_revision': '0' * 40}]:
            receipt.write_text(json.dumps(value))
            self.run_cli('preflight', '--job', self.job_id, '--stage', 'plan', fail=True)
        receipt.write_bytes(original)
        plan = self.jobdir / 'plan.md'
        plan.write_text(plan.read_text() + '\nClarification.\n')
        self.run_cli('preflight', '--job', self.job_id, '--stage', 'plan', fail=True)
        self.run_cli('plan-prepare', '--job', self.job_id)
        self.review('plan')
        plan.write_text(plan.read_text() + '\nChanged after PASS.\n')
        self.run_cli('plan-prepare', '--job', self.job_id, fail=True)

    def test_unresolved_support_cannot_prepare(self):
        self.start_plan()
        path = self.jobdir / 'context/interfaces.md'
        path.write_text('# Contract\n' + 'Unresolved {{job_id}} behavior. ' * 8)
        plan = self.jobdir / 'plan.md'
        plan.write_text(PLAN + '\n## Supporting documents\n| Path | Purpose |\n| --- | --- |\n| context/interfaces.md | Independently maintained shared host adapter contract |\n')
        self.run_cli('plan-prepare', '--job', self.job_id, fail=True)
        self.assertFalse((self.jobdir / 'records/plan/receipt.json').exists())

    def test_receipt_override_and_source_input_drift_cannot_inherit_review(self):
        self.start_plan()
        self.run_cli('plan-prepare', '--job', self.job_id)
        path = self.jobdir / 'records/plan/receipt.json'
        original = path.read_bytes()
        data = json.loads(original)
        data['fields']['plan_format'] = 'legacy'
        path.write_text(json.dumps(data))
        self.run_cli('preflight', '--job', self.job_id, '--stage', 'plan', fail=True)
        path.write_bytes(original)
        spec = self.jobdir / 'spec.md'
        spec.write_text(spec.read_text() + '\nChanged accepted source.\n')
        self.run_cli('preflight', '--job', self.job_id, '--stage', 'plan', fail=True)
        self.run_cli('plan-prepare', '--job', self.job_id, fail=True)

    def test_failed_preflight_is_read_only_and_does_not_consume_candidate(self):
        self.start_plan()
        self.run_cli('plan-prepare', '--job', self.job_id)
        plan = self.jobdir / 'plan.md'
        plan.write_text(plan.read_text().replace('AC-001, AC-002', 'AC-001'))
        before = {str(p): p.read_bytes() for p in (self.repo / '.engram-factory').rglob('*') if p.is_file()}
        self.run_cli('preflight', '--job', self.job_id, '--stage', 'plan', fail=True)
        after = {str(p): p.read_bytes() for p in (self.repo / '.engram-factory').rglob('*') if p.is_file()}
        self.assertEqual(before, after)
        self.assertFalse((self.jobdir / 'reviews/prebuild/plan').exists())


    def test_changes_prepares_candidate_two_without_reset(self):
        self.start_plan()
        self.run_cli('plan-prepare', '--job', self.job_id)
        self.run_cli('review-freeze', '--job', self.job_id, '--stage', 'plan', '--author', 'author-1',
                     '--reviewers-file', 'records/operations/reviewers.json')
        findings = self.jobdir / 'records/operations/findings.json'
        findings.write_text(json.dumps({'findings': {'F-1': 'Clarify recovery'}, 'resolves': []}))
        self.run_cli('review-record', '--job', self.job_id, '--stage', 'plan', '--candidate', '1',
                     '--role', 'red-forman', '--invocation', 'reviewer-1', '--verdict', 'CHANGES',
                     '--findings-file', 'records/operations/findings.json')
        path = self.jobdir / 'plan.md'
        path.write_text(path.read_text() + '\nRecovery clarification for F-1.\n')
        self.run_cli('plan-prepare', '--job', self.job_id)
        result = self.run_cli('review-freeze', '--job', self.job_id, '--stage', 'plan', '--author', 'author-1',
                              '--reviewers-file', 'records/operations/reviewers.json')['result']
        self.assertEqual(2, result['number'])
        self.assertIn('records/plan/receipt.json', [item['path'] for item in result['artifacts']])

    def test_noncompact_plan_is_rejected_without_rewriting(self):
        self.start_plan()
        path = self.jobdir / 'plan.md'
        path.write_text('---\nschema_version: "1.0"\nartifact_type: "plan"\n---\n\n# Old plan\n')
        original = path.read_bytes()
        self.run_cli('plan-prepare', '--job', self.job_id, fail=True)
        self.run_cli('preflight', '--job', self.job_id, '--stage', 'plan', fail=True)
        self.assertEqual(original, path.read_bytes())

    def test_plain_support_contract_flows_into_task_preflight(self):
        self.test_plain_supporting_contract_is_hash_bound_without_frontmatter()
        task = self.jobdir / 'tasks/task-001.md'
        task.write_text('# Deliver input validation\n\nImplement the accepted steps and prove both input paths without scope changes.\n')
        manifest = {'schema_version': 1, 'job_id': self.job_id, 'cycle': 1, 'phases': [{'id': 'PHASE-1', 'tasks': [
            {'id': 'TASK-001', 'document': 'tasks/task-001.md', 'sha256': test_factory.factory.digest(task), 'depends_on': []}]}]}
        (self.jobdir / 'records/operations/candidate.json').write_text(json.dumps(manifest))
        self.run_cli('tasks-initialize', '--job', self.job_id, '--manifest', 'records/operations/candidate.json')
        self.fill('release', 'engram-tasks', ['spec.md', 'plan.md', 'context/interfaces.md'])
        self.run_cli('preflight', '--job', self.job_id, '--stage', 'tasks')



if __name__ == '__main__':
    unittest.main()
