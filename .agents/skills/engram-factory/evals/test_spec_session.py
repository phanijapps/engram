"""Analyzer-led shared state, read-only correction and exact idempotent submission."""
import json
import unittest
from pathlib import Path
import test_factory
import test_spec_phase
import test_current_workflow


class SpecSessionTests(unittest.TestCase):
    setUp = test_factory.FactoryTests.setUp
    git = staticmethod(test_factory.FactoryTests.git)
    run_cli = test_factory.FactoryTests.run_cli
    job = test_factory.FactoryTests.job
    fill = test_factory.FactoryTests.fill
    setUpCurrent = test_current_workflow.CurrentWorkflowTests.setUpCurrent

    def setup_session(self):
        self.setUpCurrent()
        (self.jobdir / 'spec.md').write_text(test_spec_phase.SINGLE)
        self.run_cli('spec-session-open', '--job', self.job_id, '--author', 'author-1',
                     '--reviewers-file', 'records/operations/reviewers.json')
        return self.run_cli('spec-context', '--job', self.job_id)['result']

    def submit(self, context, operation='submit-1', fail=False, author='author-1'):
        return self.run_cli('spec-submit', '--job', self.job_id, '--author', author,
                            '--operation-id', operation, '--context-sha256', context['context_sha256'],
                            '--spec-sha256', test_factory.factory.digest(self.jobdir / 'spec.md'),
                            revision=context['board_revision'], fail=fail)

    def bytes(self):
        return {str(p):p.read_bytes() for p in (self.repo / '.engram-factory').rglob('*') if p.is_file()}

    def test_context_and_draft_check_are_read_only_without_receipt(self):
        self.setup_session()
        before = self.bytes()
        context = self.run_cli('spec-context', '--job', self.job_id)['result']
        self.assertEqual('analyze', context['stage'])
        self.assertIn('brief', context['accepted_inputs'])
        result = self.run_cli('spec-check', '--job', self.job_id)['result']
        self.assertEqual('PASS', result['structural'])
        self.assertFalse((self.jobdir / 'records/spec/receipt.json').exists())
        self.assertEqual(before, self.bytes())

    def test_invalid_draft_returns_findings_without_review_candidate(self):
        context = self.setup_session()
        p = self.jobdir / 'spec.md'
        p.write_text(p.read_text().replace('AC-001, AC-002', 'AC-001'))
        result = self.run_cli('spec-check', '--job', self.job_id)['result']
        self.assertEqual('FAIL', result['structural'])
        self.assertTrue(result['findings'])
        self.submit(context, fail=True)
        self.assertFalse((self.jobdir / 'reviews/prebuild/spec').exists())

    def test_submit_prepares_freezes_and_returns_fixed_independent_roster(self):
        context = self.setup_session()
        result = self.submit(context)['result']
        self.assertEqual('needs-dispatch', result['dispatch_status'])
        self.assertEqual({'red-forman': 'reviewer-1'}, result['reviewers'])
        self.assertEqual(1, result['candidate'])
        self.assertEqual('analyze', self.job()['stage'])
        self.assertIsNone(self.job()['approval'])
        request = json.loads((self.jobdir / result['request']['path']).read_text())
        self.assertIn('records/spec/receipt.json', [r['path'] for r in request['inputs']])
        self.run_cli('complete', '--job', self.job_id, '--stage', 'analyze', '--artifacts', 'spec.md', fail=True)

    def test_exact_retry_returns_same_request_without_mutating_state(self):
        context = self.setup_session()
        first = self.submit(context)['result']
        before = self.bytes()
        second = self.submit(context)['result']
        self.assertEqual(first['request'], second['request'])
        self.assertTrue(second['replayed'])
        self.assertEqual(before, self.bytes())

    def test_stale_context_revision_author_or_spec_is_rejected(self):
        context = self.setup_session()
        self.submit(context, author='wrong-author', fail=True)
        bad = {**context, 'context_sha256': '0'*64}
        self.submit(bad, fail=True)
        self.run_cli('spec-session-open', '--job', self.job_id, '--author', 'author-1',
                     '--reviewers-file', 'records/operations/reviewers.json', fail=True)
        first = self.submit(context)['result']
        p = self.jobdir / 'spec.md';p.write_text(p.read_text()+'\nNew draft.\n')
        self.submit(context, fail=True)
        current = self.run_cli('spec-context', '--job', self.job_id)['result']
        self.submit(current, operation='submit-2', fail=True)
        self.assertEqual(1, len(list((self.jobdir/'reviews/prebuild/spec').glob('candidate-*.json'))))

    def test_pass_is_visible_without_author_advancing_stage(self):
        context = self.setup_session();self.submit(context)
        self.run_cli('review-record', '--job', self.job_id, '--stage', 'spec', '--candidate', '1',
                     '--role', 'red-forman', '--invocation', 'reviewer-1', '--verdict', 'PASS',
                     '--findings-file', 'records/operations/findings.json')
        current = self.run_cli('spec-context', '--job', self.job_id)['result']
        self.assertEqual('PASS', current['review']['verdict'])
        self.assertEqual('analyze', self.job()['stage'])
        self.run_cli('complete', '--job', self.job_id, '--stage', 'analyze', '--artifacts', 'spec.md')
        self.assertEqual('plan', self.job()['stage'])

    def test_draft_race_cannot_publish_a_different_spec_identity(self):
        from unittest import mock
        from types import SimpleNamespace
        context = self.setup_session()
        args = SimpleNamespace(author='author-1', operation_id='race', context_sha256=context['context_sha256'],
                               spec_sha256=test_factory.factory.digest(self.jobdir/'spec.md'), expected_revision=context['board_revision'])
        original = test_factory.factory.prepare_spec
        def change(jobdir, job):
            p = jobdir / 'spec.md';p.write_text(p.read_text()+'\nConcurrent clarification.\n')
            return original(jobdir, job)
        with mock.patch.object(test_factory.factory, 'prepare_spec', side_effect=change):
            with self.assertRaisesRegex(ValueError, 'Spec bytes changed during preparation'):
                test_factory.factory.spec_session.submit(vars(test_factory.factory), self.jobdir, self.job(), context['board_revision'], args)
        self.assertFalse((self.jobdir/'reviews/prebuild/spec').exists())

    def test_context_reports_missing_unaccepted_input_without_losing_review(self):
        context = self.setup_session();self.submit(context)
        (self.jobdir/'spec.md').unlink()
        state = self.run_cli('spec-context', '--job', self.job_id)['result']
        self.assertEqual('PENDING', state['review']['verdict'])
        self.assertFalse(state['review']['applies_to_current_inputs'])
        self.assertEqual('spec.md', state['review']['input_drift'][0]['path'])
        self.assertTrue(state['review_requests'])

    def test_replay_rejects_damaged_candidate_metadata(self):
        context = self.setup_session();self.submit(context)
        candidate = self.jobdir/'reviews/prebuild/spec/candidate-0001.json'
        candidate.write_text(candidate.read_text()+' ')
        self.submit(context, fail=True)

    def test_session_roster_cannot_be_bypassed_through_ordinary_freeze(self):
        self.setup_session()
        self.run_cli('spec-prepare', '--job', self.job_id)
        roster = self.jobdir/'records/operations/alternate.json'
        roster.write_text(json.dumps({'red-forman':'other-reviewer'}))
        self.run_cli('review-freeze', '--job', self.job_id, '--stage', 'spec', '--author', 'author-1',
                     '--reviewers-file', 'records/operations/alternate.json', fail=True)
        self.assertFalse((self.jobdir/'reviews/prebuild/spec').exists())

    def test_changes_findings_are_shared_and_next_submission_keeps_counter(self):
        context = self.setup_session();self.submit(context)
        findings = self.jobdir/'records/operations/findings.json'
        findings.write_text(json.dumps({'findings':{'F-1':'Name the missing boundary'},'resolves':[]}))
        self.run_cli('review-record', '--job', self.job_id, '--stage', 'spec', '--candidate', '1',
                     '--role', 'red-forman', '--invocation', 'reviewer-1', '--verdict', 'CHANGES',
                     '--findings-file', 'records/operations/findings.json')
        p=self.jobdir/'spec.md';p.write_text(p.read_text()+'\nBoundary clarified for F-1.\n')
        state=self.run_cli('spec-context','--job',self.job_id)['result']
        self.assertEqual('Name the missing boundary',state['outstanding_findings']['red-forman']['F-1'])
        result=self.submit(state, operation='submit-2')['result']
        self.assertEqual(2,result['candidate'])


    def test_accepted_brief_race_fails_before_candidate_publication(self):
        from unittest import mock
        from types import SimpleNamespace
        context = self.setup_session()
        args=SimpleNamespace(author='author-1',operation_id='brief-race',context_sha256=context['context_sha256'],
                             spec_sha256=test_factory.factory.digest(self.jobdir/'spec.md'),expected_revision=context['board_revision'])
        original=test_factory.factory.stage_reviews.freeze_candidate
        def change(*pos, **kw):
            path=self.jobdir/'brief.md';path.write_text(path.read_text()+'\nChanged accepted brief.\n')
            return original(*pos, **kw)
        with mock.patch.object(test_factory.factory.stage_reviews,'freeze_candidate',side_effect=change):
            with self.assertRaisesRegex(ValueError,'Review inputs changed before freeze'):
                test_factory.factory.spec_session.submit(vars(test_factory.factory),self.jobdir,self.job(),context['board_revision'],args)
        self.assertFalse(list((self.jobdir/'reviews/prebuild/spec').glob('candidate-*.json')))

    def test_context_preserves_same_finding_id_from_distinct_reviewers(self):
        self.setUpCurrent()
        (self.jobdir/'spec.md').write_text(test_spec_phase.SINGLE)
        roster=self.jobdir/'records/operations/reviewers.json'
        roster.write_text(json.dumps({'red-forman':'reviewer-1','code':'reviewer-2'}))
        self.run_cli('spec-session-open','--job',self.job_id,'--author','author-1','--reviewers-file','records/operations/reviewers.json')
        context=self.run_cli('spec-context','--job',self.job_id)['result'];self.submit(context)
        for role,invocation,description in [('red-forman','reviewer-1','First concern'),('code','reviewer-2','Second concern')]:
            (self.jobdir/'records/operations/findings.json').write_text(json.dumps({'findings':{'F-1':description},'resolves':[]}))
            self.run_cli('review-record','--job',self.job_id,'--stage','spec','--candidate','1','--role',role,'--invocation',invocation,
                         '--verdict','CHANGES','--findings-file','records/operations/findings.json')
        result=self.run_cli('spec-context','--job',self.job_id)['result']['outstanding_findings']
        self.assertEqual('First concern',result['red-forman']['F-1'])
        self.assertEqual('Second concern',result['code']['F-1'])


    def test_new_operation_requires_current_revision_even_when_context_identity_is_same(self):
        context=self.setup_session()
        self.run_cli('spec-prepare','--job',self.job_id)
        self.submit(context,fail=True)
        current=self.run_cli('spec-context','--job',self.job_id)['result']
        self.assertEqual(context['context_sha256'],current['context_sha256'])
        self.assertEqual(1,self.submit(current)['result']['candidate'])

    def test_exhaustion_cannot_be_reset_through_shared_submission(self):
        context=self.setup_session()
        for candidate in range(1,4):
            if candidate>1:
                p=self.jobdir/'spec.md';p.write_text(p.read_text()+f'\nRevision {candidate}.\n')
                context=self.run_cli('spec-context','--job',self.job_id)['result']
            self.submit(context,operation=f'attempt-{candidate}')
            (self.jobdir/'records/operations/findings.json').write_text(json.dumps({'findings':{f'F-{candidate}':'Still unresolved'},'resolves':[]}))
            self.run_cli('review-record','--job',self.job_id,'--stage','spec','--candidate',str(candidate),
                         '--role','red-forman','--invocation','reviewer-1','--verdict','CHANGES',
                         '--findings-file','records/operations/findings.json')
        current=self.run_cli('spec-context','--job',self.job_id)['result']
        self.assertEqual('needs-human',current['stage'])
        self.submit(current,operation='attempt-4',fail=True)
        self.assertEqual(3,len(list((self.jobdir/'reviews/prebuild/spec').glob('candidate-*.json'))))



if __name__ == '__main__':
    unittest.main()
