"""Optional, plan-bound end-to-end execution; no remote execution or approval powers."""
import datetime as dt
import hashlib
import json
import re
from types import SimpleNamespace
from spec_content import table, keyed, ids, require

MAX_ATTEMPTS = 3
COLUMNS = ['ID', 'Acceptance', 'Command or protocol', 'Expected', 'Environment prerequisites']


def policy(fields, body, acceptance):
    enabled = fields.get('e2e', False)
    require(type(enabled) is bool, 'e2e must be a boolean')
    present = bool(re.search(r'^## E2E\s*$', body, re.M))
    require(enabled == present, 'Enabled E2E needs its scenario table; disabled E2E must not declare scenarios')
    if not enabled:
        return None
    rows = keyed(table(body, 'E2E', COLUMNS), r'E2E-\d{3}', 'E2E scenario')
    require(rows, 'E2E needs at least one scenario')
    for row in rows.values():
        require(set(ids(row['Acceptance'], 'AC')) <= acceptance, 'E2E references unknown acceptance')
        require(all(isinstance(row[k], str) and len(row[k].strip()) >= 8 and row[k].strip() != '-'
                    for k in COLUMNS[2:]), 'E2E needs a command/protocol, expected outcome and environment prerequisites')
    return {'scenarios': list(rows.values())}


def pin(job, fields, body, plan):
    """Only accepted compact plans can select a new phase; no post-Start flag."""
    if 'e2e' in fields:
        require(type(fields['e2e']) is bool and job.get('layout_version') == 3
                and fields.get('plan_format') == 'compact-v1', 'E2E selection requires a v3 compact plan')
    if fields.get('e2e', False):
        require(job.get('layout_version') == 3 and fields.get('plan_format') == 'compact-v1',
                'E2E opt-in requires a v3 compact plan')
        job['e2e'] = {'plan': plan, 'scenarios': list(keyed(table(body, 'E2E', COLUMNS), r'E2E-\d{3}', 'E2E scenario').values()),
                      'attempts': 0, 'pending': None, 'passed': None}
    else:
        require('e2e' not in job, 'E2E policy cannot be removed without owner rework')


def check_policy(namespace, jobdir, job):
    """Absent execution state is unknown, never an opt-out from an accepted plan."""
    api = SimpleNamespace(**namespace)
    plan = job['artifacts'].get('plan')
    if not plan:
        require('e2e' not in job, 'E2E state has no accepted plan')
        return
    api.intact(jobdir, plan)
    fields, body = api.metadata(api.inside(jobdir, plan['path']))
    enabled = fields.get('e2e', False)
    require(type(enabled) is bool, 'Accepted e2e selection is invalid')
    require(enabled == ('e2e' in job), 'E2E execution state differs from accepted plan selection')
    if not enabled:
        return
    require(job.get('layout_version') == 3 and fields.get('plan_format') == 'compact-v1',
            'E2E requires a v3 compact accepted plan')
    state = job['e2e']
    expected = list(keyed(table(body, 'E2E', COLUMNS), r'E2E-\d{3}', 'E2E scenario').values())
    require(isinstance(state, dict) and state.get('plan') == {k: plan[k] for k in ('path', 'sha256')}
            and state.get('scenarios') == expected, 'Pinned E2E policy differs from accepted plan')
    require(type(state.get('attempts')) is int and 0 <= state['attempts'] <= MAX_ATTEMPTS,
            'Invalid E2E attempt state')


def begin(namespace, jobdir, job, reviewers):
    api = SimpleNamespace(**namespace)
    state = job['e2e']
    require(state['plan'] == {k: job['artifacts']['plan'][k] for k in ('path', 'sha256')}, 'Accepted E2E plan changed')
    require(state['attempts'] < MAX_ATTEMPTS, 'E2E attempts exhausted; owner rework required')
    state['passed'] = None
    state['pending'] = {'snapshot': dict(job['snapshot']), 'build': dict(job['artifacts']['build']),
                        'reviewers': list(reviewers), 'builder': job['build_invocation']}
    job['stage'], job['status'] = 'e2e', 'running'


