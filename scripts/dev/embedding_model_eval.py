#!/usr/bin/env python3
"""Paired BGE retrieval evaluation over frozen, source-labeled code corpora.

Build the real server and JSONL SDK probe before running:
  cargo build --release -p engram-mcp
  cargo build --release -p engram-integration --example embedding_retrieval_probe --features fastembed
  python3 scripts/dev/embedding_model_eval.py --inputs inputs.json --output /tmp/new-eval

Inputs: {"corpora":[{"id":"flask","root":"/frozen/flask","labels":"/labels.json",
"split":"heldout"}]}. Labels may be an array or an object with `entries`.
Labels can also be grouped by corpus ID in one shared file.
Existing output directories are rejected. No shared stores are modified.
"""
import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import queue
import random
import shutil
import sqlite3
import statistics
import subprocess
import threading
import time

REPO = Path(__file__).resolve().parents[2]
ENV = {k: v for k, v in os.environ.items() if not k.startswith('ENGRAM_EMBEDDING_')}
ENV['FASTEMBED_CACHE_DIR'] = str(REPO / '.fastembed_cache')
PRESETS = ('default', 'local-alternative')


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def write(path, value):
    tmp = path.with_suffix(path.suffix + '.tmp')
    tmp.write_text(json.dumps(value, indent=2) + '\n')
    tmp.replace(path)


class Process:
    def __init__(self, argv, stderr):
        self.errors = stderr.open('w')
        self.proc = subprocess.Popen(argv, cwd=REPO, env=ENV, text=True,
                                     stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=self.errors)
        self.responses = queue.Queue()
        threading.Thread(target=self.read, daemon=True).start()

    def read(self):
        try:
            for line in self.proc.stdout:
                self.responses.put(json.loads(line))
        except Exception as error:
            self.responses.put(error)
        finally:
            self.responses.put(None)

    def send(self, value):
        self.proc.stdin.write(json.dumps(value) + '\n')
        self.proc.stdin.flush()
        result = self.responses.get(timeout=1800)
        if not isinstance(result, dict):
            raise RuntimeError(f'process ended or invalid response: {result}')
        return result

    def close(self):
        self.proc.stdin.close()
        try:
            self.proc.wait(timeout=15)
        except subprocess.TimeoutExpired:
            self.proc.kill()
            self.proc.wait()
        self.errors.close()
        if self.proc.returncode:
            raise RuntimeError(f'process exit {self.proc.returncode}')


class MCP(Process):
    def __init__(self, store, stderr, preset=None, config_path=None):
        args = [str(REPO/'target/release/engram-mcp'), '--tools', 'all', '--storage', str(store),
                '--project', 'embedding-eval']
        if preset and config_path:
            raise ValueError('select a preset or a profile, not both')
        if config_path:
            args += ['--embedding-config', str(config_path)]
        else:
            args += ['--embedding-preset', preset] if preset else ['--no-vector']
        super().__init__(args, stderr)
        self.seq = 0

    def call(self, name, args):
        self.seq += 1
        start = time.monotonic()
        result = self.send({'jsonrpc':'2.0', 'id':self.seq, 'method':'tools/call',
                            'params':{'name':name, 'arguments':args}})
        assert result.get('id') == self.seq and 'error' not in result, result
        payload = result['result']
        assert not payload.get('isError'), result
        return {'seconds':time.monotonic()-start,
                'text':'\n'.join(c.get('text','') for c in payload.get('content', []))}


def records(store):
    with sqlite3.connect(f'file:{store}/engram_data.db?mode=ro', uri=True) as db:
        return [json.loads(r[0]) for r in db.execute('SELECT record_json FROM knowledge_chunks')]


def bind_labels(root, labels, chunks):
    """Bind frozen declarations before retrieval; never inspect query results."""
    targets = {}
    for case in labels:
        units = {}
        for unit in case['requiredUnits']:
            raw = (root/unit['path']).read_bytes()
            if unit.get('sourceSha256'):
                assert hashlib.sha256(raw).hexdigest() == unit['sourceSha256'], unit['path']
            start, end = unit['startByte'], unit['endByte']
            assert 0 <= start < end <= len(raw), unit
            start_line = raw[:start].count(b'\n') + 1
            leaf = unit['qualifiedAnchor'].replace('::','.').split('.')[-1]
            uid = f"{unit['path']}:{start}:{end}"
            found = []
            for chunk in chunks:
                loc = chunk.get('location') or {}
                anchor = loc.get('anchor','').split(' ',1)[-1].replace('::','.').split('.')[-1]
                if (loc.get('path') == unit['path'] and anchor == leaf
                        and abs(loc.get('startLine',-100)-start_line) <= 1):
                    found.append(chunk)
                    targets.setdefault(chunk['id'], set()).add(uid)
                    for entity in chunk.get('entities', []):
                        targets.setdefault(entity['id'], set()).add(uid)
            units[uid] = {'label':unit, 'startLine':start_line, 'indexedChunkIds':[c['id'] for c in found]}
        case['boundUnits'] = units
    return targets


