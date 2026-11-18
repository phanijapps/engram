"""Internal native process custody. A recorded PID never grants signal authority.

Windows: documented CREATE_SUSPENDED/AssignProcessToJobObject/ResumeThread.
Mac: original unreaped child + libproc unique birth/image + kqueue exit watch.
Native smoke controls must run before treating either backend as execution proof.
"""
from __future__ import annotations
import ctypes
import json
import os
from pathlib import Path
import select
import signal
import subprocess
import sys
import time

_OWNED = {}


def _kernel():
    if sys.platform != "win32":raise RuntimeError("Windows kernel API requested on another host")
    from ctypes import wintypes as w
    kernel=ctypes.WinDLL("kernel32",use_last_error=True)
    for name,restype,args in [
        ("CreateJobObjectW",w.HANDLE,[ctypes.c_void_p,w.LPCWSTR]),
        ("AssignProcessToJobObject",w.BOOL,[w.HANDLE,w.HANDLE]),
        ("OpenThread",w.HANDLE,[w.DWORD,w.BOOL,w.DWORD]),
        ("ResumeThread",w.DWORD,[w.HANDLE]),
        ("CloseHandle",w.BOOL,[w.HANDLE]),
        ("WaitForSingleObject",w.DWORD,[w.HANDLE,w.DWORD]),
        ("GetProcessTimes",w.BOOL,[w.HANDLE,ctypes.c_void_p,ctypes.c_void_p,ctypes.c_void_p,ctypes.c_void_p]),
        ("QueryFullProcessImageNameW",w.BOOL,[w.HANDLE,w.DWORD,w.LPWSTR,ctypes.POINTER(w.DWORD)]),
        ("CreateToolhelp32Snapshot",w.HANDLE,[w.DWORD,w.DWORD]),
        ("Thread32First",w.BOOL,[w.HANDLE,ctypes.c_void_p]),
        ("Thread32Next",w.BOOL,[w.HANDLE,ctypes.c_void_p]),
        ("QueryInformationJobObject",w.BOOL,[w.HANDLE,ctypes.c_int,ctypes.c_void_p,w.DWORD,ctypes.c_void_p]),
    ]:
        fn=getattr(kernel,name);fn.restype=restype;fn.argtypes=args
    return kernel


def launch(argv, *, state=None, state_path=None, **kwargs):
    """Journal before acquisition and retain original handles on every failure."""
    state = state if state is not None else {}
    if state_path:
        state["creation_pending"]={"expected_exe":str(argv[0]),"expected_argv":list(argv)}
        Path(state_path).write_text(json.dumps(state)+"\n")
    if sys.platform=="win32":
        kwargs.pop("start_new_session",None)
        kwargs["creationflags"]=kwargs.get("creationflags",0)|0x4|0x200 # SUSPENDED, NEW_PROCESS_GROUP
    child=subprocess.Popen(argv,**kwargs)
    _OWNED[child.pid]=child
    child._engram_handles={}
    if state_path:
        state["creation_pending"]["pid"]=child.pid
        Path(state_path).write_text(json.dumps(state)+"\n")
    if sys.platform=="win32":
        kernel=_kernel();job=kernel.CreateJobObjectW(None,None)
        if not job:raise ctypes.WinError(ctypes.get_last_error())
        child._engram_handles["job"]=job
        if not kernel.AssignProcessToJobObject(job,int(child._handle)):
            raise ctypes.WinError(ctypes.get_last_error())
        # Before ResumeThread the suspended process has only its primary thread.
        class ThreadEntry(ctypes.Structure):
            _fields_=[("size",ctypes.c_uint32),("usage",ctypes.c_uint32),("tid",ctypes.c_uint32),("pid",ctypes.c_uint32),("base",ctypes.c_long),("delta",ctypes.c_long),("flags",ctypes.c_uint32)]
        snapshot=kernel.CreateToolhelp32Snapshot(4,0)
        if snapshot==ctypes.c_void_p(-1).value:raise ctypes.WinError(ctypes.get_last_error())
        entries=[];entry=ThreadEntry();entry.size=ctypes.sizeof(entry)
        try:
            more=kernel.Thread32First(snapshot,ctypes.byref(entry))
            while more:
                if entry.pid==child.pid:entries.append(entry.tid)
                more=kernel.Thread32Next(snapshot,ctypes.byref(entry))
        finally:kernel.CloseHandle(snapshot)
        if len(entries)!=1:raise RuntimeError("suspended original primary thread is not uniquely identifiable")
        thread=kernel.OpenThread(2,False,entries[0])
        if not thread:raise ctypes.WinError(ctypes.get_last_error())
        child._engram_handles["thread"]=thread
        if kernel.ResumeThread(thread)!=1:raise RuntimeError("owned primary thread did not resume from exactly one suspension")
        kernel.CloseHandle(thread);del child._engram_handles["thread"]
    if state_path:
        state.pop("creation_pending",None)
        Path(state_path).write_text(json.dumps(state)+"\n")
    return child


