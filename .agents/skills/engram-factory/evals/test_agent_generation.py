"""Generated factory wrappers bind canonical sources without owning role behavior."""
import copy
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

SCRIPT = Path(__file__).resolve().parents[1] / 'scripts/generate_agents.py'
spec = importlib.util.spec_from_file_location('factory_agent_generation', SCRIPT)
generation = importlib.util.module_from_spec(spec)
spec.loader.exec_module(generation)


class AgentGenerationTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='agent-generation-eval-')
        self.addCleanup(self.temp.cleanup)
        self.repo = Path(self.temp.name)
        self.skill = self.repo / '.agents/skills/engram-factory'
        (self.skill / 'agents').mkdir(parents=True)
        (self.skill / 'references/roles').mkdir(parents=True)
        (self.skill / 'references/shared.md').write_text('# Shared contract\n\nRespect current candidate scope and separate owner authority.\n')
        (self.skill / 'references/roles/builder.md').write_text('# Builder\n\nImplement only the assigned outcome and preserve independent review.\n')
        (self.skill / 'references/roles/reviewer.md').write_text('# Reviewer\n\nIndependently inspect the candidate and record real evidence.\n')
        (self.skill / 'references/context.md').write_text('# Context\n\nRead the revision-bound inputs that belong to your role.\n')
        self.manifest = {'schema_version': 1, 'shared': 'references/shared.md',
                         'contracts': {'builder': 'references/roles/builder.md', 'reviewer': 'references/roles/reviewer.md'},
                         'host_settings': {'claude': {'tools': ['Read', 'Grep', 'Glob', 'Bash']},
                                           'copilot': {'tools': ['read_file', 'list_directory', 'search_code', 'run_command']},
                                           'codex': {'web_search': 'disabled', 'features': {'shell_tool': True}}, 'pi': {}},
                         'permission_profiles': {'author': {'sandbox_mode': 'workspace-write'},
                                                 'reviewer': {'sandbox_mode': 'read-only'}},
                         'roles': [{'name': 'engram-builder', 'description': 'Build a scoped change.', 'contract': 'builder', 'permissions': 'author'},
                                   {'name': 'engram-code-reviewer', 'description': 'Review the exact change.', 'contract': 'reviewer',
                                    'permissions': 'reviewer', 'discipline': 'code'}]}
        self.save()

    def save(self):
        (self.skill / 'agents/roles.json').write_text(json.dumps(self.manifest))

    def run_generator(self, *args, code=0):
        result = subprocess.run([sys.executable, str(SCRIPT), '--skill-root', str(self.skill),
                                 '--repo-root', str(self.repo), *args], capture_output=True, text=True)
        self.assertEqual(code, result.returncode, result.stdout + result.stderr)
        return json.loads(result.stdout if code != 2 else result.stderr)

    def tree(self):
        result = {}
        for path in sorted(self.repo.rglob('*')):
            if path.is_symlink():
                result[path.relative_to(self.repo).as_posix()] = ('symlink', str(path.readlink()))
            elif path.is_file():
                result[path.relative_to(self.repo).as_posix()] = ('file', path.read_bytes(), path.stat().st_mtime_ns)
            elif path.is_dir():
                result[path.relative_to(self.repo).as_posix()] = ('directory',)
        return result

    def test_check_reports_missing_and_stale_without_any_writes(self):
        before = self.tree()
        result = self.run_generator('--check', code=1)
        self.assertEqual(10, len(result['stale']))
        self.assertEqual(before, self.tree())
        generated = self.run_generator()
        self.assertEqual((8, 2, 10), (generated['wrappers'], generated['pi_mirrors'], generated['written']))
        output = self.skill / 'agents/engram-builder.pi.md'
        output.write_text(output.read_text() + '\nUnauthorized duplicate role instructions.\n')
        before = self.tree()
        self.assertEqual('drift', self.run_generator('--check', code=1)['status'])
        self.assertEqual(before, self.tree())

    def test_regeneration_is_idempotent_and_contract_changes_are_bound(self):
        first = self.run_generator(); before = self.tree()
        self.assertEqual(0, self.run_generator()['written'])
        self.assertEqual(before, self.tree())
        self.assertEqual('current', self.run_generator('--check')['status'])
        context = self.skill / 'references/context.md'
        context.write_text(context.read_text() + '\nChanged a transitive context obligation.\n')
        before = self.tree()
        changed = self.run_generator('--check', code=1)
        self.assertNotEqual(first['bundle_sha256'], changed['bundle_sha256'])
        self.assertEqual(10, len(changed['stale']))
        self.assertEqual(before, self.tree())
        final = self.run_generator()
        self.assertEqual(changed['bundle_sha256'], final['bundle_sha256'])
        self.assertEqual('current', self.run_generator('--check')['status'])

    def test_expected_digest_check_is_read_only_and_never_hot_reloads(self):
        first = self.run_generator(); before = self.tree()
        self.run_generator('--check', '--expected-digest', first['bundle_sha256'])
        result = self.run_generator('--check', '--expected-digest', '0' * 64, code=1)
        self.assertFalse(result['expected_digest_matches'])
        self.assertEqual(before, self.tree())
        self.run_generator('--expected-digest', first['bundle_sha256'], code=2)
        self.assertEqual(before, self.tree())

    def test_runtime_digest_works_without_repository_or_pi_outputs(self):
        # A user-scope install has its canonical source tree but no project mirrors.
        first = generation.generate(self.skill, digest_only=True)
        self.assertFalse((self.repo / '.pi').exists())
        before = self.tree()
        result = generation.generate(self.skill, self.repo / 'does-not-exist', digest_only=True,
                                     expected_digest=first['bundle_sha256'])
        self.assertEqual('current', result['status'])
        self.assertEqual('drift', self.run_generator('--digest', '--expected-digest', '0' * 64, code=1)['status'])
        self.run_generator('--digest', '--expected-digest', first['bundle_sha256'])
        self.assertEqual(before, self.tree())
        self.assertFalse((self.repo / '.pi').exists())

    def test_host_settings_safe_descriptions_and_exact_canonical_pointers(self):
        description = 'Build: "quoted" \\ path\nnext line — café'
        self.manifest['roles'][0]['description'] = description; self.save()
        result = self.run_generator()
        for role in self.manifest['roles']:
            for host in generation.HOSTS:
                suffix = '.codex.yaml' if host == 'codex' else '.' + host + '.md'
                text = (self.skill / ('agents/' + role['name'] + suffix)).read_text()
                self.assertIn(generation.MARKER + result['bundle_sha256'], text)
                self.assertIn('`' + generation.PREFIX + 'references/shared.md`', text)
                self.assertIn('`' + generation.PREFIX + self.manifest['contracts'][role['contract']] + '`', text)
                self.assertNotIn('Implement only the assigned outcome', text)
                description_line = next(line for line in text.splitlines() if line.startswith('description: '))
                self.assertEqual(role['description'], json.loads(description_line.removeprefix('description: ')))
                if host == 'codex':
                    self.assertIn('sandbox_mode: ' + self.manifest['permission_profiles'][role['permissions']]['sandbox_mode'], text)
                    self.assertIn('web_search: disabled', text)
                    self.assertIn('  shell_tool: true', text)
                    self.assertTrue(all(not line or line.startswith('  ') for line in text.split('developer_instructions: |\n', 1)[1].split('\nfeatures:', 1)[0].splitlines()))
                elif host in {'claude', 'copilot'}:
                    self.assertIn('tools: ' + ', '.join(self.manifest['host_settings'][host]['tools']), text)
                if role['contract'] == 'reviewer': self.assertIn('Assigned discipline: code.', text)
            self.assertEqual((self.skill / ('agents/' + role['name'] + '.pi.md')).read_bytes(),
                             (self.repo / ('.pi/agents/' + role['name'] + '.md')).read_bytes())

    def test_source_change_during_check_or_before_write_cannot_report_current(self):
        original = generation._outputs
        source = self.skill / 'references/shared.md'
        for check in (True, False):
            with self.subTest(check=check):
                def change(*args):
                    outputs = original(*args)
                    source.write_text(source.read_text() + '\nConcurrent canonical change.\n')
                    return outputs
                with mock.patch.object(generation, '_outputs', side_effect=change), self.assertRaisesRegex(generation.GenerationError, 'sources changed'):
                    generation.generate(self.skill, self.repo, check=check)
                self.assertFalse((self.skill / 'agents/engram-builder.pi.md').exists())
                self.assertFalse((self.repo / '.pi').exists())

    def test_new_foreign_output_after_scan_is_preserved(self):
        original = generation._sources; calls = 0
        target = self.skill / 'agents/engram-builder.claude.md'
        def change(*args):
            nonlocal calls
            value = original(*args); calls += 1
            if calls == 2: target.write_text('New foreign agent created after conflict scan.\n')
            return value
        with mock.patch.object(generation, '_sources', side_effect=change), self.assertRaisesRegex(generation.GenerationError, 'output changed'):
            generation.generate(self.skill, self.repo)
        self.assertEqual('New foreign agent created after conflict scan.\n', target.read_text())
        self.assertFalse((self.repo / '.pi').exists())
        self.assertFalse((self.skill / 'agents/engram-builder.codex.yaml').exists())

    def test_check_cannot_report_current_after_output_changes_during_source_recheck(self):
        self.run_generator(); original = generation._sources; calls = 0
        target = self.skill / 'agents/engram-builder.claude.md'
        replacement = target.read_text() + '\nConcurrent extra instructions.\n'
        before = self.tree()
        def change(*args):
            nonlocal calls
            value = original(*args); calls += 1
            if calls == 2: target.write_text(replacement)
            return value
        with mock.patch.object(generation, '_sources', side_effect=change), self.assertRaisesRegex(generation.GenerationError, 'output changed'):
            generation.generate(self.skill, self.repo, check=True)
        after = self.tree(); relative = target.relative_to(self.repo).as_posix()
        before.pop(relative); after.pop(relative)
        self.assertEqual(before, after)
        self.assertEqual(replacement, target.read_text())

    def test_foreign_replacement_after_prune_scan_is_preserved(self):
        self.run_generator(); self.manifest['roles'] = self.manifest['roles'][:1]; self.save()
        original = generation._sources; calls = 0
        target = self.skill / 'agents/engram-code-reviewer.pi.md'
        def change(*args):
            nonlocal calls
            value = original(*args); calls += 1
            if calls == 2: target.write_text('Foreign replacement of a removed role.\n')
            return value
        with mock.patch.object(generation, '_sources', side_effect=change), self.assertRaisesRegex(generation.GenerationError, 'inventory changed'):
            generation.generate(self.skill, self.repo)
        self.assertEqual('Foreign replacement of a removed role.\n', target.read_text())
        self.assertTrue((self.skill / 'agents/engram-code-reviewer.claude.md').exists())

    def test_generation_cannot_accept_post_write_edit_as_new_expected_bytes(self):
        original = generation._write; changed = False
        target = self.skill / 'agents/engram-builder.claude.md'
        def edit_after_write(root, path, raw, observed):
            nonlocal changed
            result = original(root, path, raw, observed)
            if not changed:
                changed = True
                path.write_text(path.read_text() + '\nConcurrent extra role instructions.\n')
                return generation._snapshot(root, path)
            return result
        with mock.patch.object(generation, '_write', side_effect=edit_after_write), self.assertRaisesRegex(generation.GenerationError, 'bytes differ'):
            generation.generate(self.skill, self.repo)
        self.assertIn('Concurrent extra role instructions.', target.read_text())
        self.assertEqual('drift', self.run_generator('--check', code=1)['status'])

    def test_copied_digest_does_not_hide_changed_generated_bytes(self):
        self.run_generator()
        path = self.skill / 'agents/engram-builder.codex.yaml'
        original = path.read_text(); path.write_text(original.replace('workspace-write', 'read-only'))
        before = self.tree()
        self.assertIn(str(path), self.run_generator('--check', code=1)['stale'])
        self.assertEqual(before, self.tree())

    def test_foreign_files_survive_conflicts_and_generated_pruning(self):
        path = self.skill / 'agents/engram-builder.claude.md'
        path.write_text('# User-owned unrelated agent at a conflicting name\n')
        before = self.tree()
        self.run_generator(code=2)
        self.assertEqual(before, self.tree())
        path.unlink(); self.run_generator()
        foreign = self.skill / 'agents/engram-personal.pi.md'; foreign.write_text('User-owned specialist.\n')
        foreign_pi = self.repo / '.pi/agents/engram-code-reviewer.md'
        foreign_pi.write_text('User-owned replacement of a formerly generated name.\n')
        self.manifest['roles'] = self.manifest['roles'][:1]; self.save()
        before = self.tree(); drift = self.run_generator('--check', code=1)
        self.assertEqual(4, len(drift['unexpected_generated']))
        self.assertEqual(before, self.tree())
        self.assertEqual(4, self.run_generator()['pruned'])
        self.assertEqual('User-owned replacement of a formerly generated name.\n', foreign_pi.read_text())
        self.assertEqual('User-owned specialist.\n', foreign.read_text())
        self.assertEqual('current', self.run_generator('--check')['status'])

    def test_symlink_contract_or_transitive_reference_is_rejected_without_writes(self):
        outside = self.repo / 'outside.md'; outside.write_text('Preserve outside bytes.\n')
        source = self.skill / 'references/roles/builder.md'
        source.unlink(); source.symlink_to(outside)
        before = self.tree(); self.run_generator(code=2)
        self.assertEqual(before, self.tree())
        source.unlink(); source.write_text('Canonical builder.\n')
        (self.skill / 'references/foreign.md').symlink_to(outside)
        before = self.tree(); self.run_generator('--check', code=2)
        self.assertEqual(before, self.tree())

    def test_symlink_output_or_parent_is_rejected_without_writes(self):
        outside = self.repo / 'outside.md'; outside.write_text('Preserve outside bytes.\n')
        (self.skill / 'agents/engram-builder.pi.md').symlink_to(outside)
        before = self.tree(); self.run_generator(code=2)
        self.assertEqual(before, self.tree())
        (self.skill / 'agents/engram-builder.pi.md').unlink()
        (self.repo / '.pi').symlink_to(self.repo / 'external-agent-dir', target_is_directory=True)
        before = self.tree(); self.run_generator('--check', code=2)
        self.assertEqual(before, self.tree())

    def test_manifest_shape_and_confined_paths_fail_before_generation(self):
        original = copy.deepcopy(self.manifest)
        for mutate in (lambda d: d.update(schema_version=True),
                       lambda d: d.update(shared='../outside.md'),
                       lambda d: d['host_settings']['claude'].update(tools=['Read\nBash']),
                       lambda d: d['roles'].append(copy.deepcopy(d['roles'][0])),
                       lambda d: d['roles'][0].update(contract='missing'),
                       lambda d: d['permission_profiles']['author'].update(sandbox_mode='danger-full-access')):
            self.manifest = copy.deepcopy(original); mutate(self.manifest); self.save()
            before = self.tree(); self.run_generator(code=2)
            self.assertEqual(before, self.tree())
        (self.skill / 'agents/roles.json').write_text('{"schema_version":1,"schema_version":1}')
        before = self.tree(); self.run_generator(code=2)
        self.assertEqual(before, self.tree())
