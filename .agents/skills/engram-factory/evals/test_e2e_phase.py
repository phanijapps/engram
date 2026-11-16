"""Optional E2E routing and evidence gates through disposable controller jobs."""
import json
from pathlib import Path
import unittest
from unittest import mock
import test_factory
import test_plan_phase
import test_current_workflow

E2E_TABLE = '''
## E2E
| ID | Acceptance | Command or protocol | Expected | Environment prerequisites |
| --- | --- | --- | --- | --- |
| E2E-001 | AC-001, AC-002 | python3 check_journey.py | Missing input rejected; valid input preserved | Disposable fixture service with isolated data |
'''


class E2ETests(unittest.TestCase):
    setUp = test_factory.FactoryTests.setUp
    git = staticmethod(test_factory.FactoryTests.git)
    run_cli = test_factory.FactoryTests.run_cli
    job = test_factory.FactoryTests.job
    fill = test_factory.FactoryTests.fill
    setUpCurrent = test_current_workflow.CurrentWorkflowTests.setUpCurrent
    start_plan = test_plan_phase.PlanControllerTests.start_plan
    review = test_plan_phase.PlanControllerTests.review

    def build(self, enabled=True):
        self.start_plan()
        if enabled:
            p = self.jobdir / 'plan.md'
            p.write_text(p.read_text().replace('size_tier: "S"', 'size_tier: "S"\ne2e: true') + E2E_TABLE)
        self.run_cli('plan-prepare', '--job', self.job_id)
        self.review('plan')
        self.run_cli('complete', '--job', self.job_id, '--stage', 'plan', '--artifacts', 'plan.md')
        task = self.jobdir / 'tasks/task-001.md'
        task.write_text('# Implement the bounded fixture\n\nVerify its unit contract before handing the candidate to independent E2E execution.\n')
        manifest = {'schema_version': 1, 'job_id': self.job_id, 'cycle': 1, 'phases': [
            {'id': 'PHASE-1', 'tasks': [{'id': 'TASK-001', 'document': 'tasks/task-001.md',
                                       'sha256': test_factory.factory.digest(task), 'depends_on': []}]}]}
        (self.jobdir / 'records/operations/candidate.json').write_text(json.dumps(manifest))
        self.run_cli('tasks-initialize', '--job', self.job_id, '--manifest', 'records/operations/candidate.json')
        self.fill('release', 'engram-tasks', ['spec.md', 'plan.md'])
        self.run_cli('review-freeze', '--job', self.job_id, '--stage', 'tasks', '--phase', 'PHASE-1',
                     '--author', 'task-author', '--reviewers-file', 'records/operations/reviewers.json')
        self.run_cli('review-record', '--job', self.job_id, '--stage', 'tasks', '--phase', 'PHASE-1',
                     '--candidate', '1', '--role', 'red-forman', '--invocation', 'reviewer-1', '--verdict', 'PASS',
                     '--findings-file', 'records/operations/findings.json')
        self.run_cli('complete', '--job', self.job_id, '--stage', 'tasks', '--artifacts',
                     'tasks/manifest.json', 'tasks/task.json', 'release.md')
        instruction = self.root / 'start.txt'; instruction.write_text('Accept the exact reviewed plan and start implementation.')
        self.run_cli('start-build', '--job', self.job_id, '--actor', 'owner', '--instruction-file', str(instruction),
                     '--decision', 'accept-and-start', '--plan-sha256', self.job()['artifacts']['plan']['sha256'])
        self.run_cli('task-transition', '--job', self.job_id, '--task', 'TASK-001', '--to', 'in_progress',
                     '--task-revision', '0', '--actor', 'builder-1', '--reason', 'implement bounded change')
        self.run_cli('task-transition', '--job', self.job_id, '--task', 'TASK-001', '--to', 'done',
                     '--task-revision', '1', '--actor', 'builder-1', '--reason', 'focused contract passed',
                     '--evidence', 'tasks/task-001.md')
        self.finish_build()

    def finish_build(self):
        job = self.job()
        self.git(Path(job['worktree']), 'commit', '--allow-empty', '-m', 'fixture candidate correction')
        env = self.jobdir / f'environment-{len(job["history"])}.json'
        env.write_text(json.dumps({'environment': 'disposable-test', 'product': 'fixture', 'version': '1',
                                   'observed_at': test_factory.factory.now(), 'baseline': 'fixture-1'}))
        self.run_cli('snapshot', '--job', self.job_id, '--environment', str(env))
        job = self.job(); snap = job['snapshot']['path']
        suffix = f'-a{job["e2e"]["attempts"] + 1}' if 'e2e' in job else ''
        path = f'records/operations/build-handoff-c1-r{job["round"] + 1}{suffix}.md'
        self.fill('build', 'engram-builder', [x['path'] for x in job['artifacts'].values()] + ['tasks/task.json', snap],
                  snapshot=snap, path=path, invocation_id='builder-1')
        self.run_cli('complete', '--job', self.job_id, '--stage', 'build', '--snapshot', snap,
                     '--reviewers', 'code,security,platform', '--artifacts', path)

    def report(self, verdict='PASS', **changes):
        job = self.job(); state = job['e2e']; attempt = state['attempts'] + 1
        snap = json.loads((self.jobdir / job['snapshot']['path']).read_text())
        log = self.jobdir / f'records/e2e/run-a{attempt}.log'; log.parent.mkdir(parents=True, exist_ok=True)
        log.write_text('Disposable workflow command returned its observed fixture result.\n')
        data = {'schema_version': 1, 'job_id': self.job_id, 'cycle': 1, 'attempt': attempt,
                'author_role': 'engram-qa', 'invocation_id': 'qa-1',
                'source_snapshot': {k: job['snapshot'][k] for k in ('path', 'sha256')}, 'plan': state['plan'],
                'head_sha': snap['head_sha'], 'environment_sha256': snap['environment_sha256'],
                'observed_at': test_factory.factory.now(), 'scenarios': [
                    {'id': 'E2E-001', 'verdict': verdict, 'method': 'command', 'command': 'python3 check_journey.py',
                     'exit_code': 0 if verdict == 'PASS' else 1 if verdict == 'FAIL' else None,
                     'observed': 'Actual fixture observation or unavailable prerequisite',
                     'evidence': [test_factory.factory.ref(self.jobdir, log.relative_to(self.jobdir).as_posix())]}]}
        data.update(changes)
        path = f'records/e2e/report-c1-a{attempt}.json'
        (self.jobdir / path).write_text(json.dumps(data))
        return path, data

    def record(self, verdict='PASS', fail=False, **changes):
        path, _ = self.report(verdict, **changes)
        return self.run_cli('complete', '--job', self.job_id, '--stage', 'e2e', '--artifacts', path, fail=fail)

    def test_optout_uses_existing_build_review_route(self):
        self.build(False)
        self.assertEqual('review', self.job()['stage'])
        self.assertNotIn('e2e', self.job())
        self.assertEqual(1, self.job()['round'])

    def test_optin_independent_pass_starts_same_candidate_review(self):
        self.build()
        job = self.job(); self.assertEqual(('e2e', 0), (job['stage'], job['round']))
        self.run_cli('decision', '--job', self.job_id, '--artifact', 'missing.md', fail=True)
        self.record()
        final = self.job()
        self.assertEqual(('review', 1), (final['stage'], final['round']))
        self.assertEqual(job['snapshot'], final['snapshot'])
        self.assertEqual(['code', 'security', 'platform'], final['required_reviewers'])
        self.assertTrue(final['e2e']['passed'])

    def test_report_identity_scenarios_and_evidence_fail_closed(self):
        self.build()
        for changes in ({'invocation_id': 'builder-1'}, {'head_sha': '0' * 40}, {'environment_sha256': '0' * 64},
                        {'scenarios': []}, {'attempt': 2}):
            with self.subTest(changes=changes):
                self.record(fail=True, **changes)
        path, data = self.report(); data['scenarios'][0]['exit_code'] = 1
        (self.jobdir / path).write_text(json.dumps(data))
        self.run_cli('complete', '--job', self.job_id, '--stage', 'e2e', '--artifacts', path, fail=True)
        path, data = self.report(); data['scenarios'][0]['evidence'][0]['sha256'] = '0' * 64
        (self.jobdir / path).write_text(json.dumps(data))
        self.run_cli('complete', '--job', self.job_id, '--stage', 'e2e', '--artifacts', path, fail=True)
        self.assertEqual(0, self.job()['e2e']['attempts'])

    def test_manual_label_cannot_bypass_command_exit_and_final_review_is_separate(self):
        self.build()
        path, data = self.report()
        data['scenarios'][0]['method'] = 'manual'; data['scenarios'][0]['exit_code'] = None
        (self.jobdir / path).write_text(json.dumps(data))
        self.run_cli('complete', '--job', self.job_id, '--stage', 'e2e', '--artifacts', path, fail=True)
        self.record(fail=True, schema_version=True)
        self.record()
        job = self.job(); path = 'reviews/cycles/cycle-1/round-1/review-code.md'
        self.fill('review', 'reviewer-code', [i['path'] for i in job['artifacts'].values()],
                  job['snapshot']['path'], path, round=1, discipline='code', verdict='PASS', invocation_id='qa-1')
        self.run_cli('review', '--job', self.job_id, '--discipline', 'code', '--artifact', path, fail=True)

    def test_third_pass_followed_by_review_changes_requires_owner_rework(self):
        self.build()
        for _ in range(2):
            self.record('BLOCKED')
            self.run_cli('resume', '--job', self.job_id, '--reason', 'prerequisite restored')
        self.record()
        for reviewer in ('code', 'security', 'platform'):
            job = self.job(); path = f'reviews/cycles/cycle-1/round-1/review-{reviewer}.md'
            self.fill('review', 'reviewer-' + reviewer, [i['path'] for i in job['artifacts'].values()],
                      job['snapshot']['path'], path, round=1, discipline=reviewer, verdict='CHANGES',
                      invocation_id='reviewer-' + reviewer)
            self.run_cli('review', '--job', self.job_id, '--discipline', reviewer, '--artifact', path)
        self.assertEqual(('needs-human', 'e2e-exhausted'), (self.job()['stage'], self.job()['blocker']['code']))
        self.run_cli('resume', '--job', self.job_id, '--reason', 'cannot bypass exhausted E2E', fail=True)

    def test_fail_preserves_attempt_and_requires_new_e2e_after_rebuild(self):
        self.build(); old = self.job()['snapshot']
        self.record('FAIL')
        self.assertEqual(('build', 0), (self.job()['stage'], self.job()['round']))
        self.assertIsNone(self.job()['snapshot'])
        self.assertTrue((self.jobdir / 'records/e2e/report-c1-a1.json').exists())
        self.finish_build()
        self.assertEqual('e2e', self.job()['stage'])
        self.assertNotEqual(old, self.job()['snapshot'])
        self.record()
        self.assertEqual(('review', 2), (self.job()['stage'], self.job()['e2e']['attempts']))
        self.assertTrue((self.jobdir / 'records/operations/build-handoff-c1-r1-a1.md').exists())
        self.assertTrue((self.jobdir / 'records/operations/build-handoff-c1-r1-a2.md').exists())

    def test_blocked_attempts_are_bounded_across_resume(self):
        self.build()
        for attempt in range(1, 4):
            self.record('BLOCKED' if attempt % 2 else 'NOT_RUN')
            self.assertEqual('needs-human', self.job()['stage'])
            self.assertEqual(attempt, self.job()['e2e']['attempts'])
            if attempt < 3:
                self.run_cli('resume', '--job', self.job_id, '--reason', 'disposable prerequisite restored')
            else:
                self.run_cli('resume', '--job', self.job_id, '--reason', 'cannot reset limit', fail=True)
        self.assertEqual('e2e-exhausted', self.job()['blocker']['code'])

    def test_missing_or_changed_pinned_policy_cannot_disable_e2e(self):
        self.build()
        factory = test_factory.factory
        boardpath = self.repo / '.engram-factory/board.json'
        board = json.loads(boardpath.read_text()); original = boardpath.read_bytes()
        job = next(j for j in board['jobs'] if j['job_id'] == self.job_id)
        for change in ('missing', 'scenario'):
            modified = json.loads(original); target = next(j for j in modified['jobs'] if j['job_id'] == self.job_id)
            if change == 'missing': target.pop('e2e')
            else: target['e2e']['scenarios'][0]['Expected'] = 'Unreviewed weakened expected behavior'
            with self.subTest(change=change), self.assertRaisesRegex(ValueError, 'E2E.*(selection|policy)'):
                factory.source_check(self.repo, self.jobdir, target)
        self.assertEqual(original, boardpath.read_bytes())

    def test_evidence_change_between_validation_and_archive_rejects_ingestion(self):
        self.build(); path, data = self.report()
        factory = test_factory.factory
        boardpath = self.repo / '.engram-factory/board.json'
        before = boardpath.read_bytes(); board = json.loads(before)
        args = factory.parser().parse_args(['--repo', str(self.repo), 'complete', '--job', self.job_id,
                                           '--stage', 'e2e', '--artifacts', path])
        original = factory.record; logpath = data['scenarios'][0]['evidence'][0]['path']
        def changed(jobdir, relative):
            if relative == logpath:
                (self.jobdir / logpath).write_text('Evidence replaced after validation.\n')
            return original(jobdir, relative)
        with mock.patch.object(factory, 'record', side_effect=changed), self.assertRaisesRegex(ValueError, 'evidence changed'):
            factory.execute(args, self.repo, self.repo / '.engram-factory', board)
        self.assertEqual(before, boardpath.read_bytes())
        self.assertFalse((self.jobdir / 'reviews/cycles/cycle-1/round-1').exists())

    def test_source_or_report_tamper_cannot_reuse_e2e(self):
        self.build(); path, _ = self.report()
        worktree = Path(self.job()['worktree']); (worktree / 'app.txt').write_text('source changed\n')
        self.run_cli('complete', '--job', self.job_id, '--stage', 'e2e', '--artifacts', path, fail=True)
        self.git(worktree, 'restore', 'app.txt')
        self.record()
        (self.jobdir / path).write_text('{}')
        job = self.job(); review = f'reviews/cycles/cycle-1/round-1/review-code.md'
        self.fill('review', 'reviewer-code', [i['path'] for i in job['artifacts'].values()],
                  job['snapshot']['path'], review, round=1, discipline='code', verdict='PASS', invocation_id='review-code')
        self.run_cli('review', '--job', self.job_id, '--discipline', 'code', '--artifact', review, fail=True)

    def test_final_review_changes_requires_fresh_e2e_without_consuming_extra_round(self):
        self.build(); self.record()
        for reviewer in ('code', 'security', 'platform'):
            job = self.job(); path = f'reviews/cycles/cycle-1/round-1/review-{reviewer}.md'
            self.fill('review', 'reviewer-' + reviewer, [i['path'] for i in job['artifacts'].values()],
                      job['snapshot']['path'], path, round=1, discipline=reviewer, verdict='CHANGES',
                      invocation_id='reviewer-' + reviewer)
            self.run_cli('review', '--job', self.job_id, '--discipline', reviewer, '--artifact', path)
        self.assertEqual('build', self.job()['stage'])
        self.finish_build(); self.assertEqual(('e2e', 1), (self.job()['stage'], self.job()['round']))
        self.record(); self.assertEqual(('review', 2), (self.job()['stage'], self.job()['round']))


