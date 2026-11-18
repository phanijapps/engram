"""Readiness-only ETW session owner with acquisition-time event-ID filters.

The alternative acquisition scope is a dedicated fresh GitHub-hosted VM, NOT an
exact-child filter. Owned-tree attribution occurs later. Desktop/provider profiles
cannot authorize this scope. Only manifest-checked metadata templates are enabled.
"""
from __future__ import annotations
import ctypes
import hashlib
import json
import os
from pathlib import Path
import subprocess
import uuid
import xml.etree.ElementTree as ET


class EtwError(RuntimeError):
    pass


PROVIDERS=('Microsoft-Windows-Kernel-Process','Microsoft-Windows-TCPIP','Microsoft-Windows-DNS-Client','Microsoft-Windows-WFP')
# No binary buffers, command lines, strings of arbitrary data, headers, packet
# payloads, stack traces or unknown templates are admitted. This is an allowlist.
SCALARS={'win:UInt8','win:UInt16','win:UInt32','win:UInt64','win:Int8','win:Int16','win:Int32','win:Int64','win:Boolean','win:GUID','win:HexInt32','win:HexInt64','win:FILETIME','win:Pointer'}
FIELDS={'processid','pid','parentprocessid','parentid','createtime','exittime','exitcode','sessionid','imagechecksum','timestamp','status','querytype','queryoptions','queryid','networkqueryindex','isnetworkquery','threadid','connectionid','connid','daddr','saddr','destaddr','sourceaddr','destinationaddress','sourceaddress','remoteaddress','localaddress','remoteaddr','localaddr','dport','sport','destinationport','sourceport','remoteport','localport','size','numbytes','seqnum','starttime','endtime','filterid','layerid','protocol','direction','eventtype','dropstatus','failurecode','compartmentid','interfaceindex','subinterfaceindex','family','af','flags','processsequencenumber','parentprocesssequencenumber','packagefullnamelength','applicationidlength'}
STRINGS={'imagename','packagefullname','applicationid','queryname','daddr','saddr','destaddr','sourceaddr','destinationaddress','sourceaddress','remoteaddress','localaddress','remoteaddr','localaddr'}


def disposable_scope(value):
    required=('GITHUB_ACTIONS','GITHUB_RUN_ID','GITHUB_RUN_ATTEMPT','GITHUB_JOB','RUNNER_NAME','RUNNER_OS','RUNNER_TEMP')
    if value!='github-hosted-disposable-readiness' or os.environ.get('GITHUB_ACTIONS')!='true' or any(not os.environ.get(k) for k in required):
        raise EtwError('whole-VM metadata acquisition requires explicit dedicated GitHub-hosted readiness scope and actual job identity')
    if os.environ.get('RUNNER_ENVIRONMENT')!='github-hosted':raise EtwError('self-hosted/desktop capture is not authorized by hosted-VM scope')
    return {'acquisition':'whole dedicated fresh hosted VM metadata; NOT exact-child collection','attribution':'post-capture original-child tree','identity':{k:os.environ[k] for k in required},'scope':value}


def checked_events(provider,events):
    """Check every version sharing an enabled ID before any ETW side effect."""
    grouped={}
    for event in events:grouped.setdefault(int(event['id']),[]).append(event)
    selected=[]
    for identifier,versions in grouped.items():
        if provider.endswith('Kernel-Process') and identifier not in (1,2):continue
        accepted=True
        for event in versions:
            try:
                template=ET.fromstring(event.get('template') or '<missing/>')
            except ET.ParseError:accepted=False;break
            fields=[]
            for field in template.iter():
                tag=field.tag.rsplit('}',1)[-1]
                if tag in {'template'}:continue
                if tag!='data' or field.get('count') or field.get('length'):accepted=False;break
                name=field.get('name','').lower();kind=field.get('inType','')
                if name not in FIELDS|STRINGS or (kind not in SCALARS and not(kind=='win:UnicodeString' and name in STRINGS)):
                    accepted=False;break
                fields.append(name)
            if not fields:accepted=False
            if provider.endswith('Kernel-Process') and not {'processid','parentprocessid' if identifier==1 else 'processid'}.issubset(fields):accepted=False
            if not provider.endswith('Kernel-Process') and not any(name in fields for name in ('daddr','destaddr','destinationaddress','remoteaddress','remoteaddr','queryname','dropstatus','failurecode')):accepted=False
            if not accepted:break
        if accepted:selected.append(identifier)
    if provider.endswith('Kernel-Process') and set(selected)!={1,2}:raise EtwError('process start/exit templates are unknown or not metadata-only')
    if not selected:raise EtwError('no reviewed metadata-only attempt schemas for '+provider)
    if len(selected)>64:raise EtwError('event-ID filter exceeds reviewed bounded filter size')
    return sorted(selected)


