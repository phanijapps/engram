#!/usr/bin/env python3
"""Frozen dev-only calibration, explicit identity freeze, then untouched assessment.

Only query text enters the Rust observer or MCP. Expected-answer labels remain
in this offline process and the pure Rust metric helper, outside indexed corpora.
"""
import argparse
import fcntl
import json
import os
from pathlib import Path

from code_eval_corpus import sha256, validate_artifact_paths
from code_intelligence_eval import save
from code_evidence_eval_support import ROOT, VARIANTS, JsonProbe, inputs, store, identity, metric, logical_store_identity, artifact_paths, store_paths
from code_evidence_baselines import require_baselines
from code_evidence_calibration import digest, seal, select_threshold, strategy_maximum
from code_evidence_judgments import candidate_parents, canonical_records, packet_observations


def run_variant(args, variant, corpus, entries, artifact=None, sink=None, checkpoint=None):
    command = ['env', 'ENGRAM_CODE_CONFIG='+str(args.output/(variant+'-config.json')),
               str(args.probe), str(store(args,corpus)), 'code-eval-'+corpus]
    if artifact is not None:
        command.append(str(artifact))
    probe = JsonProbe(command)
    sink = sink if sink is not None else {'cases':[]}
    rows = sink['cases']
    try:
        for entry in entries:
            observed = None
            try:
                observed = probe.exchange({'query':entry['query']})
                # Admit only the actual requested score space with compatible current inputs.
                strategy_maximum(observed['packet'],variant)
            except BaseException as error:
                case={'id':entry['id'],'corpus':corpus,'failed':True,'error':str(error)}
                if observed is not None:case['observed']=observed
                rows.append(case)
                if checkpoint: checkpoint()
                raise
            rows.append({'id':entry['id'],'corpus':corpus,'observed':observed})
            if checkpoint: checkpoint()
            print(variant,corpus,entry['id'],round(observed['seconds'],3),flush=True)
    finally:
        probe.close()
        sink['stderr']=probe.errors
    return sink


def verify_calibrated_development(report, entries):
    """Confirm the fixed fit target in real calibrated responses before freeze."""
    negative_ids={e['id'] for e in entries if e['polarity']=='negative'}
    if len(negative_ids)!=15:
        raise ValueError('development requires the fixed 15-negative inventory')
    checks={}
    for variant in VARIANTS:
        rows=[c for name in ('engram','flask','vite') for c in report['validated'][variant+'-'+name]['cases']]
        expected=[e['id'] for name in ('engram','flask','vite') for e in entries if e['corpus']==name]
        if [c['id'] for c in rows]!=expected:
            raise ValueError('calibrated development case inventory/order mismatch')
        calibration=report['calibration'][variant]
        for case in rows:
            diagnostics=case['observed']['packet']['diagnostics']
            if case.get('failed') or diagnostics['calibrationState']!='validated' or diagnostics['calibrationId']!=calibration['artifact']['id']:
                raise ValueError('calibrated development response did not validate')
        negatives=[c['observed']['packet'] for c in rows if c['id'] in negative_ids]
        rejected=sum(p['evidenceStatus']=='insufficient_evidence' and p['items']==[] for p in negatives)
        check={'negativeCount':len(negatives),'rejectedAtRuntime':rejected,'negativeTarget':14}
        checks[variant]=check
        calibration['runtimeNegativeCheck']=check
        if calibration['negativeTarget']!=14 or rejected<14:
            raise ValueError('calibrated development runtime negative target failed: '+variant)
    return checks


