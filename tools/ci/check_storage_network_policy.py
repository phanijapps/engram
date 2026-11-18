#!/usr/bin/env python3
"""Native PLT-001 profiles: actual source-bound consumers and attempted egress."""
from __future__ import annotations

from contextlib import contextmanager
from dataclasses import dataclass
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import stat
import subprocess
import sys

_WORKFLOW = None
ROOT=Path(__file__).resolve().parents[2]
if str(ROOT) not in sys.path:sys.path.insert(0,str(ROOT))

AMBIENT_METRICS = ("saved-basic", "saved-full", "env-basic", "env-full")
# These profiles pass only on actual source-bound native observations.
PROPOSED_PROFILES = {
    "embedded-offline": "zero outbound attempts; zero model resolver/initializer events",
    "service-disabled-vectors": "only explicit selected storage/auth; no model or telemetry",
}


class ObservationError(RuntimeError):
    """Missing or incomplete observation cannot establish zero attempts."""


@dataclass(frozen=True)
class Observation:
    attempts: tuple[str, ...]
    trace: Path


def attempted_network(trace: str) -> tuple[str, ...]:
    attempts = []
    for line in trace.splitlines():
        if re.search(r"\b(connect|send|sendto|sendmsg|sendmmsg)\(", line):
            # AF_UNIX operations are local IPC; unresolved destinations fail closed.
            if "AF_UNIX" not in line:
                attempts.append(line)
    return tuple(attempts)


def read_observation(trace: Path, returncode: int) -> Observation:
    if returncode != 0 or not trace.is_file():
        raise ObservationError("trace/command failed or trace is missing")
    content = trace.read_text()
    if "+++ exited with 0 +++" not in content or "strace:" in content:
        raise ObservationError("trace is incomplete or tracer reported an error")
    if "<unfinished ...>" in content or re.search(r"<\.\.\. \w+ resumed>", content):
        raise ObservationError("split syscall trace is incomplete; cannot establish zero attempts")
    supported = {"socket", "socketpair", "bind", "listen", "accept", "accept4",
                 "connect", "getsockname", "getpeername", "getsockopt", "setsockopt",
                 "recvfrom", "recvmsg", "recvmmsg", "sendto", "sendmsg", "sendmmsg",
                 "send", "recv", "shutdown"}
    observed, completed = set(), set()
    for line in content.splitlines():
        matched = re.fullmatch(r"(?:(\d+)\s+)?(.*)", line)
        pid, body = matched.groups()
        if pid in completed:
            raise ObservationError("trace record follows process completion")
        observed.add(pid)
        if body == "+++ exited with 0 +++":
            completed.add(pid)
        elif body.startswith("+++"):
            raise ObservationError("malformed or unsuccessful process completion")
        elif re.fullmatch(r"--- SIG[A-Z0-9]+ \{.*\} ---", body):
            continue
        else:
            call = re.fullmatch(r"([a-z_][a-z_0-9]*)\(.*\)\s+=\s+(?:-?\d+|0x[0-9a-f]+)(?:\s.*)?", body)
            if call is None or call.group(1) not in supported:
                raise ObservationError("unknown or incomplete syscall trace record")
    if observed != completed:
        raise ObservationError("each observed process requires its own successful completion")
    return Observation(attempted_network(content), trace)


def trace_command(command: list[str], directory: Path, *, env: dict[str, str] | None = None,
                  deny_attempts: bool = False, tracer: str = "strace") -> Observation:
    if sys.platform != "linux" or not shutil.which(tracer):
        raise ObservationError("native syscall tracing unavailable; target remains NOT_RUN")
    directory.mkdir(parents=True, exist_ok=True)
    trace = directory / "network.trace"
    if trace.exists():
        raise ObservationError("refusing to overwrite an existing observation")
    args = [tracer, "-f", "-s", "0", "-e", "trace=network", "-o", str(trace)]
    if deny_attempts:
        args += ["-e", "inject=connect:error=EACCES", "-e", "inject=sendto:error=EACCES"]
    try:
        result = subprocess.run(args + ["--"] + command, env=env, capture_output=True,
                                timeout=30, check=False)
    except (OSError, subprocess.TimeoutExpired) as error:
        raise ObservationError("tracer execution failed") from error
    (directory / "command.stdout").write_bytes(result.stdout)
    (directory / "command.stderr").write_bytes(result.stderr)
    return read_observation(trace, result.returncode)


