"""Pure parser controls and actual disposable original-child custody, never Helix proof."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from tools.ci import provider_process as process
from tools.ci import provider_network_trace as trace


class NativeCustodyTests(unittest.TestCase):
    def test_original_child_birth_image_normal_close_and_release(self):
        process.require_capability()
        with tempfile.TemporaryDirectory(dir=os.environ.get('TMPDIR')) as directory:
            state={'processes':[]};path=Path(directory)/'pending.json'
            argv=[str(Path(sys.executable).resolve()),'-c',"import sys;print('OWNED',flush=True);sys.stdin.readline()"]
            child=process.launch(argv,state=state,state_path=path,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
            self.assertEqual(child.stdout.readline().strip(),'OWNED')
            custody={}
            identity=process.record_child(state,path,child,'control',argv[0],argv,custody=custody)
            self.assertTrue(identity['start_ticks']);self.assertEqual(identity['exe'],argv[0]);self.assertNotIn('acquisition_pending',state)
            if sys.platform=='win32':self.assertTrue(child._engram_handles.get('job'))
            stdout,stderr=child.communicate('normal close\n',timeout=15)
            self.assertEqual(child.returncode,0,stderr)
            process.release(child)
            if custody.get('pidfd') is not None:os.close(custody['pidfd'])
            self.assertNotIn(child.pid,process._OWNED)

    def test_wrong_exec_retains_original_pending_until_normal_close(self):
        with tempfile.TemporaryDirectory(dir=os.environ.get('TMPDIR')) as directory:
            state={'processes':[]};path=Path(directory)/'pending.json';argv=[str(Path(sys.executable).resolve()),'-c',"import sys;print('OWNED',flush=True);sys.stdin.readline()"]
            child=process.launch(argv,state=state,state_path=path,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
            self.assertEqual(child.stdout.readline().strip(),'OWNED');custody={}
            with self.assertRaises(RuntimeError):process.record_child(state,path,child,'control',argv[0]+'wrong',argv,timeout=.02,custody=custody)
            self.assertIn('acquisition_pending',json.loads(path.read_text()));self.assertIsNone(child.returncode)
            self.assertIs(process._OWNED[child.pid],child)
            child.communicate('normal close\n',timeout=15);self.assertEqual(child.returncode,0);process.release(child)
            if custody.get('pidfd') is not None:os.close(custody['pidfd'])


class NativeTraceParserTests(unittest.TestCase):
    def test_linux_raw_payload_exclusion_child_completeness_and_loss_controls(self):
        with tempfile.TemporaryDirectory(dir=os.environ.get('TMPDIR')) as directory:
            root=Path(directory);p=root/'syscalls.100'
            p.write_text('clone(child_stack=NULL, flags=SIGCHLD) = 101\nconnect(3, {sa_family=AF_INET, sin_port=htons(9), sin_addr=inet_addr("127.0.0.1")}, 16) = -1 EACCES (Permission denied)\n+++ exited with 0 +++\n')
            with self.assertRaises(trace.TraceError):trace.read_linux(root)
            (root/'syscalls.101').write_text('+++ exited with 0 +++\n')
            observation=trace.read_linux(root);self.assertEqual(len(observation['events']),1)
            (root/'syscalls.101').write_text('connect(3, <unfinished ...>\n')
            with self.assertRaises(trace.TraceError):trace.read_linux(root)

    def test_dtrace_never_accepts_empty_or_dropped_or_missing_child_exit(self):
        with tempfile.TemporaryDirectory(dir=os.environ.get('TMPDIR')) as directory:
            path=Path(directory)/'native.trace'
            for value,stderr in [('', ''),('START\t5\nEND\n',''),('START\t5\nEXIT\t5\nEND\n','1 drop on CPU')]:
                path.write_text(value)
                with self.assertRaises(trace.TraceError):trace.read_dtrace(path,stderr)
            path.write_text('START\t5\nNET\t5\tconnect\t3\nEXIT\t5\nEND\n');self.assertEqual(len(trace.read_dtrace(path,'')['events']),1)

    def test_etw_requires_real_process_tree_start_exit_and_loss_zero(self):
        with tempfile.TemporaryDirectory(dir=os.environ.get('TMPDIR')) as directory:
            path=Path(directory)/'events.jsonl';start={'provider':'Microsoft-Windows-Kernel-Process','event_id':1,'data':{'ProcessID':'5','ParentProcessID':'4'}};end={**start,'event_id':2}
            for rows,loss in [([],0),([start],0),([start,end],1)]:
                path.write_text('\n'.join(json.dumps(row) for row in rows))
                with self.assertRaises(trace.TraceError):trace.read_etw(path,5,loss)
            path.write_text('\n'.join(map(json.dumps,[start,end])));self.assertTrue(trace.read_etw(path,5,0)['complete'])


class AcquisitionFailureControls(unittest.TestCase):
    """Pure platform injections, not Windows or privilege execution proof."""
    def directory(self):
        value=tempfile.TemporaryDirectory(dir=os.environ.get('TMPDIR'));self.addCleanup(value.cleanup);return Path(value.name)

    def test_whole_vm_scope_is_explicit_and_never_desktop_automatic(self):
        from unittest.mock import patch
        from tools.ci.provider_windows_trace import disposable_scope,EtwError
        with patch.dict(os.environ,{},clear=True):
            for value in ('owned-process-tree','github-hosted-disposable-readiness'):
                with self.assertRaises(EtwError):disposable_scope(value)
        env={'GITHUB_ACTIONS':'true','GITHUB_RUN_ID':'1','GITHUB_RUN_ATTEMPT':'1','GITHUB_JOB':'controlled','RUNNER_NAME':'hosted','RUNNER_OS':'Windows','RUNNER_TEMP':'owned','RUNNER_ENVIRONMENT':'github-hosted'}
        with patch.dict(os.environ,env,clear=True):self.assertIn('whole dedicated',disposable_scope('github-hosted-disposable-readiness')['acquisition'])

    def test_schema_rejects_binary_commandline_unknown_and_unknown_versions(self):
        from tools.ci.provider_windows_trace import checked_events,EtwError
        safe='<template><data name="ProcessID" inType="win:UInt32"/><data name="ParentProcessID" inType="win:UInt32"/></template>'
        base=[{'id':1,'template':safe},{'id':2,'template':safe}]
        self.assertEqual(checked_events('Microsoft-Windows-Kernel-Process',base),[1,2])
        for field,kind in [('Payload','win:Binary'),('CommandLine','win:UnicodeString'),('UnknownField','win:UInt32')]:
            bad=[*base,{'id':1,'template':safe.replace('</template>',f'<data name="{field}" inType="{kind}"/></template>')}]
            with self.assertRaises(EtwError):checked_events('Microsoft-Windows-Kernel-Process',bad)

    def test_capture_finish_always_closes_on_nonzero_loss_and_decode_errors(self):
        from unittest.mock import patch,Mock
        from tools.ci.provider_windows_trace import EtwError
        for stage in ('nonzero','loss','decode'):
            capture=trace.Capture(self.directory());session=Mock();capture.session=session
            if stage=='loss':session.loss.side_effect=EtwError('controlled loss query failure')
            with patch.object(trace.sys,'platform','win32'),patch.object(trace,'_run',side_effect=trace.TraceError('controlled decode failure')):
                with self.assertRaises((trace.TraceError,EtwError)):capture.finish(1 if stage=='nonzero' else 0)
            session.close.assert_called()

    def test_actual_control_flow_cleans_rule_and_capture_at_every_failure_boundary(self):
        from contextlib import ExitStack
        from unittest.mock import patch,Mock
        from tools.ci import provider_windows_trace as windows
        for boundary in ('rule-create','wrap','session-start','launch','startup','readiness','nonzero','loss','decode'):
            with self.subTest(boundary=boundary),ExitStack() as stack:
                root=self.directory();session=Mock();session.loss.return_value=0
                if boundary=='session-start':session.start.side_effect=trace.TraceError('controlled session start failure')
                if boundary=='loss':session.loss.side_effect=windows.EtwError('controlled loss query failure')
                child=Mock();child.pid=999;child.returncode=None;child._communication_started=False;child.stdout.readline.return_value='' if boundary=='startup' else 'CONTROL_READY'
                def communicate(*args,**kwargs):
                    child.returncode=1 if boundary in ('startup','nonzero') else 0
                    return ('DENIED','controlled startup failure' if boundary=='startup' else '')
                child.communicate.side_effect=communicate
                calls=[]
                def native(command):
                    calls.append(command)
                    if 'New-NetFirewallRule' in command[4] and boundary=='rule-create':raise trace.TraceError('controlled rule create failure')
                    if 'Get-WinEvent -Path' in command[4] and boundary=='decode':raise trace.TraceError('controlled decode failure')
                    return ''
                stack.enter_context(patch.object(trace.sys,'platform','win32'))
                stack.enter_context(patch.object(windows,'disposable_scope',return_value={'scope':'pure mock only'}))
                stack.enter_context(patch.object(trace,'owned_windows_image',side_effect=lambda directory:(str(Path(directory)/'unique.exe'),os.environ.copy())))
                stack.enter_context(patch.object(trace,'_run',side_effect=native))
                stack.enter_context(patch.object(trace,'readiness',return_value={}))
                stack.enter_context(patch.object(windows,'Session',return_value=session))
                stack.enter_context(patch.object(process,'launch',side_effect=trace.TraceError('controlled launch failure') if boundary=='launch' else None,return_value=child))
                stack.enter_context(patch.object(process,'record_child',side_effect=trace.TraceError('controlled readiness failure') if boundary=='readiness' else None))
                stack.enter_context(patch.object(process,'release'))
                if boundary=='wrap':stack.enter_context(patch.object(trace.Capture,'wrap',side_effect=trace.TraceError('controlled wrap failure')))
                with self.assertRaises((trace.TraceError,windows.EtwError)):
                    trace.positive_controls(root/'controls','github-hosted-disposable-readiness')
                remove=[call for call in calls if 'Get-NetFirewallApplicationFilter' in call[4]]
                self.assertEqual(len(remove),1)
                self.assertEqual(remove[0][-1],str(root/'controls'/'connect'/'unique.exe'))
                journal=json.loads((root/'controls'/'connect'/'control-custody.json').read_text())
                self.assertTrue(journal['cleanup_confirmed'])
                if boundary not in ('rule-create','wrap'):session.close.assert_called()

    def test_etw_session_start_and_enable_failures_stop_only_exact_uuid_session(self):
        from unittest.mock import patch,Mock
        from tools.ci import provider_windows_trace as windows
        for boundary in ('start','enable'):
            api=Mock();api.StartTraceW.return_value=5 if boundary=='start' else 0;api.EnableTraceEx2.return_value=5 if boundary=='enable' else 0;api.ControlTraceW.return_value=0
            directory=self.directory();schemas=[{'name':'controlled','guid':'11111111-2222-4333-8444-555555555555','event_ids':[1,2]}]
            with patch.object(windows,'disposable_scope',return_value={'scope':'pure mock'}),patch.object(windows,'manifests',return_value=schemas),patch.object(windows,'_api',return_value=api):
                session=windows.Session(directory,lambda command:'','pure-mock')
                with self.assertRaises(windows.EtwError):session.start()
            self.assertTrue(session.closed);self.assertTrue(session.record['cleanup_confirmed'])
            self.assertEqual(api.ControlTraceW.call_args.args[1],session.name)
            self.assertEqual(api.ControlTraceW.call_args.args[3],1)

    def test_uncertain_original_child_stops_session_but_retains_rule_and_recovery(self):
        from unittest.mock import Mock,patch
        owner=trace.ControlCustody(self.directory(),'controlled-pure-fixture');owner.rule='Engram-controlled';owner.image='unique.exe';child=Mock();child.returncode=None;child._communication_started=False;child.communicate.side_effect=subprocess.TimeoutExpired('controlled',5);child.pid=999;owner.child=child;owner.capture=Mock()
        with patch.object(trace,'_run') as native:
            with self.assertRaises(trace.TraceError):owner.cleanup()
            native.assert_not_called()
        owner.capture.close.assert_called_once();self.assertEqual(owner.rule,'Engram-controlled');self.assertIn(owner,trace._RECOVERIES);self.assertFalse(json.loads(owner.path.read_text())['cleanup_confirmed'])
        trace._RECOVERIES.remove(owner) # Pure mock has no real resource.

    def test_private_image_is_unique_hash_verified_and_original_install_unchanged(self):
        from unittest.mock import patch
        root=self.directory();install=root/'original-install';install.mkdir();source=install/'python.exe';source.write_bytes(b'controlled image bytes, not an executed runtime');(install/'python312.dll').write_bytes(b'controlled DLL bytes')
        owned=root/'owned';owned.mkdir()
        with patch.object(trace.sys,'executable',str(source)),patch.object(trace.sys,'base_prefix',str(install)):
            image,env=trace.owned_windows_image(owned)
        self.assertNotEqual(Path(image),source);self.assertEqual(Path(image).read_bytes(),source.read_bytes());self.assertEqual(env['PYTHONHOME'],str(install));self.assertEqual(source.read_bytes(),b'controlled image bytes, not an executed runtime')
        other=root/'corrupt';other.mkdir()
        with patch.object(trace.sys,'executable',str(source)),patch.object(trace.shutil,'copyfile',side_effect=lambda a,b:Path(b).write_bytes(b'wrong')):
            with self.assertRaises(trace.TraceError):trace.owned_windows_image(other)

    def test_unknown_schema_prevents_native_session_acquisition_and_cleanup_error_is_retained(self):
        from unittest.mock import patch,Mock
        from tools.ci import provider_windows_trace as windows
        directory=self.directory();api=Mock()
        with patch.object(windows,'disposable_scope',return_value={'scope':'pure mock'}),patch.object(windows,'manifests',side_effect=windows.EtwError('unknown schema')),patch.object(windows,'_api',return_value=api):
            session=windows.Session(directory,lambda command:'','pure-mock')
            with self.assertRaises(windows.EtwError):session.start()
            session.close();api.StartTraceW.assert_not_called()
        api.ControlTraceW.return_value=5
        with patch.object(windows,'disposable_scope',return_value={'scope':'pure mock'}),patch.object(windows,'_api',return_value=api):
            session=windows.Session(directory,lambda command:'','pure-mock');session.started=True
            with self.assertRaises(windows.EtwError):session.close()
        self.assertFalse(session.closed);self.assertFalse(session.record['cleanup_confirmed']);self.assertEqual(session.record['phase'],'exact-session-cleanup-pending')


class CompactHandoffTests(unittest.TestCase):
    """Real owned filesystem handoff; elevated UID simulation is explicitly mocked."""
    def fixture(self):
        import uuid
        from unittest.mock import patch
        from tools.ci import check_storage_network_policy as policy
        temp=tempfile.TemporaryDirectory(dir=os.environ.get('TMPDIR'));self.addCleanup(temp.cleanup);base=Path(temp.name)
        repo=base/'repo';parent=repo/'runs'/'native';invocation=str(uuid.uuid4());source=parent/invocation;source.mkdir(parents=True,mode=0o700)
        reports={'report.json':json.dumps({'invocation':invocation,'source_revision':'fixture-source','records':[{'profile':None,'source_revision':'fixture-source','status':'PASS'}]}).encode(),'operations.jsonl':b'{"controlled":"native-only"}\n','raw.log':b'owned control diagnostics, no provider payload\n'}
        for name,data in reports.items():(source/name).write_bytes(data)
        raw=source/'private-capture';raw.mkdir(mode=0o700);(raw/'native.trace').write_bytes(b'private raw must not be exported')
        temporary=base/'temp';temporary.mkdir();destination=temporary/'compact';destination.mkdir(mode=0o700)
        self.enterContext(patch.object(policy,'ROOT',repo));self.enterContext(patch.dict(os.environ,{'TMPDIR':str(temporary)}))
        return policy,parent,invocation,source,destination,reports

    def test_exact_three_readable_files_with_unchanged_private_source(self):
        import hashlib,stat
        from unittest.mock import patch
        policy,parent,invocation,source,destination,reports=self.fixture()
        before={str(p.relative_to(source)):(p.stat().st_uid,stat.S_IMODE(p.stat().st_mode),p.read_bytes() if p.is_file() else None) for p in [source,*source.rglob('*')]}
        with patch.object(policy.os,'chown') as broad_chown,patch.object(policy.os,'chmod') as broad_chmod:
            receipt=policy.export_compact(parent,invocation,destination,'fixture-source')
            broad_chown.assert_not_called();broad_chmod.assert_not_called()
        self.assertEqual(set(p.name for p in destination.iterdir()),set(reports));self.assertEqual(receipt['owner_uid'],os.getuid())
        for name,data in reports.items():
            path=destination/name;self.assertEqual(path.read_bytes(),data);self.assertEqual(path.stat().st_uid,os.getuid());self.assertEqual(stat.S_IMODE(path.stat().st_mode),0o600);self.assertEqual(receipt['files'][name],hashlib.sha256(data).hexdigest())
        after={str(p.relative_to(source)):(p.stat().st_uid,stat.S_IMODE(p.stat().st_mode),p.read_bytes() if p.is_file() else None) for p in [source,*source.rglob('*')]}
        self.assertEqual(before,after)

    def test_root_to_caller_identity_flow_is_mocked_not_real_mac_sudo(self):
        from unittest.mock import patch
        from types import SimpleNamespace
        policy,parent,invocation,source,destination,reports=self.fixture();actual_uid=os.getuid();actual_gid=os.getgid();original=policy.os.fstat
        source_inodes={(p.stat().st_dev,p.stat().st_ino) for p in [source,*[source/name for name in reports]]}
        def source_root_owner(fd):
            value=original(fd)
            if (value.st_dev,value.st_ino) in source_inodes:
                fields={key:getattr(value,key) for key in dir(value) if key.startswith('st_')};fields['st_uid']=0;return SimpleNamespace(**fields)
            return value
        with patch.object(policy.os,'geteuid',return_value=0),patch.object(policy.os,'fstat',side_effect=source_root_owner),patch.dict(os.environ,{'SUDO_UID':str(actual_uid),'SUDO_GID':str(actual_gid)}),patch.object(policy.os,'fchown',wraps=os.fchown) as handoff:
            policy.export_compact(parent,invocation,destination,'fixture-source')
        self.assertEqual(handoff.call_count,3)
        for call in handoff.call_args_list:self.assertEqual(call.args[1:],(actual_uid,actual_gid))

    def test_stale_profile_foreign_entries_and_symlink_paths_never_export(self):
        policy,parent,invocation,source,destination,reports=self.fixture()
        with self.assertRaises(policy.ObservationError):policy.export_compact(parent,invocation,destination,'different-source')
        self.assertFalse(list(destination.iterdir()))
        unknown=destination/'unknown';unknown.write_bytes(b'keep caller data')
        with self.assertRaises(policy.ObservationError):policy.export_compact(parent,invocation,destination,'fixture-source')
        self.assertEqual(unknown.read_bytes(),b'keep caller data');unknown.unlink()
        (source/'raw.log').unlink();(source/'raw.log').symlink_to(source/'private-capture'/'native.trace')
        with self.assertRaises((policy.ObservationError,OSError)):policy.export_compact(parent,invocation,destination,'fixture-source')
        self.assertFalse(list(destination.iterdir()))
        (source/'raw.log').unlink();(source/'raw.log').write_bytes(reports['raw.log'])
        value=json.loads((source/'report.json').read_text());value['records'][0]['profile']='embedded-offline';(source/'report.json').write_text(json.dumps(value))
        with self.assertRaises(policy.ObservationError):policy.export_compact(parent,invocation,destination,'fixture-source')
        self.assertFalse(list(destination.iterdir()))


class NativeDeliveryRegressionTests(unittest.TestCase):
    """Actual POSIX launcher/file checks; no claim that DTrace/ETW ran here."""
    def root(self):
        value=tempfile.TemporaryDirectory(dir=os.environ.get('TMPDIR'));self.addCleanup(value.cleanup);return Path(value.name)

    def test_mac_dtrace_gets_single_owned_launcher_and_shell_preserves_argv(self):
        from unittest.mock import patch
        root=self.root();script=root/'arguments.py';script.write_text('import json,sys;print(json.dumps(sys.argv[1:]))\n');sentinel=root/'must-not-exist'
        wanted=['space value',"single'quote",'double"quote','$(touch '+str(sentinel)+')','semi;colon']
        with patch.object(trace.sys,'platform','darwin'),patch.object(trace,'readiness',return_value={}):
            capture=trace.Capture(root/'capture');argv=capture.wrap([str(Path(sys.executable).resolve()),str(script),*wanted])
        launcher=Path(argv[-1]);self.assertEqual(argv[-2],'-c');self.assertEqual(launcher.name,'launch-owned');self.assertEqual(launcher.stat().st_mode&0o777,0o700)
        result=subprocess.run([str(launcher)],capture_output=True,text=True,check=False)
        self.assertEqual(result.returncode,0,result.stderr);self.assertEqual(json.loads(result.stdout),wanted);self.assertFalse(sentinel.exists())

    def test_unknown_windows_schema_reports_names_types_not_values_and_still_rejects(self):
        from tools.ci.provider_windows_trace import checked_events,EtwError
        safe='<template><data name="ProcessID" inType="win:UInt32"/><data name="ParentProcessID" inType="win:UInt32"/></template>'
        forbidden='<template><data name="CommandLine" inType="win:UnicodeString" value="PRIVATE_EVENT_VALUE_MUST_NOT_APPEAR"/></template>'
        with self.assertRaises(EtwError) as raised:checked_events('Microsoft-Windows-Kernel-Process',[{'id':1,'version':4,'template':forbidden},{'id':2,'version':0,'template':safe}])
        message=str(raised.exception);self.assertIn('commandline',message);self.assertIn('win:UnicodeString',message);self.assertIn('"version": 4',message);self.assertNotIn('PRIVATE_EVENT_VALUE_MUST_NOT_APPEAR',message)
