"""Compact plans: one authored design/proof map plus controller-owned identity."""
import hashlib
import json
import re
from pathlib import Path
from types import SimpleNamespace
import spec_document
import e2e
from spec_content import table, keyed, ids, require

TEMPLATE_MARKERS = ('<Chosen seam and mechanism, material boundaries/flows, evidence and rejected alternative.>', '<Consequential choices, accepted-spec/brief decisions, source evidence and remaining assumptions.>', '<Failure/rollback triggers, recovery order, compatibility, migration and deployment implications when relevant.>', '<What changes, the order of work, and how success will be recognized.>', '<bounded action>', '<observable change>', '<owned paths>', '<result distinguishing success from failure>', '<runnable command or manual protocol>')

RECEIPT = 'records/plan/receipt.json'
CONTEXT = ('context/architecture.md', 'context/data-models.md', 'context/interfaces.md', 'context/contracts.md')


def header(text):
    require(text.startswith('---\n'), 'Plan needs compact frontmatter')
    _, raw, body = text.split('---\n', 2)
    fields = {}
    for line in raw.splitlines():
        key, value = line.split(':', 1)
        require(key not in fields, 'Duplicate plan field')
        fields[key] = json.loads(value)
    return fields, body


def derive(fields, body, spec):
    require({'schema_version', 'plan_format', 'size_tier'} <= set(fields) <= {'schema_version', 'plan_format', 'size_tier', 'e2e'}
            and type(fields['schema_version']) is int and fields['schema_version'] == 2 and fields['plan_format'] == 'compact-v1'
            and fields['size_tier'] in {'S', 'M', 'L'}, 'Invalid compact plan header; do not author machine metadata')
    require(not re.search(r'\b(?:TBD|TODO|FIXME)\b|\[NEEDS CLARIFICATION', body), 'Unresolved plan content')
    require(not any(marker in body for marker in TEMPLATE_MARKERS), 'Unresolved plan template marker')
    for heading in ('Owner summary', 'Approach', 'Rollback and risks', 'Decisions and evidence'):
        match = re.search(r'^## ' + heading + r'\s*\n(.*?)(?=^## |\Z)', body, re.M | re.S)
        require(match and len(match[1].strip()) >= 20, 'Missing substantive ' + heading)
    steps = keyed(table(body, 'Steps', ['ID', 'Action', 'Requirements', 'Depends on', 'Owned paths', 'Proof']), r'STEP-\d{3}', 'step')
    gates = keyed(table(body, 'Proof', ['ID', 'Acceptance', 'Command or protocol', 'Expected']), r'GATE-\d{3}', 'proof')
    require(re.search(r'^spec_format:\s*"compact-v1"', spec, re.M), 'Unsupported spec format: compact-v1 required')
    _, _, spec_body = spec.split('---\n', 2)
    requirements = set(keyed(table(spec_body, 'Requirements', ['ID', 'Behavior', 'Rules']), r'REQ-\d{3}', 'requirement'))
    ac_req = {r['ID']: r['Requirement'] for r in table(spec_body, 'Acceptance', ['ID', 'Requirement', 'Given', 'When', 'Then'])}
    acceptance = set(ac_req)
    require(requirements and acceptance and set(ac_req) == acceptance and set(ac_req.values()) <= requirements,
            'Accepted spec needs canonical REQ/AC definitions and complete AC-to-REQ links')
    covered_req, covered_ac, used_gates, edges = set(), set(), set(), {}
    for sid, row in steps.items():
        reqs, proofs = ids(row['Requirements'], 'REQ'), ids(row['Proof'], 'GATE')
        require(set(reqs) <= requirements and set(proofs) <= set(gates), 'Unknown step requirement or proof')
        dependencies = [] if row['Depends on'] == 'none' else ids(row['Depends on'], 'STEP')
        require(set(dependencies) <= set(steps), 'Unknown step dependency')
        edges[sid] = dependencies
        paths = [p.strip().strip('`') for p in row['Owned paths'].split(',')]
        require(paths and len(paths) == len(set(paths)) and all(p and p not in {'-', 'unknown', 'N/A', 'none'} and ':' not in p and str(Path(p)) == p and not p.startswith('/') and '\\' not in p and '..' not in Path(p).parts
                              and p != '.' and not any(c in p for c in '*?[]') for p in paths), 'Explicit canonical owned paths required')
        proof_requirements = {ac_req[a] for gate in proofs for a in ids(gates[gate]['Acceptance'], 'AC') if a in ac_req}
        require(set(reqs) <= proof_requirements, 'Step requirement/proof acceptance mismatch: ' + sid)
        covered_req.update(reqs)
        used_gates.update(proofs)
    visiting, visited = set(), set()
    def visit(sid):
        require(sid not in visiting, 'Plan dependency cycle')
        if sid in visited:
            return
        visiting.add(sid)
        for dep in edges[sid]:
            visit(dep)
        visiting.remove(sid)
        visited.add(sid)
    for sid in steps:
        visit(sid)
    for row in gates.values():
        acs = ids(row['Acceptance'], 'AC')
        require(set(acs) <= acceptance, 'Proof references unknown acceptance')
        require(row['Command or protocol'] != '-' and len(row['Expected']) >= 8, 'Proof needs executable command/manual protocol and expected result')
        covered_ac.update(acs)
    require(covered_req == requirements and covered_ac == acceptance and used_gates == set(gates), 'Plan must cover every requirement/AC through used proofs')
    attachments = []
    if re.search(r'^## Supporting documents\s*$', body, re.M):
        for row in table(body, 'Supporting documents', ['Path', 'Purpose']):
            require(row['Path'] in CONTEXT and row['Path'] not in attachments, 'Noncanonical or duplicate supporting document')
            require(len(row['Purpose'].strip()) >= 20 and row['Purpose'] != '-', 'Supporting document needs a distinct purpose')
            attachments.append(row['Path'])
    e2e_contract = e2e.policy(fields, body, acceptance)
    scenarios = {r['ID']: r for r in e2e_contract['scenarios']} if e2e_contract else {}
    for row in gates.values():
        command = row['Command or protocol']
        if command.startswith('E2E:'):
            require(re.fullmatch(r'E2E: E2E-\d{3}(?:,\s*E2E-\d{3})*', command), 'Invalid E2E proof reference')
            targets = re.findall(r'E2E-\d{3}', command)
            require(len(targets) == len(set(targets)) and set(targets) <= set(scenarios), 'Unknown or duplicate E2E proof reference')
            covered = {ac for target in targets for ac in ids(scenarios[target]['Acceptance'], 'AC')}
            require(set(ids(row['Acceptance'], 'AC')) <= covered, 'E2E proof acceptance mismatch')
    return {'steps': list(steps), 'attachments': attachments, 'e2e': e2e_contract}


