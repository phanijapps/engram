"""Construction invariants for evaluation, without expected production answers."""
import math
from pathlib import Path
import sys
import unittest
sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
from code_evidence_calibration import F32_MAX, f32, next_up, select_threshold, strategy_maximum
from code_evidence_judgments import baseline_observations, candidate_parents, judgments

class CalibrationTests(unittest.TestCase):
    def test_tied_negatives_use_next_f32_and_keep_the_entire_denominator(self):
        threshold,rejected,target=select_threshold([None]*3 + [1.0]*11 + [2.0])
        self.assertEqual(target,14);self.assertEqual(rejected,14)
        self.assertEqual(threshold,next_up(1.0))
        self.assertGreater(threshold,1.0);self.assertLess(threshold,2.0)
    def test_signed_zero_subnormals_and_finite_overflow_are_explicit(self):
        self.assertGreater(next_up(-0.0),0)
        self.assertEqual(next_up(-next_up(0)),0)
        self.assertIsNone(next_up(F32_MAX))
        with self.assertRaises(ValueError):select_threshold([F32_MAX,F32_MAX])
        with self.assertRaises(ValueError):select_threshold([math.nan])
        self.assertEqual(select_threshold([None]*15),(-F32_MAX,15,14))
    def test_degraded_fallback_never_trains_a_threshold_in_another_score_space(self):
        packet={'diagnostics':{'reindexRequired':False,'effectiveReranker':'none','requestedReranker':'cross_encoder','scorerState':'failed'},'sourceFailures':[],'items':[]}
        with self.assertRaises(ValueError):strategy_maximum(packet,'balanced')
        packet['diagnostics'].update(effectiveReranker='cross_encoder',scorerState='ready')
        self.assertIsNone(strategy_maximum(packet,'balanced'))
        packet['sourceFailures']=[{'source':'vector'}]
        with self.assertRaises(ValueError):strategy_maximum(packet,'balanced')
        packet['sourceFailures']=[{'source':'code_sources'}]
        self.assertIsNone(strategy_maximum(packet,'balanced'))

class ObservationTests(unittest.TestCase):
    def test_unknown_or_ambiguous_legacy_headers_keep_original_rank_positions(self):
        record={'code':{'unit':'u','file':{'path':'a.ts'},'ranges':[{'start':10,'end':32}],'sliceSeparator':'\n'},'text':'export function f() {}','entities':[{'name':'f'}]}
        text='mystery (Memory) — repo [vector, rrf=0.02]\nf (Function) — repo, a.ts [graph, rrf=0.01]\n'
        observed,unknown=baseline_observations(text,[record]);self.assertEqual([r['parentId'] for r in observed],['legacy-unjudged-rank-1','u']);self.assertEqual(unknown[0]['rank'],1)
        duplicate={**record,'code':{**record['code'],'unit':'other'}}
        observed,_=baseline_observations(text,[record,duplicate]);self.assertEqual(observed[1]['parentId'],'legacy-unjudged-rank-2')
    def test_candidate_fifty_counts_distinct_implementation_parents_only(self):
        rows=[{'parentUnitId':'test','category':'test'}]*100
        rows += [{'parentUnitId':'one','category':'implementation'}]*100
        rows += [{'parentUnitId':str(i),'category':'implementation'} for i in range(60)]
        self.assertEqual(len(candidate_parents(rows)),50)
        self.assertEqual(candidate_parents(rows)[:2],['one','0'])
    def test_same_anchor_with_wrong_declaration_cannot_satisfy_a_required_unit(self):
        entry={'id':'q','requiredUnits':[{'path':'a.ts','qualifiedAnchor':'A.f','startByte':10,'endByte':20,'sourceSha256':'a','grade':3,'requiredRanges':[{'startByte':12,'endByte':18}]}]}
        record={'code':{'file':{'path':'a.ts'},'unit':'u','declarationRange':{'start':9,'end':20},'sourceHash':'a'},'location':{'anchor':'method A::f'}}
        with self.assertRaises(ValueError):judgments(entry,[record])
        record['code']['declarationRange']['start']=10
        self.assertEqual(judgments(entry,[record])[0]['parentId'],'u')


