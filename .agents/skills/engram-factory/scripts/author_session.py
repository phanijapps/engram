"""Shared spec/planner interface over controller state; no author approval powers.

Invocation names are caller assertions, not authentication. Dispatch requests are
not evidence that a host launched reviewers. Existing controller owns lifecycle.
"""
import hashlib
import json
import re
from pathlib import Path
from types import SimpleNamespace
import spec_document
import plan_document


def require(condition, message):
    if not condition:
        raise ValueError(message)


def fingerprint(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(',', ':')).encode()).hexdigest()


def session(api, jobdir, job, phase='spec'):
    item = job['artifacts'].get(phase + '-session')
    require(item, f'Orchestrator must open a {phase} session before author submission')
    api.intact(jobdir, item)
    data = json.loads(api.inside(jobdir, item['path']).read_text())
    require(data['job_id'] == job['job_id'] and data['cycle'] == job['cycle'] and data['base_revision'] == job['base_sha'],
            'Spec session identity is stale')
    return data


def open_session(namespace, jobdir, job, author, reviewers_file, phase='spec'):
    api = SimpleNamespace(**namespace)
    require(job.get('layout_version') == 3 and job['stage'] == ('analyze' if phase == 'spec' else 'plan') and job['status'] == 'running', f'Open {phase} session only in its active v3 phase')
    require(phase + '-session' not in job['artifacts'], 'Spec session already exists; do not replace reviewer authority')
    raw, _ = plan_document.header(api.inside(jobdir, phase + '.md').read_text())
    require(raw.get(phase + '_format') == 'compact-v1', 'Current sessions require compact-v1')
    parent = api.stage_reviews._dir(jobdir, phase, None)
    require(not api.stage_reviews._candidates(parent), 'Open session before the first review candidate')
    reviewers = api.stage_reviews.validate_reviewers(author, json.loads(api.inside(jobdir, reviewers_file).read_text()))
    for item in job['artifacts'].values():
        api.intact(jobdir, item)
    value = {'schema_version': 1, 'job_id': job['job_id'], 'cycle': job['cycle'], 'base_revision': job['base_sha'],
             'author': author, 'reviewers': reviewers, 'authority': 'unverified caller assertion'}
    path = f'records/{phase}/session-c{job["cycle"]}.json'
    require(not api.inside(jobdir, path).exists(), 'Orphan session receipt; preserve and inspect')
    api.atomic(api.inside(jobdir, path), value)
    job['artifacts'][phase + '-session'] = api.record(jobdir, path)
    return value


def context(namespace, jobdir, job, revision, phase='spec'):
    api = SimpleNamespace(**namespace)
    for item in job['artifacts'].values():
        api.intact(jobdir, item)
    inputs = {key: value for key, value in job['artifacts'].items() if not key.startswith(phase + '-request-')}
    identity = {'job_id': job['job_id'], 'cycle': job['cycle'], 'base_revision': job['base_sha'], 'phase': phase, 'inputs': inputs}
    review = {'verdict': 'NOT_STARTED'}
    reports, findings = [], {}
    parent = api.stage_reviews._dir(jobdir, phase, None)
    candidates = api.stage_reviews._candidates(parent)
    if candidates:
        current = json.loads(candidates[-1].read_text())
        review = api.stage_reviews._status(parent, jobdir, current, check_inputs=False)
        for path in sorted(parent.glob('review-*.json')):
            data = json.loads(path.read_text())
            require(isinstance(data, dict) and isinstance(data.get('role'), str)
                    and isinstance(data.get('findings'), dict), f'Damaged {phase} review receipt: ' + path.name)
            reports.append(api.ref(jobdir, path.relative_to(jobdir).as_posix()))
            for key, value in data.get('findings', {}).items():
                findings.setdefault(data['role'], {})[key] = value
            for key in data.get('resolves', []):
                findings.setdefault(data['role'], {}).pop(key, None)
        outstanding = review.get('outstanding', {})
        if review['verdict'] != 'PENDING':
            findings = {role: {key: text for key, text in entries.items() if key in outstanding.get(role, [])}
                        for role, entries in findings.items() if role in outstanding}
        review['findings_complete'] = review['verdict'] != 'PENDING'
        drift = []
        for item in current['artifacts']:
            try:
                if api.ref(jobdir, item['path']) != item:
                    drift.append({'path': item['path'], 'problem': 'changed since review freeze'})
            except (ValueError, OSError) as exc:
                drift.append({'path': item['path'], 'problem': str(exc)})
        review['applies_to_current_inputs'] = not drift
        review['input_drift'] = drift
    return {**identity, 'board_revision': revision, 'context_sha256': fingerprint(identity),
            'stage': job['stage'], 'status': job['status'], 'job_directory': str(jobdir), 'worktree': job['worktree'],
            'accepted_inputs': inputs, 'review_requests': {k: v for k, v in job['artifacts'].items() if k.startswith(phase + '-request-')}, 'review': review, 'review_receipts': reports, 'outstanding_findings': findings,
            'session': session(api, jobdir, job, phase) if phase + '-session' in job['artifacts'] else None,
            'authority': 'no stage acceptance, Start, Finish or merge granted',
            'dispatch_status': 'review execution is unknown until host evidence exists'}