def development(args):
    manifest, corpora, entries, manifest_hash = inputs(args,'development')
    baselines = require_baselines(args,manifest,manifest_hash)
    if args.output.exists():
        raise ValueError('output directory already exists; preserve prior attempts')
    args.output.mkdir(parents=True)
    for variant, config in VARIANTS.items():
        save(args.output/(variant+'-config.json'),config)
    report = {'runState':'running','phase':'development','manifestSha256':manifest_hash,
              'inputs':identity(args),'baselineArtifacts':baselines,'raw':{},'validated':{},'calibration':{}}
    output = args.output/'development.json'
    try:
        all_cases = {}
        templates = {}
        for variant in VARIANTS:
            cases = []
            for name in corpora:
                sink=report['raw'].setdefault(variant+'-'+name,{'cases':[]})
                run = run_variant(args,variant,name,[e for e in entries if e['corpus']==name],sink=sink,checkpoint=lambda:save(output,report))
                cases.extend(run['cases'])
                save(output,report)
            all_cases[variant] = {c['id']:c for c in cases}
            bindings = {digest(c['observed']['binding']) for c in cases}
            if len(bindings)!=1:
                raise ValueError('actual binding changed across queries/corpora')
            templates[variant] = cases[0]['observed']['binding']
        for entry in entries:
            a,b = [all_cases[v][entry['id']]['observed']['candidates'] for v in ('segmented','balanced')]
            if a != b:
                raise ValueError('rerank comparison has different observed candidates')
        for variant in VARIANTS:
            negatives = [strategy_maximum(all_cases[variant][e['id']]['observed']['packet'],variant) for e in entries if e['polarity']=='negative']
            threshold,rejected,target = select_threshold(negatives)
            artifact = seal(templates[variant],threshold,entries,args.sealer)
            artifact_path = args.output/(variant+'-calibration.json')
            save(artifact_path,artifact)
            report['calibration'][variant] = {'artifact':artifact,'negativeMaxima':negatives,'rejectedAtFit':rejected,'negativeTarget':target}
            for name,corpus in corpora.items():
                sink=report['validated'].setdefault(variant+'-'+name,{'cases':[]})
                run = run_variant(args,variant,name,[e for e in entries if e['corpus']==name],artifact_path,sink=sink,checkpoint=lambda:save(output,report))
                records = canonical_records(store(args,name))
                metrics = JsonProbe([str(args.metrics)])
                try:
                    by_id = {e['id']:e for e in entries}
                    for case in run['cases']:
                        packet = case['observed']['packet']
                        if packet['diagnostics']['calibrationState']!='validated' or packet['diagnostics']['calibrationId']!=artifact['id']:
                            raise ValueError('installed development calibration did not validate')
                        case['metrics'] = metric(metrics,by_id[case['id']],records,
                            packet_observations(packet,args.base/'corpora'/name,corpus),
                            candidate_parents(case['observed']['candidates']),True)
                finally:metrics.close()
                report['validated'][variant+'-'+name] = run
                save(output,report)
        verify_calibrated_development(report,entries)
        report['runState']='complete'
    except BaseException as error:
        report['runState']='incomplete';report['error']=str(error);raise
    finally:save(output,report)


def freeze(args):
    path=args.output/'development.json'
    report=json.loads(path.read_text())
    if report['runState']!='complete' or report['inputs']!=identity(args):
        raise ValueError('development incomplete or executable/runner inputs changed')
    recorded={v:report['calibration'][v].get('runtimeNegativeCheck') for v in VARIANTS}
    if verify_calibrated_development(report,inputs(args,'development')[2])!=recorded:
        raise ValueError('development runtime negative checks differ from recorded results')
    files=artifact_paths(args)
    frozen={'version':1,'outputRoot':str(args.output.resolve()),'baseRoot':str(args.base.resolve()),'assessmentMarker':str((args.output/'assessment-started.json').resolve()),'inputs':identity(args),'artifacts':{str(p.resolve()):sha256(p) for p in files},
        'variants':VARIANTS,'developmentLabelsOnly':True,'heldoutAssessment':'one predeclared assessment; no retuning',
        'warmP95':'nearest rank ceil(.95*N)-1; all60 measured balanced requests; failures/incomplete fail gate'}
    frozen['storeIdentities']={str(p.resolve()):logical_store_identity(p) for p in store_paths(args)}
    with (args.output/'freeze.json').open('x') as out:json.dump(frozen,out,indent=2);out.write('\n')
    print('frozen',sha256(args.output/'freeze.json'),flush=True)


def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('phase',choices=['development','freeze','assess'])
    p.add_argument('--base',type=Path,default=Path.home()/'.engram/evaluations/reliable-code-intelligence')
    p.add_argument('--manifest',type=Path,default=ROOT/'core/eval/fixtures/code_intelligence/manifest.json')
    p.add_argument('--output',type=Path,required=True)
    p.add_argument('--probe',type=Path,default=ROOT/'target/release/examples/code_evidence_probe')
    p.add_argument('--sealer',type=Path,default=ROOT/'target/release/examples/code_calibration_seal')
    p.add_argument('--metrics',type=Path,default=ROOT/'target/release/examples/code_metrics')
    p.add_argument('--mcp',type=Path,default=ROOT/'target/release/engram-mcp')
    args=p.parse_args()
    if any(k.startswith('ENGRAM_CODE') for k in os.environ):
        raise ValueError('evaluation requires a clean code-config environment')
    manifest=json.loads(args.manifest.read_text())
    validate_artifact_paths(args.manifest,args.output/'development.json',args.output,
        [args.base/'corpora'/c['id'] for c in manifest['corpora']], [args.probe,args.sealer,args.metrics,args.mcp])
    with (args.base/'stores/.code-evaluation.lock').open('a') as lock:
        fcntl.flock(lock,fcntl.LOCK_EX)
        from code_evidence_assessment import assessment
        {'development':development,'freeze':freeze,'assess':assessment}[args.phase](args)

if __name__=='__main__':main()
