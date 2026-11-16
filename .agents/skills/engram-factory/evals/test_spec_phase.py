"""Behavioral spec-phase evals: one authored source, machine receipts, exact review."""
import copy
import importlib
import json
from pathlib import Path
import unittest
import test_factory
import test_current_workflow


SINGLE = '''---
schema_version: 2
spec_format: "compact-v1"
size_tier: "S"
scope: "single"
required_reviewers: ["code", "security", "platform"]
---

# Spec: missing inputs are visible

## Outcome
An operator receives a named error when a required input is absent. Existing valid inputs continue to work.

## Scenarios
| ID | Priority | Actor | Journey | Outcome | Requirements | Acceptance | Evidence |
| --- | --- | --- | --- | --- | --- | --- | --- |
| U-1 | P1 | operator | check inputs | absence is visible | REQ-001 | AC-001, AC-002 | app.txt at base |

## Requirements
| ID | Behavior | Rules |
| --- | --- | --- |
| REQ-001 | When an input is absent, report its name. | EN-004 |

## Acceptance
| ID | Requirement | Given | When | Then |
| --- | --- | --- | --- | --- |
| AC-001 | REQ-001 | absent input | check runs | error names input |
| AC-002 | REQ-001 | valid input | check runs | succeeds |

## Boundaries
Only input validation changes. No storage migration or platform mutation.

## Decisions and evidence
LED-1 honored: preserve the stated scope. app.txt is the source inspected at the base revision.
'''


class SpecContentTests(unittest.TestCase):
    def setUp(self):
        self.module = importlib.import_module('spec_document')

    def parse(self, text=SINGLE):
        _, header, body = text.split('---\n', 2)
        fields = {line.split(':', 1)[0]: json.loads(line.split(':', 1)[1])
                  for line in header.splitlines()}
        return self.module.derive(fields, body)

    def test_single_source_validates_and_returns_rules(self):
        derived = self.parse()
        self.assertEqual(['EN-004'], derived['constitution_rules_touched'])
        self.assertNotIn('readiness', derived)
        self.assertNotIn('readiness:', SINGLE)
        self.assertNotIn('sha256', SINGLE)

    def test_missing_duplicate_or_broken_trace_fails(self):
        for text in [SINGLE.replace('## Acceptance', '## Criteria'),
                     SINGLE.replace('| AC-002 |', '| AC-001 |'),
                     SINGLE.replace('| REQ-001 | absent input', '| REQ-999 | absent input'),
                     SINGLE.replace('AC-001, AC-002', 'AC-001'),
                     SINGLE.replace('| operator |', '| |'),
                     SINGLE.replace('report its name.', 'TODO'),
                     SINGLE.replace('EN-004 |', 'unknown |')]:
            with self.subTest(text=text[-120:]), self.assertRaises(ValueError):
                self.parse(text)

    def test_authored_machine_metadata_is_rejected(self):
        for field in ['readiness: {}', 'input_artifacts: []']:
            with self.assertRaises(ValueError):
                self.parse(SINGLE.replace('scope: "single"', 'scope: "single"\n' + field))

    def multi(self):
        return SINGLE.replace('scope: "single"', 'scope: "multi-surface"') + """
## Surfaces
| ID | Capability | Classification | Authority | Entry | Navigation | Scenario | Requirements | Acceptance | Normal | Empty | Loading | Error | Unavailable | Evidence |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| INV-1 | inspect inputs | in-scope | LED-1 | open settings | settings to inputs | U-1 | REQ-001 | AC-001, AC-002 | AC-002 | AC-001 | N/A: synchronous operation | AC-001 | N/A: no external service | App source at base |
| INV-2 | deployment | out-of-scope | LED-1 | - | - | - | - | - | - | - | - | - | - | scope decision |

## Review conditions
| Condition | Evidence or protocol |
| --- | --- |
| Viewports and browser | wide/narrow browser inspection protocol |
| Themes | current light and dark |
| Accessibility | keyboard and named focus checks |
| Visual judgment | owner judgment after actual inspection |
| Reference evidence | source inspected at base; live inspection unavailable |
"""

    def test_multi_surface_canonical_tables_validate_and_return_rules(self):
        result = self.parse(self.multi())
        self.assertEqual(['EN-004'], result['constitution_rules_touched'])
        self.assertNotIn('readiness', result)

    def test_multi_surface_missing_authority_states_or_conditions_fail(self):
        text = self.multi()
        for broken in [text.replace('out-of-scope | LED-1', 'out-of-scope | -'),
                       text.replace('synchronous operation', ''),
                       text.replace('settings to inputs', '-'),
                       text.replace('| Visual judgment |', '| Other condition |'),
                       text.replace('| AC-002 | AC-001 | N/A:', '| AC-999 | AC-001 | N/A:')]:
            with self.subTest(broken=broken[-100:]), self.assertRaises(ValueError):
                self.parse(broken)

    def test_compact_template_is_clean_without_machine_or_methodology_sections(self):
        text = (test_factory.SKILL / 'assets/templates/spec.md').read_text()
        self.assertLessEqual(len(text.splitlines()), 45)
        for noise in ['sha256', 'input_artifacts', 'readiness:', 'mandatory', 'ISO ', '<!--', 'worked example']:
            self.assertNotIn(noise, text)


    def test_comparison_html_and_generic_text_are_not_placeholders(self):
        for value in ['0 < count and count > 2', 'a <button> element', 'a List<T> response']:
            self.parse(SINGLE.replace('report its name.', 'report ' + value + '.'))


    def test_separator_width_must_match_header_and_body(self):
        malformed=SINGLE.replace('| --- | --- | --- | --- | --- |\n| AC-001', '| --- | --- | --- | --- | --- | --- |\n| AC-001')
        with self.assertRaisesRegex(ValueError,'Invalid table separator: Acceptance'):
            self.parse(malformed)


    def test_multi_surface_needs_at_least_one_in_scope_surface(self):
        for classification in ['support', 'infrastructure', 'out-of-scope']:
            text = self.multi().replace('| in-scope | LED-1 |', '| ' + classification + ' | LED-1 |')
            with self.subTest(classification=classification), self.assertRaises(ValueError):
                self.parse(text)

    def test_excluded_and_conditional_surfaces_need_nonblank_authority(self):
        for classification in ['out-of-scope', 'conditional']:
            for authority in ['', '   ', '-']:
                text = self.multi().replace('| out-of-scope | LED-1 |', '| ' + classification + ' | ' + authority + ' |')
                with self.subTest(classification=classification, authority=authority), self.assertRaises(ValueError):
                    self.parse(text)



