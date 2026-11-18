#!/usr/bin/env python3
"""Execute hash-bound Helix host consumers; inventory never proves execution."""
from __future__ import annotations

import argparse
import copy
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys
import time
import uuid

ROOT = Path(__file__).resolve().parents[2]
FIXTURES = ROOT / "adapters/integration/tests/fixtures"
HOSTS = ("rust", "native", "mcp-stdio", "mcp-oneshot")
MODES = ("service", "embedded")
TARGETS = ("linux-x64-gnu", "linux-arm64-gnu", "darwin-arm64", "win32-x64")
REQUIRED_OPERATIONS = set("""memory.write memory.retrieve memory.forget batch.ingest
knowledge.put_source knowledge.put_document knowledge.put_chunk knowledge.put_entity knowledge.put_relationship knowledge.get_entity
graph.neighbors identity.resolve_entity identity.resolve_relationship
ontology.put ontology.put_class ontology.put_property ontology.put_axiom ontology.get ontology.validate
taxonomy.put_scheme taxonomy.get_scheme taxonomy.put_concept taxonomy.put_relation taxonomy.list
beliefs.put beliefs.get beliefs.retract beliefs.stale beliefs.contradictions hierarchy.path retrieval.recall
procedures.put procedures.list procedures.increment_success proposals.submit proposals.list proposals.approve proposals.reject
outcomes.report outcomes.list provenance.for provenance.evidence provenance.by_source provenance.by_scope provenance.attach
consolidation.run observability.diagnostics community.overview community.member_index community.of community.scope_counts
scope.inventory archive.export archive.dry_run archive.apply maintenance.plan maintenance.apply maintenance.candidates maintenance.health
code.register code.bind code.status code.refresh code.scan code.source code.reset_preview code.reset_apply code.reset_resume code.backfill""".split())
REQUIRED_ENV = {"CARGO_BUILD_JOBS": "2", "CARGO_PROFILE_DEV_DEBUG": "0", "CARGO_PROFILE_TEST_DEBUG": "0"}
_PENDING_CHILDREN={}
FAULT_SCENARIOS = ("memory", "knowledge", "batch", "earlier-definite", "definite-continue", "cleanup-wrapped")


def fault_scenarios(host):
    # One-shot has real pack/code commands, but no direct memory/knowledge API.
    return ("pack", "code", "cleanup-wrapped") if host == "mcp-oneshot" else FAULT_SCENARIOS


class ProofError(ValueError):
    """A missing, contradictory or stale proof is never a passing skip."""


