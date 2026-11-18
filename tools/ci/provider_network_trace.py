"""Native metadata observation; acquisition scope is explicit and audited.

Capture sessions belong to one invocation. Tool availability is a prerequisite,
not an execution result. Each platform must pass its actual positive controls.
"""
from __future__ import annotations
import hashlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import ipaddress
import json
import os
from pathlib import Path
import re
import shlex
import shutil
import subprocess
import sys
import threading
import uuid


TARGETS=('linux-x64-gnu','linux-arm64-gnu','darwin-arm64','win32-x64')


def native_target():
    import platform
    return {('Linux','x86_64'):'linux-x64-gnu',('Linux','aarch64'):'linux-arm64-gnu',('Darwin','arm64'):'darwin-arm64',('Windows','AMD64'):'win32-x64'}.get((platform.system(),platform.machine()))


class TraceError(RuntimeError):
    """A missing observation, privilege or association blocks acceptance."""


def readiness():
    from tools.ci import provider_process
    provider_process.require_capability()
    tools={"linux":["strace"],"darwin":["dtrace","sandbox-exec"],"win32":["powershell.exe"]}.get(sys.platform)
    if tools is None:raise TraceError("unsupported native trace target")
    missing=[tool for tool in tools if not shutil.which(tool)]
    if missing:raise TraceError("missing native trace tools: "+", ".join(missing))
    return {"platform":sys.platform,"tools":{tool:shutil.which(tool) for tool in tools},"privilege":"NOT_PROVEN until real capture and denied connect/DNS controls"}


def _run(command):
    result=subprocess.run(command,capture_output=True,text=True,check=False)
    if result.returncode:raise TraceError("native trace control failed: "+Path(command[0]).name+"; "+result.stderr[:4000])
    return result.stdout


def read_linux(directory):
    """-ff captures every followed child/thread; match split calls per native TID."""
    paths=sorted(Path(directory).glob('syscalls.*'))
    if not paths:raise TraceError('native syscall capture missing')
    observed={int(p.name.rsplit('.',1)[1]) for p in paths};created=set();events=[]
    for path in paths:
        pid=int(path.name.rsplit('.',1)[1]);pending=None;completed=False
        for line in path.read_text().splitlines():
            line=re.sub(r'^\d+\.\d+\s+','',line)
            if completed:raise TraceError('event follows process exit')
            if line=='+++ exited with 0 +++':
                if pending:raise TraceError('unfinished native syscall at exit')
                completed=True;continue
            if line.startswith('--- SIG'):continue
            if '<unfinished ...>' in line:
                if pending:raise TraceError('ambiguous split native syscall')
                pending=line.split('<unfinished ...>')[0];continue
            if line.startswith('<... '):
                match=re.match(r'<\.\.\. (\w+) resumed>(.*)',line)
                if not pending or not match or not pending.startswith(match[1]+'('):raise TraceError('unmatched native syscall completion')
                line=pending+match[2];pending=None
            match=re.fullmatch(r'(\w+)\((.*)\)\s+=\s+(.*)',line)
            if not match:raise TraceError('unknown native syscall trace line')
            call,args,result=match.groups()
            if call in {'clone','clone3','fork','vfork'}:
                child=re.match(r'(\d+)',result)
                if child:created.add(int(child[1]))
            if call in {'connect','sendto','sendmsg','sendmmsg','send'}:
                # Send/recv calls are raw pointers only: no body/header buffers.
                # All attempted sends are retained even when EACCES/ECONNREFUSED.
                if 'AF_UNIX' not in args:events.append({'pid':pid,'call':call,'arguments':args,'result':result})
        if not completed:raise TraceError('native child/thread exit observation missing')
    if not created.issubset(observed):raise TraceError('fork/clone child missing from followed capture')
    return {'events':events,'processes':sorted(observed),'complete':True,'loss':0,'native':'strace-followed-syscalls'}


