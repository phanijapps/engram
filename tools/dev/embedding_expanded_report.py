#!/usr/bin/env python3
"""Aggregate original and additional frozen embedding evaluations, with paired
question and repository-cluster bootstrap intervals. This grades retrieval only.
"""
import argparse
import hashlib
import json
import random
from pathlib import Path

METRICS = ('hitAt1','hitAt5','hitAt10','recallAt10','mrrAt10','ndcgAt10')
LANGUAGE = {'flask':'Python','starlette':'Python','engram':'Rust','axum':'Rust','vite':'TypeScript','vitest':'TypeScript'}
SEED = 20261016
SAMPLES = 5000


def load(path):
    raw = path.read_bytes()
    value = json.loads(raw)
    assert value['state'] == 'complete', f'incomplete evaluation: {path}'
    return value, hashlib.sha256(raw).hexdigest()


def mean(values):
    return sum(values)/len(values)


def summary(cases):
    return {lane:{key:mean([c[lane]['metrics'][key] for c in cases]) for key in METRICS}
            for lane in ['vector','fused']}


def uncertainty(groups):
    """Keep model results paired. Cluster draws keep a repository's questions together."""
    flat = [d for group in groups for d in group]
    rng = random.Random(SEED)
    question = sorted(sum(rng.choices(flat,k=len(flat)))/len(flat) for _ in range(SAMPLES))
    totals = [(sum(group),len(group)) for group in groups]
    cluster = []
    for _ in range(SAMPLES):
        selected = rng.choices(totals,k=len(totals))
        cluster.append(sum(x[0] for x in selected)/sum(x[1] for x in selected))
    cluster.sort()
    return {'delta':mean(flat),'questionBootstrap95':[question[124],question[4874]],
            'repositoryClusterBootstrap95':[cluster[124],cluster[4874]],'repositoryCount':len(groups),
            'wins':sum(d>1e-12 for d in flat),'losses':sum(d < -1e-12 for d in flat),
            'ties':sum(abs(d)<=1e-12 for d in flat)}


