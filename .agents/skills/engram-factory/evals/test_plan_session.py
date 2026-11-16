"""Planner-led sessions preserve accepted spec, optional support and owner gates."""
import json
import unittest
from pathlib import Path
import test_factory
import test_plan_phase
import test_current_workflow


class PlanSessionTests(unittest.TestCase):
    setUp = test_factory.FactoryTests.setUp
    git = staticmethod(test_factory.FactoryTests.git)
    run_cli = test_factory.FactoryTests.run_cli
    job = test_factory.FactoryTests.job
    fill = test_factory.FactoryTests.fill
    setUpCurrent = test_current_workflow.CurrentWorkflowTests.setUpCurrent
    start_plan = test_plan_phase.PlanControllerTests.start_plan
    review = test_plan_phase.PlanControllerTests.review

    def setup_session(self):
        self.start_plan()
        self.run_cli('plan-session-open','--job',self.job_id,'--author','planner-1','--reviewers-file','records/operations/reviewers.json')
        return self.run_cli('plan-context','--job',self.job_id)['result']

    def submit(self, context, operation='plan-1', fail=False):
        checked=self.run_cli('plan-check','--job',self.job_id)['result']
        return self.run_cli('plan-submit','--job',self.job_id,'--author','planner-1','--operation-id',operation,
                            '--context-sha256',context['context_sha256'],'--plan-sha256',test_factory.factory.digest(self.jobdir/'plan.md'),
                            '--draft-sha256',checked.get('draft_sha256','0'*64),revision=context['board_revision'],fail=fail)

    def test_context_and_check_read_only_without_generated_receipt(self):
        self.setup_session()
        before={str(p):p.read_bytes() for p in (self.repo/'.engram-factory').rglob('*') if p.is_file()}
        state=self.run_cli('plan-context','--job',self.job_id)['result']
        self.assertIn('spec',state['accepted_inputs'])
        result=self.run_cli('plan-check','--job',self.job_id)['result']
        self.assertEqual('PASS',result['structural'])
        self.assertTrue(result['draft_sha256'])
        self.assertEqual(before,{str(p):p.read_bytes() for p in (self.repo/'.engram-factory').rglob('*') if p.is_file()})
        self.assertFalse((self.jobdir/'records/plan/receipt.json').exists())

    def test_submit_and_exact_retry_do_not_accept_or_duplicate(self):
        state=self.setup_session();first=self.submit(state)['result']
        before=(self.repo/'.engram-factory/board.json').read_bytes()
        second=self.submit(state)['result']
        self.assertEqual(first['request'],second['request'])
        self.assertTrue(second['replayed'])
        self.assertEqual(before,(self.repo/'.engram-factory/board.json').read_bytes())
        self.assertEqual('plan',self.job()['stage'])
        self.assertIsNone(self.job()['approval'])
        request=json.loads((self.jobdir/first['request']['path']).read_text())
        self.assertIn('records/plan/receipt.json',[r['path'] for r in request['inputs']])

    def test_failed_draft_and_stale_context_spend_no_candidate(self):
        state=self.setup_session()
        plan=self.jobdir/'plan.md';plan.write_text(plan.read_text().replace('AC-001, AC-002','AC-001'))
        self.assertEqual('FAIL',self.run_cli('plan-check','--job',self.job_id)['result']['structural'])
        self.submit(state,fail=True)
        self.assertFalse((self.jobdir/'reviews/prebuild/plan').exists())

    def test_support_bytes_are_bound_to_the_author_checked_draft(self):
        state=self.setup_session()
        p=self.jobdir/'context/interfaces.md';p.write_text('# Shared contract\n\nRetain the input validation boundary across independent host implementations.\n')
        plan=self.jobdir/'plan.md';plan.write_text(plan.read_text()+'\n## Supporting documents\n| Path | Purpose |\n| --- | --- |\n| context/interfaces.md | Shared host contract maintained independently |\n')
        checked=self.run_cli('plan-check','--job',self.job_id)['result']
        p.write_text(p.read_text()+'\nChanged after author check.\n')
        self.run_cli('plan-submit','--job',self.job_id,'--author','planner-1','--operation-id','stale-support',
                     '--context-sha256',state['context_sha256'],'--plan-sha256',checked['plan']['sha256'],
                     '--draft-sha256',checked['draft_sha256'],revision=state['board_revision'],fail=True)
        self.assertFalse((self.jobdir/'reviews/prebuild/plan').exists())
        result=self.submit(state)['result'];self.assertEqual(1,result['candidate'])

    def test_changes_findings_and_pass_are_shared_without_stage_advance(self):
        state=self.setup_session();self.submit(state)
        findings=self.jobdir/'records/operations/findings.json'
        findings.write_text(json.dumps({'findings':{'F-1':'Clarify recovery'},'resolves':[]}))
        self.run_cli('review-record','--job',self.job_id,'--stage','plan','--candidate','1','--role','red-forman',
                     '--invocation','reviewer-1','--verdict','CHANGES','--findings-file','records/operations/findings.json')
        plan=self.jobdir/'plan.md';plan.write_text(plan.read_text()+'\nRecovery clarification for F-1.\n')
        state=self.run_cli('plan-context','--job',self.job_id)['result']
        self.assertEqual('Clarify recovery',state['outstanding_findings']['red-forman']['F-1'])
        self.assertEqual(2,self.submit(state,operation='plan-2')['result']['candidate'])
        findings.write_text(json.dumps({'findings':{},'resolves':['F-1']}))
        self.run_cli('review-record','--job',self.job_id,'--stage','plan','--candidate','2','--role','red-forman',
                     '--invocation','reviewer-1','--verdict','PASS','--findings-file','records/operations/findings.json')
        state=self.run_cli('plan-context','--job',self.job_id)['result'];self.assertEqual('PASS',state['review']['verdict'])
        self.assertEqual('plan',self.job()['stage'])
        self.run_cli('complete','--job',self.job_id,'--stage','plan','--artifacts','plan.md')
        self.assertEqual('tasks',self.job()['stage'])

    def test_session_cannot_change_roster_or_use_spec_interface(self):
        state=self.setup_session()
        self.run_cli('spec-submit','--job',self.job_id,'--author','planner-1','--operation-id','wrong-phase',
                     '--context-sha256',state['context_sha256'],'--spec-sha256',test_factory.factory.digest(self.jobdir/'spec.md'),
                     revision=state['board_revision'],fail=True)
        self.run_cli('plan-session-open','--job',self.job_id,'--author','new-author',
                     '--reviewers-file','records/operations/reviewers.json',fail=True)
        self.assertFalse((self.jobdir/'reviews/prebuild/plan').exists())

    def test_support_race_during_preparation_is_rejected_before_freeze(self):
        from unittest import mock
        from types import SimpleNamespace
        state=self.setup_session()
        support=self.jobdir/'context/interfaces.md';support.write_text('# Contract\n\nKeep the input boundary stable for independent host implementations and validation clients.\n')
        plan=self.jobdir/'plan.md';plan.write_text(plan.read_text()+'\n## Supporting documents\n| Path | Purpose |\n| --- | --- |\n| context/interfaces.md | Shared host contract maintained independently |\n')
        checked=self.run_cli('plan-check','--job',self.job_id)['result']
        args=SimpleNamespace(author='planner-1',operation_id='race',context_sha256=state['context_sha256'],
                             plan_sha256=checked['plan']['sha256'],draft_sha256=checked['draft_sha256'],expected_revision=state['board_revision'])
        original=test_factory.factory.plan_document.prepare
        def change(*a,**kw):
            support.write_text(support.read_text()+'\nConcurrent contract change.\n');return original(*a,**kw)
        with mock.patch.object(test_factory.factory.plan_document,'prepare',side_effect=change):
            with self.assertRaisesRegex(ValueError,'Authoritative review input changed'):
                test_factory.factory.plan_session.submit(vars(test_factory.factory),self.jobdir,self.job(),state['board_revision'],args)
        self.assertFalse((self.jobdir/'reviews/prebuild/plan').exists())

    def test_pending_and_pass_do_not_allow_new_requests_or_roster_bypass(self):
        state=self.setup_session();self.submit(state)
        current=self.run_cli('plan-context','--job',self.job_id)['result']
        self.submit(current,operation='pending-2',fail=True)
        roster=self.jobdir/'records/operations/alternate.json';roster.write_text(json.dumps({'red-forman':'other-reviewer'}))
        self.run_cli('review-freeze','--job',self.job_id,'--stage','plan','--author','planner-1',
                     '--reviewers-file','records/operations/alternate.json',fail=True)
        self.run_cli('review-record','--job',self.job_id,'--stage','plan','--candidate','1','--role','red-forman',
                     '--invocation','reviewer-1','--verdict','PASS','--findings-file','records/operations/findings.json')
        current=self.run_cli('plan-context','--job',self.job_id)['result']
        self.submit(current,operation='after-pass',fail=True)
        self.assertEqual(1,len(list((self.jobdir/'reviews/prebuild/plan').glob('candidate-*.json'))))

    def test_stale_revision_wrong_identity_and_corrupt_replay_are_rejected(self):
        state=self.setup_session()
        self.run_cli('plan-prepare','--job',self.job_id)
        self.submit(state,fail=True)
        current=self.run_cli('plan-context','--job',self.job_id)['result']
        self.submit({**current,'context_sha256':'0'*64},fail=True)
        self.submit(current)
        candidate=self.jobdir/'reviews/prebuild/plan/candidate-0001.json';candidate.write_text(candidate.read_text()+' ')
        self.submit(current,fail=True)

    def test_exhaustion_is_shared_and_never_reset(self):
        state=self.setup_session()
        for number in range(1,4):
            if number>1:
                p=self.jobdir/'plan.md';p.write_text(p.read_text()+f'\nRevision {number}.\n')
                state=self.run_cli('plan-context','--job',self.job_id)['result']
            self.submit(state,operation=f'round-{number}')
            (self.jobdir/'records/operations/findings.json').write_text(json.dumps({'findings':{f'F-{number}':'Still unresolved'},'resolves':[]}))
            self.run_cli('review-record','--job',self.job_id,'--stage','plan','--candidate',str(number),'--role','red-forman',
                         '--invocation','reviewer-1','--verdict','CHANGES','--findings-file','records/operations/findings.json')
        state=self.run_cli('plan-context','--job',self.job_id)['result']
        self.assertEqual('needs-human',state['stage']);self.submit(state,operation='round-4',fail=True)
        self.assertEqual(3,len(list((self.jobdir/'reviews/prebuild/plan').glob('candidate-*.json'))))



if __name__ == '__main__':
    unittest.main()