def receipt_path(plan):
    return spec_document.receipt_path(plan, RECEIPT)


def load(plan, fields, body):
    path = receipt_path(plan)
    require(path.is_file(), 'Compact plan needs plan-prepare before preflight')
    data = spec_document.read_receipt(path, 'plan', 'engram-planner')
    require(data['plan_sha256'] == hashlib.sha256(Path(plan).read_bytes()).hexdigest(), 'Plan changed since preparation')
    # Derive only from the accepted source document, whose hash is a required input.
    spec = Path(plan).parent / 'spec.md'
    derive(fields, body, spec.read_text())
    return {**fields, **data['fields'], 'plan_receipt_base': data['base_revision']}


def validate_supports(api, jobdir, paths):
    for relative in paths:
        raw = Path(jobdir) / relative
        require(not raw.is_symlink() and not raw.parent.is_symlink(), 'Supporting document symlink is not allowed')
        content = api.inside(jobdir, relative).read_text()
        require(not content.startswith('---\n'), 'Supporting documents must be plain Markdown, not artifact metadata')
        require(len(content.strip()) > 80, 'Supporting document lacks substantive content')
        require(not re.search(r'\b(?:TBD|TODO|FIXME)\b|\[NEEDS CLARIFICATION|\{\{[a-z_][a-z0-9_-]*\}\}', content),
                'Unresolved supporting document content: ' + relative)