def process_identity(pid):
    if sys.platform=="linux":
        stat=Path(f"/proc/{pid}/stat").read_text()
        return {"pid":pid,"start_ticks":stat.rsplit(")",1)[1].split()[19],"exe":str(Path(f"/proc/{pid}/exe").resolve()),"argv":Path(f"/proc/{pid}/cmdline").read_bytes().rstrip(b"\0").decode().split("\0")}
    if sys.platform=="darwin":
        lib=ctypes.CDLL("/usr/lib/libproc.dylib",use_errno=True)
        unique=ctypes.create_string_buffer(64)
        got=lib.proc_pidinfo(pid,17,0,unique,ctypes.sizeof(unique))
        if got<32:raise OSError(ctypes.get_errno(),"libproc unique process identity unavailable")
        path=ctypes.create_string_buffer(4096)
        if lib.proc_pidpath(pid,path,4096)<=0:raise OSError(ctypes.get_errno(),"libproc image unavailable")
        # KERN_PROCARGS2: argc, executable C string, padding, argv C strings.
        libc=ctypes.CDLL(None,use_errno=True);mib=(ctypes.c_int*3)(1,49,pid);size=ctypes.c_size_t(1024*1024);args=ctypes.create_string_buffer(size.value)
        if libc.sysctl(mib,3,args,ctypes.byref(size),None,0):raise OSError(ctypes.get_errno(),"owned argv unavailable")
        argc=int.from_bytes(args.raw[:4],sys.byteorder);data=args.raw[4:size.value];data=data[data.index(b"\0")+1:].lstrip(b"\0")
        argv=[item.decode() for item in data.split(b"\0")[:argc]]
        return {"pid":pid,"start_ticks":str(int.from_bytes(unique.raw[16:24],sys.byteorder)),"exe":path.value.decode(),"argv":argv}
    if sys.platform=="win32":
        child=_OWNED.get(pid)
        if pid==os.getpid() and child is None:
            class Current:
                _handle=-1
                args=[sys.executable,*sys.argv]
            child=Current()
        if child is None:raise RuntimeError("Windows identity requires retained original child handle")
        from ctypes import wintypes as w
        k=_kernel();times=[ctypes.c_uint64() for _ in range(4)]
        if not k.GetProcessTimes(int(child._handle),*[ctypes.byref(t) for t in times]):raise ctypes.WinError(ctypes.get_last_error())
        image=ctypes.create_unicode_buffer(32768);size=w.DWORD(len(image))
        if not k.QueryFullProcessImageNameW(int(child._handle),0,image,ctypes.byref(size)):raise ctypes.WinError(ctypes.get_last_error())
        return {"pid":pid,"start_ticks":str(times[0].value),"exe":str(Path(image.value).resolve()),"argv":list(child.args)}
    raise RuntimeError("unsupported native process custody platform")