class E2EPlanTests(unittest.TestCase):
    def test_proof_can_explicitly_defer_only_declared_e2e_scenarios(self):
        parser = test_plan_phase.PlanContentTests(); parser.setUp()
        plan = test_plan_phase.PLAN.replace('size_tier: "S"', 'size_tier: "S"\ne2e: true') + E2E_TABLE
        plan = plan.replace('python3 -m unittest discover -s tests', 'E2E: E2E-001')
        parser.parse(plan)
        for invalid in (plan.replace('E2E: E2E-001', 'E2E: E2E-999'),
                        plan.replace('E2E: E2E-001', 'E2E: E2E-001, E2E-001'),
                        test_plan_phase.PLAN.replace('python3 -m unittest discover -s tests', 'E2E: E2E-001')):
            with self.assertRaises(ValueError): parser.parse(invalid)

    def test_enabled_contract_needs_valid_scenarios_and_disabled_cannot_hide_them(self):
        parser = test_plan_phase.PlanContentTests(); parser.setUp()
        good = test_plan_phase.PLAN.replace('size_tier: "S"', 'size_tier: "S"\ne2e: true') + E2E_TABLE
        self.assertEqual('E2E-001', parser.parse(good)['e2e']['scenarios'][0]['ID'])
        for job, fields in (({'layout_version': 3}, {'e2e': False}),
                            ({}, {'e2e': True, 'plan_format': 'compact-v1'})):
            with self.assertRaises(ValueError):
                test_factory.factory.e2e.pin(job, fields, E2E_TABLE, {'path': 'plan.md', 'sha256': '0' * 64})
        for invalid in (good.replace('e2e: true', 'e2e: false'), good.replace('e2e: true', 'e2e: "yes"'),
                        good.replace('AC-001, AC-002 | python3 check_journey.py', 'AC-999 | python3 check_journey.py'),
                        good.replace('## E2E', '## Missing')):
            with self.subTest(invalid=invalid[-80:]), self.assertRaises(ValueError): parser.parse(invalid)