def check(namespace, jobdir, job, phase='spec'):
    if phase == 'plan':
        return plan_document.check_draft(namespace, jobdir, job)
    result = spec_document.draft(namespace, jobdir, job)
    return {'structural': 'PASS', 'spec': result['spec'], 'independent_review': 'required'}


def operation_path(job, operation, phase='spec'):
    require(isinstance(operation, str) and re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9_-]{0,79}', operation), 'Invalid submission operation ID')
    return f'records/{phase}/submissions/c{job["cycle"]}-{operation}.json'


def replay(namespace, jobdir, job, args, phase='spec'):
    api = SimpleNamespace(**namespace)
    path = operation_path(job, args.operation_id, phase)
    if not api.inside(jobdir, path).exists():
        return None
    key = phase + '-request-' + args.operation_id
    require(key in job['artifacts'], 'Orphan submission request; preserve request/candidate/board and inspect recovery')
    for item in job['artifacts'].values():
        api.intact(jobdir, item)
    value = json.loads(api.inside(jobdir, path).read_text())
    expected = {'author': args.author, 'context_sha256': args.context_sha256, phase + '_sha256': getattr(args, phase + '_sha256'),
                'board_revision': args.expected_revision, 'operation_id': args.operation_id}
    if phase == 'plan':
        expected['draft_sha256'] = args.draft_sha256
    require(value['submission'] == expected, 'Operation ID reused with different submission')
    require(api.digest(api.inside(jobdir, phase + '.md')) == getattr(args, phase + '_sha256'), f'Submitted {phase} changed; cannot replay old review request')
    candidate_path = api.stage_reviews._dir(jobdir, phase, None) / f'candidate-{value["candidate"]:04d}.json'
    require(api.digest(candidate_path) == value['candidate_sha256'], 'Submitted candidate identity changed')
    api.stage_reviews._verify_archives(candidate_path.parent, json.loads(candidate_path.read_text()))
    for item in value['inputs']:
        require(api.ref(jobdir, item['path']) == item, 'Submitted review inputs changed')
    return {'candidate': value['candidate'], 'reviewers': value['reviewers'], 'request': api.ref(jobdir, path),
            'dispatch_status': 'consult-current-context', 'replayed': True, 'dispatch_action': 'do not redispatch on replay', 'authority': f'request only; consult current {phase}-context before acting'}


