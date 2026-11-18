#!/usr/bin/env node
/** Actual isolated native/MCP consumer. No fixture output is runtime evidence until executed. */
import { createRequire } from 'node:module';
import { readFileSync, writeFileSync, existsSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { spawn, spawnSync } from 'node:child_process';
import { createInterface } from 'node:readline';
import { dirname, resolve, join } from 'node:path';
import assert from 'node:assert/strict';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../../..');
const options = new Map();
for (let index = 2; index < process.argv.length; index += 2) {
  assert(['--request', '--artifact', '--mcp'].includes(process.argv[index]) && process.argv[index + 1], 'unsupported/missing consumer option');
  options.set(process.argv[index], process.argv[index + 1]);
}
assert.equal(Number(process.versions.node.split('.')[0]), 22, 'Node22 required before product startup');
const packet = JSON.parse(readFileSync(options.get('--request'), 'utf8'));
const artifact = options.get('--artifact') ?? options.get('--mcp');
const hash = path => createHash('sha256').update(readFileSync(path)).digest('hex');
const before = hash(artifact);
assert.equal(before, packet.artifact_sha256, 'actual consumed artifact does not match packet');
assert(packet.calls?.length && packet.scope && packet.invocation, 'complete bound call packet required');
assert.equal(new Set(packet.calls.map(call => call.id)).size, packet.calls.length, 'unique call IDs required');
assert(packet.calls.every(call => typeof call.id === 'string' && call.id.length), 'operation is not call identity');
const catalogue = new Map(['provider_operations.json', 'provider_code_operations.json'].flatMap(file =>
  JSON.parse(readFileSync(join(root, 'adapters/integration/tests/fixtures', file), 'utf8')).operations).map(value => [value.operation, value]));
const results = new Map();
const receipts = [];
let actualMcpScope;
const scopeTuple = scope => ['tenant', 'subject', 'workspace', 'session', 'environment'].map(key => scope[key] ?? null);
function pointer(value, path) {
  assert(path === '' || path.startsWith('/'), 'JSON pointer required');
  for (const part of path.split('/').slice(1)) value = value?.[part.replaceAll('~1', '/').replaceAll('~0', '~')];
  return value;
}
function references(value) {
  if (Array.isArray(value)) return value.map(references);
  if (value && typeof value === 'object') {
    if (Object.hasOwn(value, '$result')) {
      const [id, path = ''] = value.$result.split('#');
      assert(results.has(id), 'result reference must name an already executed call');
      const result = pointer(results.get(id), path);
      assert.notEqual(result, undefined, 'missing actual result pointer');
      return result;
    }
    return Object.fromEntries(Object.entries(value).map(([key, item]) => [key, references(item)]));
  }
  return value;
}
function oneShotRoute(call) {
  const route=catalogue.get(call.operation)?.oneshot;
  const argv=call.oneshot_argv;
  assert(route?.applicable && Array.isArray(argv) && argv[0]===route.command,'actual existing one-shot command required');
  // Selectors are literal and checked for every call before ANY host startup.
  // Result references may supply values (not a different operation selector).
  assert(!argv.some(value=>value==='--help'||value==='-h'),'help cannot prove a business operation');
  if(route.command==='code') {
    let operation;
    const values=new Set(['--op','--repo','--branch-ref','--branchRef','--path','--request-id','--requestId','--journal','--decision']);
    for(let index=1;index<argv.length;index++) {
      const token=argv[index];
      assert(typeof token==='string','code argument name must be literal');
      if(values.has(token)) {
        const value=argv[++index];assert(value!==undefined,'code option needs a value');
        if(token==='--op'){assert(operation===undefined,'duplicate/conflicting code operation');operation=value;}
      }
    }
    assert.equal(operation,route.operation,'effective code operation differs from the claimed operation');
  } else if(route.command==='pack') {
    assert.equal(argv[1],'install','only structural pack install proves vocabulary writes');
    assert(typeof argv[2]==='string' && argv[2].length && !argv[2].startsWith('-'),'literal pack identity must precede options');
    assert(!argv.includes('--op'),'pack cannot carry a conflicting operation selector');
  } else assert(!argv.includes('--op'),'top-level command cannot carry a code selector');
  const valueFlags={
    code:['--op','--repo','--branch-ref','--branchRef','--path','--request-id','--requestId','--journal','--decision'],
    pack:['--catalogue','--slice-size','--validate-graph'],
    'fetch-content':['--path','--mode','--symbol'],
    recall:['--query','-q','--limit'],
    'backfill-embeddings':[],
  }[route.command]??[];
  const switches={code:['--json'],pack:[],'fetch-content':[],recall:['--json','--include-code-artifacts'],'backfill-embeddings':['--json']}[route.command]??[];
  for(let index=route.command==='pack'?3:1;index<argv.length;index++) {
    const token=argv[index];
    assert(typeof token==='string','references require an explicit option value role');
    assert(valueFlags.includes(token)||switches.includes(token),'per-call argv permits only operation arguments; startup configuration is independently bound');
    if(valueFlags.includes(token)) {
      const value=argv[++index];assert(value!==undefined,'one-shot option needs a value');
      if(typeof value!=='string') {
        assert(valueFlags.includes(token)&&token!=='--op','reference cannot alter a selector or launch binding');
        assert(value && typeof value==='object' && !Array.isArray(value) && Object.keys(value).length===1 && typeof value.$result==='string','only explicit prior-result references are argument values');
      }
    }
  }
  return {command:route.command,operation:route.command==='pack'?'install':route.operation??route.command};
}
function oneShotArguments(call) {
  oneShotRoute(call);
  const argv=call.oneshot_argv.map((value,index)=>{
    const resolved=references(value);
    if(typeof resolved==='string') {
      if(typeof value!=='string')assert(!resolved.startsWith('-'),'resolved value cannot introduce help or option selectors');
      return resolved;
    }
    assert(call.oneshot_argv[index-1]==='--journal' && resolved && typeof resolved==='object','only journal values accept structured prior results');
    return JSON.stringify(resolved);
  });
  oneShotRoute({...call,oneshot_argv:argv});
  return argv;
}
function oneShotResult(text, route) {
  let value;
  try { value=JSON.parse(text.trim()); } catch { return text.trim(); }
  if(route.command==='code' && value?.command==='code') {
    assert.equal(value.op,route.operation,'actual code result belongs to another operation');
    if(value.ok===true && typeof value.output==='string') {
      try { value=JSON.parse(value.output); } catch { value=value.output; }
    }
  }
  if(value?.content?.[0]?.text){try{return JSON.parse(value.content[0].text);}catch{return value.content[0].text;}}
  return value;
}
function memorySentinel(written, read) {
  const record=written?.record;
  assert(record?.id && typeof record.content?.text==='string' && record.content.text.length,'actual written memory sentinel missing');
  assert.deepEqual(scopeTuple(record.scope),scopeTuple(packet.scope),'written sentinel full scope differs');
  const items=read?.items??read?.context?.items;
  if (Array.isArray(items)) {
    assert(items.some(item=>item.targetId===record.id && item.targetType==='memory' && item.content===record.content.text),'read did not return the actual written memory ID/value');
  } else if (Array.isArray(read?.memories)) {
    const row=read.memories.find(item=>item.id===record.id);
    assert(row && row.content===record.content.text,'archive did not return the actual written memory ID/value');
    assert.deepEqual(scopeTuple(JSON.parse(row.scope)),scopeTuple(packet.scope),'archived sentinel full scope differs');
  } else assert.fail('empty/unrelated read cannot establish sentinel readback');
}
function checked(call, value, error, observations = []) {
  const expected = call.expect;
  assert(expected && ['ok', 'error', 'unsupported'].includes(expected.status), 'explicit outcome assertion required');
  const assertions = ['actual_artifact', 'selected_provider', 'full_scope', 'expected_outcome'];
  if (expected.status === 'ok') assert.equal(error, undefined, 'actual operation failed');
  else {
    assert(error !== undefined, 'expected failure must actually be returned');
    if (expected.status === 'unsupported') {
      // This asserts the existing public envelope, never chooses retry or effect certainty.
      assert(String(error).includes('capability unsupported'), 'canonical unsupported reason missing');
      assertions.push('canonical_unsupported_reason');
    }
    if (expected.status === 'error') assert(expected.error_contains, 'error must assert a specific existing public reason');
    if (expected.error_contains) assert(String(error).includes(expected.error_contains), 'expected public error reason missing');
    for (const secret of expected.redact ?? []) assert(!String(error).includes(secret), 'private diagnostic leaked');
  }
  for (const [path, wanted] of Object.entries(expected.equals ?? {})) {
    assert.deepEqual(pointer(value, path), references(wanted), 'actual result differs at ' + path);
    assertions.push('equals:' + path);
  }
  for (const path of expected.nonempty ?? []) {
    const selected = pointer(value, path);
    assert(selected !== undefined && selected !== null && (!Array.isArray(selected) || selected.length), 'empty actual result at ' + path);
    assertions.push('nonempty:' + path);
  }
  const reason = error === undefined ? undefined : String(error);
  const publicOutcome = call.operation==='batch.ingest'
    ? (typeof value==='string'?value:{status:value?.status,steps:value?.steps?.map(step=>({step:step.step,status:step.status,error:step.error}))}) : undefined;
  for(const secret of expected.redact??[]) assert(!JSON.stringify(publicOutcome??null).includes(secret),'private batch diagnostic leaked');
  const receipt = { schema: 'engram.provider-operation.v1', call_id: call.id, operation: call.operation,
    capability: catalogue.get(call.operation).capability,
    ...Object.fromEntries(['provider', 'mode', 'host', 'target', 'scope', 'invocation', 'source_revision', 'lock_sha256', 'artifact_sha256'].map(key => [key, packet[key]])),
    store:packet.config.helixdb?.logical_store??null,
    scope:actualMcpScope??packet.scope,
    status: 'PASS', outcome: expected.status, assertions, ...(reason ? { reason } : {}),
    result_sha256: createHash('sha256').update(JSON.stringify(value ?? null)).digest('hex'),
    ...(publicOutcome!==undefined?{public_outcome:publicOutcome}:{}),
    ...(observations.length ? { observations } : {}) };
  receipts.push(receipt);
  results.set(call.id, value ?? null);
}

function runNative() {
  assert.equal(packet.host, 'native');
  const addon = createRequire(import.meta.url)(artifact);
  let owner;
  let unknown = false;
  try {
    if (packet.fixture) {
      assert.equal(typeof addon.NativeProvider.openInstrumented, 'function', 'missing actual instrumented artifact');
      owner = addon.NativeProvider.openInstrumented(JSON.stringify(packet.config), JSON.stringify({
        invocation: packet.invocation, sourceRoot: packet.fixture.sourceRoot, sourceRevision: packet.source_revision,
        artifactSha256: before, ownedRoot: packet.fixture.ownedRoot,
      }));
    } else {
      assert.equal(addon.NativeProvider.openInstrumented, undefined, 'production artifact unexpectedly exposes controls');
      owner = new addon.NativeProvider(JSON.stringify(packet.config));
    }
    const capabilities = JSON.parse(owner.capabilitiesJson());
    assert(capabilities.memory, 'selected provider failed capability discovery');
    for (const call of packet.calls) {
      assert(catalogue.has(call.operation), 'unknown canonical operation');
      if (unknown && call.after_unknown !== 'blocked-admission') throw new Error('BLOCKED: original work stopped after typed uncertainty');
      const definition = catalogue.get(call.operation);
      if (definition.native.getter) assert.equal(typeof owner[definition.native.getter], 'function', 'missing actual native capability route');
      let previous = 0;
      if (packet.fixture) {
        if (call.control) owner.fixtureControlJson(JSON.stringify(call.control));
        previous = JSON.parse(owner.fixtureControlJson('{"operation":"snapshot"}')).observations.length;
      }
      let api;
      let value;
      let error;
      try {
        const args = references(call.arguments ?? {});
        if (['knowledge.put_source', 'knowledge.put_document', 'knowledge.put_chunk'].includes(call.operation)) {
          const batch = { idempotency_key: packet.invocation + ':' + call.id, source: null, source_id: null,
            scope: packet.scope, documents: [], chunks: [], facts: [], entities: [], relationships: [], evidence: [], embeddings: [] };
          if (call.operation === 'knowledge.put_source') batch.source = args;
          else if (call.operation === 'knowledge.put_document') batch.documents = [args];
          else batch.chunks = [args];
          api = owner.requireBatchApi();
          value = JSON.parse(api.ingestJson(JSON.stringify(batch)));
        } else {
          api = definition.native.getter ? owner[definition.native.getter]() : owner;
          if (typeof api[definition.native.method] !== 'function') throw Object.assign(new Error('missing actual native operation route'), { routeMissing: true });
          const result = api[definition.native.method](JSON.stringify(args));
          value = typeof result === 'string' ? JSON.parse(result) : result;
        }
      } catch (failure) {
        if (failure.routeMissing) throw failure;
        error = failure;
      } finally {
        if (api && api !== owner) api.close();
      }
      const observations = packet.fixture ? JSON.parse(owner.fixtureControlJson('{"operation":"snapshot"}')).observations.slice(previous) : [];
      for (const observation of observations) {
        assert.deepEqual(scopeTuple(observation.scope), scopeTuple(packet.scope), 'foreign typed observation');
        assert.equal(observation.mode, packet.mode);
        if (observation.outcome === 'Unknown') unknown = true;
      }
      checked(call, value, error, observations);
    }
    if (packet.fixture) writeFileSync(join(packet.fixture.ownedRoot, 'native-fixture-observation.json'), owner.fixtureControlJson('{"operation":"snapshot"}'), { flag: 'wx', mode: 0o600 });
  } finally {
    owner?.close();
  }
}

function mcpArgs() {
  const config = packet.config;
  assert.deepEqual(scopeTuple(config.scope??packet.scope),scopeTuple(packet.scope),'configured/packet full scope differs');
  assert(['subject','session','environment'].every(key=>packet.scope[key]===undefined||packet.scope[key]===null),'existing MCP launch contract cannot represent optional dimensions including empty');
  assert.equal(typeof packet.scope.workspace,'string','existing MCP launch always has Some(project); workspace None is unrepresentable');
  actualMcpScope={tenant:packet.scope.tenant,workspace:packet.scope.workspace};
  assert.deepEqual(scopeTuple(actualMcpScope),scopeTuple(packet.scope),'actual supported MCP launch tuple differs');
  const args = ['--backend', config.storage_provider??'sqlite','--storage', config.storage_path, '--trusted-root', config.trusted_root, '--org', packet.scope.tenant];
  args.push('--project',actualMcpScope.workspace);
  if (config.storage_provider==='helixdb') {
    assert.equal(config.helixdb.mode,packet.mode);
    args.push('--helix-mode',packet.mode,'--helix-store',config.helixdb.logical_store);
    if (packet.mode === 'embedded') args.push('--helix-path', config.helixdb.path);
    else args.push('--helix-endpoint', config.helixdb.endpoint, '--helix-credential-env', config.helixdb.credential_env);
  } else if (config.storage_provider==='pgvector') {
    assert(config.pgvector_connection_string,'actual private PG fixture credentials required');
  }
  args.push(config.enable_vector ? '--vector' : '--no-vector', '--embedding-provider', 'none');
  return args;
}
function mcpEnvironment() {
  const env={...process.env};
  const credential=packet.config.helixdb?.credential_env;
  for(const key of Object.keys(env))if(key.startsWith('ENGRAM_')&&key!==credential)delete env[key];
  if(packet.config.storage_provider==='pgvector')env.ENGRAM_PG_CONNECTION_STRING=packet.config.pgvector_connection_string;
  return env;
}

async function runMcp() {
  const args = mcpArgs();
  if (packet.host === 'mcp-stdio') {
    const argv = [...args, '--tools', 'all'];
    if (packet.fixture) {
      const file = join(packet.fixture.ownedRoot, 'mcp-fixture-declaration.json');
      writeFileSync(file, JSON.stringify({ ...packet.fixture, invocation: packet.invocation, sourceRoot: packet.fixture.sourceRoot,
        sourceRevision: packet.source_revision, artifactSha256: before, host: 'mcp-stdio', ownedRoot: packet.fixture.ownedRoot }), { flag: 'wx', mode: 0o600 });
      argv.unshift('__fixture', file);
    }
    const child = spawn(artifact, argv, { stdio: ['pipe', 'pipe', 'pipe'],env:mcpEnvironment() });
    const waiting = new Map();
    let diagnostic = '';
    child.stderr.on('data', bytes => { diagnostic += bytes; });
    const closed = new Promise((resolveClose, reject) => { child.once('close', resolveClose); child.once('error', reject); });
    const lines = createInterface({ input: child.stdout });
    lines.on('line', line => {
      try {
        const message = JSON.parse(line);
        if (waiting.has(message.id)) { waiting.get(message.id).resolve(message); waiting.delete(message.id); }
      } catch { /* Non-JSON output cannot manufacture an operation response. */ }
    });
    child.once('close', () => { for (const pending of waiting.values()) pending.reject(new Error('owned MCP closed before its exact response')); });
    const exchange = (id, method, params) => new Promise((resolveReply, reject) => {
      waiting.set(id, { resolve: resolveReply, reject });
      child.stdin.write(JSON.stringify({ jsonrpc: '2.0', id, method, params }) + '\n');
    });
    try {
      const initialized = await exchange('__initialize__', 'initialize', {});
      assert(!initialized.error, 'actual MCP startup failed');
      const listed = await exchange('__inventory__', 'tools/list', {});
      const available = new Set(listed.result?.tools?.map(tool => tool.name));
      let uncertain = false;
      for (const call of packet.calls) {
        assert(!uncertain || call.after_unknown === 'blocked-admission', 'later logical work after typed uncertainty');
        const live = packet.fixture ? join(packet.fixture.ownedRoot, 'effect-events.jsonl') : undefined;
        const previous = live && existsSync(live) ? readFileSync(live,'utf8').split('\n').filter(Boolean).length : 0;
        if (call.control && call.control.operation!=='snapshot') {
          assert(packet.fixture,'private call control needs an instrumented actual host');
          const scope=Object.fromEntries(Object.entries(packet.scope).filter(([,value])=>value!==null));
          const binding={host:'mcp-stdio',invocation:packet.invocation,sourceRevision:packet.source_revision,artifactSha256:before,actual:{backend:'helixdb',mode:packet.mode,store:packet.config.helixdb.logical_store,scope}};
          // The instrumented transport consumes this exact private declaration
          // before the next real mutation. No production RPC/tool is added.
          writeFileSync(join(packet.fixture.ownedRoot,'fixture-control.json'),JSON.stringify({...call.control,binding}),{flag:'wx',mode:0o600});
        }
        const definition = catalogue.get(call.operation);
        assert(definition && available.has(definition.mcp_tool), 'required actual MCP operation route missing');
        const response = await exchange(call.id, 'tools/call', { name: definition.mcp_tool,
          arguments: references(call.mcp_arguments ?? call.arguments ?? {}) });
        let value = response.result;
        if (value?.content?.[0]?.text) {
          try { value = JSON.parse(value.content[0].text); } catch { value = value.content[0].text; }
        }
        const observations = live && existsSync(live) ? readFileSync(live,'utf8').split('\n').filter(Boolean).slice(previous).map(line=>JSON.parse(line).event).filter(row=>row.phase==='Observed') : [];
        if (observations.some(row=>row.outcome==='Unknown')) uncertain = true;
        checked(call, value, response.error ? response.error.message : undefined, observations);
      }
    } finally {
      child.stdin.end();
      assert.equal(await closed, 0, 'owned MCP process did not close normally');
      lines.close();
      if (packet.fixture?.ownedRoot && diagnostic) writeFileSync(join(packet.fixture.ownedRoot, 'mcp-child.log'), diagnostic, { flag: 'wx', mode: 0o600 });
    }
  } else {
    assert.equal(packet.host, 'mcp-oneshot');
    for(const call of packet.calls)oneShotRoute(call);
    for (const call of packet.calls) {
      const argv = [...oneShotArguments(call), ...args];
      if (packet.fixture) {
        assert.equal(packet.calls.length,1,'one-shot fault case must be one actual command/process, never replay');
        const file = join(packet.fixture.ownedRoot,'mcp-fixture-declaration.json');
        writeFileSync(file,JSON.stringify({...packet.fixture,invocation:packet.invocation,sourceRoot:packet.fixture.sourceRoot,sourceRevision:packet.source_revision,artifactSha256:before,host:'mcp-oneshot'}),{flag:'wx',mode:0o600});
        argv.unshift('__fixture',file);
      }
      const child = spawnSync(artifact, argv, { encoding: 'utf8', maxBuffer: 64 * 1024 * 1024,env:mcpEnvironment() });
      assert.equal(child.error, undefined, 'owned one-shot startup failed');
      const value=oneShotResult(child.stdout,oneShotRoute(call));
      const recorded=packet.fixture?join(packet.fixture.ownedRoot,'mcp-fixture-observation.json'):undefined;
      const observations=recorded&&existsSync(recorded)?JSON.parse(readFileSync(recorded,'utf8')).observations:[];
      checked(call, value, child.status === 0 ? undefined : value?.error || child.stderr || 'actual one-shot operation failed',observations);
    }
  }
}

try {
  if (options.has('--artifact')) runNative(); else await runMcp();
  assert.equal(hash(artifact), before, 'actual consumed artifact changed');
  for (const receipt of receipts) process.stdout.write(JSON.stringify({ ...receipt, artifact_sha256_after: hash(artifact) }) + '\n');
  if (packet.proof_obligations) {
    assert(!packet.fixture,'production bootstrap proof cannot use a fault artifact');
    const sentinel=packet.calls.find(call=>call.id===packet.proof_obligations.sentinel_call_id);
    assert(sentinel && Object.keys(sentinel.expect.equals??{}).length && results.has(sentinel.id),'actual sentinel readback with explicit value assertions required');
    assert(['memory.retrieve','archive.export'].includes(sentinel.operation),'sentinel must read the actual memory family');
    const written=packet.calls.find(call=>call.id===packet.proof_obligations.sentinel_write_call_id);
    assert(written?.operation==='memory.write' && written.expect.status==='ok' && results.has(written.id),'prior actual memory sentinel write required');
    memorySentinel(results.get(written.id),results.get(sentinel.id));
    if (options.has('--artifact')) {
      const addon=createRequire(import.meta.url)(artifact);
      const reopened=new addon.NativeProvider(JSON.stringify(packet.config));
      let api;
      try {
        const route=catalogue.get(sentinel.operation);
        api=route.native.getter?reopened[route.native.getter]():reopened;
        const raw=api[route.native.method](JSON.stringify(references(sentinel.arguments??{})));
        const actual=typeof raw==='string'?JSON.parse(raw):raw;
        memorySentinel(results.get(written.id),actual);
        for(const [path,wanted] of Object.entries(sentinel.expect.equals)) assert.deepEqual(pointer(actual,path),references(wanted),'reopened actual sentinel differs');
      } finally { if(api&&api!==reopened)api.close();reopened.close(); }
      assert.throws(()=>reopened.capabilitiesJson(),'closed facade retained access');
      assert.equal(typeof addon.bootstrapIntegrationProvider,'function','actual native bootstrap entry point required');
      const boot=addon.bootstrapIntegrationProvider(JSON.stringify(packet.config));
      assert(JSON.parse(boot.capabilitiesJson).memory,'actual bootstrap capabilities missing');
    } else {
      const originalCalls=packet.calls;
      try { packet.calls=[sentinel]; await runMcp();memorySentinel(results.get(written.id),results.get(sentinel.id)); } finally {packet.calls=originalCalls;}
    }
    process.stdout.write(JSON.stringify({schema:'engram.provider-bootstrap.v1',artifact_sha256:before,bootstrap:true,sentinel_readback:true,owned_close:true,close_origin:'normal host owner release and process wait; no held NativeClose attestation'})+'\n');
  }
} catch (error) {
  process.stderr.write(JSON.stringify({ status: 'FAIL', reason: String(error), dataRetained: true, invocation: packet.invocation }) + '\n');
  process.exitCode = 1;
}