def validate(namespace, jobdir, job, artifacts):
    api = SimpleNamespace(**namespace)
    fields, body = api.metadata(api.inside(jobdir, 'plan.md'))
    require(fields.get('plan_receipt_base') == job['base_sha'], 'Plan receipt source base changed')
    authored, _ = header(api.inside(jobdir, 'plan.md').read_text())
    info = derive(authored, body, api.inside(jobdir, 'spec.md').read_text())
    require(artifacts == ['plan.md'] + info['attachments'], 'Compact plan handoff must use declared supporting documents in order')
    present = {p for p in CONTEXT if api.inside(jobdir, p).is_file()}
    require(present == set(info['attachments']), 'Undeclared supporting documents: ' + str(present - set(info['attachments'])))
    validate_supports(api, jobdir, info['attachments'])
    upstream = [job['artifacts'][key]['path'] for key in ('spec', 'rework') if key in job['artifacts']]
    api.validate_artifact(jobdir, 'plan.md', job, 'plan', {'engram-planner'}, upstream + info['attachments'], None)
    expected = ['plan'] + [Path(p).stem for p in info['attachments']]
    return {'expected': expected, 'required': upstream, 'fields': {key: fields for key in expected}}


def draft(namespace, jobdir, job):
    api = SimpleNamespace(**namespace)
    for item in job['artifacts'].values():
        api.intact(jobdir, item)
    path = api.inside(jobdir, 'plan.md')
    text = path.read_text()
    fields, body = header(text)
    info = derive(fields, body, api.inside(jobdir, 'spec.md').read_text())
    require({p for p in CONTEXT if api.inside(jobdir, p).is_file()} == set(info['attachments']), 'Declare each supporting document and its purpose')
    refs = [api.ref(jobdir, p) for p in ['plan.md'] + info['attachments']]
    validate_supports(api, jobdir, info['attachments'])
    require(path.read_text() == text and all(api.ref(jobdir, r['path']) == r for r in refs), 'Plan/support bytes changed during draft check')
    return {'text': text, 'fields': fields, 'info': info, 'draft_inputs': refs}


def check_draft(namespace, jobdir, job):
    value = draft(namespace, jobdir, job)
    refs = value['draft_inputs']
    digest = hashlib.sha256(json.dumps(refs, sort_keys=True, separators=(',', ':')).encode()).hexdigest()
    return {'structural': 'PASS', 'plan': refs[0], 'draft_sha256': digest, 'draft_inputs': refs, 'independent_review': 'required'}


def prepare(namespace, jobdir, job):
    api = SimpleNamespace(**namespace)
    require(job.get('layout_version') == 3 and job['stage'] == 'plan' and job['status'] == 'running', 'Plan preparation requires active v3 planning')
    for item in job['artifacts'].values():
        api.intact(jobdir, item)
    review_dir = api.stage_reviews._dir(jobdir, 'plan', None)
    if review_dir.exists() and api.stage_reviews._candidates(review_dir):
        with api.stage_reviews._locked(review_dir):
            candidates = api.stage_reviews._candidates(review_dir)
            current = json.loads(candidates[-1].read_text())
            status = api.stage_reviews._status(review_dir, jobdir, current, check_inputs=False)
            require(status['verdict'] in {'CHANGES', 'ERROR'} and len(candidates) < 3, 'Do not alter pending, PASS or exhausted plan inputs')
    path = api.inside(jobdir, 'plan.md')
    checked = draft(namespace, jobdir, job)
    text, fields, info = checked['text'], checked['fields'], checked['info']
    inputs = [job['artifacts'][key]['path'] for key in ('spec', 'rework') if key in job['artifacts']] + info['attachments']
    refs = [api.ref(jobdir, p) for p in inputs]
    validate_supports(api, jobdir, info['attachments'])
    generated = {'schema_version': 2, 'artifact': 'plan', 'job_id': job['job_id'], 'cycle': job['cycle'],
                 'status': 'ready', 'author_role': 'engram-planner', 'profiles': job['profiles'],
                 'source_snapshot': None, 'input_artifacts': refs}
    receipt = {'schema_version': 1, 'plan_sha256': api.digest(path), 'base_revision': job['base_sha'], 'fields': generated}
    target = receipt_path(path)
    if target.exists():
        old = spec_document.read_receipt(target, 'plan', 'engram-planner')
        old_fields = dict(old['fields'])
        old_fields.pop('updated_at')
        if {**old, 'fields': old_fields} == receipt:
            return {'receipt': api.ref(jobdir, RECEIPT), 'changed': False}
    generated['updated_at'] = api.now()
    require(path.read_text() == text and all(api.ref(jobdir, r['path']) == r for r in refs), 'Plan inputs changed during preparation')
    api.atomic(target, receipt)
    return {'receipt': api.ref(jobdir, RECEIPT), 'changed': True, 'independent_review': 'required'}