def assert_offline(observation: Observation, model_events: Path) -> None:
    if observation.attempts:
        raise ObservationError("outbound network attempt observed, including denied attempts")
    if not model_events.is_file():
        raise ObservationError("model resolver/initializer probe is missing")
    events = json.loads(model_events.read_text())
    if not isinstance(events, list) or events:
        raise ObservationError("model resolver/initializer invocation observed")


@contextmanager
def telemetry_fixture(directory: Path, variant: str):
    """Change child-only environment and owned preference bytes, never global prefs."""
    if variant not in AMBIENT_METRICS:
        raise ValueError("unknown ambient metrics fixture")
    root = directory / "helix-home"
    root.mkdir(parents=True)
    preferences = root / "metrics.toml"
    kind, level = variant.split("-")
    preferences.write_text(f'level = "{level if kind == "saved" else "off"}"\ninstallation_id = "00000000-0000-4000-8000-000000000001"\nuser_id = "engram-test-user"\n')
    before = preferences.read_bytes()
    (directory / "preferences.before").write_bytes(before)
    env = os.environ.copy()
    for name in tuple(env):
        if name.startswith("HELIX_TELEMETRY_"):
            env.pop(name)
    env["HELIX_HOME"] = str(root)
    if kind == "env":
        env["HELIX_TELEMETRY_LEVEL"] = level
    try:
        yield env, preferences
    finally:
        after = preferences.read_bytes() if preferences.exists() else None
        if after is not None:
            (directory / "preferences.after").write_bytes(after)
        (directory / "preference-hashes.json").write_text(json.dumps({
            "before": hashlib.sha256(before).hexdigest(),
            "after": hashlib.sha256(after).hexdigest() if after is not None else None,
        }) + "\n")
        if before != after:
            raise ObservationError("saved preferences changed during observation")