class FreezeAndDenominatorTests(unittest.TestCase):
    def test_failed_positive_and_negative_cases_remain_in_their_denominators(self):
        from code_evidence_assessment import summarize
        entries=[{'id':str(i),'polarity':'answerable' if i<2 else 'negative'} for i in range(4)]
        def row(i,score,failed=False,status=None):
            return {'id':str(i),'corpus':'engram','failed':failed,'evidenceStatus':status,'metrics':{'candidateRecallAt50':score,'requiredUnitRecallAt10':score,'requiredRangeCoverage':score,'mrrAt10':score,'ndcgAt10':score,'allRequiredRecovered':score==1,'unjudgedPrimaryItems':0}}
        result=summarize([row(0,1),row(1,0,True),row(2,None,status='insufficient_evidence'),row(3,None,True)],entries)
        self.assertEqual(result['answerableCount'],2);self.assertEqual(result['requiredUnitRecallAt10'],.5)
        self.assertEqual(result['negativeCount'],2);self.assertEqual(result['negativeInsufficient'],1)
        self.assertEqual(result['failedCalls'],2)
    def test_drift_or_repeated_assessment_never_releases_heldout_queries(self):
        import json,tempfile
        from types import SimpleNamespace
        from unittest.mock import patch
        import code_evidence_assessment as module
        with tempfile.TemporaryDirectory() as directory:
            from code_evidence_eval_support import artifact_paths,store_paths
            output=Path(directory);args=SimpleNamespace(output=output,base=output/'base')
            frozen={'outputRoot':str(output.resolve()),'baseRoot':str(args.base.resolve()),'assessmentMarker':str((output/'assessment-started.json').resolve()),
                    'inputs':{'old':'hash'},'artifacts':{str(p.resolve()):'hash' for p in artifact_paths(args)},'storeIdentities':{str(p.resolve()):{} for p in store_paths(args)}}
            (output/'freeze.json').write_text(json.dumps(frozen))
            with patch.object(module,'identity',return_value={}),patch.object(module,'sha256',return_value='hash'),patch.object(module,'logical_store_identity',return_value={}),patch.object(module,'inputs') as release:
                with self.assertRaises(ValueError):module.assessment(args)
                release.assert_not_called()
            frozen['inputs']={};(output/'freeze.json').write_text(json.dumps(frozen));(output/'assessment-started.json').write_text('{}')
            with patch.object(module,'identity',return_value={}),patch.object(module,'sha256',return_value='hash'),patch.object(module,'logical_store_identity',return_value={}),patch.object(module,'inputs') as release:
                with self.assertRaises(FileExistsError):module.assessment(args)
                release.assert_not_called()


class StoreAndTimingTests(unittest.TestCase):
    def test_logical_freeze_includes_vector_blob_changes_but_ignores_checkpoint_layout(self):
        import sqlite3,tempfile
        from code_evidence_eval_support import logical_store_identity
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory)
            with sqlite3.connect(root/'engram_data.db') as db:
                db.execute('PRAGMA journal_mode=WAL')
                db.execute('CREATE TABLE vectors_ns_vector_chunks(id INTEGER PRIMARY KEY, data BLOB)')
                db.execute('INSERT INTO vectors_ns_vector_chunks VALUES(1,?)',(b'original-vector',))
            before=logical_store_identity(root)
            with sqlite3.connect(root/'engram_data.db') as db:db.execute('PRAGMA wal_checkpoint(TRUNCATE)')
            self.assertEqual(before,logical_store_identity(root))
            with sqlite3.connect(root/'engram_data.db') as db:db.execute('UPDATE vectors_ns_vector_chunks SET data=?',(b'changed-vector',))
            self.assertNotEqual(before,logical_store_identity(root))
    def test_p95_uses_fixed_nearest_rank_without_interpolation(self):
        from code_evidence_assessment import nearest_rank_p95
        self.assertEqual(nearest_rank_p95(list(range(60))),56)
        self.assertIsNone(nearest_rank_p95([]))


