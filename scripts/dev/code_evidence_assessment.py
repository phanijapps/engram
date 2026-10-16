"""One frozen assessment. No threshold/model/config changes after label release."""
import json
import math
from pathlib import Path
import struct
import time
from code_eval_corpus import sha256
from code_intelligence_eval import EvaluationServer, save
from code_evidence_eval_support import VARIANTS, JsonProbe, ProbeProtocolError, identity, inputs, metric, store, logical_store_identity, artifact_paths, store_paths
from code_evidence_calibration import strategy_maximum
from code_evidence_judgments import baseline_observations, candidate_parents, canonical_records, packet_observations


def typed_packet(value, key=''):
    if isinstance(value, dict):
        return {k:typed_packet(v,k) for k,v in value.items() if k!='phaseMs'}
    if isinstance(value,list):
        return [typed_packet(v) for v in value]
    if key in ('fusionScore','rerankScore','sourceScore') and isinstance(value,(float,int)):
        return struct.pack('!f',value).hex()
    return value


def summarize(rows, entries):
    lookup={e['id']:e for e in entries}
    positive=[c for c in rows if lookup[c['id']]['polarity']=='answerable']
    negative=[c for c in rows if lookup[c['id']]['polarity']=='negative']
    result={'answerableCount':len(positive),'negativeCount':len(negative),'failedCalls':sum(bool(c.get('failed')) for c in rows)}
    for key in ('candidateRecallAt50','requiredUnitRecallAt10','requiredRangeCoverage','mrrAt10','ndcgAt10'):
        values=[c['metrics'][key] for c in positive if c['metrics'][key] is not None]
        result[key]=sum(values)/len(values) if values else None
    result['allRequiredRecovered']=sum(c['metrics']['allRequiredRecovered'] for c in positive)
    result['negativeInsufficient']=sum(not c.get('failed') and c.get('evidenceStatus')=='insufficient_evidence' for c in negative)
    result['unjudgedPrimaryItems']=sum(c['metrics']['unjudgedPrimaryItems'] for c in rows)
    result['perRepository']={name:summarize_subset([c for c in positive if c['corpus']==name]) for name in ('engram','flask','vite')}
    return result


def summarize_subset(rows):
    return {'answerableCount':len(rows),'requiredUnitRecallAt10':sum(c['metrics']['requiredUnitRecallAt10'] for c in rows)/len(rows) if rows else None}


def assess_baseline(args, entries, report, output, metrics):
    report['baseline']={}
    for name in ('engram','flask','vite'):
        selected=[e for e in entries if e['corpus']==name]
        records=canonical_records(store(args,name,baseline=True))
        server=EvaluationServer(args.base/'baseline-engram-mcp',store(args,name,baseline=True),'code-eval-'+name)
        result={'cases':[],'historicalRankingOnly':True,'candidateMetricsAvailable':False}
        report['baseline'][name]=result
        try:
            result['startup']=server.call('capability_report',{})
            result['warmup']=server.call('search',{'query':selected[0]['query'],'limit':10,'diagnostics':True})
            for entry in selected:
                call=server.call('search',{'query':entry['query'],'limit':10,'diagnostics':True})
                primary,unresolved=baseline_observations(call['text'],records)
                measured=metric(metrics,entry,records,(primary,[]),[],not call['failed'])
                measured['candidateRecallAt50']=None
                result['cases'].append({'id':entry['id'],'corpus':name,**call,'metrics':measured,'unresolvedHeaders':unresolved,'primary':primary})
                save(output,report)
                print('baseline',entry['id'],round(call['seconds'],3),flush=True)
        finally:
            server.close();result['stderr']=server.errors;save(output,report)