def execute_profile(args):
    """Execute each actual native cell; unavailable cells remain explicit BLOCKED."""
    import argparse
    import uuid
    from tools.ci import provider_network_trace as trace
    global _WORKFLOW
    destination=Path(args.output).resolve()
    if not destination.is_relative_to(ROOT/'runs') or any(p.is_symlink() for p in (destination,*destination.parents)):
        raise ObservationError('profile evidence must remain under owned nonsymlink checkout runs')
    # The fixed accepted parent remains unchanged; each invocation is new.
    output=destination/str(uuid.uuid4());output.mkdir(parents=True,mode=0o700)
    records=[];targets=(args.target,) if args.target else trace.TARGETS if args.all_supported_targets else (trace.native_target(),)
    source=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip()
    for target in targets:
        row={'target':target,'profile':args.profile,'source_revision':source,'status':'BLOCKED','execution_environment':{key:os.environ.get(key) for key in ('GITHUB_ACTIONS','GITHUB_RUN_ID','GITHUB_RUN_ATTEMPT','GITHUB_JOB','RUNNER_NAME','RUNNER_OS','RUNNER_ARCH','RUNNER_ENVIRONMENT','RUNNER_TEMP','TMPDIR','TEMP','TMP')},'requested_acquisition_scope':args.acquisition_scope}
        try:
            if target!=trace.native_target():raise ObservationError('requires actual target runner, never cross-compilation')
            trace.readiness()
            row['instrumentation_controls']=trace.positive_controls(output/target/'positive-controls',args.acquisition_scope)
            if args.native_preflight:
                # Actual owned child checks execute on this target; no provider proof.
                command=[sys.executable,'-m','unittest','tools.ci.tests.test_provider_native_controls.NativeCustodyTests']
                result=subprocess.run(command,cwd=ROOT,capture_output=True,text=True)
                (output/'raw.log').open('a').write('$ '+' '.join(command)+'\n'+result.stdout+result.stderr)
                if result.returncode:raise ObservationError('actual native child custody control failed')
                row.update(status='PASS',kind='native instrumentation/custody controls ONLY; Helix profiles NOT_RUN')
            else:
                from tools.ci import run_provider_e2e as workflow
                _WORKFLOW=workflow
                if not all((args.artifacts,args.expected_build_record,args.expected_build_record_sha256)):
                    raise ObservationError('actual native artifacts, independent expected record and pin required')
                # Run actual canonical resolver/init positive+negative probes;
                # zero model requests alone cannot prove no cached initialization.
                command=['cargo','+1.88.0','test','--offline','--locked','-p','engram-integration','--features','fastembed,http-embeddings','--lib','disabled_model_tests::','--','--test-threads=1','--nocapture']
                result=subprocess.run(command,cwd=ROOT,capture_output=True,text=True)
                (output/'raw.log').open('a').write('$ '+' '.join(command)+'\n'+result.stdout+result.stderr)
                marker='MODEL_EARLY_RETURN_SPY available_and_missing=true disabled_resolver_init_request=0 enabled_positive_each=1'
                if result.returncode or marker not in result.stdout+result.stderr:raise ObservationError('real canonical early-return/positive spies missing or failed')
                variants=args.ambient_metrics.split(',')
                if not variants or len(variants)!=len(set(variants)) or any(v not in AMBIENT_METRICS for v in variants):raise ObservationError('invalid/missing ambient telemetry variants')
                row['variants']=[]
                for variant in variants:
                    for available in (True,False):
                        owned=output/target/(variant+('-available' if available else '-missing'));owned.mkdir(parents=True)
                        with telemetry_fixture(owned,variant) as (environment,preferences):
                            model=trace.EmbeddingFixture(available);model.start()
                            observer=trace.ConsumerObserver(args.profile,model)
                            old_env=os.environ.copy();old_observer=getattr(workflow,'_NETWORK_OBSERVER',None)
                            try:
                                os.environ.update(environment);workflow._NETWORK_OBSERVER=observer
                                namespace=argparse.Namespace(provider='helixdb',mode='embedded' if args.profile=='embedded-offline' else 'service',host=None,scenario=None,target=target,all_supported_targets=False,artifacts=args.artifacts,expected_build_record=args.expected_build_record,expected_build_record_sha256=args.expected_build_record_sha256,phase='operations',output=str(owned/'consumers'))
                                code=workflow.execute(namespace)
                                if code or len(observer.observations)!=len(workflow.HOSTS):raise ObservationError('required actual traced host operation/close cell missing or failed')
                                if model.requests:raise ObservationError('disabled vectors contacted model fixture')
                                row['variants'].append({'variant':variant,'model_credential_available':available,'actual_hosts':observer.observations,'model_http_requests':model.requests})
                            finally:
                                workflow._NETWORK_OBSERVER=old_observer;os.environ.clear();os.environ.update(old_env)
                                if workflow.consumer_closure_confirmed(owned/'consumers'):model.close()
                                elif not workflow._PENDING_CHILDREN:model.close()
                                # Pending original consumer retains dependent fixture.
                row.update(status='PASS',kind='actual native selected-provider profiles')
        except (OSError,ValueError,RuntimeError,ObservationError,trace.TraceError) as error:
            row['reason']=str(error) if isinstance(error,(ObservationError,trace.TraceError)) else 'native prerequisite/execution failed; inspect retained exact custody'
        for diagnostic in (output/target).rglob('control.stderr'):
            with (output/'raw.log').open('a') as raw:raw.write(str(diagnostic.relative_to(output))+'\n'+diagnostic.read_text()[:16384]+'\n')
        records.append(row)
        with (output/'operations.jsonl').open('a') as ledger:ledger.write(json.dumps(row)+'\n')
    status='PASS' if records and all(r['status']=='PASS' for r in records) else 'BLOCKED'
    (output/'report.json').write_text(json.dumps({'status':status,'source_revision':source,'invocation':output.name,'records':records},indent=2)+'\n')
    (output/'raw.log').touch(exist_ok=True)
    print(json.dumps({'status':status,'output':str(output)}))
    return 0 if status=='PASS' else 2