class AssessmentIntegrityTests(unittest.TestCase):
    def test_copied_freeze_cannot_verify_originals_but_execute_different_artifacts(self):
        import json,tempfile
        from types import SimpleNamespace
        from unittest.mock import patch
        import code_evidence_assessment as module
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);old=root/'original';copy=root/'copy';copy.mkdir()
            args=SimpleNamespace(output=copy,base=root/'base')
            frozen={'outputRoot':str(old),'baseRoot':str(args.base),'assessmentMarker':str(old/'assessment-started.json'),'inputs':{},'artifacts':{},'storeIdentities':{}}
            (copy/'freeze.json').write_text(json.dumps(frozen))
            with patch.object(module,'inputs') as release:
                with self.assertRaisesRegex(ValueError,'root/marker'):module.assessment(args)
                release.assert_not_called()
            frozen.update(outputRoot=str(copy),assessmentMarker=str(copy/'assessment-started.json'))
            (copy/'freeze.json').write_text(json.dumps(frozen))
            with patch.object(module,'inputs') as release:
                with self.assertRaisesRegex(ValueError,'path inventories'):module.assessment(args)
                release.assert_not_called()
    def test_development_never_traverses_unselected_judgments(self):
        import json,tempfile,hashlib
        from code_evidence_eval_support import selected_frozen
        with tempfile.TemporaryDirectory() as directory:
            base=Path(directory);root=base/'corpus';root.mkdir();raw=b'function f() {}';(root/'a.ts').write_bytes(raw)
            labels=[{'id':'dev','split':'development','polarity':'answerable','requiredUnits':[{'path':'a.ts','startByte':0,'endByte':len(raw),'grade':3,'requiredRanges':[{'startByte':0,'endByte':len(raw)}]}]},
                    {'id':'reserved','split':'heldout','requiredUnits':'must never be traversed in development'}]
            path=base/'labels.json';path.write_text(json.dumps(labels))
            manifest={'corpora':[{'id':'synthetic','files':{'a.ts':hashlib.sha256(raw).hexdigest()},'labelsFile':'labels.json','labelsSha256':hashlib.sha256(path.read_bytes()).hexdigest()}],'queryOrder':{'development':['dev'],'heldout':['reserved']}}
            file=base/'manifest.json';file.write_text(json.dumps(manifest))
            self.assertEqual(selected_frozen(file,'synthetic',root,'development')[2],[labels[0]])
    def test_failed_baseline_or_fast_run_invalidates_an_otherwise_passing_assessment(self):
        import copy
        from code_evidence_assessment import finish_metrics
        entries=[]
        for name,count,negative in [('engram',10,6),('flask',15,7),('vite',15,7)]:
            entries += [{'id':f'{name}-p{i}','corpus':name,'split':'heldout','polarity':'answerable'} for i in range(count)]
            entries += [{'id':f'{name}-n{i}','corpus':name,'split':'heldout','polarity':'negative'} for i in range(negative)]
        entries += [{'id':f'original{i}','corpus':'engram','split':'original','polarity':'answerable'} for i in range(10)]
        entries += [{'id':f'exact{i}','corpus':'engram','split':'identifier','polarity':'answerable'} for i in range(2)]
        report={'balancedMcp':{'operationalPass':True}}
        for variant in ['baseline','balanced','segmented','fast']:
            report[variant]={}
            for name in ['engram','flask','vite']:
                cases=[]
                for entry in [e for e in entries if e['corpus']==name]:
                    positive=entry['polarity']=='answerable'
                    metric={'candidateRecallAt50':1 if positive else None,'requiredUnitRecallAt10':1 if positive else None,'requiredRangeCoverage':1 if positive else None,'mrrAt10':1 if positive else 0,'ndcgAt10':1 if positive else 0,'allRequiredRecovered':positive,'unjudgedPrimaryItems':0}
                    cases.append({'id':entry['id'],'corpus':name,'failed':False,'validated':True,'evidenceStatus':'candidates_available' if positive else 'insufficient_evidence','metrics':metric,'observed':{'candidates':[]}})
                report[variant][name]={'cases':cases,'startup':{'failed':False},'warmup':{'failed':False}}
        valid=copy.deepcopy(report);finish_metrics(valid,entries);self.assertTrue(valid['allQualityAndWarmGatesPass'])
        for variant in ['baseline','fast']:
            broken=copy.deepcopy(report);broken[variant]['engram']['cases'][0]['failed']=True
            finish_metrics(broken,entries);self.assertFalse(broken['allQualityAndWarmGatesPass'])
        broken=copy.deepcopy(report);broken['baseline']['engram']['startup']['failed']=True
        finish_metrics(broken,entries);self.assertFalse(broken['allQualityAndWarmGatesPass'])