def sha(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def canonical_uuid(value) -> bool:
    try:
        return isinstance(value, str) and str(uuid.UUID(value)) == value
    except (ValueError, TypeError, AttributeError):
        return False


def scope_tuple(value: dict) -> tuple:
    keys = ("tenant", "subject", "workspace", "session", "environment")
    if not isinstance(value, dict) or set(value) - set(keys) or not isinstance(value.get("tenant"), str) or not value["tenant"]:
        raise ProofError("invalid complete Scope binding")
    if any(value.get(key) is not None and not isinstance(value[key], str) for key in keys):
        raise ProofError("invalid optional Scope dimension")
    # JSON absent/null are the same domain Option::None. Empty remains Some("").
    return tuple(value.get(key) for key in keys)


def catalogue() -> list[dict]:
    operations = []
    for filename in ("provider_operations.json", "provider_code_operations.json"):
        operations.extend(json.loads((FIXTURES / filename).read_text())["operations"])
    validate_catalogue(operations)
    return operations


def validate_catalogue(operations: list[dict]) -> None:
    identifiers = [row.get("operation") for row in operations]
    if len(set(identifiers)) != len(identifiers) or set(identifiers) != REQUIRED_OPERATIONS:
        raise ProofError("missing, extra or duplicate operation in canonical inventory")
    for row in operations:
        if not row.get("capability") or not row.get("expected_error_reason_required"):
            raise ProofError("operation lacks capability or explicit error-reason obligation")
        if not row.get("native", {}).get("method") or not row.get("mcp_tool"):
            raise ProofError("required native/stdio operation route omitted")
        one = row.get("oneshot", {})
        if not isinstance(one.get("applicable"), bool) or not one.get("reason"):
            raise ProofError("one-shot applicability requires a concrete existing-contract reason")
        if one["applicable"] and not one.get("command"):
            raise ProofError("applicable one-shot command omitted")
        rust = row.get("rust", {"applicable": True})
        if not rust.get("applicable", True):
            if row["operation"] != "code.backfill" or not rust.get("source") or "No EngramProvider" not in rust.get("reason", ""):
                raise ProofError("unapproved Rust N/A or missing source-backed contract reason")
        if not row.get("assertions_required"):
            raise ProofError("operation lacks actual assertion obligations")


def inventory(operations=None) -> list[dict]:
    rows = []
    for operation in operations if operations is not None else catalogue():
        for host in HOSTS:
            for mode in MODES:
                for target in TARGETS:
                    applicable = True
                    reason = None
                    if host == "mcp-oneshot":
                        applicable = operation["oneshot"]["applicable"]
                        reason = operation["oneshot"]["reason"]
                    elif host == "rust" and not operation.get("rust", {}).get("applicable", True):
                        applicable = False
                        reason = operation["rust"]["reason"]
                    rows.append({"operation": operation["operation"], "capability": operation["capability"], "provider": "helixdb", "host": host, "mode": mode, "target": target, "required": applicable, "na_reason": reason if not applicable else None, "status": "NOT_RUN" if applicable else "N/A", "error_reason_required": True})
    return rows


def local_target():
    return {("Linux", "x86_64"): "linux-x64-gnu", ("Linux", "aarch64"): "linux-arm64-gnu", ("Darwin", "arm64"): "darwin-arm64", ("Windows", "AMD64"): "win32-x64"}.get((platform.system(), platform.machine()))


def snapshot_digest(files):
    return hashlib.sha256(json.dumps(files,sort_keys=True,separators=(',',':')).encode()).hexdigest()


def source_snapshot(approved_new_source_paths=(), *, root=None):
    """All tracked source bytes/types plus explicitly authorized new source files."""
    if root is None:root=ROOT
    tracked=set(subprocess.check_output(['git','ls-files','-z'],cwd=root).decode().split('\0'))-{''}
    files={}
    approved=tracked|set(approved_new_source_paths)
    # Validate the complete allowlist before resolving even an approved link.
    for name in approved:
        relative=Path(name)
        if not relative.parts or relative.is_absolute() or '..' in relative.parts or (name not in tracked and relative.parts[0] in {'runs','target','node_modules','.context','.git','.engram-factory'}):
            raise ProofError('runtime/evidence path cannot be an immutable source input')
    def checked_source(name, chain=()):
        # Inspect only approved names. Never follow a link into private/untracked
        # data just to discover that its referent was not a source input.
        if name not in approved or name in chain or len(chain)>=40:
            raise ProofError('source symlink referent is unapproved or cyclic')
        relative=Path(name)
        if relative.is_absolute() or '..' in relative.parts:
            raise ProofError('source symlink referent escapes the approved snapshot')
        for parent in relative.parents:
            if parent!=Path('.') and (root/parent).is_symlink():
                raise ProofError('source snapshot cannot traverse a directory symlink')
        path=root/relative
        if path.is_symlink():
            target=os.readlink(path)
            if os.path.isabs(target):
                raise ProofError('absolute source symlink referent is not approved')
            referent=os.path.normpath(str(relative.parent/target))
            checked_source(referent, (*chain,name))
            return {'kind':'symlink','sha256':hashlib.sha256(os.fsencode(target)).hexdigest()}
        if path.is_file():return {'kind':'file','sha256':sha(path)}
        raise ProofError('source snapshot contains an unavailable tracked input')
    for name in sorted(approved):
        # Tracked framing Markdown is immutable repository input too. Runtime
        # roots remain forbidden for NEW paths; never scan ignored/untracked
        # fixtures, credentials, caches or binaries into the source snapshot.
        files[name]=checked_source(name)
    return {'files':files,'approved_new_source_paths':sorted(set(approved_new_source_paths)-tracked),'snapshot_sha256':snapshot_digest(files)}


def load_expected_build_record(path, pinned_sha, bundle_root):
    """Content integrity under an external orchestration pin, not author authentication."""
    path=Path(path)
    if not re_full_sha(pinned_sha) or not path.is_absolute() or path.is_symlink() or not path.is_file() or path.resolve().is_relative_to(Path(bundle_root).resolve()):
        raise ProofError('independently pinned build record must be outside the artifact bundle')
    if sha(path)!=pinned_sha:raise ProofError('independent build-record pin differs')
    record=json.loads(path.read_text())
    if record.get('schema')!='engram.provider-build-record.v1':raise ProofError('unsupported trusted build record')
    producer=record.get('producer',{})
    head=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip()
    if producer.get('commit')!=head or not producer.get('repository') or not producer.get('run_id'):
        raise ProofError('trusted producer repository/run/commit identity missing or stale')
    expected=record.get('source',{})
    actual=source_snapshot(expected.get('approved_new_source_paths',[]))
    if expected!=actual or record.get('lock_sha256')!=sha(ROOT/'Cargo.lock'):
        raise ProofError('actual complete source snapshot or lock differs from pinned build')
    if not record.get('artifacts'):raise ProofError('trusted producer record contains no actual produced artifacts')
    return record


def re_full_sha(value):
    return isinstance(value,str) and len(value)==64 and all(char in '0123456789abcdef' for char in value)


def validate_host_scope(scope,host):
    scope_tuple(scope)
    if host.startswith('mcp-'):
        if any(scope.get(key) is not None for key in ('subject','session','environment')) or not isinstance(scope.get('workspace'),str):
            raise ProofError('MCP launch cannot represent this exact None/empty full-scope tuple')


def trusted_artifact_entry(record,trusted):
    matches=[row for row in trusted.get('artifacts',[]) if all(row.get(key)==record.get(key) for key in ('host','mode','target','profile','instrumented','sha256'))]
    if len(matches)!=1:raise ProofError('artifact bytes/profile were not independently produced for this host')
    return matches[0]


def bound_file(record,path_key,hash_key,bundle_root,expected_hash):
    path=Path(record.get(path_key,''))
    root=Path(bundle_root).resolve()
    if not path.is_absolute():path=root/path
    if not path.is_file() or any(parent.is_symlink() for parent in (path,*path.parents)) or not path.resolve().is_relative_to(root):
        raise ProofError('delivered build/plan file escapes its artifact collection')
    if not re_full_sha(expected_hash) or record.get(hash_key)!=expected_hash or sha(path)!=expected_hash:
        raise ProofError('delivered build/plan file differs from independent template hash')
    return path


def validate_artifact(record: dict, *, host: str, mode: str, target: str, source: str, lock: str, trusted: dict, bundle_root=None) -> Path:
    expected = {"provider": "helixdb", "host": host, "mode": mode, "target": target, "source_revision": source, "lock_sha256": lock}
    if any(record.get(key) != value for key, value in expected.items()):
        raise ProofError("wrong provider/host/mode/target or stale source/lock artifact")
    path = Path(record.get("path", ""))
    if not path.is_absolute() and bundle_root is not None:path=Path(bundle_root)/path
    if not path.is_absolute() or path.is_symlink() or not path.is_file():
        raise ProofError("missing or symlinked required consumer artifact")
    if sha(path) != record.get("sha256"):
        raise ProofError("stale consumer artifact hash")
    if bundle_root is not None and (not path.resolve().is_relative_to(Path(bundle_root).resolve()) or any(parent.is_symlink() for parent in path.parents)):
        raise ProofError('produced artifact escapes its delivered collection')
    features = set(record.get("features", []))
    if "helixdb-" + mode not in features or features & {"sqlite", "pgvector", "helixdb-" + ("service" if mode == "embedded" else "embedded")}:
        raise ProofError("wrong or unisolated provider feature subset")
    if record.get("instrumented") != ("test-controls" in features):
        raise ProofError("production/instrumented feature identity contradiction")
    if record.get("profile") not in {"debug", "release"}:
        raise ProofError("missing actual artifact build profile")
    forbidden = {"engram-conformance", "engram-store-sqlite", "engram-backend-sqlite", "engram-store-pgvector", "engram-backend-pgvector"}
    if not record.get("production_compiler_packages") or forbidden & set(record["production_compiler_packages"]):
        raise ProofError("missing or contaminated actual production compiler closure")
    if not record.get("link_inputs"):
        raise ProofError("missing actual native link input evidence")
    produced=trusted_artifact_entry(record,trusted)
    package='engram-node' if host=='native' else 'engram-provider-e2e' if host=='rust' else 'engram-mcp'
    kind='cdylib' if host=='native' else 'bin'
    if produced.get('package')!=package or produced.get('kind')!=kind or set(produced.get('features',[]))!=features:
        raise ProofError('trusted compiler target/kind/features differ from actual required host')
    root=Path(bundle_root or record.get('_bundle_root','')).resolve()
    compiler_path=bound_file(record,'compiler_record','compiler_record_sha256',root,produced.get('compiler_record_sha256'))
    link_path=bound_file(record,'link_record','link_record_sha256',root,produced.get('link_record_sha256'))
    compiler=json.loads(compiler_path.read_text());links=json.loads(link_path.read_text())
    if compiler.get('exit')!=0 or 'build' not in compiler.get('command',[]) or compiler.get('package')!=package or compiler.get('artifact_sha256')!=sha(path) or compiler.get('kind')!=kind or compiler.get('source_snapshot_sha256')!=trusted['source']['snapshot_sha256']:
        raise ProofError('actual Cargo producer record does not bind these bytes/source/target')
    units=compiler.get('units',[])
    own=[unit for unit in units if unit.get('reason')=='compiler-artifact' and unit.get('package_id','').split('#')[-1].split('@')[0]==package and kind in unit.get('target',{}).get('crate_types',[])]
    packages={unit.get('package_id','').split('#')[-1].split('@')[0] for unit in units if unit.get('reason')=='compiler-artifact'}
    target_sources=compiler.get('target_sources',[])
    if not own or not any(set(unit.get('features',[]))==features for unit in own) or packages!=set(record['production_compiler_packages']) or packages&forbidden or not target_sources or any(name not in trusted['source']['files'] for name in target_sources) or not links.get('inputs') or links.get('inputs')!=record['link_inputs']:
        raise ProofError('missing actual Cargo target or independently bound linker/compiler inputs')
    return path


def preflight_plan(artifact,trusted,bundle_root,host,mode,scenario=None):
    produced=trusted_artifact_entry(artifact,trusted)
    selected=None
    if artifact.get('request_plan') is not None:
        path=bound_file(artifact,'request_plan','request_plan_sha256',bundle_root,produced.get('request_plan_sha256'))
        if scenario is None:selected=(path,produced['request_plan_sha256'])
    declared=artifact.get('fault_plans',{})
    if not isinstance(declared,dict):raise ProofError('invalid fault plan inventory')
    for name,path in declared.items():
        digest=produced.get('fault_plan_sha256',{}).get(name)
        bound=bound_file({'plan':path,'hash':artifact.get('fault_plan_sha256',{}).get(name)},'plan','hash',bundle_root,digest)
        if name==scenario:selected=(bound,digest)
    if selected is None:raise ProofError('required hash-bound operation/fault template missing')
    path,digest=selected;data=path.read_bytes()
    if hashlib.sha256(data).hexdigest()!=digest:raise ProofError('template changed while reading its verified bytes')
    packet=json.loads(data)
    validate_host_scope(packet.get('scope'),host)
    config=packet.get('config',{})
    if config.get('storage_provider')!='helixdb' or config.get('helixdb',{}).get('mode')!=mode:
        raise ProofError('template provider/mode differs from selected host')
    if config.get('scope') is not None and scope_tuple(config['scope'])!=scope_tuple(packet['scope']):
        raise ProofError('template configured scope differs from its call binding')
    if mode=='service' and (config.get('enable_vector') is not True or config.get('embedding_provider',{}).get('provider_type')!='none'):
        raise ProofError('service template requires current vector-enabled/provider-none admission')
    if scenario is not None and (scenario not in fault_scenarios(host) or not packet.get('fixture')):
        raise ProofError('required actual fault initializer/scenario missing')
    return packet,digest


def validate_receipts(expected: list[dict], receipts: list[dict], binding: dict) -> None:
    if binding.get('provider')=='helixdb' and (not isinstance(binding.get('store'),str) or not binding['store']):
        raise ProofError('independent configured logical store is missing from call expectations')
    capabilities = {row["operation"]: row["capability"] for row in catalogue()}
    wanted = {call["id"]: call for call in expected}
    if len(wanted) != len(expected) or not wanted:
        raise ProofError("empty or duplicate call IDs; operation names are not call identity")
    seen = set()
    for receipt in receipts:
        identifier = receipt.get("call_id")
        if identifier not in wanted or identifier in seen:
            raise ProofError("unexpected or duplicate independent call receipt")
        seen.add(identifier)
        call = wanted[identifier]
        if receipt.get("schema") != "engram.provider-operation.v1" or receipt.get("operation") != call["operation"]:
            raise ProofError("wrong operation receipt")
        if receipt.get("capability") != capabilities.get(call["operation"]):
            raise ProofError("operation capability omitted or mismatched")
        if any((scope_tuple(receipt.get(key)) != scope_tuple(value)) if key == "scope" else receipt.get(key) != value for key, value in binding.items()):
            raise ProofError("receipt source/artifact/host/full-scope binding mismatch")
        if receipt.get("status") != "PASS" or not receipt.get("assertions") or not receipt.get("result_sha256"):
            raise ProofError("failed, skipped, empty or unasserted required call")
        if receipt.get("outcome") != call.get("expect", {}).get("status"):
            raise ProofError("actual outcome does not match the declared assertion")
        if receipt.get("outcome") != "ok" and not receipt.get("reason"):
            raise ProofError("missing truthful non-success capability/error reason")
    if seen != set(wanted):
        raise ProofError("required operation/call omitted")


def validate_trace(events: list[dict], binding: dict, unknown: str | None = None) -> None:
    """Validate typed producers, not counters or uncorrelated textual markers."""
    sends, acknowledged = {}, set()
    for event in events:
        if event.get("phase") not in {"BeforeSend", "AfterAck"}:
            raise ProofError("unknown transport evidence phase")
        for field in ("actor", "call", "session", "backend", "mode", "store", "scope", "item", "step", "commitId"):
            if field not in event or event[field] is None or (field != "scope" and event[field] == ""):
                raise ProofError("unbound effect frame")
        for field in ("backend", "mode", "store", "scope"):
            if (scope_tuple(event[field]) != scope_tuple(binding[field])) if field == "scope" else event[field] != binding[field]:
                raise ProofError("effect scope/store/backend/mode mismatch")
        if not canonical_uuid(event["commitId"]):
            raise ProofError("noncanonical actual commit UUID")
        key = (event["call"], event["step"], event["commitId"])
        if event["phase"] == "BeforeSend":
            if key in sends:
                raise ProofError("replayed mutation/call frame")
            sends[key] = event
        else:
            if key not in sends or key in acknowledged or any(sends[key][f] != event[f] for f in ("actor", "call", "session", "backend", "mode", "store", "scope", "item", "step", "commitId")):
                raise ProofError("ack without the same actual prior send/frame")
            acknowledged.add(key)
    if unknown is not None:
        if not canonical_uuid(unknown) or not any(key[2] == unknown for key in sends) or any(key[2] == unknown for key in acknowledged):
            raise ProofError("uncertain original UUID lacks its actual send or was falsely acknowledged")
        order = [event["commitId"] for event in events if event["phase"] == "BeforeSend"]
        first = order.index(unknown)
        if any(identifier != unknown for identifier in order[first + 1:]):
            raise ProofError("later mutation after uncertainty")


def audit_fault(snapshot: dict, calls: list[dict], receipts: list[dict], binding: dict, scenario: str, exit_code: int, gateway_rows=()) -> dict:
    """Judge actual typed producers; an expected process failure alone proves nothing."""
    if snapshot.get("recorderFailed") or not snapshot.get("binding"):
        raise ProofError("missing or failed actual fixture recorder")
    actual = snapshot["binding"]
    effect_binding = {"backend":"helixdb", "mode":binding["mode"], "store":binding.get("store"), "scope":binding["scope"]}
    if not isinstance(binding.get("store"),str) or not binding['store']:raise ProofError("independent configured logical store required")
    if not actual.get("store") or any(scope_tuple(actual[k]) != scope_tuple(effect_binding[k]) if k == "scope" else actual.get(k) != effect_binding[k] for k in effect_binding):
        raise ProofError("fixture snapshot actual store/full-scope binding mismatch")
    validate_receipts(calls, receipts, binding)
    events = snapshot.get("events", [])
    observations = snapshot.get("observations", [])
    unknowns = [row for row in observations if row.get("outcome") == "Unknown"]
    if scenario == "definite-continue":
        if unknowns or exit_code != 0 or not any(row.get("outcome") == "DefiniteFailure" for row in observations):
            raise ProofError("definite contrast must be actually observed without uncertainty")
        if not any(row.get("phase") == "AfterAck" for row in events):
            raise ProofError("later definite-failure continuation never acknowledged")
        validate_trace(events, effect_binding)
        return {"logical_outcome":"DEFINITE_FAILURE_WITH_CONTINUATION", "canonical_reconciliation":"NOT_APPLICABLE"}
    if len({row.get("operationId") for row in unknowns}) != 1:
        raise ProofError("missing or ambiguous original typed unknown UUID")
    original = unknowns[0].get("operationId")
    if not canonical_uuid(original) or any(row.get("commitId") != original or not row.get("step") for row in unknowns):
        raise ProofError("unknown does not bind actual original commit/step")
    validate_trace(events, effect_binding, original)
    correlated=[row for row in receipts if any(effect.get('outcome')=='Unknown' and effect.get('operationId')==original for effect in row.get('observations',[]))]
    if not correlated:raise ProofError("unknown native frame is not tied to an actual host call receipt")
    expected_operation={'memory':{'memory.write'},'knowledge':{'knowledge.put_entity','knowledge.put_relationship'},'batch':{'batch.ingest'}}.get(scenario)
    if expected_operation and not any(row['operation'] in expected_operation for row in correlated):raise ProofError("fault scenario exercised a different actual operation")
    if binding['host']=='mcp-oneshot' and any(not call.get('oneshot_argv') or call['oneshot_argv'][0] not in {'pack','code','backfill-embeddings'} for call in calls):
        raise ProofError("one-shot post-error assertion N/A is limited to actual pack/code command lifecycles")
    blocked = [row for row in observations if row.get("outcome") == "BlockedByPriorUnknown"]
    if (binding["host"] != "mcp-oneshot" and not blocked) or any(row.get("operationId") != original or row.get("commitId") is not None or row.get("step") is not None for row in blocked):
        raise ProofError("missing exact-ID blocked admission without replacement commit")
    for row in observations:
        if any(scope_tuple(row.get(key)) != scope_tuple(value) if key == "scope" else row.get(key) != value for key, value in effect_binding.items()):
            raise ProofError("foreign terminal observation")
    native = [row for row in snapshot.get("nativeReceipts", []) if row.get("commitId") == original and row.get("canonicalReceiptMatches") is True]
    gateway = [row for row in gateway_rows if row.get("canonical_receipt_correlated") is True and row.get("event", {}).get("commitId") == original
               and row.get("binding", {}).get("invocation") == binding["invocation"] and row.get("binding", {}).get("host") == binding["host"]
               and row.get("binding", {}).get("artifactSha256") == binding["artifact_sha256"] and row.get("client_after_ack") is False]
    if not native and not gateway:
        raise ProofError("no actual landed original receipt correlation; counters are insufficient")
    if scenario == "cleanup-wrapped" and original not in snapshot.get("cleanupGuardRefusals", []):
        raise ProofError("cleanup wrapper lacks actual owned-pending close-guard refusal")
    if scenario == "earlier-definite" and not any(row.get("outcome") == "DefiniteFailure" for row in observations):
        raise ProofError("earlier definite failure was not actually observed")
    if not any(row.get("phase") == "AfterAck" and row.get("commitId") != original for row in events):
        raise ProofError("prior acknowledged success preservation was not exercised")
    public = json.dumps(receipts)
    if original not in public:
        raise ProofError("public direct/batch error projection lost the original actual UUID")
    for call in calls:
        for secret in call.get("expect", {}).get("redact", []):
            if secret in public:
                raise ProofError("private record or credential leaked in public projection")
    blocked_assertion = {"status":"N/A","reason":"Existing pack/code one-shot command aborts on Unknown and has no same-process post-error API; a new process would reopen/replay the unresolved namespace","source":["mcp/engram-mcp/src/main.rs","mcp/engram-mcp/src/pack.rs"]} if binding["host"] == "mcp-oneshot" else {"status":"PASS","reason":"Actual typed BlockedByPriorUnknown without replacement commit or send"}
    return {"original_operation_id":original,"logical_outcome":"UNKNOWN_RETAINED", "canonical_reconciliation":"NOT_PERFORMED","blocked_admission":blocked_assertion,
            "landed_evidence":"CANONICAL_RESPONSE_CORRELATION", "cleanup_origin":"FIXTURE_OWNED_GUARD_REFUSAL" if scenario == "cleanup-wrapped" else "NOT_ASSERTED",
            "native_close_attestation":"NOT_OBSERVED", "consumer_exit":exit_code}


def bind_owned_packet(packet: dict, output: Path, host: str, mode: str, invocation: str, provider="helixdb") -> dict:
    """Replace all mutable store locations; no input plan may target a user's store."""
    value = copy.deepcopy(packet)
    validate_host_scope(value.get('scope'),host)
    original_scope = scope_tuple(value.get("scope"))
    config = value.get("config", {})
    if provider not in {"helixdb","sqlite","pgvector"} or config.get("storage_provider") != provider or (provider=="helixdb" and config.get("helixdb", {}).get("mode") != mode):
        raise ProofError("request plan selects a different provider/mode")
    owned = output / host / str(uuid.uuid4())
    owned.mkdir(parents=True, mode=0o700)
    fresh_scope = {**value["scope"],"tenant":"fixture-"+str(uuid.uuid4())}
    def rebind(item):
        if isinstance(item,dict):
            if set(item)=={"$fixture"}:
                relative=Path(item["$fixture"])
                if relative.is_absolute() or '..' in relative.parts: raise ProofError("fixture reference escapes owned root")
                return str(owned / relative)
            if "tenant" in item and set(item).issubset({"tenant","subject","workspace","session","environment"}) and scope_tuple(item)==original_scope:
                return copy.deepcopy(fresh_scope)
            return {key:rebind(child) for key,child in item.items()}
        if isinstance(item,list):return [rebind(child) for child in item]
        if isinstance(item,str) and item.startswith('{'):
            try:
                parsed=json.loads(item)
                if isinstance(parsed,dict) and "tenant" in parsed and scope_tuple(parsed)==original_scope:return json.dumps(fresh_scope,separators=(',',':'))
            except (ValueError,TypeError):pass
        return item
    value=rebind(value)
    config=value["config"]
    for name,content in value.pop("seed_files",{}).items():
        relative=Path(name)
        if relative.is_absolute() or '..' in relative.parts or not isinstance(content,str):raise ProofError("invalid owned fixture file")
        path=owned/relative;path.parent.mkdir(parents=True,exist_ok=True)
        with path.open('x') as file:file.write(content)
    config.update(storage_path=str(owned / "storage"), trusted_root=str(owned), scope=value["scope"])
    if provider=="helixdb":config["helixdb"].update(logical_store="fixture-" + str(uuid.uuid4()), path=str(owned / "native"))
    if provider=="helixdb" and mode == "service" and (config.get("enable_vector") is not True or config.get("embedding_provider", {}).get("provider_type") != "none"):
        raise ProofError("current service positives require vector-enabled/provider-none admission")
    value["config"] = config
    if value.get("fixture") is not None:
        value["fixture"].update(ownedRoot=str(owned), sourceRoot=str(ROOT), invocation=invocation)
    return value


def self_test_omissions() -> int:
    original = catalogue()
    rejected = []
    for name, mutation in (
        ("operation", lambda data: data.pop()),
        ("capability", lambda data: data[0].pop("capability")),
        ("error-reason", lambda data: data[0].pop("expected_error_reason_required")),
        ("native-route", lambda data: data[0]["native"].pop("method")),
        ("stdio-route", lambda data: data[0].pop("mcp_tool")),
        ("one-shot-reason", lambda data: data[0]["oneshot"].pop("reason")),
    ):
        data = copy.deepcopy(original)
        mutation(data)
        try:
            validate_catalogue(data)
        except ProofError:
            rejected.append(name)
        else:
            raise ProofError("omission accepted: " + name)
    print(json.dumps({"control_status": "PASS", "rejected": rejected, "native_workflow_status": "NOT_RUN"}))
    return len(rejected)


def helper_module():
    spec = importlib.util.spec_from_file_location("owned_helix_fixture", ROOT / "tools/ci/helixdb_server.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def run_owned(argv: list[str], output: Path, packet: dict, stdin: str | None = None) -> tuple[int, str]:
    """Retain exact Linux Popen/pidfd custody; deadlines never kill or replay."""
    if platform.system() != "Linux":
        raise ProofError("BLOCKED: actual equivalent native ownership/instrumentation backend required")
    if _PENDING_CHILDREN:raise ProofError('original owned consumer remains pending; no replacement or subsequent work')
    helper = helper_module()
    helper.require_pidfd_capability()
    executable = Path(shutil.which(argv[0]) or argv[0]).resolve()
    argv = [str(executable), *argv[1:]]
    before = sha(executable)
    start = time.monotonic()
    child = subprocess.Popen(argv, cwd=ROOT, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, env=os.environ.copy())
    pending=output/'pending-process.json'
    state={'processes':[],'invocation':packet['invocation'],'scope':packet['scope'],'artifact_sha256':packet['artifact_sha256'],'source_revision':packet['source_revision'],'lock_sha256':packet['lock_sha256'],'expected_executable_sha256':before}
    custody={'process':child,'output':output,'state':state,'state_path':pending,'started':start}
    _PENDING_CHILDREN[child.pid]=custody
    try:
        identity=helper.record_child(state,pending,child,'consumer',str(executable),argv,custody=custody)
    except (OSError,RuntimeError,ValueError) as error:
        state['acquisition_failure']=type(error).__name__
        pending.write_text(json.dumps(state,indent=2)+'\n')
        with (output/'operations.jsonl').open('a') as log:log.write(json.dumps({'event':'exec-acquisition-failed','custody':state,'still_owned':True,'no_replay':True})+'\n')
        if child.poll() is not None:drain_pending_children(1)
        raise ProofError('owned consumer exec acquisition failed; pending custody retained, dependent cleanup blocked') from error
    with (output / "operations.jsonl").open("a") as log:
        log.write(json.dumps({"event": "before-exec", "identity": identity, "executable_sha256": before, "artifact_sha256": packet["artifact_sha256"], "invocation": packet["invocation"], "scope": packet["scope"], "source_revision": packet["source_revision"], "lock_sha256": packet["lock_sha256"]}) + "\n")
    try:
        stdout, stderr = child.communicate(stdin, timeout=1200)
    except subprocess.TimeoutExpired:
        state.update(identity=identity,reason='deadline observed; no signal, replacement or replay; exact owned custody retained')
        pending.write_text(json.dumps(state, indent=2) + "\n")
        raise ProofError("BLOCKED: original owned process remains pending; preserve data/identity")
    after = sha(executable)
    with (output / "raw.log").open("a") as raw:
        raw.write("$ " + " ".join(argv) + "\n" + stdout + stderr)
    with (output / "operations.jsonl").open("a") as log:
        log.write(json.dumps({"event": "reaped", "identity": identity, "exit": child.returncode, "seconds": time.monotonic() - start, "executable_sha256_after": after, "unchanged": before == after}) + "\n")
    if custody.get('pidfd') is not None:os.close(custody['pidfd'])
    del _PENDING_CHILDREN[child.pid]
    pending.unlink()
    if before != after:
        raise ProofError("executed consumer changed during invocation")
    return child.returncode, stdout


def drain_pending_children(observation_seconds=1):
    """Observe original owners only; never signal, replay or replace a child."""
    for pid,custody in list(_PENDING_CHILDREN.items()):
        child=custody['process'];output=custody['output']
        try:stdout,stderr=child.communicate(timeout=observation_seconds)
        except subprocess.TimeoutExpired:continue
        with (output/'raw.log').open('a') as raw:raw.write(stdout+stderr)
        with (output/'operations.jsonl').open('a') as log:log.write(json.dumps({'event':'original-pending-child-reaped','custody':custody['state'],'exit':child.returncode,'logical_outcome':'UNPROVEN_RETAINED','native_close_attestation':'NOT_OBSERVED'})+'\n')
        if custody.get('pidfd') is not None:os.close(custody['pidfd'])
        custody['state_path'].unlink(missing_ok=True)
        del _PENDING_CHILDREN[pid]
    return not _PENDING_CHILDREN


def hold_pending_child_custody():
    if _PENDING_CHILDREN:print(json.dumps({'status':'BLOCKED','pending_owned_children':list(_PENDING_CHILDREN),'action':'retaining original Popen/pidfd; exact guarded closure required, no replacement/replay'}),file=sys.stderr,flush=True)
    while _PENDING_CHILDREN:drain_pending_children(1)


def consumer_closure_confirmed(output):
    return not _PENDING_CHILDREN and not (Path(output)/'pending-process.json').exists()


def load_json_lines(path):
    return [json.loads(line) for line in path.read_text().splitlines()] if path.is_file() else []


def execute(args) -> int:
    source = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    lock = sha(ROOT / "Cargo.lock")
    records = []
    manifest_path = Path(args.artifacts)
    if not manifest_path.is_file():
        raise ProofError("BLOCKED: required current artifact/fixture manifest is missing")
    manifest = json.loads(manifest_path.read_text())
    if manifest.get("schema") != "engram.provider-artifacts.v1":
        raise ProofError("BLOCKED: unsupported artifact manifest schema")
    if not args.expected_build_record or not args.expected_build_record_sha256:
        raise ProofError('independent orchestration build record and pin required')
    bundle_root=manifest_path.resolve().parent
    trusted=load_expected_build_record(args.expected_build_record,args.expected_build_record_sha256,bundle_root)
    targets = (args.target,) if args.target else TARGETS if args.all_supported_targets else (local_target(),)
    hosts = (args.host,) if args.host else HOSTS
    prepared={}
    phases=('operations','faults') if args.phase=='all' else (args.phase,)
    # Reject stale/foreign bytes and every selected plan before creating any
    # mutable namespace or acquiring a helper/consumer process.
    for target in targets:
        for host in hosts:
            for phase in phases:
                selected=[row for row in manifest.get('artifacts',[]) if all(row.get(key)==value for key,value in {'host':host,'mode':args.mode,'target':target,'instrumented':phase=='faults'}.items())]
                if len(selected)!=1:raise ProofError('required host/mode/target/profile missing or ambiguous')
                artifact=selected[0]
                path=validate_artifact(artifact,host=host,mode=args.mode,target=target,source=source,lock=lock,trusted=trusted,bundle_root=bundle_root)
                scenarios=((args.scenario,) if args.scenario else fault_scenarios(host)) if phase=='faults' else (None,)
                for scenario in scenarios:
                    packet,digest=preflight_plan(artifact,trusted,bundle_root,host,args.mode,scenario)
                    if phase=='operations':
                        expected={row['operation'] for row in inventory() if row['required'] and row['host']==host and row['mode']==args.mode and row['target']==target}
                        if expected-{call.get('operation') for call in packet.get('calls',[])}:raise ProofError('operation template omits an applicable required call')
                    prepared[target,host,phase,scenario]=(artifact,path,packet,digest)
    destination = Path(args.output).resolve()
    if not destination.is_relative_to(ROOT / "runs") or any(p.is_symlink() for p in (destination, *destination.parents)):
        raise ProofError("owned evidence must remain under nonsymlink checkout runs")
    invocation = str(uuid.uuid4())
    output = destination / invocation
    output.mkdir(parents=True, mode=0o700)
    helper_root = output / "helper"
    helper_started = False
    old_credential = None
    credential_name = None
    helper = ROOT / "tools/ci/helixdb_server.py"
    try:
        for target in targets:
            for host in hosts:
                phases = ("operations", "faults") if args.phase == "all" else (args.phase,)
                for phase in phases:
                    scenarios = ((args.scenario,) if args.scenario else fault_scenarios(host)) if phase == "faults" else (None,)
                    for scenario in scenarios:
                        result = {"host":host,"target":target,"mode":args.mode,"provider":args.provider,"phase":phase,"scenario":scenario,"status":"BLOCKED"}
                        try:
                            if target != local_target() or platform.system() != "Linux":
                                raise ProofError("native ownership/instrumentation implementation unavailable on this target; no cross-compile PASS")
                            artifact,path,template,template_digest=prepared[target,host,phase,scenario]
                            packet = bind_owned_packet(template, output, host, args.mode, invocation)
                            packet.update({"invocation":invocation,"source_revision":source,"lock_sha256":lock,"artifact_sha256":artifact["sha256"],"host":host,"target":target,"mode":args.mode,"provider":"helixdb"})
                            if phase == "operations":
                                required = {row["operation"] for row in inventory() if row["required"] and row["host"] == host and row["mode"] == args.mode and row["target"] == target}
                                if required - {call.get("operation") for call in packet.get("calls", [])}:
                                    raise ProofError("required operation omitted from actual request plan")
                            elif scenario not in fault_scenarios(host) or not packet.get("fixture"):
                                raise ProofError("required fault scenario or actual fixture initializer missing")
                            if args.mode == "service":
                                if not helper_started:
                                    provision = subprocess.run(["/usr/bin/python3",str(helper),"provision","--output",str(helper_root)],cwd=ROOT,capture_output=True,text=True)
                                    with (output / "raw.log").open("a") as raw: raw.write("owned helper provision exit="+str(provision.returncode)+"\n"+provision.stderr)
                                    if provision.returncode:
                                        raise ProofError("owned helper provision failed; preserve acquisition state")
                                    helper_started = True
                                    info = json.loads(provision.stdout.splitlines()[-1])
                                    credential_name = info["HELIX_TEST_CREDENTIAL_ENV"]
                                    old_credential = os.environ.get(credential_name)
                                    os.environ[credential_name] = Path(info["credential_file"]).read_text().strip()
                                state = json.loads((helper_root / "state.json").read_text())
                                packet["config"]["helixdb"].update(endpoint=state["endpoint"],credential_env=state["credential_env"],path=None)
                            private = output / (host + "-" + str(uuid.uuid4()) + "-request.json")
                            private.write_text(json.dumps(packet)+"\n"); private.chmod(0o600)
                            if phase == "faults" and args.mode == "service" and packet.get("gateway_fault"):
                                declaration = {"action":packet["gateway_fault"],"binding":{"host":host,"invocation":invocation,"sourceRevision":source,"artifactSha256":artifact["sha256"],"actual":{"backend":"helixdb","mode":args.mode,"store":packet["config"]["helixdb"]["logical_store"],"scope":packet["scope"]}},"trace_file":str(Path(packet["fixture"]["ownedRoot"]) / "effect-events.jsonl")}
                                with (helper_root / "fault-control.json").open("x") as control: json.dump(declaration,control)
                            argv = [str(path),"--request",str(private)] if host == "rust" else ["node",str(ROOT / "packages/node/test/provider-e2e.mjs"),"--request",str(private),"--artifact" if host == "native" else "--mcp",str(path)]
                            before = sha(path)
                            code, stdout = run_owned(argv,output,packet)
                            receipts = [json.loads(line) for line in stdout.splitlines() if line.startswith('{') and json.loads(line).get("schema") == "engram.provider-operation.v1"]
                            binding = {**{key:packet[key] for key in ("provider","host","mode","target","scope","invocation","source_revision","lock_sha256","artifact_sha256")},"store":packet['config']['helixdb']['logical_store']}
                            validate_receipts(packet["calls"],receipts,binding)
                            if phase == "faults":
                                owned = Path(packet["fixture"]["ownedRoot"])
                                snapshots = list(owned.glob("*-fixture-observation.json"))
                                if len(snapshots) != 1:
                                    raise ProofError("actual final fixture snapshot missing or ambiguous")
                                result.update(audit_fault(json.loads(snapshots[0].read_text()),packet["calls"],receipts,binding,scenario,code,load_json_lines(helper_root / "requests.jsonl")))
                            elif code:
                                raise ProofError("actual operation consumer exited nonzero")
                            after = sha(path)
                            if before != after: raise ProofError("consumed native artifact changed")
                            if source_snapshot(trusted['source']['approved_new_source_paths'])!=trusted['source']:raise ProofError('complete source snapshot changed during execution')
                            result.update(status="PASS",executed_calls=len(receipts),consumed_artifact_before=before,consumed_artifact_after=after,physical_process="REAPED",original_template_sha256=template_digest,generated_owned_request_sha256=sha(private),expected_build_record_sha256=args.expected_build_record_sha256)
                        except (ProofError,OSError,ValueError) as error:
                            result["reason"] = str(error)
                            if (output/'pending-process.json').exists():raise ProofError('original consumer pending; stop all subsequent work and retain helper custody') from error
                        records.append(result)
    finally:
        if credential_name:
            if old_credential is None: os.environ.pop(credential_name,None)
            else: os.environ[credential_name] = old_credential
        if helper_started and consumer_closure_confirmed(output):
            closed = subprocess.run(["/usr/bin/python3",str(helper),"cleanup","--output",str(helper_root)],cwd=ROOT,capture_output=True,text=True)
            with (output / "raw.log").open("a") as raw: raw.write("owned helper cleanup exit="+str(closed.returncode)+"\n"+closed.stdout+closed.stderr)
            if closed.returncode: records.append({"status":"BLOCKED","reason":"owned helper cleanup unconfirmed; preserve all resources"})
    report = {"schema":"engram.provider-e2e.v1","invocation":invocation,"source_revision":source,"lock_sha256":lock,"provider":args.provider,"mode":args.mode,"phase":args.phase,"records":records,"status":"PASS" if records and all(row["status"] == "PASS" for row in records) else "BLOCKED","inventory_is_execution":False,"execution_scope":{"hosts":hosts,"targets":targets,"scenario":args.scenario},"all_targets_proven":set(targets) == set(TARGETS) and all(row["status"] == "PASS" for row in records)}
    (output / "report.json").write_text(json.dumps(report,indent=2)+"\n")
    print(json.dumps({"status":report["status"],"report":str(output / "report.json")}))
    return 0 if report["status"] == "PASS" else 2


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check-inventory", action="store_true")
    parser.add_argument("--self-test-omissions", action="store_true")
    parser.add_argument("--provider", choices=("helixdb",), default="helixdb")
    parser.add_argument("--mode", choices=MODES)
    parser.add_argument("--host", choices=HOSTS)
    parser.add_argument("--scenario", choices=(*FAULT_SCENARIOS,"pack","code"))
    parser.add_argument("--all-supported-targets", action="store_true")
    parser.add_argument("--target", choices=TARGETS)
    parser.add_argument("--artifacts", default="runs/provider-builds/single/artifacts.json")
    parser.add_argument('--expected-build-record')
    parser.add_argument('--expected-build-record-sha256')
    parser.add_argument("--phase", choices=("operations", "faults", "all"), default="all")
    parser.add_argument("--output", default="runs/e2e/helix")
    args = parser.parse_args(argv)
    try:
        if args.check_inventory or args.self_test_omissions:
            operations = catalogue()
            cells = inventory(operations)
            if args.self_test_omissions:
                self_test_omissions()
            print(json.dumps({"inventory_status": "PASS", "operations": len(operations), "cells": len(cells), "required": sum(c["required"] for c in cells), "workflow_status": "NOT_RUN", "targets": TARGETS}))
            return 0
        if args.mode is None:
            raise ProofError("selected Helix execution mode required")
        for name, value in REQUIRED_ENV.items():
            if os.environ.get(name) != value:
                raise ProofError("BLOCKED: required bounded build environment missing")
        if not os.environ.get("TMPDIR"):
            raise ProofError("BLOCKED: invocation-owned temporary root required")
        return execute(args)
    except (ProofError, OSError, ValueError) as error:
        print(json.dumps({"status": "BLOCKED", "reason": str(error), "workflow_status": "NOT_RUN"}), file=sys.stderr)
        return 2


if __name__ == "__main__":
    try:status=main()
    finally:hold_pending_child_custody()
    raise SystemExit(status)