# No copied buffers: only process identity, socket FD/address and resolver name.
DTRACE = r'''
#pragma D option quiet
#pragma D option bufsize=16m
BEGIN { tracked[$target]=1; printf("START\t%d\n", $target); }
proc:::create /tracked[pid]/ { tracked[args[0]->pr_pid]=1; printf("CHILD\t%d\t%d\n",pid,args[0]->pr_pid); }
proc:::exit /tracked[pid]/ { printf("EXIT\t%d\n",pid); }
pid$target::getaddrinfo:entry { printf("DNS\t%d\t%s\n",pid,copyinstr(arg0)); }
END { printf("END\n"); }
'''
for _call,_address in [('connect','arg1'),('sendto','arg4')]:
    DTRACE += (f'syscall::{_call}*:entry /tracked[pid] && {_address} && *(uint8_t*)copyin({_address}+1,1)==2/ '
        + '{ printf("IP4\\t%d\\t'+_call+'\\t%d\\t%x\\t%x\\n",pid,arg0,*(uint16_t*)copyin('+_address+'+2,2),*(uint32_t*)copyin('+_address+'+4,4)); }\n')
    DTRACE += (f'syscall::{_call}*:entry /tracked[pid] && {_address} && *(uint8_t*)copyin({_address}+1,1)==30/ '
        + '{ printf("IP6\\t%d\\t'+_call+'\\t%d\\t%x\\t%llx\\t%llx\\n",pid,arg0,*(uint16_t*)copyin('+_address+'+2,2),*(uint64_t*)copyin('+_address+'+8,8),*(uint64_t*)copyin('+_address+'+16,8)); }\n')
    DTRACE += f'syscall::{_call}*:entry /tracked[pid] && !{_address}/ '+ '{ printf("NET\\t%d\\t'+_call+'\\t%d\\n",pid,arg0); }\n'
DTRACE += 'syscall::sendmsg*:entry /tracked[pid]/ { printf("NET\\t%d\\tsendmsg\\t%d\\n",pid,arg0); }\n'


def read_dtrace(path, stderr):
    if re.search(r'(?i)drop|error|failed|not permitted|invalid probe',stderr):raise TraceError('DTrace loss/privilege/probe failure')
    roots=[];created=set();exited=set();events=[];ended=False
    for line in Path(path).read_text().splitlines():
        fields=line.split('\t');kind=fields[0]
        if kind=='START' and len(fields)==2:roots.append(int(fields[1]))
        elif kind=='CHILD' and len(fields)==3:created.add(int(fields[2]))
        elif kind=='EXIT' and len(fields)==2:exited.add(int(fields[1]))
        elif kind in {'IP4','IP6'}:
            if (kind=='IP4' and len(fields)!=6) or (kind=='IP6' and len(fields)!=7):raise TraceError('native sockaddr metadata incomplete')
            port=int.from_bytes(int(fields[4],16).to_bytes(2,sys.byteorder),'big')
            address=b''.join(int(v,16).to_bytes(4 if kind=='IP4' else 8,sys.byteorder) for v in fields[5:])
            ip=str(ipaddress.ip_address(address));family='AF_INET' if kind=='IP4' else 'AF_INET6'
            sockaddr=f'{fields[3]}, {{sa_family={family}, sin_port=htons({port}), sin_addr=inet_addr("{ip}")}}'
            events.append({'pid':int(fields[1]),'call':fields[2],'arguments':sockaddr,'result':'native-entry'})
        elif kind in {'NET','DNS'} and len(fields)>=3:events.append({'pid':int(fields[1]),'call':fields[2] if kind=='NET' else 'dns','arguments':'\t'.join(fields[3:]),'result':'native-entry'})
        elif kind=='END':ended=True
        else:raise TraceError('unrecognized DTrace native record')
    if len(roots)!=1 or not ended or set(roots)|created != exited:raise TraceError('DTrace child-tree completion missing')
    return {'events':events,'processes':sorted(exited),'complete':True,'loss':0,'native':'dtrace-syscall-process-dns'}



def read_etw(path,root_pid,loss):
    if loss:raise TraceError('ETW capture dropped native events')
    records=[json.loads(line) for line in Path(path).read_text(encoding='utf-8-sig').splitlines() if line]
    owned={root_pid};started=set();exited=set();events=[]
    for row in records:
        data={k.lower():v for k,v in row.get('data',{}).items()}
        pid=int(data.get('processid',data.get('pid',row.get('pid',0))))
        provider=row.get('provider','');event=int(row.get('event_id',-1))
        if provider=='Microsoft-Windows-Kernel-Process':
            if event==1 and (pid==root_pid or int(data.get('parentprocessid',0)) in owned):owned.add(pid);started.add(pid)
            elif event==2 and pid in owned:exited.add(pid)
        elif pid in owned and provider in {'Microsoft-Windows-TCPIP','Microsoft-Windows-Winsock-AFD','Microsoft-Windows-DNS-Client','Microsoft-Windows-WFP'}:
            # Unrecognized fields are not interpreted as an absent attempt.
            events.append({'pid':pid,'call':'dns' if provider.endswith('DNS-Client') else 'native-network','arguments':data,'result':{'event_id':event,'provider':provider}})
    if not records or started!=owned or exited!=owned:raise TraceError('ETW original/descendant start-exit association missing')
    return {'events':events,'processes':sorted(owned),'complete':True,'loss':loss,'native':'ETW-kernel-process-network-DNS-WFP'}