COMPACT_NAMES=('report.json','operations.jsonl','raw.log')


def export_compact(preflight_root,invocation,destination,revision):
    """After preflight exits, hand only exact compact files to its actual caller.

    Private traces/recovery trees stay untouched. This separate invocation does
    no tracing and owns no original child. Directory descriptors and O_EXCL /
    O_NOFOLLOW avoid adopting a replacement/symlink or preexisting output.
    """
    import uuid
    if os.name!='posix':raise ObservationError('compact ownership export is a POSIX elevation boundary')
    try:
        if str(uuid.UUID(invocation))!=invocation:raise ValueError()
    except (ValueError,AttributeError):raise ObservationError('invalid native invocation identity') from None
    root=Path(preflight_root).absolute();target=Path(destination).absolute()
    allowed=(ROOT/'runs').absolute();temporary=Path(os.environ['TMPDIR']).absolute()
    if not root.is_relative_to(allowed) or '..' in root.parts or not target.is_relative_to(temporary) or target==temporary or '..' in target.parts:
        raise ObservationError('compact handoff paths escape owned runs/temp roots')
    for path in (root,root/invocation,target):
        if any(p.is_symlink() for p in (path,*path.parents)):raise ObservationError('symlinked compact handoff path')
    caller_uid=int(os.environ['SUDO_UID']) if os.geteuid()==0 and os.environ.get('SUDO_UID') else os.getuid()
    caller_gid=int(os.environ['SUDO_GID']) if os.geteuid()==0 and os.environ.get('SUDO_GID') else os.getgid()
    flags=os.O_RDONLY|os.O_DIRECTORY|os.O_NOFOLLOW
    source_fd=os.open(root/invocation,flags);target_fd=None;created=[]
    try:
        source_stat=os.fstat(source_fd)
        if source_stat.st_uid!=os.geteuid() or stat.S_IMODE(source_stat.st_mode)!=0o700:raise ObservationError('native source is not this privileged owner private invocation')
        target_fd=os.open(target,flags);target_stat=os.fstat(target_fd)
        if target_stat.st_uid!=caller_uid or target_stat.st_gid!=caller_gid or stat.S_IMODE(target_stat.st_mode)!=0o700 or os.listdir(target_fd):
            raise ObservationError('compact destination must be fresh empty actual-caller-owned0700 directory')
        contents={};identities={}
        for name in COMPACT_NAMES:
            fd=os.open(name,os.O_RDONLY|os.O_NOFOLLOW,dir_fd=source_fd)
            try:
                before=os.fstat(fd)
                if not stat.S_ISREG(before.st_mode) or before.st_uid!=os.geteuid() or before.st_nlink!=1 or before.st_size>16*1024*1024:raise ObservationError('foreign/nonregular linked compact source')
                with os.fdopen(os.dup(fd),'rb') as stream:data=stream.read()
                after=os.fstat(fd)
                if (before.st_ino,before.st_size,before.st_mtime_ns)!=(after.st_ino,after.st_size,after.st_mtime_ns):raise ObservationError('compact source changed while read')
                contents[name]=data;identities[name]=(before.st_dev,before.st_ino,before.st_size,before.st_mtime_ns)
            finally:os.close(fd)
        report=json.loads(contents['report.json'])
        if report.get('invocation')!=invocation or report.get('source_revision')!=revision or not report.get('records') or any(row.get('profile') is not None or row.get('source_revision')!=revision for row in report['records']):
            raise ObservationError('compact source is not this source-bound native readiness invocation')
        hashes={}
        for name,data in contents.items():
            fd=os.open(name,os.O_WRONLY|os.O_CREAT|os.O_EXCL|os.O_NOFOLLOW,0o600,dir_fd=target_fd)
            try:
                identity=os.fstat(fd);created.append((name,identity.st_dev,identity.st_ino))
                with os.fdopen(os.dup(fd),'wb') as stream:stream.write(data);stream.flush();os.fsync(stream.fileno())
                os.fchmod(fd,0o600);os.fchown(fd,caller_uid,caller_gid)
            finally:os.close(fd)
            hashes[name]=hashlib.sha256(data).hexdigest()
        # Sources, destination directory and exact created files must still be
        # the observed objects; raw/control subdirectories are never enumerated.
        for name,identity in identities.items():
            st=os.stat(name,dir_fd=source_fd,follow_symlinks=False)
            if (st.st_dev,st.st_ino,st.st_size,st.st_mtime_ns)!=identity:raise ObservationError('source replaced during compact handoff')
        current=os.stat(target,follow_symlinks=False)
        if (current.st_dev,current.st_ino)!=(target_stat.st_dev,target_stat.st_ino):raise ObservationError('destination replaced during handoff')
        for name,device,inode in created:
            st=os.stat(name,dir_fd=target_fd,follow_symlinks=False)
            if not stat.S_ISREG(st.st_mode) or (st.st_dev,st.st_ino)!=(device,inode) or st.st_uid!=caller_uid:raise ObservationError('created compact file replaced')
        return {'invocation':invocation,'source_revision':revision,'owner_uid':caller_uid,'files':hashes,'destination':str(target)}
    except BaseException:
        if target_fd is not None:
            for name,device,inode in created:
                try:
                    current=os.stat(name,dir_fd=target_fd,follow_symlinks=False)
                    if (current.st_dev,current.st_ino)==(device,inode):os.unlink(name,dir_fd=target_fd)
                except FileNotFoundError:pass
        raise
    finally:
        os.close(source_fd)
        if target_fd is not None:os.close(target_fd)