def assess_variant(args, variant, entries, corpora, report, output, metrics):
    report[variant]={}
    artifact=json.loads((args.output/(variant+'-calibration.json')).read_text())
    for name,corpus in corpora.items():
        selected=[e for e in entries if e['corpus']==name]
        if not selected:
            continue
        records=canonical_records(store(args,name))
        command=['env','ENGRAM_CODE_CONFIG='+str(args.output/(variant+'-config.json')),str(args.probe),str(store(args,name)),
                 'code-eval-'+name,str(args.output/(variant+'-calibration.json'))]
        probe=None
        result={'cases':[],'children':[]};report[variant][name]=result
        def close_child(after_case):
            nonlocal probe
            if probe is not None:
                previous=probe;probe=None
                try:previous.close()
                finally:
                    result['children'].append({'afterCase':after_case,'stderr':list(previous.errors)})
                    save(output,report)
        try:
            probe=JsonProbe(command)
            for entry in selected:
                start=time.monotonic()
                observed=None;restart=False;recorded=False
                try:
                    observed=probe.exchange({'query':entry['query']})
                    packet=observed['packet'];strategy_maximum(packet,variant)
                    validated=packet['diagnostics']['calibrationState']=='validated' and packet['diagnostics']['calibrationId']==artifact['id']
                    measured=metric(metrics,entry,records,packet_observations(packet,args.base/'corpora'/name,corpus),candidate_parents(observed['candidates']),validated)
                    case={'id':entry['id'],'corpus':name,'observed':observed,'metrics':measured,'evidenceStatus':packet['evidenceStatus'],
                          'validated':validated,'failed':not validated}
                except Exception as error:
                    case={'id':entry['id'],'corpus':name,'failed':True,'error':str(error),'seconds':time.monotonic()-start,
                          'validated':False}
                    if observed is not None:case['observed']=observed
                    result['cases'].append(case);recorded=True;save(output,report)
                    try:case['metrics']=metric(metrics,entry,records,([],[]),[],False)
                    except Exception as metric_error:
                        case['metricError']=str(metric_error);save(output,report);raise
                    restart=isinstance(error,ProbeProtocolError)
                if not recorded:result['cases'].append(case)
                save(output,report)
                print(variant,entry['id'],'failed' if case['failed'] else 'ok',flush=True)
                if restart:
                    close_child(entry['id'])
                    if entry['id']!=selected[-1]['id']:
                        result['protocolRestarts']=result.get('protocolRestarts',0)+1
                        try:probe=JsonProbe(command)
                        except Exception as error:
                            result['restartError']=str(error);save(output,report);raise
        finally:
            close_child(result['cases'][-1]['id'] if result['cases'] else None)
            save(output,report)


def nearest_rank_p95(values):
    ordered=sorted(values)
    return ordered[math.ceil(.95*len(ordered))-1] if ordered else None


def measure_mcp(args, entries, report, output):
    measured=[];report['balancedMcp']={'cases':measured,'warmups':{},'startups':{},'packetMismatches':[]}
    expected={c['id']:c for run in report['balanced'].values() for c in run['cases']}
    for name in ('engram','flask','vite'):
        selected=[e for e in entries if e['corpus']==name and e['split']=='heldout']
        extra=['--config',str(args.output/'balanced-config.json'),'--code-calibration',str(args.output/'balanced-calibration.json')]
        server=EvaluationServer(args.mcp,store(args,name),'code-eval-'+name,tool_profile='full',extra_args=extra)
        try:
            report['balancedMcp']['startups'][name]=server.call('maintain',{'op':'capabilities'})
            report['balancedMcp']['warmups'][name]=server.call('code',{'op':'evidence','query':selected[0]['query']})
            for entry in selected:
                start=time.monotonic()
                try:
                    call=server.call('code',{'op':'evidence','query':entry['query']},timeout=120)
                except Exception as error:
                    measured.append({'id':entry['id'],'corpus':name,'failed':True,'error':str(error),'seconds':time.monotonic()-start})
                    save(output,report)
                    raise
                if not call['failed']:
                    packet=json.loads(call['text'])
                    expected_case=expected[entry['id']]
                    if expected_case['failed'] or typed_packet(packet)!=typed_packet(expected_case['observed']['packet']):
                        report['balancedMcp']['packetMismatches'].append(entry['id'])
                    call['packet']=packet
                measured.append({'id':entry['id'],'corpus':name,**call});save(output,report)
                print('balanced-mcp',entry['id'],round(call['seconds'],3),flush=True)
        finally:
            server.close();report['balancedMcp'].setdefault('stderr',{})[name]=server.errors;save(output,report)
    p95=nearest_rank_p95([c['seconds'] for c in measured])
    report['balancedMcp'].update(measuredRequests=len(measured),p95Seconds=p95,
        operationalPass=len(measured)==60 and not any(c['failed'] for c in measured) and not any(c['failed'] for k in ('startups','warmups') for c in report['balancedMcp'][k].values()) and not report['balancedMcp']['packetMismatches'] and p95<=30)