class Capture:
    """Owned-tree Linux/Mac capture; explicitly scoped VM metadata on Windows.

    Windows whole-VM acquisition is readiness-only and is never represented as
    kernel-filtered exact-child collection or permission for a desktop/profile.
    """
    def __init__(self,directory,acquisition_scope='owned-process-tree'):
        self.directory=Path(directory);self.directory.mkdir(parents=True,exist_ok=True);self.session=None;self.root_pid=None;self.acquisition_scope=acquisition_scope
    def wrap(self,argv):
        readiness()
        if sys.platform=='linux':
            return ['strace','-ff','-ttt','-s','0','-yy','-e','trace=network,process','-e','raw=sendto,sendmsg,sendmmsg,recvfrom,recvmsg,recvmmsg,send,recv,execve,execveat','-o',str(self.directory/'syscalls'), '--',*argv]
        if sys.platform=='darwin':
            script=self.directory/'native.d';script.write_text(DTRACE)
            # DTrace's -c command parser is not our POSIX argv serializer.
            # Give it one whitespace-free owned launcher path; the private
            # executable script uses exec+POSIX quoting for the original argv.
            launcher=(self.directory/'launch-owned').resolve()
            if any(c.isspace() for c in str(launcher)):
                raise TraceError('owned Mac launcher path contains unsupported DTrace command whitespace')
            with launcher.open('x') as stream:stream.write('#!/bin/sh\nexec '+shlex.join(argv)+'\n')
            launcher.chmod(0o700)
            return ['dtrace','-q','-s',str(script),'-o',str(self.directory/'native.trace'),'-c',str(launcher)]
        from tools.ci.provider_windows_trace import Session,EtwError
        self.session=Session(self.directory,_run,self.acquisition_scope)
        try:self.session.start()
        except EtwError as error:
            self.close();raise TraceError(str(error)) from error
        except BaseException:
            self.close();raise
        return list(argv)
    def close(self):
        if self.session is not None:self.session.close()
    def finish(self,code,stderr=''):
        try:
            if code:raise TraceError('actual traced consumer failed')
            if sys.platform=='linux':return read_linux(self.directory)
            if sys.platform=='darwin':return read_dtrace(self.directory/'native.trace',stderr)
            if self.session is None:raise TraceError('actual filtered native session missing')
            loss=self.session.loss()
            self.session.close()
            path=self.directory/'native.etl';out=self.directory/'native-events.jsonl'
            script="& { param($src,$dst) Get-WinEvent -Path $src -Oldest | ForEach-Object { $x=[xml]$_.ToXml();$d=@{};foreach($v in $x.Event.EventData.Data){if($v.Name -in @('ProcessID','PID','ParentProcessID','ImageName','daddr','dport','DestAddr','DestinationAddress','RemoteAddress','RemoteAddr','DestinationPort','RemotePort','Status','QueryName')){$d[$v.Name]=[string]$v.'#text'}};@{provider=[string]$x.Event.System.Provider.Name;event_id=[int]$x.Event.System.EventID;pid=[int]$x.Event.System.Execution.ProcessID;data=$d}|ConvertTo-Json -Compress -Depth 8 } | Set-Content -Encoding utf8 $dst }"
            _run(['powershell.exe','-NoProfile','-NonInteractive','-Command',script,str(path),str(out)])
            result=read_etw(out,self.root_pid,loss)
            result['acquisition_scope']=self.session.scope
            return result
        finally:self.close()


