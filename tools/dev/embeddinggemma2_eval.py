#!/usr/bin/env python3
"""Evaluate a pinned locally served EmbeddingGemma 2 checkpoint on BGE's frozen inputs.
The server must accept already-prefixed strings at /v1/embeddings. No model or
shared-store mutation; copies only the prior evaluation's vector-free seed stores.
"""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import sqlite3
import time

from embedding_model_eval import MCP, Process, REPO, bind_labels, digest, metrics, paired, records, summarize, write

REVISION = '914f7f89142e33e77833254d9c9b90c3cef7303b'
MODEL = f'google/embeddinggemma-2@{REVISION}'


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument('--baseline', type=Path, required=True)
    ap.add_argument('--output', type=Path, required=True)
    ap.add_argument('--endpoint', required=True)
    args = ap.parse_args()
    args.output.mkdir(parents=True, exist_ok=False)
    baseline = json.loads((args.baseline/'run.json').read_text())
    assert baseline['state'] == 'complete'
    run = {'state':'running','model':MODEL,'revision':REVISION,'dimensions':768,
           'baseline':str(args.baseline),'baselineReceiptSha256':digest(args.baseline/'run.json'),
           'mcpBinarySha256':digest(REPO/'target/release/engram-mcp'),
           'probeSha256':digest(REPO/'target/release/examples/embedding_retrieval_probe'), 'corpora':{}}
    assert run['mcpBinarySha256'] == baseline['binarySha256'], 'MCP runtime changed'
    write(args.output/'run.json',run)
    for name, original in baseline['corpora'].items():
        dest = args.output/name
        dest.mkdir()
        root = Path(original['root'])
        manifest = json.loads((args.baseline/name/'source-manifest.json').read_text())
        for path, sha in manifest.items():
            assert digest(root/path) == sha, (name,path)
        raw_labels = (args.baseline/name/'questions-frozen.json').read_bytes()
        assert hashlib.sha256(raw_labels).hexdigest() == original['questionsSha256']
        labels = json.loads(raw_labels)
        (dest/'questions-frozen.json').write_bytes(raw_labels)
        write(dest/'source-manifest.json',manifest)
        store = dest/'store'
        store.mkdir()
        source = args.baseline/name/'seed'
        with sqlite3.connect(f'file:{source}/engram_data.db?mode=ro',uri=True) as src, sqlite3.connect(store/'engram_data.db') as dst:
            src.backup(dst)
        for directory in ['lexical','scan-manifests']:
            if (source/directory).exists():
                shutil.copytree(source/directory,store/directory)
        chunks = records(store)
        assert chunks == records(source)
        targets = bind_labels(root,labels,chunks)
        config = {'provider_type':'openai-compatible','model':MODEL,'dimensions':768,
                  'endpoint':args.endpoint, 'query_instruction':'task: code retrieval | query: ',
                  'passage_instruction':'title: none | text: ', 'request_timeout_secs':300,
                  'batch_size':32,'normalization':'l2'}
        profile = ['[embedding_provider]']
        profile += [f'{key} = {json.dumps(value)}' for key,value in config.items()]
        (dest/'embedding.toml').write_text('\n'.join(profile)+'\n')
        model = {'cases':[],'indexCalls':[],'questions':len(labels),'chunks':len(chunks),
                 'indexedRequiredUnits':sum(bool(u['indexedChunkIds']) for c in labels for u in c['boundUnits'].values()),
                 'requiredUnits':sum(len(c['boundUnits']) for c in labels),
                 'questionsSha256':original['questionsSha256']}
        run['corpora'][name] = model
        server = MCP(store,dest/'index.stderr.log',config_path=dest/'embedding.toml')
        try:
            model['capabilities'] = server.call('capability_report',{})
            assert 'vector: supported' in model['capabilities']['text']
            for _ in range(1000):
                progress = server.call('reindex',{'limit':256})
                model['indexCalls'].append(progress)
                write(args.output/'run.json',run)
                print(name,progress['text'].replace('\n',' '),round(progress['seconds'],2),flush=True)
                if 'backlog drained' in progress['text']:
                    break
                assert 'embedded 0 chunks' not in progress['text']
            else:
                raise RuntimeError('embedding backlog did not drain')
        finally:
            server.close()
        with sqlite3.connect(store/'engram_data.db') as db:
            model['vectorRows'] = db.execute('SELECT count(*) FROM vectors_rowids').fetchone()[0]
        assert model['vectorRows'] == original['nonemptyChunks']
        sdk = {'storage_path':str(store),'trusted_root':str(args.output),'scope_policy':'Strict',
               'embedding_provider':config,'migration_mode':'Apply','capability_policy':'FailClosed',
               'enable_vector':True,'sqlite_storage_layout':{'kind':'single_file','file_name':'engram_data.db'}}
        write(dest/'config.json',sdk)
        probe = Process([str(REPO/'target/release/examples/embedding_retrieval_probe'),str(dest/'config.json')],dest/'query.stderr.log')
        def request(query):
            return {'query':query,'scope':{'tenant':'default','workspace':'embedding-eval'},
                    'requester':{'actor':{'id':'engram-mcp','kind':'agent'}},'modes':[],
                    'limit':10,'includeExplanations':True}
        try:
            model['warmup'] = probe.send(request('How is application configuration loaded?'))
            model['space'] = model['warmup']['space']
            assert model['space']['model'] == MODEL and model['space']['dimensions'] == 768
            for case in labels:
                response = probe.send(request(case['query']))
                assert not response['fused'].get('sourceFailures'),response['fused'].get('sourceFailures')
                scored = {'id':case['id'],'query':case['query'],'response':response}
                for mode,hits in [('vector',response['vector']),('fused',response['fused'].get('items',[]))]:
                    scored[mode] = {'metrics':metrics(hits,case,targets)}
                model['cases'].append(scored)
                write(args.output/'run.json',run)
                print(name,case['id'],scored['vector']['metrics']['recallAt10'],scored['fused']['metrics']['recallAt10'],flush=True)
        finally:
            probe.close()
        model['summary'] = summarize(model['cases'])
        model['pairedAgainstSmall'] = paired(original['models']['default']['cases'],model['cases'])
        model['pairedAgainstBase'] = paired(original['models']['local-alternative']['cases'],model['cases'])
        for path,sha in manifest.items():
            assert digest(root/path) == sha
        assert records(store) == chunks
        write(args.output/'run.json',run)
    all_cases = [c for x in run['corpora'].values() for c in x['cases']]
    run['overall'] = summarize(all_cases)
    for preset, field in [('default','pairedAgainstSmall'),('local-alternative','pairedAgainstBase')]:
        run[field] = paired([c for x in baseline['corpora'].values() for c in x['models'][preset]['cases']],all_cases)
    assert digest(args.baseline/'run.json') == run['baselineReceiptSha256']
    run['state'] = 'complete'
    write(args.output/'run.json',run)
    print('COMPLETE',json.dumps(run['overall']),flush=True)


if __name__ == '__main__':
    main()