def main():
    ap=argparse.ArgumentParser(description=__doc__)
    for flag in ['original-bge','original-gemma','additional-bge','additional-gemma']:
        ap.add_argument('--'+flag,type=Path,required=True,help='Complete run.json receipt')
    ap.add_argument('--output-prefix',type=Path,required=True)
    args=ap.parse_args()
    values={};hashes={}
    for key in ['original_bge','original_gemma','additional_bge','additional_gemma']:
        path=getattr(args,key);values[key],sha=load(path);hashes[key]={'path':str(path),'sha256':sha}
    report={'date':'2026-10-16','inputReceipts':hashes,'bootstrapSamples':SAMPLES,'bootstrapSeed':SEED,
            'primaryMetric':'SDK fused per-question required-unit Recall@10','corpora':{},'groups':{}}
    for phase in ['original','additional']:
        bge=values[phase+'_bge'];gemma=values[phase+'_gemma']
        assert set(bge['corpora'])==set(gemma['corpora'])
        for name,c in bge['corpora'].items():
            g=gemma['corpora'][name]
            models={'BGE Small':c['models']['default']['cases'],'BGE Base':c['models']['local-alternative']['cases'],
                    'EmbeddingGemma 2':g['cases']}
            identities=[[(x['id'],x['query']) for x in cases] for cases in models.values()]
            assert all(ids==identities[0] for ids in identities)
            assert c['questions']==len(identities[0])
            assert g['questionsSha256']==c['questionsSha256']
            for m in c['models'].values():assert m['vectorRows']==c['nonemptyChunks']
            assert g['vectorRows']==c['nonemptyChunks']
            for cases in models.values():
                for case in cases:
                    assert not case['response']['fused'].get('sourceFailures')
                    for lane in ['vector','fused']:
                        for metric in METRICS:assert 0<=case[lane]['metrics'][metric]<=1
            report['corpora'][name]={'phase':phase,'language':LANGUAGE[name],'questions':len(identities[0]),
                'chunks':c['chunks'],'requiredUnits':c['requiredUnits'],'indexedRequiredUnits':c['indexedRequiredUnits'],
                'questionsSha256':c['questionsSha256'],'sourceLabelsSha256':c['sourceLabelsSha256'],
                'models':{model:{'summary':summary(cases),'cases':[{k:x[k] for k in ['id','query','vector','fused']} for x in cases]}
                          for model,cases in models.items()}}
    groups={'additional60':[n for n,c in report['corpora'].items() if c['phase']=='additional'],
            'original45':[n for n,c in report['corpora'].items() if c['phase']=='original'],
            'combined105':list(report['corpora'])}
    groups.update({language:[n for n,c in report['corpora'].items() if c['language']==language]
                   for language in ['Python','Rust','TypeScript']})
    for label,names in groups.items():
        models={model:[case for name in names for case in report['corpora'][name]['models'][model]['cases']]
                for model in ['BGE Small','BGE Base','EmbeddingGemma 2']}
        data={'repositories':names,'questions':len(models['BGE Small']),
              'models':{model:summary(cases) for model,cases in models.items()},'gemmaComparisons':{}}
        for comparator in ['BGE Small','BGE Base']:
            data['gemmaComparisons'][comparator]={}
            for lane in ['vector','fused']:
                data['gemmaComparisons'][comparator][lane]={}
                for metric in ['recallAt10','mrrAt10','ndcgAt10']:
                    deltas=[]
                    for name in names:
                        pairs=report['corpora'][name]['models']
                        a=pairs[comparator]['cases'];b=pairs['EmbeddingGemma 2']['cases']
                        deltas.append([y[lane]['metrics'][metric]-x[lane]['metrics'][metric] for x,y in zip(a,b)])
                    data['gemmaComparisons'][comparator][lane][metric]=uncertainty(deltas)
        report['groups'][label]=data
    assert report['groups']['additional60']['questions']==60
    assert report['groups']['original45']['questions']==45
    assert report['groups']['combined105']['questions']==105
    assert all(report['groups'][l]['questions']==35 for l in ['Python','Rust','TypeScript'])
    lines=['# Expanded embedding comparison: 105 questions across six repositories','',
           'This follow-up adds 60 fixed answerable questions on Starlette, Axum and Vitest to the original 45 on Flask, Engram and Vite. Each model was measured on all questions. The combined set has 35 questions per language (Python, Rust and TypeScript). It tests retrieval of source-labeled implementation units, not generated-answer accuracy.','',
           '## Additional 60 questions','',
           'This is the primary follow-up set. The earlier 45 influenced the decision to continue testing and are shown separately. The labels are reused historical source judgments, not a new independent human-labeled blind test.','']
    for label,title in [('additional60','Additional set'),('combined105','Combined descriptive set')]:
        lines+=['### '+title,'','| Metric | BGE Small | BGE Base | EmbeddingGemma 2 |','| --- | ---: | ---: | ---: |']
        for lane in ['vector','fused']:
            for metric in ['recallAt10','mrrAt10','ndcgAt10']:
                scores=[report['groups'][label]['models'][m][lane][metric] for m in ['BGE Small','BGE Base','EmbeddingGemma 2']]
                formatted=[f'{v:.1%}' if metric=='recallAt10' else f'{v:.3f}' for v in scores]
                lines+=['| '+lane+' / '+metric+' | '+' | '.join(formatted)+' |']
        lines+=['']
    for lane in ['vector','fused']:
        lines+=['## '+lane.capitalize()+' Recall@10 by repository','',
                '| Repository | Questions | BGE Small | BGE Base | Gemma 2 |','| --- | ---: | ---: | ---: | ---: |']
        for name,c in report['corpora'].items():
            scores=[c['models'][m]['summary'][lane]['recallAt10'] for m in ['BGE Small','BGE Base','EmbeddingGemma 2']]
            lines+=['| '+name+' | '+str(c['questions'])+' | '+' | '.join(f'{v:.1%}' for v in scores)+' |']
        lines+=['']
    lines+=['## Language breakdown','',
            '| Language / lane | Questions | Small Recall@10 | Base Recall@10 | Gemma Recall@10 |','| --- | ---: | ---: | ---: | ---: |']
    for language in ['Python','Rust','TypeScript']:
        for lane in ['vector','fused']:
            d=report['groups'][language]
            lines+=['| '+language+' / '+lane+' | '+str(d['questions'])+' | '+' | '.join(f"{d['models'][m][lane]['recallAt10']:.1%}" for m in ['BGE Small','BGE Base','EmbeddingGemma 2'])+' |']
    lines+=['','## Primary measure uncertainty','',
            'Gemma minus comparator for fused required-unit Recall@10. Question bootstrap keeps model outputs paired. Repository-cluster bootstrap resamples whole repositories with their questions together. These are descriptive sensitivity intervals; three new or six total repositories are too few to establish broad population superiority.','',
            '| Set / comparator | Delta | Question bootstrap 95% | Repository-cluster bootstrap 95% | Wins / losses / ties |','| --- | ---: | --- | --- | --- |']
    for label in ['additional60','combined105']:
        for comparator in ['BGE Small','BGE Base']:
            d=report['groups'][label]['gemmaComparisons'][comparator]['fused']['recallAt10'];q=d['questionBootstrap95'];c=d['repositoryClusterBootstrap95']
            lines+=[f"| {label} / {comparator} | {d['delta']:+.4f} | [{q[0]:+.4f}, {q[1]:+.4f}] | [{c[0]:+.4f}, {c[1]:+.4f}] | {d['wins']} / {d['losses']} / {d['ties']} |"]
    total_chunks=sum(c['chunks'] for c in report['corpora'].values());required=sum(c['requiredUnits'] for c in report['corpora'].values());covered=sum(c['indexedRequiredUnits'] for c in report['corpora'].values())
    lines+=['','## Controls and limitations','',
            f'- {total_chunks:,} chunks per model across six repositories; {covered}/{required} required-unit judgments represented in the indexes. No question was dropped because retrieval failed or a labeled unit was unindexed.',
            '- Production sources, questions and model settings were frozen before the additional queries. Model settings, chunker, fusion and ranking were unchanged. The [analysis protocol](embedding-expanded-protocol-2026-10-16.md) was documented while the BGE baseline was running and before the new Gemma run, not as a preregistered blind experiment.',
            '- Each corpus was scanned once, then its vector-free seed knowledge/graph data and lexical sidecars were copied for each model. Model/corpus query IDs and texts were checked for exact equality, every nonempty chunk had a vector and all measured queries completed without retrieval-source failure.',
            '- BGE Small/384 and BGE Base/768 used the same cached FastEmbed models. Gemma used the same pinned `google/embeddinggemma-2` revision `914f7f89142e33e77833254d9c9b90c3cef7303b`, text-only FP32 inference, 768 dimensions and task/document prefixes as the first comparison. No dimensions, prefixes, questions or fusion thresholds were tuned.',
            '- Gemma ran on GPU through the temporary loopback reference server; BGE ran on CPU. Some work overlapped across repositories. Timing is not a comparative model-speed benchmark. Native in-process Gemma 2 remains unimplemented.',
            '- Labels are agent-authored and may omit useful alternative implementations. These measurements do not evaluate generated answers, absent-feature rejection or production user satisfaction.',
            '- Original methods and model provenance: [BGE comparison](embedding-model-comparison-2026-10-16.md), [Gemma reference comparison](embeddinggemma2-comparison-2026-10-16.md). Complete input receipt paths and hashes, per-question metrics and secondary uncertainty analyses are in the accompanying JSON.','']
    args.output_prefix.with_suffix('.json').write_text(json.dumps(report,indent=2)+'\n')
    args.output_prefix.with_suffix('.md').write_text('\n'.join(lines))
    print(json.dumps({k:{'questions':v['questions'],'models':v['models'],'primaryComparisons':{m:d['fused']['recallAt10'] for m,d in v['gemmaComparisons'].items()}} for k,v in report['groups'].items() if k in ['additional60','combined105']},indent=2))


if __name__=='__main__':
    main()