def require_capability():
    if sys.platform=="linux":
        if not callable(getattr(os,"pidfd_open",None)) or not callable(getattr(signal,"pidfd_send_signal",None)):
            raise RuntimeError("native Linux pidfd APIs required; use /usr/bin/python3")
        fd=os.pidfd_open(os.getpid())
        try:signal.pidfd_send_signal(fd,0,None,0)
        finally:os.close(fd)
    elif sys.platform=="darwin":
        process_identity(os.getpid())
        watch=select.kqueue();watch.close()
    elif sys.platform=="win32":
        k=_kernel();handle=k.CreateJobObjectW(None,None)
        if not handle:raise ctypes.WinError(ctypes.get_last_error())
        k.CloseHandle(handle)
    else:raise RuntimeError("unsupported native process custody platform")


def acquire_process_identity(process,expected_exe,expected_argv,timeout=5,descriptor=None,identity_reader=None):
    reader=identity_reader or process_identity
    initial=reader(process.pid)["start_ticks"];deadline=time.monotonic()+timeout
    while True:
        if process.poll() is not None:raise RuntimeError("owned child exited during identity acquisition; retain failed root")
        identity=reader(process.pid)
        if identity["start_ticks"]!=initial:raise RuntimeError("owned birth identity changed during acquisition")
        if os.path.normcase(identity["exe"])==os.path.normcase(expected_exe) and identity["argv"]==expected_argv:return identity
        if time.monotonic()>=deadline:raise RuntimeError("owned exec identity unconfirmed; pending custody retained")
        time.sleep(.01)


def record_child(state,state_path,process,role,expected_exe,expected_argv,*,custody=None,timeout=5,identity_reader=None):
    reader=identity_reader or process_identity
    state["acquisition_pending"]={"pid":process.pid,"role":role,"initial_identity":None,"expected_exe":expected_exe,"expected_argv":expected_argv}
    Path(state_path).write_text(json.dumps(state,indent=2)+"\n")
    descriptor=None
    try:
        if sys.platform=="linux":
            descriptor=os.pidfd_open(process.pid)
            if custody is not None:custody.update(process=process,pidfd=descriptor,state=state,state_path=state_path)
            state['acquisition_pending']['pidfd_acquired']=True
        elif sys.platform=="darwin":
            watch=select.kqueue();watch.control([select.kevent(process.pid,filter=select.KQ_FILTER_PROC,flags=select.KQ_EV_ADD|select.KQ_EV_ENABLE,fflags=select.KQ_NOTE_EXIT|select.KQ_NOTE_EXEC)],0,0)
            process._engram_handles={**getattr(process,'_engram_handles',{}),'watch':watch}
            if custody is not None:custody.update(process=process,state=state,state_path=state_path)
        elif sys.platform=="win32":
            if not getattr(process,'_engram_handles',{}).get('job'):raise RuntimeError('Windows child must be suspended/job-bound before resume')
            if custody is not None:custody.update(process=process,state=state,state_path=state_path)
        state['acquisition_pending']['initial_identity']=reader(process.pid)
        Path(state_path).write_text(json.dumps(state,indent=2)+"\n")
        identity=acquire_process_identity(process,expected_exe,expected_argv,timeout=timeout,descriptor=descriptor,identity_reader=reader)
        state['processes'].append({**identity,'role':role});state.pop('acquisition_pending',None)
        Path(state_path).write_text(json.dumps(state,indent=2)+"\n")
        return identity
    finally:
        if descriptor is not None and custody is None:os.close(descriptor)