def metrics(hits, case, targets):
    gold = set(case['boundUnits'])
    assert gold
    seen, ranks, dcg = set(), [], 0.0
    for rank, hit in enumerate(hits[:10], 1):
        new = (targets.get(hit['targetId'], set()) & gold) - seen
        if new:
            seen |= new
            ranks.append(rank)
            dcg += 1 / math.log2(rank+1)
    ideal = sum(1/math.log2(i+1) for i in range(1, min(len(gold),10)+1))
    return {'hitAt1':float(bool(ranks) and ranks[0] <= 1),
            'hitAt5':float(bool(ranks) and ranks[0] <= 5), 'hitAt10':float(bool(ranks)),
            'recallAt10':len(seen)/len(gold), 'mrrAt10':1/ranks[0] if ranks else 0.0,
            'ndcgAt10':dcg/ideal, 'requiredUnitsRetrieved':len(seen), 'requiredUnits':len(gold)}


def summarize(cases):
    return {mode:{key:statistics.mean(c[mode]['metrics'][key] for c in cases)
                  for key in ('hitAt1','hitAt5','hitAt10','recallAt10','mrrAt10','ndcgAt10')}
            for mode in ('vector','fused')}


def paired(first, second):
    assert [c['id'] for c in first] == [c['id'] for c in second]
    output = {}
    for mode in ('vector','fused'):
        output[mode] = {}
        for key in ('hitAt10','recallAt10','mrrAt10','ndcgAt10'):
            deltas = [b[mode]['metrics'][key]-a[mode]['metrics'][key] for a,b in zip(first,second)]
            rng = random.Random(20261016)
            samples = sorted(statistics.mean(rng.choices(deltas,k=len(deltas))) for _ in range(5000))
            output[mode][key] = {'delta':statistics.mean(deltas), 'pairedBootstrap95':[samples[124],samples[4874]],
                                 'wins':sum(d>0 for d in deltas), 'losses':sum(d<0 for d in deltas),
                                 'ties':sum(d==0 for d in deltas)}
    return output


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument('--inputs',type=Path,required=True)
    ap.add_argument('--output',type=Path,required=True)
    ap.add_argument('--resume',action='store_true',help='Resume the same frozen run after an interrupted process')
    args = ap.parse_args()
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=args.resume)
    inputs = json.loads(args.inputs.read_text())
    run = {'state':'preparing', 'binarySha256':digest(REPO/'target/release/engram-mcp'),
           'probeSha256':digest(REPO/'target/release/examples/embedding_retrieval_probe'),
           'models':list(PRESETS), 'corpora':{}}
    if args.resume:
        prior = json.loads((out/'run.json').read_text())
        assert prior['binarySha256'] == run['binarySha256'] and prior['probeSha256'] == run['probeSha256']
        run = prior
    write(out/'run.json',run)
    for corpus in inputs['corpora']:
        name, root = corpus['id'], Path(corpus['root']).resolve()
        dest = out/name
        dest.mkdir(exist_ok=args.resume)
        source = Path(corpus['labels'])
        original = json.loads(source.read_text())
        if isinstance(original, list):
            labels = original
        elif 'entries' in original:
            labels = original['entries']
        else:
            labels = original[name]
        labels = [c for c in labels if c['polarity']=='answerable' and c['split']==corpus['split']]
        assert labels
        labels = json.loads(json.dumps(labels))
        manifest = {str(p.relative_to(root)):digest(p) for p in sorted(root.rglob('*'))
                    if p.is_file() and not p.is_symlink() and '.git' not in p.relative_to(root).parts}
        if args.resume and (dest/'source-manifest.json').exists():
            assert json.loads((dest/'source-manifest.json').read_text()) == manifest
            assert json.loads((dest/'questions-frozen.json').read_text()) == labels
        else:
            write(dest/'source-manifest.json', manifest)
            write(dest/'questions-frozen.json', labels)
        result = {'root':str(root), 'sourceLabelsSha256':digest(source), 'sourceFiles':len(manifest),
                  'questions':len(labels), 'questionsSha256':digest(dest/'questions-frozen.json'), 'models':{}}
        if name in run['corpora']:
            prior = run['corpora'][name]
            for key in ('root','sourceLabelsSha256','questionsSha256'):
                assert prior[key] == result[key]
            result = prior
        run['corpora'][name] = result
        write(out/'run.json',run)
        print(name,'scan',flush=True)
        if not (dest/'seed').exists():
            server = MCP(dest/'seed',dest/'scan.stderr.log')
            try:
                result['scan'] = server.call('scan_repo',{'path':str(root)})
                assert 'errors: 0' in result['scan']['text'], result['scan']
            finally:
                server.close()
        chunks = records(dest/'seed')
        targets = bind_labels(root,labels,chunks)
        result['chunks'] = len(chunks)
        result['nonemptyChunks'] = sum(bool(c['text'].strip()) for c in chunks)
        result['requiredUnits'] = sum(len(c['boundUnits']) for c in labels)
        result['indexedRequiredUnits'] = sum(bool(u['indexedChunkIds']) for c in labels for u in c['boundUnits'].values())
        write(dest/'label-bindings.json',labels)
        print(name,'chunks',len(chunks),'indexed gold',result['indexedRequiredUnits'],'/',result['requiredUnits'],flush=True)
        for preset in PRESETS:
            store = dest/preset
            if not store.exists():
                store.mkdir()
                with sqlite3.connect(dest/'seed/engram_data.db') as src, sqlite3.connect(store/'engram_data.db') as dst:
                    src.backup(dst)
                for directory in ('lexical','scan-manifests'):
                    if (dest/'seed'/directory).exists():
                        shutil.copytree(dest/'seed'/directory,store/directory)
            assert records(store) == chunks
            model = result['models'].setdefault(preset, {'indexCalls':[], 'cases':[]})
            if 'summary' in model:
                continue
            server = MCP(store,dest/f'{preset}.index.stderr.log',preset)
            started = time.monotonic()
            try:
                model['capabilities'] = server.call('capability_report',{})
                assert 'vector: supported' in model['capabilities']['text']
                for _ in range(1000):
                    progress = server.call('reindex',{'limit':256})
                    model['indexCalls'].append(progress)
                    write(out/'run.json',run)
                    print(name,preset,progress['text'].replace('\n',' '),round(progress['seconds'],2),flush=True)
                    if 'backlog drained' in progress['text']:
                        break
                    assert 'embedded 0 chunks' not in progress['text'],progress
                else:
                    raise RuntimeError('embedding backlog did not drain')
                model['indexSeconds'] = time.monotonic()-started
            finally:
                server.close()
            with sqlite3.connect(store/'engram_data.db') as db:
                model['vectorRows'] = db.execute('SELECT count(*) FROM vectors_rowids').fetchone()[0]
            assert model['vectorRows'] == result['nonemptyChunks'], model['vectorRows']
            config = {'storage_path':str(store),'trusted_root':str(out),'scope_policy':'Strict',
                      'embedding_provider':{'preset':preset},'migration_mode':'Apply',
                      'capability_policy':'FailClosed','enable_vector':True,
                      'sqlite_storage_layout':{'kind':'single_file','file_name':'engram_data.db'}}
            write(dest/f'{preset}.config.json',config)
            probe = Process([str(REPO/'target/release/examples/embedding_retrieval_probe'),str(dest/f'{preset}.config.json')],
                            dest/f'{preset}.query.stderr.log')
            def request(query):
                return {'query':query,'scope':{'tenant':'default','workspace':'embedding-eval'},
                        'requester':{'actor':{'id':'engram-mcp','kind':'agent'}},'modes':[],
                        'limit':10,'includeExplanations':True}
            try:
                model['warmup'] = probe.send(request('How is application configuration loaded?'))
                model['space'] = model['warmup']['space']
                for case in labels:
                    if any(c['id'] == case['id'] for c in model['cases']):
                        continue
                    response = probe.send(request(case['query']))
                    assert not response['fused'].get('sourceFailures', []),response['fused'].get('sourceFailures', [])
                    scored = {'id':case['id'],'query':case['query'], 'response':response}
                    for mode,hits in [('vector',response['vector']),('fused',response['fused'].get('items', []))]:
                        scored[mode] = {'metrics':metrics(hits,case,targets)}
                    model['cases'].append(scored)
                    write(out/'run.json',run)
                    print(name,preset,case['id'],scored['vector']['metrics']['recallAt10'],scored['fused']['metrics']['recallAt10'],flush=True)
            finally:
                probe.close()
            model['summary'] = summarize(model['cases'])
            model['databaseBytes'] = (store/'engram_data.db').stat().st_size
            write(out/'run.json',run)
        result['paired'] = paired(result['models'][PRESETS[0]]['cases'],result['models'][PRESETS[1]]['cases'])
        for path, sha in manifest.items():
            assert digest(root/path) == sha, path
        assert digest(dest/'questions-frozen.json') == result['questionsSha256']
        result['corpusUnchanged'] = True
        write(out/'run.json',run)
    run['overall'] = {}
    for preset in PRESETS:
        cases = [c for r in run['corpora'].values() for c in r['models'][preset]['cases']]
        run['overall'][preset] = summarize(cases)
    run['overall']['paired'] = paired(
        [c for r in run['corpora'].values() for c in r['models'][PRESETS[0]]['cases']],
        [c for r in run['corpora'].values() for c in r['models'][PRESETS[1]]['cases']])
    run['state'] = 'complete'
    write(out/'run.json',run)
    print('COMPLETE',out,flush=True)


if __name__ == '__main__':
    main()
