"""Harness omission/custody controls. These tests make no native workflow claim."""
import copy
import hashlib
import importlib.util
import json
import tempfile
import subprocess
import os
import time
import unittest
from pathlib import Path
from unittest.mock import Mock, patch

from tools.ci import run_provider_e2e as runner


class InventoryControls(unittest.TestCase):
    def test_complete_four_axis_inventory_is_not_execution(self):
        rows = runner.inventory()
        self.assertEqual(len(rows), len(runner.REQUIRED_OPERATIONS) * 4 * 2 * 4)
        self.assertEqual({r['host'] for r in rows}, set(runner.HOSTS))
        self.assertEqual({r['target'] for r in rows}, set(runner.TARGETS))
        self.assertTrue(all(r['status'] == 'NOT_RUN' for r in rows if r['required']))
        self.assertTrue(all(r['na_reason'] for r in rows if not r['required']))

    def test_each_omitted_operation_fails(self):
        operations = runner.catalogue()
        for index in range(len(operations)):
            with self.subTest(operation=operations[index]['operation']):
                with self.assertRaises(runner.ProofError):
                    runner.validate_catalogue(operations[:index] + operations[index + 1:])

    def test_required_host_capability_reason_and_duplicate_controls(self):
        self.assertEqual(runner.self_test_omissions(), 6)
        rows = runner.catalogue()
        with self.assertRaises(runner.ProofError):
            runner.validate_catalogue(rows + [copy.deepcopy(rows[0])])

    def test_rust_backfill_na_cannot_hide_another_required_operation(self):
        rows = runner.catalogue()
        rows[0]['rust'] = {'applicable': False, 'reason': 'No EngramProvider because omitted', 'source': ['irrelevant']}
        with self.assertRaises(runner.ProofError):
            runner.validate_catalogue(rows)
        actual = [r for r in runner.inventory() if r['host'] == 'rust' and not r['required']]
        self.assertEqual({r['operation'] for r in actual}, {'code.backfill'})

    def test_none_is_not_empty_in_any_scope_dimension(self):
        original = {'tenant': 'owned'}
        self.assertEqual(runner.scope_tuple(original), runner.scope_tuple({**original, 'subject': None}))
        for key in ('subject', 'workspace', 'session', 'environment'):
            self.assertNotEqual(runner.scope_tuple(original), runner.scope_tuple({**original, key: ''}))