class EmbeddingFixture:
    """Owned loopback constructor fixture; answers valid4D, never downloads a model."""
    def __init__(self,available=True):
        self.requests=0;self.available=available;self.credential_env='ENGRAM_MODEL_FIXTURE_'+uuid.uuid4().hex;self.previous=None
    def start(self):
        owner=self
        class Handler(BaseHTTPRequestHandler):
            def do_POST(self):
                size=int(self.headers.get('content-length','0'))
                if size>1024*1024:self.send_error(413);return
                self.rfile.read(size);owner.requests+=1
                body=json.dumps({'data':[{'index':0,'embedding':[1.,0.,0.,0.]}]}).encode()
                self.send_response(200);self.send_header('Content-Type','application/json');self.send_header('Content-Length',str(len(body)));self.end_headers();self.wfile.write(body)
            def log_message(self,*args):pass
        self.server=ThreadingHTTPServer(('127.0.0.1',0),Handler);self.server.daemon_threads=False
        self.endpoint='http://127.0.0.1:'+str(self.server.server_port)
        self.previous=os.environ.get(self.credential_env)
        if self.available:os.environ[self.credential_env]='owned-model-fixture-not-a-user-key'
        else:os.environ.pop(self.credential_env,None)
        self.thread=threading.Thread(target=self.server.serve_forever,name='engram-owned-embedding-fixture');self.thread.start()
    def close(self):
        self.server.shutdown();self.server.server_close();self.thread.join(5)
        if self.thread.is_alive():raise TraceError('owned fixture thread closure unconfirmed')
        if self.previous is None:os.environ.pop(self.credential_env,None)
        else:os.environ[self.credential_env]=self.previous


_RECOVERIES=[]


def owned_windows_image(directory):
    """Unique private image: a firewall rule cannot affect shared Python users."""
    directory=Path(directory)/'owned-interpreter';directory.mkdir(mode=0o700)
    source=Path(sys.executable).resolve();destination=directory/'python-control.exe'
    files=[(source,destination)]
    for pattern in ('python*.dll','vcruntime*.dll'):
        files.extend((path,directory/path.name) for path in source.parent.glob(pattern))
    identity=[]
    for original,copy in files:
        before=hashlib.sha256(original.read_bytes()).hexdigest();shutil.copyfile(original,copy)
        if hashlib.sha256(copy.read_bytes()).hexdigest()!=before or hashlib.sha256(original.read_bytes()).hexdigest()!=before:
            raise TraceError('private control interpreter copy/source changed')
        identity.append({'source':str(original),'path':str(copy),'sha256':before})
    (directory/'image-identity.json').write_text(json.dumps(identity,indent=2)+'\n')
    environment=os.environ.copy();environment['PYTHONHOME']=sys.base_prefix
    return str(destination.resolve()),environment


class ControlCustody:
    """Side effects are journaled before native acquisition, not after success."""
    def __init__(self,directory,scope):
        self.directory=Path(directory);self.capture=Capture(directory,scope);self.rule=None;self.image=None;self.child=None;self.state={'processes':[]};self.custody={};self.argv=None
        self.path=self.directory/'control-custody.json';self.pending=self.directory/'pending-process.json';self.record={'rule':None,'capture_scope':scope,'cleanup_confirmed':False};self.journal('prepared')
    def journal(self,phase,**values):
        self.record.update(phase=phase,**values);self.path.write_text(json.dumps(self.record,indent=2)+'\n')
    def acquire_rule(self,kind,image):
        self.rule='Engram-'+str(uuid.uuid4());self.image=image
        self.journal('rule-create-pending',rule={'name':self.rule,'image':image,'protocol':'TCP' if kind=='connect' else 'UDP','port':9 if kind=='connect' else 53})
        script="& { param($name,$image,$protocol,$port) if(Get-NetFirewallRule -Name $name -ErrorAction SilentlyContinue){throw 'owned rule name already exists'};New-NetFirewallRule -Name $name -DisplayName $name -Direction Outbound -Action Block -Program $image -Protocol $protocol -RemoteAddress 127.0.0.1 -RemotePort $port -ErrorAction Stop | Out-Null }"
        _run(['powershell.exe','-NoProfile','-NonInteractive','-Command',script,self.rule,image,'TCP' if kind=='connect' else 'UDP','9' if kind=='connect' else '53'])
        self.journal('rule-created')
    def remove_rule(self):
        if not self.rule:return
        # Query the exact rule AND private program before removal. A failed
        # create may leave nothing; never delete an unrelated/colliding rule.
        script="& { param($name,$image) $r=Get-NetFirewallRule -Name $name -ErrorAction SilentlyContinue;if($r){$p=@($r|Get-NetFirewallApplicationFilter);if($p.Count -ne 1 -or $p[0].Program -ne $image){throw 'foreign rule image; refusing cleanup'};$r|Remove-NetFirewallRule -ErrorAction Stop};if(Get-NetFirewallRule -Name $name -ErrorAction SilentlyContinue){throw 'exact rule cleanup unconfirmed'} }"
        _run(['powershell.exe','-NoProfile','-NonInteractive','-Command',script,self.rule,self.image]);self.rule=None
    def cleanup(self):
        from tools.ci import provider_process
        failures=[]
        # Stop passive capture even if original child closure is uncertain. It
        # never signals a child; it prevents continued whole-VM acquisition.
        try:self.capture.close()
        except BaseException as error:failures.append('exact session close: '+str(error))
        child=self.child
        if child is None:
            creation=self.state.get('creation_pending',{})
            candidate=provider_process._OWNED.get(creation.get('pid'))
            if candidate is not None and candidate.args==self.argv:child=candidate;self.child=child
        closed=child is None
        if child is not None:
            try:
                # On early readiness/acquisition failure, request normal exit
                # through the original pipe. The controlled child sends nothing
                # unless it receives the exact go token.
                if child.returncode is None:
                    if getattr(child,'_communication_started',False):child.communicate(timeout=5)
                    else:child.communicate('stop\n',timeout=5)
                provider_process.release(child);closed=True
                if self.custody.get('pidfd') is not None:os.close(self.custody.pop('pidfd'))
                self.pending.unlink(missing_ok=True)
            except (OSError,ValueError,RuntimeError,subprocess.TimeoutExpired) as error:
                failures.append('original child/kernel closure pending: '+type(error).__name__)
        if closed:
            try:self.remove_rule()
            except BaseException as error:failures.append('exact private-image rule cleanup: '+str(error))
        else:
            failures.append('private-image rule retained until authoritative original-child closure')
            if self not in _RECOVERIES:_RECOVERIES.append(self)
        self.journal('cleanup-pending' if failures else 'closed',cleanup_confirmed=not failures,recovery=failures,child_pid=child.pid if child else None)
        if failures:raise TraceError('native control cleanup unconfirmed; retain '+str(self.path))