def manifests(run):
    # Template metadata is inspected before starting or enabling any session.
    script="& { param($name) $p=Get-WinEvent -ListProvider $name -ErrorAction Stop;@{name=$p.Name;guid=[string]$p.Id;events=@($p.Events|ForEach-Object{@{id=$_.Id;version=$_.Version;template=$_.Template}})}|ConvertTo-Json -Compress -Depth 8 }"
    records=[]
    for provider in PROVIDERS:
        data=json.loads(run(['powershell.exe','-NoProfile','-NonInteractive','-Command',script,provider]))
        if data.get('name')!=provider:raise EtwError('native provider manifest identity mismatch')
        data['event_ids']=checked_events(provider,data['events']);records.append(data)
    return records


class _Guid(ctypes.Structure):
    _fields_=[('a',ctypes.c_uint32),('b',ctypes.c_uint16),('c',ctypes.c_uint16),('d',ctypes.c_ubyte*8)]
    @classmethod
    def parse(cls,value):return cls.from_buffer_copy(uuid.UUID(value).bytes_le)


class _Wnode(ctypes.Structure):
    _fields_=[('size',ctypes.c_uint32),('provider',ctypes.c_uint32),('history',ctypes.c_uint64),('time',ctypes.c_uint64),('guid',_Guid),('context',ctypes.c_uint32),('flags',ctypes.c_uint32)]


class _Properties(ctypes.Structure):
    _fields_=[('wnode',_Wnode),('buffer',ctypes.c_uint32),('minimum',ctypes.c_uint32),('maximum',ctypes.c_uint32),('file_size',ctypes.c_uint32),('mode',ctypes.c_uint32),('flush',ctypes.c_uint32),('enable',ctypes.c_uint32),('age',ctypes.c_int32),('buffers',ctypes.c_uint32),('free',ctypes.c_uint32),('events_lost',ctypes.c_uint32),('written',ctypes.c_uint32),('log_lost',ctypes.c_uint32),('realtime_lost',ctypes.c_uint32),('logger_thread',ctypes.c_void_p),('file_offset',ctypes.c_uint32),('name_offset',ctypes.c_uint32)]


def _api():
    from ctypes import wintypes as w
    api=ctypes.WinDLL('advapi32',use_last_error=True)
    api.StartTraceW.argtypes=[ctypes.POINTER(ctypes.c_uint64),w.LPCWSTR,ctypes.c_void_p];api.StartTraceW.restype=w.ULONG
    api.ControlTraceW.argtypes=[ctypes.c_uint64,w.LPCWSTR,ctypes.c_void_p,w.ULONG];api.ControlTraceW.restype=w.ULONG
    api.EnableTraceEx2.argtypes=[ctypes.c_uint64,ctypes.POINTER(_Guid),w.ULONG,ctypes.c_ubyte,ctypes.c_uint64,ctypes.c_uint64,w.ULONG,ctypes.c_void_p];api.EnableTraceEx2.restype=w.ULONG
    return api


