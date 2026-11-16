"""Compact spec adaptation and controller-owned receipts; no authored hash arrays."""
import hashlib
import json
import re
from pathlib import Path
from types import SimpleNamespace
from spec_content import derive

RECEIPT = 'records/spec/receipt.json'


def receipt_path(spec, relative=RECEIPT):
    root = Path(spec).parent.resolve()
    path = root / relative
    for part in (path, path.parent, path.parent.parent):
        if part.is_symlink():
            raise ValueError('Unsafe spec receipt path')
    if not path.resolve().is_relative_to(root):
        raise ValueError('Spec receipt escapes job')
    return path


def read_receipt(path, kind="spec", role="engram-analyzer"):
    data = json.loads(path.read_text())
    if not isinstance(data, dict) or set(data) != {'schema_version', kind + '_sha256', 'base_revision', 'fields'}:
        raise ValueError('Invalid spec receipt object')
    if (type(data['schema_version']) is not int or data['schema_version'] != 1
            or not isinstance(data[kind + '_sha256'], str) or not re.fullmatch(r'[0-9a-f]{64}', data[kind + '_sha256'])):
        raise ValueError('Invalid spec receipt version or hash')
    generated = data['fields']
    keys = {'schema_version', 'artifact', 'job_id', 'cycle', 'status', 'author_role', 'profiles',
            'source_snapshot', 'input_artifacts', 'updated_at'}
    if not isinstance(generated, dict) or set(generated) != keys:
        raise ValueError('Invalid generated spec fields; author controls format and reviewer applicability')
    if (type(generated['schema_version']) is not int or generated['schema_version'] != 2 or generated['artifact'] != kind or generated['status'] != 'ready'
            or generated['author_role'] != role or generated['source_snapshot'] is not None):
        raise ValueError('Invalid generated spec identity')
    if (not isinstance(generated['job_id'], str) or type(generated['cycle']) is not int or generated['cycle'] < 1
            or not isinstance(generated['updated_at'], str)
            or not isinstance(generated['profiles'], list) or not all(isinstance(v, str) for v in generated['profiles'])
            or not isinstance(generated['input_artifacts'], list)
            or not isinstance(data['base_revision'], str)):
        raise ValueError('Invalid generated spec metadata types')
    for item in generated['input_artifacts']:
        if (not isinstance(item, dict) or set(item) != {'path', 'sha256'}
                or not isinstance(item['path'], str) or not isinstance(item['sha256'], str)
                or not re.fullmatch(r'[0-9a-f]{64}', item['sha256'])):
            raise ValueError('Invalid generated spec input reference')
    return data


def load(spec, fields, body):
    path = receipt_path(spec)
    if not path.is_file():
        raise ValueError('Compact spec needs spec-prepare before preflight')
    data = read_receipt(path)
    if data['spec_sha256'] != hashlib.sha256(Path(spec).read_bytes()).hexdigest():
        raise ValueError('Spec changed since preparation; run spec-prepare before review')
    return {**fields, **data['fields'], **derive(fields, body), 'spec_receipt_base': data['base_revision']}


def draft(namespace, jobdir, job):
    api = SimpleNamespace(**namespace)
    # Raw authored content intentionally needs no generated receipt.
    text = api.inside(jobdir, 'spec.md').read_text()
    api.require(text.startswith('---\n'), 'Spec draft needs frontmatter')
    _, raw, body = text.split('---\n', 2)
    fields = {}
    for line in raw.splitlines():
        key, value = line.split(':', 1)
        api.require(key not in fields, 'Duplicate spec draft field')
        fields[key] = json.loads(value)
    derived = derive(fields, body)
    roles = fields.get('required_reviewers')
    api.require(isinstance(roles, list) and all(isinstance(r, str) for r in roles)
            and len(roles) == len(set(roles)) and api.MANDATORY <= set(roles) <= set(api.REVIEWERS), 'Invalid analysis reviewer set')
    api.require(fields.get('mode') is None or fields['mode'] in api.MODES, 'Invalid analysis mode declaration')
    for item in job['artifacts'].values():
        api.intact(jobdir, item)
    if 'brief' in job['artifacts']:
        for led in set(re.findall(r'LED-\d+', api.inside(jobdir, job['artifacts']['brief']['path']).read_text())):
            api.require(led in body, 'Analysis does not address brief ledger row ' + led)
    return {'fields': fields, 'body': body, 'derived': derived, 'text': text, 'spec': api.ref(jobdir, 'spec.md')}