class ArtifactControls(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.path = Path(self.directory.name) / 'controlled-artifact'
        self.path.write_bytes(b'controlled artifact bytes, not a native runtime proof')
        digest=hashlib.sha256(self.path.read_bytes()).hexdigest()
        files={'bindings/node/src/lib.rs':{'kind':'file','sha256':'a'*64}}
        self.trusted={'source':{'files':files,'snapshot_sha256':runner.snapshot_digest(files)},'artifacts':[]}
        unit={'reason':'compiler-artifact','package_id':'path+file:///controlled#engram-node@0.1.0','target':{'crate_types':['cdylib']},'features':['helixdb-embedded']}
        inputs=[{'path':'controlled-compiler-input','sha256':'b'*64}]
        compiler=Path(self.directory.name)/'compiler.json';links=Path(self.directory.name)/'links.json'
        compiler.write_text(json.dumps({'command':['cargo','+1.88.0','build'],'exit':0,'package':'engram-node','kind':'cdylib','artifact_sha256':digest,'source_snapshot_sha256':self.trusted['source']['snapshot_sha256'],'target_sources':['bindings/node/src/lib.rs'],'units':[unit]}))
        links.write_text(json.dumps({'inputs':inputs}))
        self.record={'provider':'helixdb','host':'native','mode':'embedded','target':'linux-x64-gnu','source_revision':'source','lock_sha256':'lock','path':str(self.path),'sha256':digest,'features':['helixdb-embedded'],'instrumented':False,'profile':'debug','production_compiler_packages':['engram-node'],'link_inputs':inputs,'compiler_record':str(compiler),'compiler_record_sha256':runner.sha(compiler),'link_record':str(links),'link_record_sha256':runner.sha(links)}
        self.trusted['artifacts']=[{**{key:self.record[key] for key in ('host','mode','target','profile','instrumented','sha256','features','compiler_record_sha256','link_record_sha256')},'package':'engram-node','kind':'cdylib'}]

    def check(self, record):
        return runner.validate_artifact(record,host='native',mode='embedded',target='linux-x64-gnu',source='source',lock='lock',trusted=self.trusted,bundle_root=Path(self.directory.name))

    def test_arbitrary_module_and_emitters_do_not_match_the_independent_produced_pin(self):
        for name,content in [('emitter.cjs',b'module.exports={NativeProvider:class{}}'),('emitter',b'print invented success')]:
            path=Path(self.directory.name)/name;path.write_bytes(content)
            with self.assertRaises(runner.ProofError):self.check({**self.record,'path':str(path),'sha256':runner.sha(path)})
        altered=copy.deepcopy(self.record);altered['link_inputs']=[{'path':'invented','sha256':'f'*64}]
        with self.assertRaises(runner.ProofError):self.check(altered)

    def test_valid_metadata_only_is_not_workflow_execution(self):
        self.assertEqual(self.check(self.record), self.path)

    def test_missing_addon_stale_bytes_wrong_provider_mode_source_and_feature(self):
        for key, value in [('path', str(self.path)+'-removed'), ('sha256','stale'), ('provider','sqlite'), ('mode','service'), ('target','win32-x64'), ('source_revision','old'), ('lock_sha256','old'), ('features',['helixdb-embedded','sqlite']), ('instrumented',True), ('profile',None), ('link_inputs',[]), ('production_compiler_packages',['engram-conformance'])]:
            with self.subTest(key=key), self.assertRaises(runner.ProofError):
                self.check({**self.record, key:value})

    def test_actual_artifact_removal_is_detected(self):
        self.path.unlink()
        with self.assertRaises(runner.ProofError):
            self.check(self.record)

    def test_dev_graph_cannot_substitute_for_production_closure(self):
        with self.assertRaises(runner.ProofError):
            self.check({**self.record, 'production_compiler_packages':[], 'dev_packages':['engram-node']})


class PlanIntegrityControls(unittest.TestCase):
    setUp=ArtifactControls.setUp
    def test_operation_and_fault_template_hashes_are_checked_before_namespace(self):
        root=Path(self.directory.name)
        template={'scope':{'tenant':'owned'},'config':{'storage_provider':'helixdb','helixdb':{'mode':'embedded'}},'fixture':{'controlled':True},'calls':[]}
        operation=root/'operations.json';fault=root/'fault.json'
        operation.write_text(json.dumps(template));fault.write_text(json.dumps(template))
        self.record.update(request_plan=str(operation),request_plan_sha256=runner.sha(operation),fault_plans={'memory':str(fault)},fault_plan_sha256={'memory':runner.sha(fault)})
        self.trusted['artifacts'][0].update(request_plan_sha256=runner.sha(operation),fault_plan_sha256={'memory':runner.sha(fault)})
        packet,digest=runner.preflight_plan(self.record,self.trusted,root,'native','embedded')
        self.assertEqual(digest,runner.sha(operation));self.assertEqual(packet,template)
        for path,scenario in [(operation,None),(fault,'memory')]:
            before=path.read_bytes();path.write_bytes(before+b' changed')
            with patch.object(Path,'mkdir') as mkdir,patch.object(runner.subprocess,'Popen') as spawn:
                with self.assertRaises(runner.ProofError):runner.preflight_plan(self.record,self.trusted,root,'native','embedded',scenario)
                mkdir.assert_not_called();spawn.assert_not_called()
            path.write_bytes(before)
        outside=root.parent/(root.name+'-outside-controlled-plan.json')
        outside.write_bytes(fault.read_bytes());self.addCleanup(outside.unlink)
        self.record['fault_plans']['memory']=str(outside)
        with self.assertRaises(runner.ProofError):runner.preflight_plan(self.record,self.trusted,root,'native','embedded','memory')


class IndependentSnapshotControls(unittest.TestCase):
    """Real local source bytes with a separately pinned record; no host workflow."""
    def setUp(self):
        self.directory=tempfile.TemporaryDirectory();self.addCleanup(self.directory.cleanup)
        self.root=Path(self.directory.name);self.bundle=self.root/'bundle';self.bundle.mkdir()
        for name,data in [('tracked.rs','original'),('Cargo.lock','controlled lock')]:(self.root/name).write_text(data)
        for command in [['init','-q'],['add','tracked.rs','Cargo.lock'],['-c','user.name=Control','-c','user.email=control@localhost','-c','commit.gpgsign=false','commit','-qm','controlled-source']]:
            subprocess.run(['git','-C',str(self.root),*command],check=True,capture_output=True)
        (self.root/'new.rs').write_text('approved new source')
        head=subprocess.check_output(['git','rev-parse','HEAD'],cwd=self.root,text=True).strip()
        self.record={'schema':'engram.provider-build-record.v1','producer':{'repository':'controlled-repository','run_id':'controlled-run','commit':head},'source':runner.source_snapshot(['new.rs'],root=self.root),'lock_sha256':runner.sha(self.root/'Cargo.lock'),'artifacts':[{'controlled':'not native evidence'}]}
        self.path=self.root/'expected-build-record.json';self.path.write_text(json.dumps(self.record));self.pin=runner.sha(self.path)
    def load(self,path=None,pin=None):
        with patch.object(runner,'ROOT',self.root):return runner.load_expected_build_record(path or self.path,pin or self.pin,self.bundle)
    def test_source_record_is_external_and_content_bound_not_authenticated_identity(self):
        self.assertEqual(self.load(),self.record)
        inside=self.bundle/'self-asserted.json';inside.write_text(self.path.read_text())
        with self.assertRaises(runner.ProofError):self.load(inside)
        with self.assertRaises(runner.ProofError):self.load(pin='0'*64)
    def test_same_head_changed_tracked_or_approved_new_bytes_are_rejected(self):
        for name in ['tracked.rs','new.rs','Cargo.lock']:
            path=self.root/name;original=path.read_bytes();path.write_bytes(original+b' changed same HEAD')
            with self.subTest(name=name),self.assertRaises(runner.ProofError):self.load()
            path.write_bytes(original)
    def test_changed_type_cannot_hide_behind_equal_content(self):
        original=(self.root/'tracked.rs').read_bytes();(self.root/'copy.rs').write_bytes(original)
        (self.root/'tracked.rs').unlink();(self.root/'tracked.rs').symlink_to('copy.rs')
        with self.assertRaises(runner.ProofError):self.load()

    def test_external_and_untracked_referents_are_rejected_without_reading_them(self):
        external=self.root.parent/(self.root.name+'-private.rs')
        external.write_text('private external source');self.addCleanup(external.unlink)
        link=self.root/'new.rs';link.unlink()
        for target in [str(external),'untracked.rs']:
            link.symlink_to(target)
            with patch.object(runner,'sha',side_effect=AssertionError('no referent may be opened')):
                # Check the link directly before unrelated regular snapshot inputs.
                with patch.object(runner.subprocess,'check_output',return_value=b'new.rs\0'):
                    with self.assertRaises(runner.ProofError):runner.source_snapshot(root=self.root)
            link.unlink()

    def test_nested_approved_referent_bytes_and_types_are_bound(self):
        directory=self.root/'nested';directory.mkdir()
        (directory/'module.rs').write_text('fn original() {}')
        (self.root/'new.rs').unlink();(self.root/'new.rs').symlink_to('nested/module.rs')
        (directory/'alias.rs').symlink_to('../new.rs')
        approved=['new.rs','nested/module.rs','nested/alias.rs']
        before=runner.source_snapshot(approved,root=self.root)
        self.assertEqual(before['files']['new.rs']['kind'],'symlink')
        (directory/'module.rs').write_text('fn changed() {}')
        self.assertNotEqual(before,runner.source_snapshot(approved,root=self.root))
        (directory/'module.rs').unlink();(directory/'module.rs').symlink_to('../new.rs')
        with self.assertRaises(runner.ProofError):runner.source_snapshot(approved,root=self.root)

    def test_directory_link_cannot_hide_external_or_untracked_source(self):
        (self.root/'nested').symlink_to(self.bundle,target_is_directory=True)
        with self.assertRaises(runner.ProofError):runner.source_snapshot(['nested/hidden.rs'],root=self.root)


class OneShotRouteControls(unittest.TestCase):
    def test_actual_consumer_parser_binding_and_prior_journal_results(self):
        source=(runner.ROOT/'packages/node/test/provider-e2e.mjs').read_text()
        body=source[source.index('function pointer('):source.index('function memorySentinel(')]
        definitions=runner.catalogue()
        program="import assert from 'node:assert/strict';assert.equal(process.versions.node.split('.')[0],'22');const catalogue=new Map("+json.dumps([[row['operation'],row] for row in definitions])+ ");const results=new Map();"+body+"""
        const call=(operation,argv)=>({operation,oneshot_argv:argv});
        assert.deepEqual(oneShotRoute(call('code.reset_apply',['code','--op','reset-apply'])),{command:'code',operation:'reset-apply'});
        for(const argv of [['code','--op','status'],['code','--op','reset-apply','--op','status'],['code','--op','status','--op','reset-apply'],['code','--op','reset-apply','--help'],['code','--op','reset-apply','-h']])assert.throws(()=>oneShotRoute(call('code.reset_apply',argv)));
        for(const subcommand of ['list','status','widen','help'])assert.throws(()=>oneShotRoute(call('ontology.put',['pack',subcommand,'schema'])));
        assert.throws(()=>oneShotRoute(call('ontology.put',['pack','install','schema','--help'])));
        assert.throws(()=>oneShotRoute(call('ontology.put',['pack','install','--storage','wrong-pack'])));
        assert.equal(oneShotRoute(call('ontology.put',['pack','install','schema'])).operation,'install');
        assert.equal(oneShotRoute(call('code.scan',['code','--op','rebuild'])).operation,'rebuild');
        assert.equal(oneShotRoute(call('code.source',['fetch-content','--path','src.rs'])).command,'fetch-content');
        const journal={requestId:'actual-prior-request',generation:'original-generation'};
        const preview=oneShotResult(JSON.stringify({ok:true,command:'code',op:'reset-preview',output:JSON.stringify({journal})}),{command:'code',operation:'reset-preview'});
        results.set('preview',preview);
        assert.throws(()=>oneShotResult(JSON.stringify({ok:true,command:'code',op:'status',output:'{}'}),{command:'code',operation:'reset-preview'}));
        const apply=call('code.reset_apply',['code','--op','reset-apply','--journal',{$result:'preview#/journal'}]);
        const argv=oneShotArguments(apply);assert.deepEqual(JSON.parse(argv[4]),journal);
        assert.throws(()=>oneShotArguments(call('code.reset_resume',['code','--op','reset-resume','--journal',{$result:'future#/journal'}])));
        assert.throws(()=>oneShotArguments(call('code.reset_apply',['code','--op','reset-apply','--repo',{$result:'preview#/journal'}])));
        results.set('prior',{flag:'--help',path:'src/actual.rs',packCatalogue:'owned-catalogue.json'});
        for(const candidate of [call('ontology.put',['pack','install','schema',{$result:'prior#/flag'}]),call('code.source',['fetch-content',{$result:'prior#/flag'}])])assert.throws(()=>oneShotRoute(candidate));
        for(const candidate of [call('ontology.put',['pack','install','schema','--catalogue',{$result:'prior#/flag'}]),call('code.source',['fetch-content','--path',{$result:'prior#/flag'}]),call('code.reset_apply',['code','--op','reset-apply','--repo',{$result:'prior#/flag'}])])assert.throws(()=>oneShotArguments(candidate));
        assert.deepEqual(oneShotArguments(call('code.source',['fetch-content','--path',{$result:'prior#/path'}])),['fetch-content','--path','src/actual.rs']);
        assert.deepEqual(oneShotArguments(call('ontology.put',['pack','install','schema','--catalogue',{$result:'prior#/packCatalogue'}])),['pack','install','schema','--catalogue','owned-catalogue.json']);
        assert.throws(()=>oneShotRoute(call('code.reset_apply',['code','--op',{$result:'prior#/flag'}])));
        assert.throws(()=>oneShotRoute(call('code.source',['fetch-content','--storage',{$result:'prior#/path'}])));
        const launchFlags=['--domain','--subdomain','--project','--org','--storage','--trusted-root','--backend','--helix-mode','--helix-store','--helix-endpoint','--helix-credential-env','--helix-path','--pg-connection-string','--ontology','--taxonomy','--layout','--db-file','--tools','--embedding-provider','--embedding-model','--embedding-url','--vector','--no-vector','--future-startup-setting'];
        const routes=[['retrieval.recall',['recall','--query','needle']],['code.status',['code','--op','status']],['ontology.put',['pack','install','schema']],['code.source',['fetch-content','--path','src.rs']],['code.backfill',['backfill-embeddings']]];
        for(const flag of launchFlags)for(const [operation,argv] of routes)assert.throws(()=>oneShotRoute(call(operation,[...argv,flag,'foreign'])));
        assert.deepEqual(oneShotArguments(call('code.reset_apply',['code','--op','reset-apply','--repo','actual-repo','--journal',{$result:'preview#/journal'},'--decision','apply','--json'])),['code','--op','reset-apply','--repo','actual-repo','--journal',JSON.stringify(journal),'--decision','apply','--json']);
        assert.deepEqual(oneShotArguments(call('retrieval.recall',['recall','-q','needle','--limit','3','--json','--include-code-artifacts'])),['recall','-q','needle','--limit','3','--json','--include-code-artifacts']);
        assert.deepEqual(oneShotArguments(call('ontology.put',['pack','install','schema','--catalogue','owned.json','--slice-size','all','--validate-graph','graph'])),['pack','install','schema','--catalogue','owned.json','--slice-size','all','--validate-graph','graph']);
        console.log('effective routes and actual prior-result journal binding passed; no product process spawned');
        """
        result=subprocess.run(['node','--input-type=module','--eval',program],capture_output=True,text=True)
        self.assertEqual(result.returncode,0,result.stdout+result.stderr)

class McpScopeBindingControls(unittest.TestCase):
    def test_actual_args_reject_unrepresentable_none_empty_dimensions_before_spawn(self):
        source=(runner.ROOT/'packages/node/test/provider-e2e.mjs').read_text()
        body=source[source.index('function mcpArgs() {'):source.index('function mcpEnvironment()')]
        program="import assert from 'node:assert/strict';assert.equal(process.versions.node.split('.')[0],'22');let actualMcpScope;let packet;const scopeTuple=s=>['tenant','subject','workspace','session','environment'].map(k=>s[k]??null);"+body+"""
        const base={tenant:'owned',workspace:'workspace'};
        for(const key of ['subject','session','environment'])for(const value of [null,'','present']){
          packet={scope:{...base,[key]:value},config:{storage_path:'/owned/storage',trusted_root:'/owned',storage_provider:'sqlite',enable_vector:false}};
          if(value===null){mcpArgs();assert.deepEqual(scopeTuple(actualMcpScope),scopeTuple(packet.scope));}else assert.throws(()=>mcpArgs());
        }
        for(const value of [undefined,null,'']){
          packet={scope:{tenant:'owned',workspace:value},config:{storage_path:'/owned/storage',trusted_root:'/owned',storage_provider:'sqlite',enable_vector:false}};
          if(value===''){const argv=mcpArgs();assert.equal(argv[argv.indexOf('--project')+1],'');assert.equal(actualMcpScope.workspace,'');}else assert.throws(()=>mcpArgs());
        }
        console.log('supported Scope tuple positives and every None/empty negative passed; no host spawned');
        """
        result=subprocess.run(['node','--input-type=module','--eval',program],capture_output=True,text=True)
        self.assertEqual(result.returncode,0,result.stdout+result.stderr)
        for key in ('subject','session','environment'):
            for value in ('','present'):
                with self.assertRaises(runner.ProofError):runner.validate_host_scope({'tenant':'owned','workspace':'workspace',key:value},'mcp-stdio')
        for value in (None,):
            with self.assertRaises(runner.ProofError):runner.validate_host_scope({'tenant':'owned','workspace':value},'mcp-stdio')
        runner.validate_host_scope({'tenant':'owned','workspace':''},'mcp-stdio')


class ReceiptControls(unittest.TestCase):
    def setUp(self):
        self.call = {'id':'same-method-first', 'operation':'memory.write', 'expect':{'status':'ok'}}
        self.binding = {'provider':'helixdb','mode':'embedded','host':'native','target':'linux-x64-gnu','scope':{'tenant':'owned'},'store':'owned','invocation':'invocation','source_revision':'source','lock_sha256':'lock','artifact_sha256':'artifact'}
        self.receipt = {'schema':'engram.provider-operation.v1','call_id':self.call['id'],'operation':'memory.write','capability':'memory',**self.binding,'status':'PASS','outcome':'ok','assertions':['actual expected-outcome assertion'],'result_sha256':'actual-result-digest'}

    def test_complete_independent_receipt(self):
        runner.validate_receipts([self.call], [self.receipt], self.binding)

    def test_omission_failed_skipped_wrong_host_capability_reason(self):
        for key, value in [('status','SKIP'),('status','FAIL'),('host','rust'),('capability',None),('operation','archive.apply'),('call_id','missing'),('assertions',[]),('result_sha256',None),('outcome','unsupported')]:
            with self.subTest(key=key), self.assertRaises(runner.ProofError):
                runner.validate_receipts([self.call], [{**self.receipt,key:value}], self.binding)
        with self.assertRaises(runner.ProofError):
            runner.validate_receipts([self.call], [], self.binding)
        expected = {**self.call, 'expect':{'status':'unsupported'}}
        with self.assertRaises(runner.ProofError):
            runner.validate_receipts([expected], [{**self.receipt, 'outcome':'unsupported'}], self.binding)

    def test_repeated_methods_require_distinct_call_ids(self):
        second = {**self.call, 'id':'same-method-second'}
        runner.validate_receipts([self.call,second], [self.receipt,{**self.receipt,'call_id':second['id']}], self.binding)
        with self.assertRaises(runner.ProofError):
            runner.validate_receipts([self.call,second], [self.receipt,self.receipt], self.binding)


class TypedTraceControls(unittest.TestCase):
    def setUp(self):
        self.id = '11111111-2222-4333-8444-555555555555'
        self.binding = {'backend':'helixdb','mode':'embedded','store':'owned','scope':{'tenant':'owned'}}
        self.before = {'phase':'BeforeSend','actor':'actor','call':'call','session':'session','item':'item','step':'step','commitId':self.id,**self.binding}

    def test_complete_typed_send_ack_and_unknown_contrast(self):
        runner.validate_trace([self.before,{**self.before,'phase':'AfterAck'}], self.binding)
        runner.validate_trace([self.before], self.binding, self.id)
        with self.assertRaises(runner.ProofError):
            runner.validate_trace([self.before,{**self.before,'phase':'AfterAck'}], self.binding,self.id)

    def test_missing_identity_mismatched_scope_forged_uuid_ack_and_replay(self):
        for key in ('actor','call','session','backend','mode','store','scope','item','step','commitId'):
            altered = copy.deepcopy(self.before); altered.pop(key)
            with self.subTest(key=key), self.assertRaises(runner.ProofError):
                runner.validate_trace([altered],self.binding)
        for field, value in [('scope',{'tenant':'foreign'}),('store','other'),('commitId','credential-secret/private-record')]:
            with self.subTest(field=field), self.assertRaises(runner.ProofError):
                runner.validate_trace([{**self.before,field:value}],self.binding)
        with self.assertRaises(runner.ProofError):
            runner.validate_trace([{**self.before,'phase':'AfterAck'}],self.binding)
        with self.assertRaises(runner.ProofError):
            runner.validate_trace([self.before,self.before],self.binding)
        with self.assertRaises(runner.ProofError):
            runner.validate_trace([self.before,{**self.before,'call':'later','commitId':'21111111-2222-4333-8444-555555555555'}],self.binding,self.id)


class OwnedExecAcquisitionControls(unittest.TestCase):
    """Pure acquisition-race controls; not actual native ownership evidence."""
    @classmethod
    def setUpClass(cls):
        spec = importlib.util.spec_from_file_location('helper_control', runner.ROOT/'tools/ci/helixdb_server.py')
        cls.helper = importlib.util.module_from_spec(spec); spec.loader.exec_module(cls.helper)

    def test_preexec_empty_argv_is_not_accepted(self):
        child = Mock(pid=123); child.poll.return_value=None
        initial={'pid':123,'start_ticks':'original','exe':'/old/python','argv':['']}
        ready={**initial,'exe':'/owned/server','argv':['/owned/server']}
        helper=self.helper
        with patch.object(helper.os,'pidfd_open',return_value=99,create=True), patch.object(helper.signal,'pidfd_send_signal',create=True), patch.object(helper.os,'close'), patch.object(helper,'process_identity',side_effect=[initial,initial,ready]), patch.object(helper.time,'sleep'):
            self.assertEqual(helper.acquire_process_identity(child,'/owned/server',['/owned/server']),ready)

    def test_changed_start_identity_fails_without_signaling(self):
        child=Mock(pid=123); child.poll.return_value=None
        initial={'pid':123,'start_ticks':'original','exe':'/old/python','argv':['']}
        helper=self.helper
        with patch.object(helper.os,'pidfd_open',return_value=99,create=True), patch.object(helper.signal,'pidfd_send_signal',create=True) as signal_call, patch.object(helper.os,'close'), patch.object(helper,'process_identity',side_effect=[initial,{**initial,'start_ticks':'foreign'}]):
            with self.assertRaisesRegex(RuntimeError,'identity changed'):
                helper.acquire_process_identity(child,'/owned/server',['/owned/server'])
            signal_call.assert_not_called()


class LiveConsumerCustodyControls(unittest.TestCase):
    """Actual Linux Popen/pidfd controls; no provider/native/storage/network proof."""
    def setUp(self):
        self.directory=tempfile.TemporaryDirectory();self.addCleanup(self.cleanup_closed_fixture)
        self.output=Path(self.directory.name)
        self.packet={'artifact_sha256':'a'*64,'invocation':'owned-process-control','scope':{'tenant':'controlled'},'source_revision':'source','lock_sha256':'lock'}
        self.helper=runner.helper_module();self.helper.require_pidfd_capability()
        self.addCleanup(self.close_original)
    def cleanup_closed_fixture(self):
        if runner._PENDING_CHILDREN:
            self.directory._finalizer.detach()
            print(json.dumps({'retained_pending_custody_root':str(self.output),'native_workflow':False}))
        else:
            operations=self.output/'operations.jsonl'
            if operations.is_file():print(json.dumps({'live_process_control':self.id(),'actual_custody':runner.load_json_lines(operations),'native_workflow':False}))
            self.directory.cleanup()
    def close_original(self):
        for custody in list(runner._PENDING_CHILDREN.values()):
            child=custody['process']
            if child.poll() is None and child.stdin:
                try:child.stdin.write('release\n');child.stdin.flush()
                except (OSError,ValueError):pass
        self.assertTrue(runner.drain_pending_children(3),'controlled original child did not exit; preserve pending custody')
    def launch(self,program):
        return runner.run_owned(['/usr/bin/python3','-c',program],self.output,self.packet)
    def test_acquisition_error_retains_real_owner_and_blocks_dependent_cleanup(self):
        def fail(process,*args,**kwargs):
            state=json.loads((self.output/'pending-process.json').read_text())
            self.assertEqual(state['acquisition_pending']['pid'],process.pid)
            self.assertTrue(state['acquisition_pending']['pidfd_acquired'])
            raise RuntimeError('controlled acquisition error')
        with patch.object(self.helper,'acquire_process_identity',side_effect=fail),patch.object(runner,'helper_module',return_value=self.helper):
            with self.assertRaises(runner.ProofError):self.launch("import sys,select;print('controlled-ready',flush=True);select.select([sys.stdin],[],[],2)")
        self.assertFalse(runner.consumer_closure_confirmed(self.output))
        self.assertTrue(runner._PENDING_CHILDREN)
        with patch.object(runner.subprocess,'Popen') as spawn:
            with self.assertRaises(runner.ProofError):self.launch('pass')
            spawn.assert_not_called()
        self.close_original();self.assertTrue(runner.consumer_closure_confirmed(self.output))
    def test_live_real_exec_mismatch_is_retained_then_original_self_exit_is_reaped(self):
        original=self.helper.acquire_process_identity
        def delayed(process,exe,argv,**kwargs):
            time.sleep(.06)
            return original(process,exe,argv,timeout=.02,descriptor=kwargs.get('descriptor'))
        with patch.object(self.helper,'acquire_process_identity',side_effect=delayed),patch.object(runner,'helper_module',return_value=self.helper):
            with self.assertRaises(runner.ProofError):self.launch("import os;os.execv('/usr/bin/sleep',['/usr/bin/sleep','0.3'])")
        self.assertFalse(runner.consumer_closure_confirmed(self.output))
        self.assertTrue(runner.drain_pending_children(2))
        self.assertTrue(runner.consumer_closure_confirmed(self.output))
    def test_early_exit_is_observed_via_original_popen_not_left_live(self):
        original=self.helper.acquire_process_identity
        def delayed(process,exe,argv,**kwargs):
            time.sleep(.06)
            return original(process,exe,argv,timeout=.02,descriptor=kwargs.get('descriptor'))
        with patch.object(self.helper,'acquire_process_identity',side_effect=delayed),patch.object(runner,'helper_module',return_value=self.helper):
            with self.assertRaises(runner.ProofError):self.launch('pass')
        self.assertFalse(runner._PENDING_CHILDREN)
        self.assertTrue(runner.consumer_closure_confirmed(self.output))
        self.assertIn('original-pending-child-reaped',(self.output/'operations.jsonl').read_text())


class GatewayCorrelationControls(unittest.TestCase):
    """Pure forged-response controls; not actual network or native evidence."""
    @classmethod
    def setUpClass(cls):
        spec = importlib.util.spec_from_file_location('gateway_controls', runner.ROOT/'tools/ci/helixdb_server.py')
        cls.helper = importlib.util.module_from_spec(spec); spec.loader.exec_module(cls.helper)

    def test_partition_owner_id_payload_and_mutation_guard_are_required(self):
        payload = '{"owner":"owned-token","epoch":1,"partition":"owned"}'
        event = {'commitId':'11111111-2222-4333-8444-555555555555','expectedReceiptSha256':hashlib.sha256(payload.encode()).hexdigest(),'receiptContext':{'partition':'owned','mutationGuard':'guard'}}
        response = {'owner':[{}],'guard':[{}],'receipt':[{'public_id':event['commitId'],'partition':'owned','payload':payload}]}
        self.assertTrue(self.helper.matching_native_receipt(response,event))
        for key,value in [('public_id','foreign'),('partition','foreign'),('payload',payload.replace('owned-token','foreign-token'))]:
            bad=copy.deepcopy(response);bad['receipt'][0][key]=value
            self.assertFalse(self.helper.matching_native_receipt(bad,event))
        for key in ('owner','guard'):
            bad=copy.deepcopy(response);bad[key]=[]
            self.assertFalse(self.helper.matching_native_receipt(bad,event))

    def test_wrong_invocation_host_artifact_and_missing_real_request_do_not_select(self):
        with tempfile.TemporaryDirectory() as root:
            owned=Path(root); helper=owned/'helper';helper.mkdir();trace=owned/'effect-events.jsonl'
            payload='{"owner":"secret-not-logged","partition":"owned"}'
            binding={'invocation':'owned-invocation','host':'native','sourceRevision':'source','artifactSha256':'artifact','actual':{'backend':'helixdb','mode':'service','store':'owned','scope':{'tenant':'owned'}}}
            event={'phase':'BeforeSend','call':'actual-call','session':'actual-session','step':'actual-step','commitId':'11111111-2222-4333-8444-555555555555','expectedReceiptSha256':hashlib.sha256(payload.encode()).hexdigest(),**binding['actual']}
            trace.write_text(json.dumps({'binding':binding,'event':event})+'\n')
            control={'action':'definite_failure','binding':binding,'trace_file':str(trace)}
            path=helper/'fault-control.json';path.write_text(json.dumps(control))
            self.assertIsNotNone(self.helper.bound_fault(helper,{'actual-literal':payload}))
            self.assertIsNone(self.helper.bound_fault(helper,{'different':'request'}))
            for key in ('invocation','host','artifactSha256'):
                bad=copy.deepcopy(control);bad['binding'][key]='foreign'
                path.write_text(json.dumps(bad))
                if key=='host':
                    with self.assertRaises(ValueError):self.helper.bound_fault(helper,{'actual-literal':payload})
                else:self.assertIsNone(self.helper.bound_fault(helper,{'actual-literal':payload}))

class FaultAuditorControls(unittest.TestCase):
    """Controlled typed records test the auditor, never a native fault claim."""
    def setUp(self):
        self.original='11111111-2222-4333-8444-555555555555';self.prior='aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee'
        self.binding={'provider':'helixdb','mode':'embedded','host':'native','target':'linux-x64-gnu','scope':{'tenant':'owned'},'store':'owned','invocation':'invocation','source_revision':'source','lock_sha256':'lock','artifact_sha256':'artifact'}
        actual={'backend':'helixdb','mode':'embedded','store':'owned','scope':self.binding['scope']}
        frame={'actor':'actor','call':'call','session':'session','item':'item','step':'step',**actual}
        self.calls=[{'id':'unknown','operation':'memory.write','expect':{'status':'error','redact':['private-record']}}]
        self.receipts=[{'schema':'engram.provider-operation.v1','call_id':'unknown','operation':'memory.write','capability':'memory',**self.binding,'status':'PASS','outcome':'error','reason':'OutcomeUnknown '+self.original,'assertions':['actual fixture assertion'],'result_sha256':'digest'}]
        self.snapshot={'binding':actual,'events':[{**frame,'phase':'BeforeSend','commitId':self.prior},{**frame,'phase':'AfterAck','commitId':self.prior},{**frame,'phase':'BeforeSend','commitId':self.original}],
                       'observations':[{**frame,'outcome':'Unknown','commitId':self.original,'operationId':self.original},{**frame,'call':'blocked-call','outcome':'BlockedByPriorUnknown','step':None,'commitId':None,'operationId':self.original}],
                       'nativeReceipts':[{'commitId':self.original,'canonicalReceiptMatches':True}],'cleanupGuardRefusals':[self.original]}
        self.receipts[0]['observations']=[self.snapshot['observations'][0]]
    def audit(self,snapshot=None,receipts=None,host=None):
        binding={**self.binding,**({'host':host} if host else {})}
        rows=receipts or self.receipts
        calls=copy.deepcopy(self.calls)
        if host:rows=[{**row,'host':host} for row in rows]
        if host=='mcp-oneshot':
            calls[0].update(operation='ontology.put',oneshot_argv=['pack','install','fixture'])
            rows=[{**row,'operation':'ontology.put','capability':'ontology'} for row in rows]
        return runner.audit_fault(snapshot or self.snapshot,calls,rows,binding,'cleanup-wrapped',1)
    def test_valid_typed_unknown_keeps_logical_custody_separate_from_reap(self):
        result=self.audit();self.assertEqual(result['logical_outcome'],'UNKNOWN_RETAINED')
        self.assertEqual(result['canonical_reconciliation'],'NOT_PERFORMED')
        self.assertEqual(result['native_close_attestation'],'NOT_OBSERVED')
    def test_expected_nonzero_missing_receipt_or_forged_blocking_never_passes(self):
        for mutate in [lambda row:row.update(nativeReceipts=[]),lambda row:row.update(cleanupGuardRefusals=[]),lambda row:row['observations'][1].update(commitId=self.prior),lambda row:row['observations'][0].update(operationId='private-record'),lambda row:row['events'].append({**row['events'][-1],'commitId':'99999999-8888-4777-8666-555555555555'})]:
            bad=copy.deepcopy(self.snapshot);mutate(bad)
            with self.assertRaises(runner.ProofError):self.audit(snapshot=bad)
        with self.assertRaises(runner.ProofError):self.audit(receipts=[{**self.receipts[0],'reason':self.original+' private-record'}])
    def test_coherent_foreign_store_snapshot_cannot_redefine_configured_expectation(self):
        bad=copy.deepcopy(self.snapshot);bad['binding']['store']='foreign'
        for row in bad['events']+bad['observations']:row['store']='foreign'
        with self.assertRaises(runner.ProofError):self.audit(snapshot=bad)

    def test_same_process_blocked_assertion_na_is_only_real_oneshot_lifecycle(self):
        unblocked=copy.deepcopy(self.snapshot);unblocked['observations']=unblocked['observations'][:1]
        with self.assertRaises(runner.ProofError):self.audit(snapshot=unblocked)
        result=self.audit(snapshot=unblocked,host='mcp-oneshot')
        self.assertEqual(result['blocked_admission']['status'],'N/A')


class OwnedPacketControls(unittest.TestCase):
    def test_input_plan_cannot_open_foreign_storage_or_reuse_logical_store(self):
        with tempfile.TemporaryDirectory() as root:
            packet={'scope':{'tenant':'owned','subject':''},'config':{'storage_provider':'helixdb','storage_path':'/foreign','trusted_root':'/','helixdb':{'mode':'embedded','path':'/foreign-native','logical_store':'production'}}}
            first=runner.bind_owned_packet(packet,Path(root),'native','embedded','invocation')
            second=runner.bind_owned_packet(packet,Path(root),'native','embedded','invocation')
            self.assertNotEqual(first['config']['helixdb']['logical_store'],second['config']['helixdb']['logical_store'])
            self.assertTrue(Path(first['config']['storage_path']).is_relative_to(Path(root)))
            self.assertEqual(packet['config']['storage_path'],'/foreign')


if __name__ == '__main__':
    unittest.main()
