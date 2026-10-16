"""Offline canonical judgments and exact observations; never supplied to models."""
import hashlib
import json
import re
import sqlite3
from pathlib import Path


def canonical_records(store):
    with sqlite3.connect('file:' + str(Path(store) / 'engram_data.db') + '?mode=ro', uri=True) as db:
        return [json.loads(row[0]) for row in db.execute('SELECT record_json FROM knowledge_chunks')]


def judgments(entry, records):
    result = []
    for expected in entry['requiredUnits']:
        found = []
        for record in records:
            code = record.get('code')
            if not code or code['file']['path'] != expected['path']:
                continue
            anchor = (record.get('location', {}).get('anchor') or '').partition(' ')[2].replace('::', '.')
            if anchor != expected['qualifiedAnchor'].replace('::', '.'):
                continue
            span = code.get('declarationRange')
            if span != {'start': expected['startByte'], 'end': expected['endByte']}:
                continue
            if code['sourceHash'].removeprefix('sha256:') != expected['sourceSha256']:
                raise ValueError('judgment source hash mismatch')
            found.append(code['unit'])
        if len(set(found)) != 1:
            raise ValueError(f"canonical judgment missing or ambiguous: {entry['id']}:{expected['path']}:{expected['qualifiedAnchor']}")
        result.append({'parentId': found[0], 'grade': expected['grade'],
                       'requiredRanges': [[r['startByte'], r['endByte']] for r in expected['requiredRanges']]})
    if len({r['parentId'] for r in result}) != len(result):
        raise ValueError('different judgments resolve to one parent')
    return result


def candidate_parents(rows):
    seen = set()
    result = []
    for row in rows:
        parent = row['parentUnitId']
        if row['category'] == 'implementation' and parent not in seen:
            seen.add(parent)
            result.append(parent)
    return result[:50]


def packet_observations(packet, corpus_root, corpus):
    def observe(item):
        parent = item.get('parentUnitId', item.get('unitId'))
        ranges = []
        for part in item['excerpts']:
            ref = part['source']
            path = Path(corpus_root) / ref['path']
            if ref['path'] not in corpus['files'] or not path.resolve().is_relative_to(Path(corpus_root).resolve()):
                raise ValueError('evidence path outside frozen inventory')
            raw = path.read_bytes()
            if hashlib.sha256(raw).hexdigest() != corpus['files'][ref['path']] or ref['sourceHash'].removeprefix('sha256:') != corpus['files'][ref['path']]:
                raise ValueError('evidence source hash mismatch')
            if not 0 <= ref['startByte'] < ref['endByte'] <= len(raw):
                raise ValueError('evidence range outside source')
            if ref['unitId'] != parent or raw[ref['startByte']:ref['endByte']].decode() != part['text']:
                raise ValueError('evidence excerpt/unit mismatch')
            ranges.append([ref['startByte'], ref['endByte']])
        return {'parentId': parent, 'excerpts': ranges}
    return [observe(i) for i in packet['items']], [observe(i) for i in packet['supportingUnits']]


def _chunk_label(text):
    return next((line.strip()[:60] for line in text.splitlines() if line.strip()), '(empty chunk)')


def _prefix_ranges(record, visible):
    """Map only the actual emitted canonical prefix; separator bytes are not code."""
    code = record['code']
    remaining = len(visible.encode())
    result = []
    for i, span in enumerate(code['ranges']):
        if i:
            remaining = max(0, remaining - len(code['sliceSeparator'].encode()))
        size = min(remaining, span['end'] - span['start'])
        if size:
            result.append([span['start'], span['start'] + size])
        remaining -= size
        if not remaining:
            break
    return result


HEADER = re.compile(r'^(.+) — (.+) \[([^\n]*\brrf=[^\n]*)\]$', re.MULTILINE)


def baseline_observations(text, records):
    """Conservative legacy reconstruction; every header occupies its original slot.

    Exact visible prefix disambiguates equal 60-character chunk labels. Unknown
    headers keep grade-zero placeholders, and an entity header supplies no code.
    """
    matches = list(HEADER.finditer(text))
    items, unresolved = [], []
    for rank, header in enumerate(matches[:10]):
        label, source, _scores = header.groups()
        name, marker, kind = label.rpartition(' (') if label.endswith(')') else (label, '', '')
        if marker:
            kind = kind[:-1]
        else:
            name = label
        _repo, separator, path = source.rpartition(', ')
        if not separator:
            path = None
        candidates = []
        body = text[header.end() + 1:matches[rank + 1].start() if rank + 1 < len(matches) else len(text)]
        for record in records:
            code = record.get('code')
            if not code or code['file']['path'] != path:
                continue
            if kind == 'Chunk':
                prefix = record['text'][:500]
                if _chunk_label(record['text']) == name and body.startswith(prefix):
                    candidates.append((code['unit'], _prefix_ranges(record, prefix)))
            elif any(e.get('name') == name for e in record.get('entities', [])):
                candidates.append((code['unit'], []))
        unique = {parent for parent, _ in candidates}
        if len(unique) == 1:
            parent = next(iter(unique))
            ranges = next(r for p, r in candidates if p == parent)
            items.append({'parentId': parent, 'excerpts': ranges})
        else:
            placeholder = f'legacy-unjudged-rank-{rank + 1}'
            items.append({'parentId': placeholder, 'excerpts': []})
            unresolved.append({'rank': rank + 1, 'name': name, 'kind': kind, 'path': path, 'matchingParents': len(unique)})
    return items, unresolved