def submit(namespace, jobdir, job, revision, args, phase='spec'):
    api = SimpleNamespace(**namespace)
    operation_path(job, args.operation_id, phase)
    require(job.get('layout_version') == 3 and job['stage'] == ('analyze' if phase == 'spec' else 'plan') and job['status'] == 'running', f'{phase.title()} submission requires its active v3 phase')
    require(args.expected_revision is not None and args.expected_revision == revision, 'Stale or missing board revision')
    state = context(namespace, jobdir, job, revision, phase)
    shared = session(api, jobdir, job, phase)
    require(args.author == shared['author'], f'Wrong {phase} author invocation')
    require(args.context_sha256 == state['context_sha256'], f'Stale {phase} context')
    require(api.digest(api.inside(jobdir, phase + '.md')) == getattr(args, phase + '_sha256'), f'{phase.title()} bytes changed since submission request')
    require(state['review']['verdict'] in {'NOT_STARTED', 'CHANGES', 'ERROR'}, f'Prior {phase} review is pending, PASS or exhausted; do not resubmit')
    checked = check(namespace, jobdir, job, phase)
    if phase == 'plan':
        require(args.draft_sha256 == checked['draft_sha256'], 'Plan/support bytes changed since author check')
    prepared = api.prepare_spec(jobdir, job) if phase == 'spec' else plan_document.prepare(namespace, jobdir, job)
    stage = 'analyze' if phase == 'spec' else 'plan'
    api.handoff.validate(namespace, jobdir, job, stage, api.handoff.artifacts_for(namespace, jobdir, job, stage))
    require(api.digest(api.inside(jobdir, phase + '.md')) == getattr(args, phase + '_sha256'), f'{phase.title()} bytes changed during preparation')
    paths = api.v3_candidate_paths(jobdir, job, phase)
    expected = [api.ref(jobdir, p) for p in sorted(paths)]
    accepted = {r['path']: {'path': r['path'], 'sha256': r['sha256']} for r in job['artifacts'].values()}
    accepted[phase + '.md'] = {'path': phase + '.md', 'sha256': getattr(args, phase + '_sha256')}
    receipt = spec_document.RECEIPT if phase == 'spec' else plan_document.RECEIPT
    accepted[receipt] = prepared['receipt']
    if phase == 'plan':
        accepted.update({r['path']: r for r in checked['draft_inputs']})
    require(all(r == accepted[r['path']] for r in expected), 'Authoritative review input changed during submission')
    candidate = api.stage_reviews.freeze_candidate(jobdir, phase, None, shared['author'], paths, shared['reviewers'], expected_artifacts=expected)
    for item in job['artifacts'].values():
        api.intact(jobdir, item)
    require(all(api.ref(jobdir, r['path']) == r for r in expected), 'Review inputs changed during freeze; preserve candidate and inspect')
    require(next(r['sha256'] for r in candidate['artifacts'] if r['path'] == phase + '.md') == getattr(args, phase + '_sha256')
            and api.digest(api.inside(jobdir, phase + '.md')) == getattr(args, phase + '_sha256'), f'{phase.title()} bytes changed during freeze; preserve candidate and inspect')
    path = operation_path(job, args.operation_id, phase)
    value = {'schema_version': 1, 'job_id': job['job_id'], 'cycle': job['cycle'], 'candidate': candidate['number'],
             'reviewers': shared['reviewers'], 'author': shared['author'], 'inputs': candidate['artifacts'],
             'candidate_sha256': api.digest(api.stage_reviews._dir(jobdir, phase, None) / f'candidate-{candidate["number"]:04d}.json'),
             'submission': {'author': args.author, 'context_sha256': args.context_sha256, phase + '_sha256': getattr(args, phase + '_sha256'),
                            'board_revision': args.expected_revision, 'operation_id': args.operation_id},
             'dispatch_status': 'needs-dispatch', 'execution_evidence': None,
             'authority': 'unverified caller assertion; not stage acceptance or owner approval'}
    if phase == 'plan':
        value['submission']['draft_sha256'] = args.draft_sha256
    value['phase'] = phase
    require(not api.inside(jobdir, path).exists(), 'Submission request already exists; preserve and inspect')
    api.atomic(api.inside(jobdir, path), value)
    job['artifacts'][phase + '-request-' + args.operation_id] = api.record(jobdir, path)
    api.event(job, phase + '-submit', {'operation_id': args.operation_id, 'candidate': candidate['number']})
    return {'candidate': candidate['number'], 'reviewers': shared['reviewers'], 'request': api.ref(jobdir, path),
            'dispatch_status': 'needs-dispatch', 'replayed': False, 'authority': 'request only; no stage acceptance or owner approval'}