class SpecControllerTests(unittest.TestCase):
    setUp = test_factory.FactoryTests.setUp
    git = staticmethod(test_factory.FactoryTests.git)
    run_cli = test_factory.FactoryTests.run_cli
    job = test_factory.FactoryTests.job
    fill = test_factory.FactoryTests.fill
    setUpCurrent = test_current_workflow.CurrentWorkflowTests.setUpCurrent

    def write_spec(self):
        self.setUpCurrent()
        (self.jobdir / 'spec.md').write_text(SINGLE)

    def test_prepare_freeze_review_complete_without_authoring_hashes(self):
        self.write_spec()
        before = (self.jobdir / 'spec.md').read_bytes()
        self.run_cli('spec-prepare', '--job', self.job_id)
        receipt = self.jobdir / 'records/spec/receipt.json'
        first = receipt.read_bytes()
        self.run_cli('spec-prepare', '--job', self.job_id)
        self.assertEqual(first, receipt.read_bytes())
        self.assertEqual(before, (self.jobdir / 'spec.md').read_bytes())
        self.run_cli('preflight', '--job', self.job_id, '--stage', 'spec')
        frozen = self.run_cli('review-freeze', '--job', self.job_id, '--stage', 'spec', '--author', 'author-1',
                             '--reviewers-file', 'records/operations/reviewers.json')['result']
        self.assertIn('records/spec/receipt.json', [r['path'] for r in frozen['artifacts']])
        self.run_cli('review-record', '--job', self.job_id, '--stage', 'spec', '--candidate', '1',
                     '--role', 'red-forman', '--invocation', 'reviewer-1', '--verdict', 'PASS',
                     '--findings-file', 'records/operations/findings.json')
        self.run_cli('complete', '--job', self.job_id, '--stage', 'analyze', '--artifacts', 'spec.md')
        self.assertEqual('plan', self.job()['stage'])
        self.assertIn('spec-receipt', self.job()['artifacts'])
        self.run_cli('spec-prepare', '--job', self.job_id, fail=True)

    def test_changed_body_requires_reprepare_and_cannot_rewrite_pass(self):
        self.write_spec()
        self.run_cli('spec-prepare', '--job', self.job_id)
        spec = self.jobdir / 'spec.md'
        spec.write_text(spec.read_text().replace('Only input validation changes.', 'Only scoped input validation changes.'))
        self.run_cli('preflight', '--job', self.job_id, '--stage', 'spec', fail=True)
        self.run_cli('spec-prepare', '--job', self.job_id)
        self.run_cli('review-freeze', '--job', self.job_id, '--stage', 'spec', '--author', 'author-1',
                     '--reviewers-file', 'records/operations/reviewers.json')
        self.run_cli('review-record', '--job', self.job_id, '--stage', 'spec', '--candidate', '1',
                     '--role', 'red-forman', '--invocation', 'reviewer-1', '--verdict', 'PASS',
                     '--findings-file', 'records/operations/findings.json')
        spec.write_text(spec.read_text() + '\nChanged after review.\n')
        before = (self.jobdir / 'records/spec/receipt.json').read_bytes()
        self.run_cli('spec-prepare', '--job', self.job_id, fail=True)
        self.assertEqual(before, (self.jobdir / 'records/spec/receipt.json').read_bytes())

    def test_receipt_tamper_and_missing_receipt_fail_read_only(self):
        self.write_spec()
        self.run_cli('preflight', '--job', self.job_id, '--stage', 'spec', fail=True)
        self.run_cli('spec-prepare', '--job', self.job_id)
        receipt = self.jobdir / 'records/spec/receipt.json'
        data = json.loads(receipt.read_text())
        data['fields']['job_id'] = 'wrong-job'
        receipt.write_text(json.dumps(data))
        self.run_cli('preflight', '--job', self.job_id, '--stage', 'spec', fail=True)

    def test_changes_can_prepare_next_candidate_without_resetting_review_count(self):
        self.write_spec()
        self.run_cli('spec-prepare', '--job', self.job_id)
        self.run_cli('review-freeze', '--job', self.job_id, '--stage', 'spec', '--author', 'author-1',
                     '--reviewers-file', 'records/operations/reviewers.json')
        findings = self.jobdir / 'records/operations/findings.json'
        findings.write_text(json.dumps({'findings': {'F-1': 'Clarify boundary'}, 'resolves': []}))
        self.run_cli('review-record', '--job', self.job_id, '--stage', 'spec', '--candidate', '1',
                     '--role', 'red-forman', '--invocation', 'reviewer-1', '--verdict', 'CHANGES',
                     '--findings-file', 'records/operations/findings.json')
        spec = self.jobdir / 'spec.md'
        spec.write_text(spec.read_text() + '\nBoundary clarified for F-1.\n')
        self.run_cli('spec-prepare', '--job', self.job_id)
        result = self.run_cli('review-freeze', '--job', self.job_id, '--stage', 'spec', '--author', 'author-1',
                             '--reviewers-file', 'records/operations/reviewers.json')
        self.assertEqual(2, result['result']['number'])

    def test_noncompact_spec_is_rejected_without_rewriting(self):
        self.setUpCurrent()
        path = self.jobdir / 'spec.md'
        path.write_text('---\nschema_version: "1.0"\nartifact_type: "spec"\n---\n\n# Old spec\n')
        before = path.read_bytes()
        self.run_cli('spec-prepare', '--job', self.job_id, fail=True)
        self.run_cli('preflight', '--job', self.job_id, '--stage', 'spec', fail=True)
        self.assertEqual(before, path.read_bytes())


    def test_receipt_cannot_override_authored_format_or_reviewers(self):
        self.write_spec()
        self.run_cli('spec-prepare', '--job', self.job_id)
        path = self.jobdir / 'records/spec/receipt.json'
        original = path.read_bytes()
        for key, value in [('spec_format', 'legacy'), ('required_reviewers', ['code']), ('readiness', {})]:
            data = json.loads(original)
            data['fields'][key] = value
            path.write_text(json.dumps(data))
            self.run_cli('preflight', '--job', self.job_id, '--stage', 'spec', fail=True)
            self.run_cli('review-freeze', '--job', self.job_id, '--stage', 'spec', '--author', 'author-1',
                         '--reviewers-file', 'records/operations/reviewers.json', fail=True)
            self.assertFalse((self.jobdir / 'reviews/prebuild/spec').exists())
        path.write_bytes(original)
        self.run_cli('preflight', '--job', self.job_id, '--stage', 'spec')

    def test_receipt_source_base_cannot_silently_drift(self):
        self.write_spec()
        self.run_cli('spec-prepare', '--job', self.job_id)
        path = self.jobdir / 'records/spec/receipt.json'
        data = json.loads(path.read_text())
        data['base_revision'] = '0' * 40
        path.write_text(json.dumps(data))
        failure = self.run_cli('preflight', '--job', self.job_id, '--stage', 'spec', fail=True)
        self.assertIn('source base', failure['error'])


    def test_malformed_receipts_return_clear_errors_without_tracebacks(self):
        self.write_spec()
        self.run_cli('spec-prepare', '--job', self.job_id)
        path = self.jobdir / 'records/spec/receipt.json'
        original = path.read_bytes()
        bad_stamp = json.loads(original)
        bad_stamp['fields']['updated_at'] = 12
        for value in [[], None, {}, bad_stamp]:
            path.write_text(json.dumps(value))
            self.run_cli('preflight', '--job', self.job_id, '--stage', 'spec', fail=True)
            self.run_cli('spec-prepare', '--job', self.job_id, fail=True)
        path.write_bytes(original)



if __name__ == '__main__':
    unittest.main()