class ProbeIsolationTests(unittest.TestCase):
    def test_rejected_development_response_is_preserved_before_abort(self):
        import tempfile
        from types import SimpleNamespace
        from unittest.mock import patch
        import code_evidence_eval as module
        observed={'packet':{'rejected':'raw response'}}
        with tempfile.TemporaryDirectory() as directory:
            args=SimpleNamespace(output=Path(directory),base=Path(directory),probe='probe')
            sink={'cases':[]};checkpoints=[]
            with patch.object(module,'JsonProbe') as factory,patch.object(module,'strategy_maximum',side_effect=ValueError('wrong strategy')):
                factory.return_value.exchange.return_value=observed
                factory.return_value.errors=['child stderr']
                with self.assertRaisesRegex(ValueError,'wrong strategy'):
                    module.run_variant(args,'balanced','engram',[{'id':'q','query':'synthetic'}],sink=sink,checkpoint=lambda:checkpoints.append(len(sink['cases'])))
            self.assertEqual(sink['cases'][0]['observed'],observed)
            self.assertTrue(sink['cases'][0]['failed']);self.assertEqual(checkpoints,[1])
            self.assertEqual(sink['stderr'],['child stderr'])
    def test_assessment_preserves_rejected_response_and_restart_failure(self):
        import json,tempfile
        from types import SimpleNamespace
        from unittest.mock import Mock,patch
        import code_evidence_assessment as module
        from code_evidence_eval_support import ProbeProtocolError
        with tempfile.TemporaryDirectory() as directory:
            output=Path(directory);(output/'balanced-calibration.json').write_text('{"id":"cal"}')
            args=SimpleNamespace(output=output,base=output,probe='probe')
            entries=[{'id':'one','query':'synthetic','corpus':'engram'},{'id':'two','query':'synthetic','corpus':'engram'}]
            for failure in ['validation','protocol']:
                child=Mock();child.errors=['first child stderr']
                observed={'packet':{'rejected':'complete raw packet'}}
                if failure=='validation':child.exchange.return_value=observed
                else:child.exchange.side_effect=ProbeProtocolError('late child')
                report={};path=output/'assessment.json'
                with patch.object(module,'canonical_records',return_value=[]),patch.object(module,'JsonProbe',side_effect=[child,OSError('cannot restart')]),patch.object(module,'metric',return_value={'zero':0}),patch.object(module,'strategy_maximum',side_effect=ValueError('wrong state')):
                    if failure=='protocol':
                        with self.assertRaisesRegex(OSError,'cannot restart'):module.assess_variant(args,'balanced',entries,{'engram':{}},report,path,None)
                    else:module.assess_variant(args,'balanced',entries,{'engram':{}},report,path,None)
                stored=json.loads(path.read_text())['balanced']['engram']
                self.assertTrue(stored['cases'][0]['failed'])
                self.assertEqual(stored['children'][0]['stderr'],['first child stderr'])
                if failure=='validation':self.assertEqual(stored['cases'][0]['observed'],observed)
                else:self.assertEqual(stored['restartError'],'cannot restart')
    def test_late_response_after_timeout_cannot_be_relabelled_as_the_next_query(self):
        from code_evidence_eval_support import JsonProbe,ProbeProtocolError
        code="import sys,time,json; first=json.loads(sys.stdin.readline()); time.sleep(.10); print(json.dumps({'reply':first}),flush=True); [print(json.dumps({'reply':json.loads(line)}),flush=True) for line in sys.stdin]"
        probe=JsonProbe([sys.executable,'-u','-c',code])
        try:
            with self.assertRaisesRegex(ProbeProtocolError,'deadline'):probe.exchange({'query':'first'},timeout=.01)
            with self.assertRaisesRegex(ProbeProtocolError,'poisoned'):probe.exchange({'query':'second'},timeout=.5)
        finally:probe.close()
        fresh=JsonProbe([sys.executable,'-u','-c',code])
        try:self.assertEqual(fresh.exchange({'query':'second'},timeout=2),{'reply':{'query':'second'}})
        finally:fresh.close()