def finish_pending_controls():
    """Only retry dependent cleanup after retained original children actually exit."""
    for owner in list(_RECOVERIES):
        owner.cleanup();_RECOVERIES.remove(owner)


def positive_controls(directory,acquisition_scope='owned-process-tree'):
    """Denied TCP and direct UDP-DNS controls; not whole OS-resolver proof."""
    root=Path(directory);root.mkdir(parents=True,exist_ok=False);results=[]
    # Validate broad-scope authorization BEFORE a private rule/session side effect.
    if sys.platform=='win32':
        from tools.ci.provider_windows_trace import disposable_scope
        disposable_scope(acquisition_scope)
    for kind in ('connect','dns'):
        owned=root/kind;owner=ControlCustody(owned,acquisition_scope)
        try:
            code="import socket,errno; s=socket.socket(socket.AF_INET,"+('socket.SOCK_STREAM' if kind=='connect' else 'socket.SOCK_DGRAM')+");\ntry:\n "+("s.connect(('127.0.0.1',9))" if kind=='connect' else "s.sendto(bytes.fromhex('000101000001000000000000017807696e76616c69640000010001'),('127.0.0.1',53))")+"\nexcept OSError as e:\n assert e.errno in (1,13,10013),e.errno; print('DENIED',flush=True)\nelse:\n raise AssertionError('positive control was not denied')\nfinally:\n s.close()"
            code="import sys;print('CONTROL_READY',flush=True);\nif sys.stdin.readline().strip()!='go':sys.exit(0)\n"+code
            interpreter=sys.executable;environment=os.environ.copy()
            if sys.platform=='win32':
                interpreter,environment=owned_windows_image(owned);owner.acquire_rule(kind,interpreter)
            control=owned/'positive-control.py'
            with control.open('x') as stream:stream.write(code+'\n')
            control.chmod(0o600)
            command=[interpreter,str(control.resolve())]
            if sys.platform=='darwin':
                sandbox=owned/'deny-network.sb'
                with sandbox.open('x') as stream:stream.write('(version 1)\n(allow default)\n(deny network-outbound)\n')
                sandbox.chmod(0o600)
                command=['sandbox-exec','-f',str(sandbox.resolve()),*command]
            argv=owner.capture.wrap(command)
            if sys.platform=='linux':argv[1:1]=['-e','inject=connect:error=EACCES','-e','inject=sendto:error=EACCES']
            argv=[str(Path(shutil.which(argv[0]) or argv[0]).resolve()),*argv[1:]];owner.argv=argv
            from tools.ci import provider_process
            owner.child=provider_process.launch(argv,state=owner.state,state_path=owner.pending,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,env=environment)
            child=owner.child;owner.capture.root_pid=child.pid;owner.journal('child-started',child_pid=child.pid)
            if child.stdout.readline().strip()!='CONTROL_READY':
                stdout,stderr=child.communicate(timeout=5);(owned/'control.stderr').write_text(stderr)
                raise TraceError('controlled child startup failed; '+stderr[:4000])
            provider_process.record_child(owner.state,owner.pending,child,'native-positive',argv[0],argv,custody=owner.custody)
            stdout,stderr=child.communicate('go\n',timeout=30)
            (owned/'control.stdout').write_text(stdout);(owned/'control.stderr').write_text(stderr)
            observation=owner.capture.finish(child.returncode,stderr)
            if stdout.strip()!='DENIED' or not observation['events']:raise TraceError('denied positive attempt not actually observed')
            results.append({'control':kind,'status':'PASS','events':len(observation['events']),'complete':observation['complete'],'loss':observation['loss'],'scope':observation.get('acquisition_scope','original child tree'),'DNS_limit':'direct UDP datagram only; OS resolver/daemon mediation NOT_PROVEN' if kind=='dns' else None})
        finally:owner.cleanup()
    return results


