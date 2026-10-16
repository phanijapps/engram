"""Frozen evaluation process, corpus and identity helpers shared by run phases."""
import json
import queue
from pathlib import Path
from code_eval_corpus import sha256, verify_corpus, verify_labels
from code_query_ablation import QueryProbe
from code_evidence_judgments import judgments

ROOT = Path(__file__).resolve().parents[2]
VARIANTS = {
    'segmented': {'profile': 'balanced-v1', 'fusion': {'rerank': {'strategy': 'none', 'lambda': .5}}},
    'balanced': {'profile': 'balanced-v1'},
    'fast': {'profile': 'fast-v1'},
}

class ProbeProtocolError(RuntimeError):
    """No further input may be sent to this uncorrelated child process."""


class JsonProbe(QueryProbe):
    def __init__(self, command):
        super().__init__(command)
        self.poisoned=False

    def exchange(self, payload, timeout=120):
        if self.poisoned:
            raise ProbeProtocolError('evaluation helper is poisoned; close/restart before another query')
        try:
            self.process.stdin.write(json.dumps(payload) + '\n')
            self.process.stdin.flush()
        except OSError as error:
            self.poisoned=True
            raise ProbeProtocolError('evaluation helper write failed') from error
        try:
            raw = self.responses.get(timeout=timeout)
        except queue.Empty as error:
            self.poisoned=True
            raise ProbeProtocolError('evaluation helper deadline exceeded') from error
        if raw is None:
            self.poisoned=True
            raise ProbeProtocolError('evaluation helper exited: ' + '\n'.join(self.errors[-5:]))
        try:
            result = json.loads(raw)
        except (ValueError,TypeError) as error:
            self.poisoned=True
            raise ProbeProtocolError('malformed evaluation helper response') from error
        if not isinstance(result,dict):
            self.poisoned=True
            raise ProbeProtocolError('evaluation helper response must be an object')
        if 'error' in result:
            raise RuntimeError('evaluation helper: ' + result['error'])
        return result

    def close(self,grace=10):
        self.poisoned=True
        return super().close(grace)


def selected_frozen(manifest_path, corpus_id, corpus_root, split):
    """Verify opaque combined label bytes, then traverse selected judgments only.

    JSON envelope parsing materializes the file; unselected query/judgment fields
    are never traversed, verified, evaluated, emitted or supplied to a model.
    """
    import hashlib
    raw=manifest_path.read_bytes();manifest=json.loads(raw)
    corpus=next(c for c in manifest['corpora'] if c['id']==corpus_id)
    root=corpus_root.resolve(strict=True);labels=(manifest_path.parent/corpus['labelsFile']).resolve(strict=True)
    if labels.is_relative_to(root) or manifest_path.resolve().is_relative_to(root):
        raise ValueError('labels/manifest inside indexed corpus')
    label_bytes=labels.read_bytes()
    if hashlib.sha256(label_bytes).hexdigest()!=corpus['labelsSha256']:
        raise ValueError('frozen combined label digest changed')
    selected=[e for e in json.loads(label_bytes) if e['split']==split]
    verify_corpus(root,corpus);verify_labels(root,corpus,selected)
    by_id={e['id']:e for e in selected};order=manifest['queryOrder'][split]
    if len(order)!=len(set(order)) or len(by_id)!=len(selected):
        raise ValueError('duplicate frozen query identity')
    ordered=[by_id[i] for i in order if i in by_id]
    if len(ordered)!=len(selected):raise ValueError('frozen query order incomplete')
    return manifest,corpus,ordered,hashlib.sha256(raw).hexdigest()


def artifact_paths(args):
    return [args.output/'development.json',*(args.output/(v+'-'+kind+'.json') for v in VARIANTS for kind in ('config','calibration'))]


def store_paths(args):
    return [store(args,n,baseline=b) for n in ('engram','flask','vite') for b in (False,True)]


def inputs(args, split):
    ordered, corpora = [], {}
    for name in ('engram', 'flask', 'vite'):
        manifest, corpus, entries, manifest_hash = selected_frozen(args.manifest, name, args.base/'corpora'/name, split)
        corpora[name] = corpus
        ordered.extend(entries)
    by_id = {e['id']: e for e in ordered}
    ordered = [by_id[i] for i in manifest['queryOrder'][split] if i in by_id]
    return manifest, corpora, ordered, manifest_hash


