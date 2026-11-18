#!/usr/bin/env python3
"""Inspect isolated production dependency closures, separately from test graphs."""
from __future__ import annotations
import argparse
import hashlib
import itertools
import json
import platform
import shutil
import subprocess
import sys
import os
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
if str(ROOT) not in sys.path:sys.path.insert(0,str(ROOT))
SQLITE = {"engram-store-sqlite", "rusqlite", "sqlite-vec", "libsqlite3-sys"}
POSTGRES = {"engram-store-pgvector", "engram-backend-pgvector", "tokio-postgres", "postgres", "sqlx"}
HELIX = {"engram-store-helixdb", "engram-backend-helixdb", "helix-db", "server"}
ALWAYS_FORBIDDEN = {"engram-conformance"}
PROVIDERS = ("sqlite", "pgvector", "helixdb-service", "helixdb-embedded")
TARGETS = {"linux-x64-gnu": "x86_64-unknown-linux-gnu", "linux-arm64-gnu": "aarch64-unknown-linux-gnu", "darwin-arm64": "aarch64-apple-darwin", "win32-x64": "x86_64-pc-windows-msvc"}


def native_target() -> str | None:
    return {("Linux", "x86_64"): "linux-x64-gnu", ("Linux", "aarch64"): "linux-arm64-gnu", ("Darwin", "arm64"): "darwin-arm64", ("Windows", "AMD64"): "win32-x64"}.get((platform.system(), platform.machine()))


def provider_subsets(matrix: str) -> list[tuple[str, ...]]:
    if matrix == "single":
        return [(p,) for p in PROVIDERS] + [("helixdb-service", "helixdb-embedded")]
    if matrix == "explicit-combinations":
        return [subset for count in range(1, len(PROVIDERS) + 1) for subset in itertools.combinations(PROVIDERS, count)]
    raise ValueError("unknown provider build matrix")


def forbidden_dependencies(selected: tuple[str, ...]) -> set[str]:
    forbidden = set(ALWAYS_FORBIDDEN)
    if "sqlite" not in selected:
        forbidden |= SQLITE | {"engram-backend-sqlite"}
    if "pgvector" not in selected:
        forbidden |= POSTGRES
    if not any(p.startswith("helixdb-") for p in selected):
        forbidden |= HELIX
    return forbidden


def package_names(output: str) -> set[str]:
    return {line.split()[0] for line in output.splitlines() if line.strip()}


def violations(production: set[str], forbidden: set[str]) -> list[str]:
    return [f"forbidden production dependency: {name}" for name in sorted(production & (forbidden | ALWAYS_FORBIDDEN))]


def dependency_graph(package: str, features: str, edges: str) -> set[str]:
    command = ["cargo", "+1.88.0", "tree", "--offline", "--locked", "--package", package,
               "--no-default-features", "--edges", edges, "--prefix", "none",
               "--format", "{p}"]
    if features:
        command += ["--features", features]
    result = subprocess.run(command, cwd=ROOT, check=True, text=True, capture_output=True)
    return package_names(result.stdout)


def sha(path: Path) -> str:
    result = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            result.update(block)
    return result.hexdigest()


def compiler_units(output: str) -> tuple[set[str], list[dict]]:
    """Actual production compiler units, not a dev-unified filtered tree."""
    names, artifacts = set(), []
    for line in output.splitlines():
        try:
            message = json.loads(line)
        except ValueError:
            continue
        if message.get("reason") != "compiler-artifact":
            continue
        package = message["package_id"].split("#")[-1].split("@")[0]
        names.add(package)
        artifacts.append(message)
    return names, artifacts


def runtime_command(package, artifact, request):
    """Only repository-owned consumers can produce runtime proof."""
    if package == "engram-provider-e2e":
        return [str(artifact),"--request",str(request)]
    if package in {"engram-node","engram-mcp"}:
        return ["node",str(ROOT / "packages/node/test/provider-e2e.mjs"),"--request",str(request),
                "--artifact" if package == "engram-node" else "--mcp",str(artifact)]
    raise ValueError("unknown actual runtime consumer package")