def finish_metrics(report, entries):
    by_id={e['id']:e for e in entries}
    report['summaries']={}
    rows={v:[c for run in report[v].values() for c in run['cases']] for v in ('baseline',*VARIANTS)}
    for variant,cases in rows.items():
        report['summaries'][variant]={split:summarize([c for c in cases if by_id[c['id']]['split']==split],[e for e in entries if e['split']==split]) for split in ('heldout','original','identifier')}
    same_candidates=True
    paired=[]
    segmented={c['id']:c for c in rows['segmented']}
    baseline={c['id']:c for c in rows['baseline']}
    for case in rows['balanced']:
        off=segmented[case['id']]
        if case['failed'] or off['failed'] or case['observed']['candidates']!=off['observed']['candidates']:
            same_candidates=False
        paired.append({'id':case['id'],'baseline':baseline[case['id']]['metrics'],'segmented':off['metrics'],'balanced':case['metrics']})
    report['paired']=paired
    scores=report['summaries']['balanced'];held=scores['heldout'];off=report['summaries']['segmented']['heldout']
    identifier=[c for c in rows['balanced'] if by_id[c['id']]['split']=='identifier']
    controls=sum(c['validated'] and c['metrics']['mrrAt10']==1.0 and c['metrics']['allRequiredRecovered'] for c in identifier)
    gates={
        'originalAllRequiredAtLeast8of10':scores['original']['answerableCount']==10 and scores['original']['allRequiredRecovered']>=8,
        'identifierBothRankOne':len(identifier)==2 and controls==2,
        'heldoutCandidateRecall50':held['answerableCount']==40 and held['candidateRecallAt50']>=.90,
        'heldoutRequiredUnitRecall10':held['requiredUnitRecallAt10']>=.80,
        'heldoutPerRepositoryRecall':all(r['answerableCount']>0 and r['requiredUnitRecallAt10']>=.70 for r in held['perRepository'].values()),
        'heldoutNegatives18of20':held['negativeCount']==20 and held['negativeInsufficient']>=18,
        'rerankerNdcgNonregression':same_candidates and held['ndcgAt10']+1e-12>=off['ndcgAt10'],
        'allVariantCaseInventoriesComplete':all([c['id'] for c in cases]==[e['id'] for n in ('engram','flask','vite') for e in entries if e['corpus']==n] for cases in rows.values()),
        'allVariantCallsValid':not any(c['failed'] for cases in rows.values() for c in cases),
        'baselineStartupsAndWarmupsValid':not any(run[k]['failed'] for run in report['baseline'].values() for k in ('startup','warmup')),
        'warmBalancedP95':report['balancedMcp']['operationalPass'],
    }
    report['gates']=gates;report['allQualityAndWarmGatesPass']=all(gates.values());report['featureShipped']=False


def assessment(args):
    freeze_path=args.output/'freeze.json';frozen=json.loads(freeze_path.read_text())
    if frozen.get('outputRoot')!=str(args.output.resolve()) or frozen.get('baseRoot')!=str(args.base.resolve()) or frozen.get('assessmentMarker')!=str((args.output/'assessment-started.json').resolve()):
        raise ValueError('assessment root/marker differs from frozen runtime paths')
    if set(frozen['artifacts'])!={str(p.resolve()) for p in artifact_paths(args)} or set(frozen['storeIdentities'])!={str(p.resolve()) for p in store_paths(args)}:
        raise ValueError('frozen path inventories differ from actual runtime artifacts/stores')
    if frozen['inputs']!=identity(args) or any(sha256(Path(path))!=expected for path,expected in frozen['artifacts'].items()):
        raise ValueError('freeze input/artifact drift; held-out inputs have not been released')
    if any(logical_store_identity(Path(path))!=expected for path,expected in frozen['storeIdentities'].items()):
        raise ValueError('frozen persisted store identity drift before held-out release')
    with (args.output/'assessment-started.json').open('x') as marker:
        json.dump({'freezeSha256':sha256(freeze_path),'startedAtUnix':time.time()},marker)
    # No held-out query or judgment is selected before the immutable freeze check.
    manifest,corpora,heldout,manifest_hash=inputs(args,'heldout')
    entries=heldout+inputs(args,'original')[2]+inputs(args,'identifier')[2]
    report={'runState':'running','phase':'one frozen assessment','manifestSha256':manifest_hash,'freezeSha256':sha256(freeze_path),'featureShipped':False}
    output=args.output/'assessment.json';metrics=JsonProbe([str(args.metrics)])
    try:
        assess_baseline(args,entries,report,output,metrics)
        for variant in VARIANTS:
            assess_variant(args,variant,entries,corpora,report,output,metrics)
        measure_mcp(args,entries,report,output)
        finish_metrics(report,entries)
        report['storeIdentitiesPreserved']=all(logical_store_identity(Path(path))==expected for path,expected in frozen['storeIdentities'].items())
        report['gates']['frozenStoresPreserved']=report['storeIdentitiesPreserved']
        report['allQualityAndWarmGatesPass']=all(report['gates'].values())
        report['runState']='complete'
    except BaseException as error:
        report.update(runState='incomplete',error=str(error));raise
    finally:
        metrics.close();save(output,report)