def store(args, name, baseline=False):
    suffix = 'baseline' if baseline else ('t5-context-v2' if name == 'flask' else 't5-context')
    return args.base/'stores'/(name+'-'+suffix)


def identity(args):
    paths = [args.manifest, args.probe, args.sealer, args.metrics, args.mcp,
             args.base/'baseline-engram-mcp', Path(__file__),
             ROOT/'docs/research/reliable-code-intelligence-baseline-2026-10-14.json',
             *(args.base/'results'/name for name in ('engram-baseline-original.json','engram-baseline-identifier.json',
                'engram-baseline-development.json','flask-baseline-development.json','vite-baseline-development.json','authorization-baseline.json')),
             *(Path(__file__).parent/name for name in ('code_evidence_eval.py','code_evidence_baselines.py','code_evidence_assessment.py','code_evidence_calibration.py','code_evidence_judgments.py',
               'code_eval_corpus.py','code_intelligence_eval.py','code_query_ablation.py'))]
    return {str(p.resolve()): sha256(p) for p in paths}


def metric(metrics, entry, records, observed, candidates, validated):
    primary, supporting = observed
    return metrics.exchange({'judgments':judgments(entry,records),'candidateParents':candidates,
        'primary':primary,'supporting':supporting,'validated':validated})


def logical_store_identity(directory):
    """Hash SQL/vector rows and the actual durable lexical index without sqlite-vec.

    Virtual definitions are hashed via sqlite_master; their ordinary shadow
    tables carry data. WAL/checkpoint byte layout is intentionally irrelevant.
    """
    import hashlib
    import sqlite3
    result={}
    with sqlite3.connect('file:'+str(directory/'engram_data.db')+'?mode=ro',uri=True) as db:
        db.execute('BEGIN')
        tables=db.execute("SELECT name,sql FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name").fetchall()
        for name,sql in tables:
            value=hashlib.sha256((sql or '').encode());count=0
            if sql and 'CREATE VIRTUAL TABLE' not in sql.upper():
                escaped='"'+name.replace('"','""')+'"'
                order='rowid' if 'WITHOUT ROWID' not in sql.upper() else ','.join(str(i+1) for i in range(len(db.execute('PRAGMA table_info('+escaped+')').fetchall())))
                for row in db.execute('SELECT * FROM '+escaped+' ORDER BY '+order):
                    encoded=[{'blobSha256':hashlib.sha256(x).hexdigest(),'bytes':len(x)} if isinstance(x,bytes) else x for x in row]
                    value.update(json.dumps(encoded,sort_keys=True,separators=(',',':')).encode());value.update(b'\n');count+=1
            result[name]={'rows':count,'sha256':value.hexdigest()}
    return {'sqlTables':result,'lexical':lexical_identity(directory/'lexical')}

# Exact transient lock paths from the installed Tantivy 0.22.1 directory contract.
# All other files, metadata, namespace markers and directory inventory are bound.
LEXICAL_TRANSIENT_LOCKS={'.tantivy-writer.lock','.tantivy-meta.lock'}


def lexical_identity(root):
    import hashlib
    if root.is_symlink():
        raise ValueError('frozen lexical root must not be a symlink')
    if not root.exists():
        return {'present':False,'directories':[],'files':{}}
    if not root.is_dir():
        raise ValueError('frozen lexical root must be a directory')
    result={'present':True,'directories':[],'files':{}}
    for path in sorted(root.rglob('*')):
        relative=path.relative_to(root).as_posix()
        if path.is_symlink():
            raise ValueError('frozen lexical inventory must not contain symlinks')
        if path.is_dir():
            if path.name in LEXICAL_TRANSIENT_LOCKS:
                raise ValueError('transient lexical lock must be a regular file')
            result['directories'].append(relative)
        elif path.is_file():
            if path.name in LEXICAL_TRANSIENT_LOCKS:
                continue
            value=hashlib.sha256();size=0
            with path.open('rb') as stream:
                for block in iter(lambda:stream.read(1024*1024),b''):
                    value.update(block);size+=len(block)
            result['files'][relative]={'bytes':size,'sha256':value.hexdigest()}
        else:
            raise ValueError('frozen lexical inventory contains a nonregular entry')
    return result