class Session:
    def __init__(self,directory,run,scope):
        self.directory=Path(directory);self.run=run;self.scope=disposable_scope(scope);self.name='Engram-'+str(uuid.uuid4());self.handle=ctypes.c_uint64();self.started=False;self.pending=False;self.closed=False
        self.path=self.directory/'etw-custody.json';self.record={'name':self.name,'scope':self.scope,'phase':'prepared','cleanup_confirmed':False}
    def journal(self,phase,**values):
        self.record.update(phase=phase,**values);self.path.write_text(json.dumps(self.record,indent=2)+'\n')
        if os.name!='nt':self.path.chmod(0o600)
    def properties(self):
        filename=str(self.directory/'native.etl');size=ctypes.sizeof(_Properties)+2*(len(filename)+len(self.name)+2);buffer=ctypes.create_string_buffer(size);p=ctypes.cast(buffer,ctypes.POINTER(_Properties));p.contents.wnode.size=size;p.contents.wnode.flags=0x20000;p.contents.wnode.context=1;p.contents.buffer=64;p.contents.minimum=16;p.contents.maximum=128;p.contents.mode=1 # sequential file, no circular overwrite
        p.contents.name_offset=ctypes.sizeof(_Properties);name=self.name.encode('utf-16-le')+b'\0\0';ctypes.memmove(ctypes.addressof(buffer)+p.contents.name_offset,name,len(name));p.contents.file_offset=p.contents.name_offset+len(name);data=filename.encode('utf-16-le')+b'\0\0';ctypes.memmove(ctypes.addressof(buffer)+p.contents.file_offset,data,len(data))
        return buffer,p
    def start(self):
        # Windows mode bits do not establish a private ACL. Restrict only this
        # invocation directory to the actual current token and SYSTEM before raw
        # session acquisition; no user/global preference or parent ACL changes.
        acl="& { param($path) $a=New-Object System.Security.AccessControl.DirectorySecurity;$a.SetAccessRuleProtection($true,$false);foreach($s in @([System.Security.Principal.WindowsIdentity]::GetCurrent().User.Value,'S-1-5-18')){$id=New-Object System.Security.Principal.SecurityIdentifier($s);$r=New-Object System.Security.AccessControl.FileSystemAccessRule($id,'FullControl','ContainerInherit,ObjectInherit','None','Allow');$a.AddAccessRule($r)};Set-Acl -LiteralPath $path -AclObject $a -ErrorAction Stop }"
        self.run(['powershell.exe','-NoProfile','-NonInteractive','-Command',acl,str(self.directory)])
        checked=manifests(self.run)
        self.journal('schemas-validated',schemas_sha256=hashlib.sha256(json.dumps(checked,sort_keys=True).encode()).hexdigest(),schemas=[{'name':p['name'],'guid':p['guid'],'event_ids':p['event_ids']} for p in checked])
        buffer,_=self.properties();api=_api();self.pending=True;self.journal('start-pending')
        try:
            status=api.StartTraceW(ctypes.byref(self.handle),self.name,buffer)
            if status:raise EtwError('StartTraceW failed: '+str(status))
            self.started=True;self.pending=False;self.journal('started')
            class Descriptor(ctypes.Structure):_fields_=[('ptr',ctypes.c_uint64),('size',ctypes.c_uint32),('type',ctypes.c_uint32)]
            class Parameters(ctypes.Structure):_fields_=[('version',ctypes.c_uint32),('property',ctypes.c_uint32),('flags',ctypes.c_uint32),('source',_Guid),('filters',ctypes.POINTER(Descriptor)),('count',ctypes.c_uint32)]
            for provider in checked:
                ids=provider['event_ids'];raw=bytes([1,0])+len(ids).to_bytes(2,'little')+b''.join(i.to_bytes(2,'little') for i in ids);event_filter=ctypes.create_string_buffer(raw)
                descriptor=Descriptor(ctypes.addressof(event_filter),len(raw),0x80000200);parameters=Parameters();parameters.version=2;parameters.filters=ctypes.pointer(descriptor);parameters.count=1;guid=_Guid.parse(provider['guid'])
                status=api.EnableTraceEx2(self.handle,ctypes.byref(guid),1,0,0xffffffffffffffff,0,0,ctypes.byref(parameters))
                if status:raise EtwError('native event-ID filtered provider enable failed: '+str(status))
            self.journal('filtered-capture-active')
        except BaseException:
            self.close();raise
    def loss(self):
        buffer,p=self.properties();status=_api().ControlTraceW(self.handle,self.name,buffer,0)
        if status:raise EtwError('native ETW loss query failed: '+str(status))
        return int(p.contents.events_lost+p.contents.log_lost+p.contents.realtime_lost)
    def close(self):
        if self.closed:return
        if not(self.started or self.pending):self.closed=True;self.journal('closed-no-session',cleanup_confirmed=True);return
        buffer,_=self.properties();status=_api().ControlTraceW(self.handle,self.name,buffer,1)
        # ERROR_WMI_INSTANCE_NOT_FOUND proves this exact UUID session absent.
        if status not in (0,4201):
            self.journal('exact-session-cleanup-pending',cleanup_status=int(status));raise EtwError('exact native session cleanup unconfirmed: '+str(status))
        self.started=False;self.pending=False;self.closed=True;self.journal('closed',cleanup_confirmed=True)