def producer_evidence(output, package, command, exit_code, units, artifact, source, features):
    """Observed compiler output and hashed real inputs, not a bundle assertion."""
    from tools.ci import run_provider_e2e as workflow
    kind='cdylib' if package=='engram-node' else 'bin'
    if exit_code or not units or 'build' not in command:raise ValueError('actual successful Cargo build required')
    own=[unit for unit in units if unit.get('package_id','').split('#')[-1].split('@')[0]==package and kind in unit.get('target',{}).get('crate_types',[])]
    if not own:raise ValueError('actual required compiler target missing')
    target_sources=[]
    for unit in own:
        target_source=Path(unit['target']['src_path']).resolve().relative_to(ROOT).as_posix()
        if target_source not in source['files']:raise ValueError('compiled target source is absent from immutable source snapshot')
        target_sources.append(target_source)
    inputs=[{'path':filename,'sha256':sha(Path(filename))} for unit in units for filename in unit.get('filenames',[]) if Path(filename).is_file()]
    if not inputs:raise ValueError('actual compiler/link inputs unavailable')
    identifier=artifact['sha256']
    directory=output/'producer-records'/(identifier+'-'+__import__('uuid').uuid4().hex);directory.mkdir(parents=True)
    compiler=directory/'compiler.json';links=directory/'links.json'
    with compiler.open('x') as file:json.dump({'schema':'engram.compiler-record.v1','package':package,'kind':kind,'command':command,'exit':exit_code,'units':units,'artifact_sha256':identifier,'source_snapshot_sha256':source['snapshot_sha256'],'features':features,'target_sources':target_sources},file,sort_keys=True)
    with links.open('x') as file:json.dump({'schema':'engram.link-record.v1','inputs':inputs},file,sort_keys=True)
    return {'package':package,'kind':kind,'compiler_record':str(compiler),'compiler_record_sha256':sha(compiler),'link_record':str(links),'link_record_sha256':sha(links),'link_inputs':inputs}