def main() -> int:
    import argparse
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--self-test',action='store_true')
    parser.add_argument('--native-preflight',action='store_true')
    parser.add_argument('--export-native-preflight',metavar='INVOCATION_UUID')
    parser.add_argument('--compact-export',metavar='FRESH_CALLER_DIRECTORY')
    parser.add_argument('--acquisition-scope',choices=('owned-process-tree','github-hosted-disposable-readiness'),default='owned-process-tree')
    parser.add_argument('--profile',choices=tuple(PROPOSED_PROFILES))
    parser.add_argument('--ambient-metrics',default=','.join(AMBIENT_METRICS))
    parser.add_argument('--all-supported-targets',action='store_true')
    parser.add_argument('--target',choices=('linux-x64-gnu','linux-arm64-gnu','darwin-arm64','win32-x64'))
    parser.add_argument('--output')
    parser.add_argument('--artifacts',default=os.environ.get('ENGRAM_PROVIDER_E2E_ARTIFACTS'))
    parser.add_argument('--expected-build-record',default=os.environ.get('ENGRAM_PROVIDER_E2E_EXPECTED_BUILD_RECORD'))
    parser.add_argument('--expected-build-record-sha256',default=os.environ.get('ENGRAM_PROVIDER_E2E_EXPECTED_BUILD_RECORD_SHA256'))
    args=parser.parse_args()
    if args.export_native_preflight:
        if not args.output or not args.compact_export or args.profile or args.native_preflight or args.self_test:parser.error('separate compact handoff requires only output/invocation/caller directory')
        revision=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip()
        print(json.dumps(export_compact(args.output,args.export_native_preflight,args.compact_export,revision)))
        return 0
    if args.compact_export:parser.error('compact export belongs only to separate post-preflight handoff')
    if args.self_test:
        if args.profile or args.native_preflight:parser.error('self-test does not execute profiles/native preflight')
        return subprocess.run([sys.executable,'-m','unittest','tools.ci.tests.test_storage_network_policy']).returncode
    if not args.output or not (args.profile or args.native_preflight):parser.error('profile/native-preflight and owned output required')
    if args.acquisition_scope=='github-hosted-disposable-readiness' and not args.native_preflight:parser.error('dedicated hosted-VM metadata scope is readiness-only, not a provider/private-data profile')
    return execute_profile(args)


if __name__=='__main__':
    try:status=main()
    finally:
        if _WORKFLOW is not None:_WORKFLOW.hold_pending_child_custody()
        from tools.ci import provider_process
        provider_process.hold_pending()
        from tools.ci import provider_network_trace
        provider_network_trace.finish_pending_controls()
    raise SystemExit(status)