def _report(api, jobdir, job, path, expected_attempt):
    state = job['e2e']
    data = json.loads(api.inside(jobdir, path).read_text())
    require(isinstance(data, dict), 'E2E report must be an object')
    required = {'schema_version', 'job_id', 'cycle', 'attempt', 'author_role', 'invocation_id',
                'source_snapshot', 'plan', 'head_sha', 'environment_sha256', 'observed_at', 'scenarios'}
    require(set(data) == required and type(data['schema_version']) is int and data['schema_version'] == 1 and data['job_id'] == job['job_id']
            and type(data['cycle']) is int and data['cycle'] == job['cycle']
            and type(data['attempt']) is int and data['attempt'] == expected_attempt,
            'Wrong E2E report identity or attempt')
    require(data['author_role'] == 'engram-qa' and isinstance(data['invocation_id'], str)
            and data['invocation_id'].strip() and data['invocation_id'] != job.get('build_invocation'),
            'E2E requires an independent QA invocation')
    snap = json.loads(api.inside(jobdir, job['snapshot']['path']).read_text())
    require(data['source_snapshot'] == {k: job['snapshot'][k] for k in ('path', 'sha256')}
            and data['plan'] == state['plan'] and data['head_sha'] == snap['head_sha']
            and data['environment_sha256'] == snap['environment_sha256'], 'E2E candidate, plan or environment changed')
    require(isinstance(data['observed_at'], str), 'E2E observation needs a timestamp')
    stamp = dt.datetime.fromisoformat(data['observed_at'].replace('Z', '+00:00'))
    require(stamp.tzinfo is not None, 'E2E observation needs timezone')
    declared = {r['ID']: r for r in state['scenarios']}
    require(isinstance(data['scenarios'], list) and len(data['scenarios']) == len(declared), 'E2E must report every declared scenario')
    seen, refs = set(), []
    for row in data['scenarios']:
        require(isinstance(row, dict) and set(row) == {'id', 'verdict', 'method', 'command', 'exit_code', 'observed', 'evidence'},
                'Invalid E2E scenario result')
        sid = row['id']
        require(isinstance(sid, str) and sid in declared and sid not in seen, 'Duplicate or unknown E2E scenario')
        seen.add(sid)
        require(isinstance(row['verdict'], str) and isinstance(row['method'], str)
                and row['verdict'] in {'PASS', 'FAIL', 'NOT_RUN', 'BLOCKED'} and row['method'] in {'command', 'manual'},
                'Invalid E2E scenario verdict/method')
        require(row['command'] == declared[sid]['Command or protocol'], 'E2E command/protocol differs from accepted plan')
        method = 'manual' if declared[sid]['Command or protocol'].startswith('MANUAL:') else 'command'
        require(row['method'] == method, 'E2E execution method must match accepted command/protocol')
        require(isinstance(row['observed'], str) and len(row['observed'].strip()) >= 8, 'E2E needs an actual result or blocker')
        require(isinstance(row['evidence'], list), 'Invalid E2E evidence list')
        if row['verdict'] in {'PASS', 'FAIL'}:
            require(row['evidence'], 'Executed E2E scenario requires evidence')
            require((row['method'] == 'manual' and row['exit_code'] is None)
                    or (row['method'] == 'command' and type(row['exit_code']) is int), 'Executed E2E command needs its actual exit code')
            if row['verdict'] == 'PASS' and row['method'] == 'command':
                require(row['exit_code'] == 0, 'E2E PASS requires successful command exit')
        else:
            require(row['exit_code'] is None, 'Unexecuted E2E cannot claim an exit code')
        for item in row['evidence']:
            require(isinstance(item, dict) and set(item) == {'path', 'sha256'}
                    and isinstance(item['path'], str) and isinstance(item['sha256'], str)
                    and re.fullmatch(r'[0-9a-f]{64}', item['sha256']) and item['path'] != path,
                    'Invalid or self-referencing E2E evidence')
            require(api.ref(jobdir, item['path']) == item, 'Missing or changed E2E evidence')
            refs.append(item)
    return data, refs


def complete(namespace, repo, jobdir, job, args, start_review):
    api = SimpleNamespace(**namespace)
    require(job.get('layout_version') == 3 and 'e2e' in job and job['stage'] == 'e2e'
            and job['status'] == 'running', 'No active E2E phase')
    state = job['e2e']
    require(state['attempts'] < MAX_ATTEMPTS and state['pending'], 'E2E attempts exhausted or no candidate')
    pending = state['pending']
    require(job['snapshot'] == pending['snapshot'] and job['artifacts']['build'] == pending['build']
            and job['build_invocation'] == pending['builder'], 'E2E pending candidate changed')
    api.source_check(repo, jobdir, job)
    attempt = state['attempts'] + 1
    path = f'records/e2e/report-c{job["cycle"]}-a{attempt}.json'
    require(args.artifacts == [path], 'E2E needs canonical report ' + path)
    report, evidence = _report(api, jobdir, job, path, attempt)
    report_ref = api.ref(jobdir, path)
    for item in evidence:
        key = 'e2e-evidence-' + hashlib.sha256(item['path'].encode()).hexdigest() + '-' + str(attempt)
        recorded = api.record(jobdir, item['path'])
        require({k: recorded[k] for k in ('path', 'sha256')} == item, 'E2E evidence changed during ingestion')
        job['artifacts'][key] = recorded
    job['artifacts']['e2e-attempt-' + str(attempt)] = api.record(jobdir, path)
    api.source_check(repo, jobdir, job)
    require(api.ref(jobdir, path) == report_ref, 'E2E report changed during ingestion')
    # Revalidate the complete report/evidence edge after archiving, not only each file.
    rechecked, _ = _report(api, jobdir, job, path, attempt)
    require(rechecked == report, 'E2E report changed during ingestion')
    state['attempts'] = attempt
    verdicts = {r['verdict'] for r in report['scenarios']}
    api.event(job, 'complete-e2e', {'attempt': attempt, 'report': report_ref, 'verdicts': sorted(verdicts)})
    if verdicts == {'PASS'}:
        state['passed'] = {'report': report_ref, 'snapshot': dict(job['snapshot']), 'build': dict(job['artifacts']['build'])}
        state['pending'] = None
        start_review(jobdir, job, pending['reviewers'])
    elif attempt >= MAX_ATTEMPTS:
        api.block(job, 'E2E attempts exhausted; owner decision required', 'e2e-exhausted')
    elif 'FAIL' in verdicts:
        state['pending'], state['passed'] = None, None
        job['stage'], job['status'], job['snapshot'] = 'build', 'running', None
    else:
        api.block(job, 'E2E environment/prerequisites unavailable; resolve before retry')
    return job


def require_pass(namespace, jobdir, job):
    api = SimpleNamespace(**namespace)
    state = job.get('e2e')
    if not state:
        return
    passed = state.get('passed')
    require(passed and passed['snapshot'] == job['snapshot'] and passed['build'] == job['artifacts'].get('build'),
            'Current candidate needs independent E2E PASS')
    require(api.ref(jobdir, passed['report']['path']) == passed['report'], 'E2E report changed')
    report, _ = _report(api, jobdir, job, passed['report']['path'], state['attempts'])
    require(all(r['verdict'] == 'PASS' for r in report['scenarios']), 'E2E report is not PASS')