class LexicalFreezeTests(unittest.TestCase):
    def test_lock_exclusions_do_not_allow_symlink_fifo_or_directory(self):
        import os,tempfile
        from code_evidence_eval_support import lexical_identity
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);target=root/'.tantivy-writer.lock'
            target.symlink_to(root/'missing')
            with self.assertRaisesRegex(ValueError,'symlink'):lexical_identity(root)
            target.unlink();os.mkfifo(target)
            with self.assertRaisesRegex(ValueError,'nonregular'):lexical_identity(root)
            target.unlink();target.mkdir()
            with self.assertRaisesRegex(ValueError,'regular file'):lexical_identity(root)
    def test_lexical_mutation_addition_and_removal_invalidate_store_freeze(self):
        import tempfile,sqlite3
        from code_evidence_eval_support import logical_store_identity
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory)
            with sqlite3.connect(root/'engram_data.db') as db:db.execute('CREATE TABLE canonical(id TEXT)')
            lexical=root/'lexical/source-projections-v1';lexical.mkdir(parents=True)
            meta=lexical/'meta.json';meta.write_text('{"segments":["a"]}')
            segment=lexical/'a.store';segment.write_bytes(b'original segment')
            frozen=logical_store_identity(root)
            meta.write_text('{"segments":["b"]}')
            self.assertNotEqual(frozen,logical_store_identity(root))
            meta.write_text('{"segments":["a"]}')
            self.assertEqual(frozen,logical_store_identity(root))
            added=lexical/'b.term';added.write_bytes(b'new term index')
            self.assertNotEqual(frozen,logical_store_identity(root));added.unlink()
            segment.unlink();self.assertNotEqual(frozen,logical_store_identity(root))
    def test_only_declared_transient_tantivy_locks_are_excluded(self):
        import tempfile
        from code_evidence_eval_support import lexical_identity
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);(root/'meta.json').write_text('{}');before=lexical_identity(root)
            for name in ['.tantivy-writer.lock','.tantivy-meta.lock']:(root/name).write_bytes(b'lock state')
            self.assertEqual(before,lexical_identity(root))
            (root/'.other.lock').write_bytes(b'not a declared transient')
            self.assertNotEqual(before,lexical_identity(root))

class DevelopmentRuntimeTests(unittest.TestCase):
    def test_validated_calibration_id_alone_cannot_pass_runtime_negative_target(self):
        import copy
        from code_evidence_eval import verify_calibrated_development
        from code_evidence_eval_support import VARIANTS
        entries=[{'id':str(i),'corpus':'engram','polarity':'negative'} for i in range(15)]
        report={'validated':{},'calibration':{}}
        for variant in VARIANTS:
            report['calibration'][variant]={'artifact':{'id':'cal'},'negativeTarget':14}
            for name in ['engram','flask','vite']:
                report['validated'][variant+'-'+name]={'cases':[{'id':e['id'],'observed':{'packet':{'diagnostics':{'calibrationState':'validated','calibrationId':'cal'},'evidenceStatus':'insufficient_evidence','items':[]}}} for e in entries if e['corpus']==name]}
        self.assertEqual(verify_calibrated_development(report,entries)['balanced']['rejectedAtRuntime'],15)
        broken=copy.deepcopy(report)
        for case in broken['validated']['balanced-engram']['cases'][:2]:case['observed']['packet']['items']=['unexpected primary']
        with self.assertRaisesRegex(ValueError,'runtime negative target'):verify_calibrated_development(broken,entries)
        self.assertEqual(broken['calibration']['balanced']['runtimeNegativeCheck']['rejectedAtRuntime'],13)
        broken=copy.deepcopy(report);broken['validated']['fast-engram']['cases'].pop()
        with self.assertRaisesRegex(ValueError,'inventory/order'):verify_calibrated_development(broken,entries)

if __name__=='__main__':unittest.main()