def build_matrix(args) -> int:
    output = Path(args.output).resolve()
    if not output.is_relative_to(ROOT/'runs') or any(path.is_symlink() for path in (output,*output.parents)):
        raise ValueError("build/runtime evidence must remain under owned checkout runs")
    if output.exists():
        raise ValueError("build evidence output must be new; retain prior attempts")
    output.mkdir(parents=True)
    current = native_target()
    targets = [args.target] if args.target else list(TARGETS) if args.all_supported_targets else [current]
    source = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    lock = sha(ROOT / "Cargo.lock")
    plans = json.loads(Path(args.request_plans).read_text()) if args.request_plans else {}
    from tools.ci import run_provider_e2e as workflow
    approved=json.loads(Path(args.approved_new_source_paths).read_text()) if args.approved_new_source_paths else []
    build_source=workflow.source_snapshot(approved)
    if plans and plans.get("schema") != "engram.provider-build-plans.v1":
        raise ValueError("unsupported actual build/runtime request-plan schema")
    published = []
    records, omissions = [], []
    for target in targets:
        if target not in TARGETS or target != current:
            omissions.append({"target": target, "status": "BLOCKED", "reason": "requires actual native runner, import/bootstrap/shutdown and platform ownership/instrumentation; cross compilation is insufficient"})
            continue
        for subset in provider_subsets(args.matrix):
            if args.providers and not set(subset).issubset(set(args.providers.split(','))):
                continue
            for package, host in (("engram-provider-e2e", "rust"), ("engram-mcp", "mcp-stdio"), ("engram-node", "native")):
                feature_sets = [list(subset)]
                if args.with_embeddings:
                    feature_sets += [[*subset, "fastembed", "http-embeddings"]]
                for features in feature_sets:
                    label = package + "-" + "+".join(features)
                    argv = ["cargo", "+1.88.0", "build", "--offline", "--locked", "--release", "--package", package, "--no-default-features", "--features", ",".join(features), "--target", TARGETS[target], "--message-format=json-render-diagnostics"]
                    command = subprocess.run(argv, cwd=ROOT, capture_output=True, text=True)
                    with (output / "raw.log").open("a") as raw:
                        raw.write("$ " + " ".join(argv) + "\n" + command.stdout + command.stderr)
                    names, units = compiler_units(command.stdout)
                    errors = violations(names, forbidden_dependencies(subset))
                    if any("test-controls" in unit.get("features", []) for unit in units):
                        errors.append("instrumented test-controls feature in production compiler units")
                    if not units:
                        errors.append("no actual compiler units")
                    artifact_paths = []
                    for unit in units:
                        if unit.get("target", {}).get("name", "").replace("_", "-") not in {package, "engram-mcp", "engram-provider-e2e", "engram-node"}:
                            continue
                        for filename in unit.get("filenames", []):
                            path = Path(filename)
                            if path.is_file() and (path.suffix in {".so", ".dylib", ".dll", ".exe"} or filename == unit.get("executable")):
                                digest = sha(path)
                                destination = output / "artifacts" / target / label / (digest + (".node" if host == "native" else path.suffix))
                                destination.parent.mkdir(parents=True, exist_ok=True)
                                with destination.open("xb") as file, path.open("rb") as original:
                                    shutil.copyfileobj(original, file)
                                destination.chmod(path.stat().st_mode)
                                artifact_paths.append({"path": str(destination), "sha256": digest})
                    links = [{"path":filename,"sha256":sha(Path(filename))} for unit in units for filename in unit.get("filenames", []) if Path(filename).is_file()]
                    evidence=producer_evidence(output,package,argv,command.returncode,units,artifact_paths[0],build_source,features) if command.returncode==0 and len(artifact_paths)==1 else {}
                    plan = [row for row in plans.get("plans", []) if row.get("package") == package and row.get("target") == target and set(row.get("features", [])) == set(features)]
                    runtime = "BLOCKED"
                    runtime_reason = "required actual import/bootstrap/sentinel/shutdown request plan unavailable"
                    if command.returncode == 0 and not errors and len(artifact_paths) == 1 and len(plan) == 1:
                        if "runtime_argv" in plan[0]:
                            raise ValueError("caller-selected runtime programs cannot establish actual host proof")
                        request_path = Path(plan[0].get("request_plan", ""))
                        if not request_path.is_file():
                            raise ValueError("declared actual runtime request plan missing")
                        packet = json.loads(request_path.read_text())
                        required = ("bootstrap", "sentinel_readback", "owned_close")
                        if not all(packet.get("proof_obligations", {}).get(key) is True for key in required):
                            raise ValueError("runtime plan omits import/bootstrap/sentinel/close obligations")
                        import uuid
                        from tools.ci import run_provider_e2e as workflow
                        owned = output / 'runtime' / str(uuid.uuid4());owned.mkdir(parents=True,mode=0o700)
                        selected_provider=packet['config'].get('storage_provider')
                        selected_mode=packet['config'].get('helixdb',{}).get('mode') if packet['config'].get('helixdb') else 'not_applicable'
                        required_feature='helixdb-'+selected_mode if selected_provider=='helixdb' else selected_provider
                        if required_feature not in subset:raise ValueError("runtime plan selects a provider outside the compiled feature subset")
                        invocation=str(uuid.uuid4())
                        packet=workflow.bind_owned_packet(packet,owned,host,selected_mode,invocation,provider=selected_provider)
                        packet.update(invocation=invocation,source_revision=source,lock_sha256=lock,target=target,artifact_sha256=artifact_paths[0]['sha256'],provider=selected_provider,host=host,mode=selected_mode)
                        fixture_root=owned/'helper';fixture_started=False;credential_name=None;old_credential=None
                        if selected_provider=='helixdb' and selected_mode=='service':
                            provision=subprocess.run(['/usr/bin/python3',str(ROOT/'tools/ci/helixdb_server.py'),'provision','--output',str(fixture_root)],cwd=ROOT,capture_output=True,text=True)
                            with (owned/'raw.log').open('a') as raw:raw.write('owned helper provision exit='+str(provision.returncode)+'\n'+provision.stderr)
                            if provision.returncode:raise ValueError("owned runtime service acquisition failed; preserve state")
                            fixture_started=True
                            info=json.loads(provision.stdout.splitlines()[-1]);credential_name=info['HELIX_TEST_CREDENTIAL_ENV']
                            import os
                            old_credential=os.environ.get(credential_name);os.environ[credential_name]=Path(info['credential_file']).read_text().strip()
                            packet['config']['helixdb'].update(path=None,endpoint=info['HELIX_TEST_ENDPOINT'],credential_env=credential_name)
                        request_path=owned/'request.json'
                        request_path.write_text(json.dumps(packet)+'\n');request_path.chmod(0o600)
                        runtime_args = runtime_command(package,artifact_paths[0]["path"],request_path.resolve())
                        before = sha(Path(artifact_paths[0]["path"]))
                        try:
                            code,stdout=workflow.run_owned(runtime_args,owned,packet)
                        finally:
                            if credential_name:
                                if old_credential is None:os.environ.pop(credential_name,None)
                                else:os.environ[credential_name]=old_credential
                            if fixture_started and workflow.consumer_closure_confirmed(owned):
                                closed=subprocess.run(['/usr/bin/python3',str(ROOT/'tools/ci/helixdb_server.py'),'cleanup','--output',str(fixture_root)],cwd=ROOT,capture_output=True,text=True)
                                with (owned/'raw.log').open('a') as raw:raw.write('owned helper cleanup exit='+str(closed.returncode)+'\n'+closed.stdout+closed.stderr)
                                if closed.returncode:raise ValueError("owned runtime service cleanup unconfirmed")
                        after = sha(Path(artifact_paths[0]["path"]))
                        receipts = [json.loads(line) for line in stdout.splitlines() if line.startswith('{')]
                        proved = any(row.get("schema") == "engram.provider-bootstrap.v1" and row.get("artifact_sha256") == before and all(row.get(key) is True for key in required) for row in receipts)
                        runtime = "PASS" if code == 0 and before == after and proved else "FAIL"
                        runtime_reason = "actual consumed artifact and runtime assertions" if runtime == "PASS" else "actual runtime failed or omitted bootstrap/sentinel/close proof"
                        if len(subset) == 1 and subset[0].startswith("helixdb-"):
                            mode = subset[0].removeprefix("helixdb-")
                            for routed_host in ((host, "mcp-oneshot") if host == "mcp-stdio" else (host,)):
                                operation_plan=Path(plan[0].get('operation_plans',{}).get(routed_host,''))
                                if not operation_plan.is_file():continue
                                catalogue=workflow.inventory()
                                expected={row['operation'] for row in catalogue if row['required'] and row['host']==routed_host and row['mode']==mode and row['target']==target}
                                supplied=json.loads(operation_plan.read_text())
                                if expected-{call.get('operation') for call in supplied.get('calls',[])}:raise ValueError("published operation plan omits required host operations")
                                plan_copy=output/'plans'/str(uuid.uuid4());plan_copy.mkdir(parents=True)
                                bound_plan=plan_copy/'operations.json';bound_plan.write_text(json.dumps(supplied)+'\n');bound_plan.chmod(0o600)
                                published.append({**artifact_paths[0],**evidence,"provider":"helixdb","mode":mode,"host":routed_host,"target":target,"features":features,"profile":"release","instrumented":False,"source_revision":source,"lock_sha256":lock,"production_compiler_packages":sorted(names),"request_plan":str(bound_plan),"request_plan_sha256":sha(bound_plan),"runtime_status":runtime})
                    record = {"package": package, "host": host, "target": target, "providers": subset, "features": features, "profile": "release", "source_revision": source, "lock_sha256": lock, "build_status": "PASS" if command.returncode == 0 and not errors and artifact_paths else "FAIL", "status":"PASS" if command.returncode == 0 and not errors and artifact_paths and runtime == "PASS" else "BLOCKED" if runtime == "BLOCKED" else "FAIL", "exit": command.returncode, "errors": errors, "production_compiler_packages": sorted(names), "compiler_artifacts": artifact_paths, "runtime_status": runtime, "runtime_reason":runtime_reason,"link_inputs":links}
                    records.append(record)
                    with (output / "operations.jsonl").open("a") as ledger:
                        ledger.write(json.dumps(record) + "\n")
    report = {"schema": "engram.provider-builds.v1", "source_revision": source, "lock_sha256": lock, "matrix": args.matrix, "records": records, "omissions": omissions, "status": "BLOCKED" if omissions or any(row["status"] == "BLOCKED" for row in records) else "PASS" if records and all(r["status"] == "PASS" for r in records) else "FAIL", "note": "build/closure proof is not runtime import/bootstrap/sentinel/shutdown proof"}
    (output / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    portable=[]
    for row in published:
        entry=dict(row)
        for key in ('path','compiler_record','link_record','request_plan'):
            if key in entry:entry[key]=Path(entry[key]).relative_to(output).as_posix()
        if 'fault_plans' in entry:entry['fault_plans']={name:Path(path).relative_to(output).as_posix() for name,path in entry['fault_plans'].items()}
        portable.append(entry)
    (output / "artifacts.json").write_text(json.dumps({"schema":"engram.provider-artifacts.v1","artifacts":portable},indent=2)+"\n")
    if workflow.source_snapshot(approved)!=build_source:raise ValueError('complete source snapshot changed during actual producer build')
    trusted={'schema':'engram.provider-build-record.v1','producer':{'repository':os.environ.get('GITHUB_REPOSITORY',args.producer_repository),'run_id':os.environ.get('GITHUB_RUN_ID','local-'+__import__('uuid').uuid4().hex),'commit':source},'source':build_source,'lock_sha256':lock,'artifacts':[{key:value for key,value in row.items() if key not in {'path','compiler_record','link_record','link_inputs','request_plan','fault_plans','runtime_status','production_compiler_packages','provider','source_revision','lock_sha256'}} for row in published]}
    expected_record=output.with_name(output.name+'.expected-build-record.json')
    with expected_record.open('x') as file:json.dump(trusted,file,sort_keys=True)
    print(json.dumps({'expected_build_record':str(expected_record),'expected_build_record_sha256':sha(expected_record),'trust':'content integrity only when independently pinned by orchestration; no PKI/author authentication'}))
    print(json.dumps({"status": report["status"], "records": len(records), "blocked_targets": len(omissions), "artifacts":str(output / "artifacts.json")}))
    return 2 if report["status"] == "BLOCKED" else int(report["status"] != "PASS")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--package", default="engram-integration")
    parser.add_argument("--features", default="fastembed,http-embeddings")
    parser.add_argument("--matrix", choices=("single", "explicit-combinations"))
    parser.add_argument("--all-supported-targets", action="store_true")
    parser.add_argument("--target", choices=tuple(TARGETS))
    parser.add_argument("--with-embeddings", action="store_true")
    parser.add_argument("--output", default="runs/provider-builds/single")
    parser.add_argument("--request-plans", help="actual per-package/features/target import/bootstrap/sentinel/close plans; absence is BLOCKED")
    parser.add_argument("--providers", help="explicit comma-separated provider recipes within the selected matrix")
    parser.add_argument("--approved-new-source-paths",help="orchestration-approved JSON list of new source files; tracked sources are always included")
    parser.add_argument("--producer-repository",default="local-orchestration",help="local orchestration label; CI uses its actual GITHUB_REPOSITORY")
    args = parser.parse_args()
    if args.matrix:
        return build_matrix(args)
    production = dependency_graph(args.package, args.features, "normal,build")
    tests = dependency_graph(args.package, args.features, "normal,build,dev")
    errors = violations(production, SQLITE)
    print("production (normal/build):", ", ".join(sorted(production)))
    print("test-only graph additions:", ", ".join(sorted(tests - production)))
    for error in errors:
        print(error)
    return int(bool(errors))


if __name__ == "__main__":
    try:status=main()
    finally:
        from tools.ci import run_provider_e2e as workflow
        workflow.hold_pending_child_custody()
    raise SystemExit(status)