def permitted(observation,allowed):
    if not observation.get('complete') or observation.get('loss')!=0:raise TraceError('native trace is not complete/loss-free')
    connections={}
    for event in observation['events']:
        args=event['arguments'];call=event['call']
        if not allowed:raise TraceError('outbound attempt in embedded offline profile')
        if call=='dns':raise TraceError('unexpected resolver activity in explicit numeric loopback storage profile')
        if isinstance(args,dict):
            destination=next((args[k] for k in ('daddr','destaddr','destinationaddress','remoteaddress','remoteaddr') if k in args),None)
            port=next((args[k] for k in ('dport','destinationport','remoteport') if k in args),None)
            if destination is None or port is None:raise TraceError('native service event lacks authoritative destination association')
            if (str(ipaddress.ip_address(destination)),int(port)) not in allowed:raise TraceError('unconfigured native destination attempted')
            continue
        fd=re.match(r'\s*(0x[0-9a-f]+|\d+)',args)
        if not fd:raise TraceError('network attempt lacks socket association')
        fd=int(fd[1],0)
        ipv4=re.search(r'inet_addr\("([^\"]+)"\)',args)
        ipv6=re.search(r'inet_pton\(AF_INET6, "([^\"]+)"',args)
        port=re.search(r'(?:sin6?_port)=htons\((\d+)\)',args)
        if port and (ipv4 or ipv6):
            endpoint=(str(ipaddress.ip_address((ipv4 or ipv6)[1])),int(port[1]))
            if endpoint not in allowed:raise TraceError('unconfigured storage/auth destination attempted')
            connections.setdefault(fd,set()).add(endpoint)
        elif call in {'send','sendto','sendmsg','sendmmsg'}:
            if len(connections.get(fd,set()))!=1:raise TraceError('send destination cannot be uniquely associated with configured connection')
        else:raise TraceError('unknown native attempt destination; no service exception inferred')


class ConsumerObserver:
    """Internal integration hook around actual pinned consumers, not a wire flag."""
    def __init__(self,profile,model_fixture):
        self.profile=profile;self.model=model_fixture;self.captures={};self.allowed=set();self.observations=[]
    def configure(self,packet):
        from urllib.parse import urlsplit
        packet['config']['enable_vector']=False
        packet['config']['embedding_provider'].update(provider_type='openai-compatible',model='disabled-ambient-model',dimensions=4,endpoint=self.model.endpoint,api_key_env=self.model.credential_env)
        if self.profile=='service-disabled-vectors':
            url=urlsplit(packet['config']['helixdb']['endpoint'])
            # The owned fixture is explicit numeric loopback. No ambient DNS,
            # proxy, arbitrary redirect/auth destination is admitted by guessing.
            host=str(ipaddress.ip_address(url.hostname))
            if not ipaddress.ip_address(host).is_loopback:raise TraceError('network policy fixture must use owned explicit loopback origin')
            self.allowed.add((host,url.port or (443 if url.scheme=='https' else 80)))
    def wrap(self,argv,output):
        capture=Capture(Path(output)/'network');self.captures[str(output)]=capture
        return capture.wrap(argv)
    def started(self,output,pid):self.captures[str(output)].root_pid=pid
    def finish(self,output,code,stderr=''):
        observation=self.captures[str(output)].finish(code,stderr)
        permitted(observation,self.allowed)
        if self.model.requests:raise TraceError('disabled actual host contacted model endpoint')
        self.observations.append({'processes':observation['processes'],'attempts':len(observation['events']),'complete':True,'loss':0,'native':observation['native']})