def release(process):
    if process.poll() is None:raise RuntimeError('original child is still live; no release/replacement')
    handles=getattr(process,'_engram_handles',{})
    if sys.platform=='win32' and handles.get('job'):
        class Accounting(ctypes.Structure):
            _fields_=[('times',ctypes.c_uint64*4),('faults',ctypes.c_uint32),('total',ctypes.c_uint32),('active',ctypes.c_uint32),('terminated',ctypes.c_uint32)]
        info=Accounting();k=_kernel()
        if not k.QueryInformationJobObject(handles['job'],1,ctypes.byref(info),ctypes.sizeof(info),None) or info.active:
            raise RuntimeError('owned job descendants still live or exit accounting unavailable')
        k.CloseHandle(handles.pop('job'))
    if 'watch' in handles:handles.pop('watch').close()
    _OWNED.pop(process.pid,None)


def owned_alive(identity):
    try:return all(process_identity(identity['pid']).get(key)==value for key,value in identity.items() if key!='role')
    except (OSError,ValueError,RuntimeError):return False


def stop_original(child,identity,timeout=30):
    """Graceful signal to retained original child only; never adopt a recorded PID."""
    if _OWNED.get(child.pid) is not child:raise RuntimeError('original process object custody missing')
    if child.returncode is not None:return
    actual=process_identity(child.pid)
    if any(actual.get(k)!=v for k,v in identity.items() if k!='role'):raise RuntimeError('original child changed before graceful close')
    # An unreaped Mac original child cannot have its PID recycled. Windows
    # retains the actual process object handle and dedicated process group.
    child.send_signal(signal.CTRL_BREAK_EVENT if sys.platform=='win32' else signal.SIGTERM)
    child.wait(timeout=timeout)
    release(child)


def exit_watch(identity):
    """Observe an authenticated owner exit using a kernel watch, never signal it."""
    pid=identity['pid']
    if sys.platform=='darwin':
        if process_identity(pid)['start_ticks']!=identity['start_ticks']:raise RuntimeError('owner birth identity changed')
        watch=select.kqueue();watch.control([select.kevent(pid,filter=select.KQ_FILTER_PROC,flags=select.KQ_EV_ADD|select.KQ_EV_ENABLE,fflags=select.KQ_NOTE_EXIT)],0,0)
        if process_identity(pid)['start_ticks']!=identity['start_ticks']:watch.close();raise RuntimeError('owner changed during watch acquisition')
        def wait(timeout):
            try:
                events=watch.control(None,1,timeout)
                if not events or not events[0].fflags&select.KQ_NOTE_EXIT:raise RuntimeError('authenticated owner exit not observed')
            finally:watch.close()
        return wait
    if sys.platform=='win32':
        from ctypes import wintypes as w
        k=_kernel();k.OpenProcess.argtypes=[w.DWORD,w.BOOL,w.DWORD];k.OpenProcess.restype=w.HANDLE
        handle=k.OpenProcess(0x100000|0x1000,False,pid)
        if not handle:raise ctypes.WinError(ctypes.get_last_error())
        times=[ctypes.c_uint64() for _ in range(4)]
        if not k.GetProcessTimes(handle,*[ctypes.byref(t) for t in times]) or str(times[0].value)!=identity['start_ticks']:
            k.CloseHandle(handle);raise RuntimeError('authenticated owner handle birth mismatch')
        def wait(timeout):
            try:
                if k.WaitForSingleObject(handle,int(timeout*1000))!=0:raise RuntimeError('authenticated owner kernel exit not observed')
            finally:k.CloseHandle(handle)
        return wait
    raise RuntimeError('owner-mediated fixture control only used on native Mac/Windows')


def hold_pending():
    """Do not abandon a live original controlled child at interpreter shutdown."""
    announced=False
    while _OWNED:
        for pid,child in list(_OWNED.items()):
            try:child.communicate(timeout=1)
            except subprocess.TimeoutExpired:continue
            try:release(child)
            except RuntimeError:continue
        if _OWNED and not announced:
            print('BLOCKED: original controlled child/kernel custody retained; no replacement or forced cleanup',file=sys.stderr,flush=True);announced=True
