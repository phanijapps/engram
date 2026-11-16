"""Current-only preflight proof: content, receipts, stable reads and no mutation."""
import json
import os
import subprocess
import unittest
from unittest import mock
import test_factory


class CandidatePreflightTests(unittest.TestCase):
    setUp = test_factory.FactoryTests.setUp
    git = staticmethod(test_factory.FactoryTests.git)
    run_cli = test_factory.FactoryTests.run_cli
    create = test_factory.FactoryTests.create
    job = test_factory.FactoryTests.job
    fill = test_factory.FactoryTests.fill

    def bytes(self):
        return {str(p.relative_to(self.repo)): p.read_bytes() for p in (self.repo / '.engram-factory').rglob('*') if p.is_file()}

    def test_receipt_free_content_check_rejects_invalid_spec_without_writes(self):
        self.create(); self.fill('spec', 'engram-analyzer', [])
        path = self.jobdir / 'spec.md'; path.write_text(path.read_text().replace('report its name.', 'TODO'))
        before = self.bytes()
        result = self.run_cli('spec-check', '--job', self.job_id)['result']
        self.assertEqual('FAIL', result['structural'])
        self.assertEqual(before, self.bytes())

    def test_preflight_rejects_stale_authored_bytes_without_mutation(self):
        self.create(); self.fill('spec', 'engram-analyzer', [])
        path = self.jobdir / 'spec.md'; path.write_text(path.read_text() + '\nChanged draft.\n')
        before = self.bytes()
        self.run_cli('preflight', '--job', self.job_id, '--stage', 'spec', fail=True)
        self.assertEqual(before, self.bytes())

    def test_current_preflight_does_not_create_missing_lock(self):
        self.create(); self.fill('spec', 'engram-analyzer', [])
        (self.repo / '.engram-factory/board.lock').unlink()
        before = self.bytes()
        self.assertEqual('PASS', self.run_cli('preflight', '--job', self.job_id, '--stage', 'spec')['result']['structural'])
        self.assertEqual(before, self.bytes())

    def test_old_board_is_rejected_before_lock_or_backup_creation(self):
        self.create(); self.fill('spec', 'engram-analyzer', [])
        path = self.repo / '.engram-factory/board.json'
        board = json.loads(path.read_text()); board['schema_version'] = '1.0'; path.write_text(json.dumps(board))
        (path.parent / 'board.lock').unlink()
        before = self.bytes()
        self.run_cli('preflight', '--job', self.job_id, '--stage', 'spec', fail=True)
        self.assertEqual(before, self.bytes())
        self.assertFalse((path.parent / 'board-v1-backup.json').exists())

    def test_generated_required_brief_reference_cannot_be_removed(self):
        self.create(); self.fill('spec', 'engram-analyzer', [])
        path = self.jobdir / 'records/spec/receipt.json'; data = json.loads(path.read_text())
        data['fields']['input_artifacts'] = [r for r in data['fields']['input_artifacts'] if r['path'] != 'brief.md']
        path.write_text(json.dumps(data))
        self.run_cli('preflight', '--job', self.job_id, '--stage', 'spec', fail=True)

    def test_preflight_includes_controller_owned_spec_receipt_and_brief(self):
        self.create(); self.fill('spec', 'engram-analyzer', [])
        paths = {r['path'] for r in self.run_cli('preflight', '--job', self.job_id, '--stage', 'spec')['result']['inputs']}
        self.assertTrue({'spec.md', 'brief.md', 'records/spec/receipt.json'} <= paths)

    def test_plain_cli_disables_repository_module_bytecode_writes(self):
        self.create(); self.fill('spec', 'engram-analyzer', [])
        env = dict(os.environ); env.pop('PYTHONDONTWRITEBYTECODE', None)
        cache = self.root / 'python-cache'; env['PYTHONPYCACHEPREFIX'] = str(cache)
        r = subprocess.run([test_factory.sys.executable, str(test_factory.SCRIPT), '--repo', str(self.repo), 'preflight', '--job', self.job_id, '--stage', 'spec'], env=env, capture_output=True, text=True)
        self.assertEqual(0, r.returncode, r.stderr)
        self.assertFalse(any('engram-factory' in str(p) for p in cache.rglob('*.pyc')))

    def test_preflight_rejects_accepted_evidence_changed_during_validation(self):
        self.create(); self.fill('spec', 'engram-analyzer', [])
        feature = self.jobdir / 'feature.md'
        validate = test_factory.factory.handoff.validate
        def change(*args, **kwargs):
            result = validate(*args, **kwargs)
            feature.write_text('Changed during validation.')
            return result
        with mock.patch.object(test_factory.factory.handoff, 'validate', side_effect=change):
            with self.assertRaisesRegex(ValueError, 'Inputs changed during preflight'):
                test_factory.factory.main(['--repo', str(self.repo), 'preflight', '--job', self.job_id, '--stage', 'spec'])


if __name__ == '__main__':
    unittest.main()
