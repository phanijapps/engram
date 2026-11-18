'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const vm = require('node:vm');
const { spawnSync } = require('node:child_process');
const root = path.resolve(__dirname, '..');

function launch({ env = {}, argv = [], platform = 'linux', arch = 'x64', version = '1.2.3', installed = true } = {}) {
  let spawned;
  let error = '';
  let exit;
  const requireMock = (name) => {
    if (name === './package.json') return { version: '1.2.3' };
    if (name === 'node:child_process') return { spawn: (bin, args) => {
      spawned = { bin, args }; return { on() {} };
    }};
    if (name === 'node:fs') return { existsSync: () => true };
    if (name === 'node:path') return path;
    if (name === '/package/package.json') return { version };
    throw new Error(`unexpected require ${name}`);
  };
  const resolved = [];
  requireMock.resolve = (name) => {
    resolved.push(name);
    if (!installed) throw new Error('missing');
    return '/package/package.json';
  };
  try {
    vm.runInNewContext(fs.readFileSync(path.join(root, 'bin.js'), 'utf8'), {
      require: requireMock,
      process: { env, platform, arch, argv: ['node', 'bin.js', ...argv],
        stderr: { write: text => { error += text; } },
        exit: code => { exit = code; throw new Error('exit'); },
      },
    });
  } catch (err) { if (err.message !== 'exit') throw err; }
  return { spawned, resolved, error, exit };
}

test('all platforms resolve one canonical binary and pass runtime args unchanged', () => {
  for (const [platform, arch, suffix] of [
    ['linux', 'x64', 'linux-x64-gnu'], ['linux', 'arm64', 'linux-arm64-gnu'],
    ['darwin', 'arm64', 'darwin-arm64'], ['win32', 'x64', 'win32-x64'],
  ]) {
    const result = launch({ platform, arch, argv: ['--slim', '--storage', '/tmp/store'] });
    assert.deepEqual(result.resolved, [`@engramlabs/mcp-${suffix}/package.json`]);
    assert.deepEqual(Array.from(result.spawned.args), ['--slim', '--storage', '/tmp/store']);
    assert.equal(result.error, '');
  }
});

test('deprecated env disables inference on the same binary; CLI takes precedence', () => {
  for (const value of ['1', 'true', 'yes']) {
    const result = launch({ env: { ENGRAM_NO_FULL: value }, argv: ['--vector'] });
    assert.deepEqual(Array.from(result.spawned.args), ['--slim', '--vector']);
    assert.match(result.error, /deprecated/);
    assert.equal(result.resolved.length, 1);
  }
  for (const value of ['', '0', 'false']) {
    assert.deepEqual(Array.from(launch({ env: { ENGRAM_NO_FULL: value } }).spawned.args), []);
  }
});

test('deprecated env keeps one-shot subcommand first and explicit mode last', () => {
  for (const command of ['status', 'recall']) {
    const result = launch({ env: { ENGRAM_NO_FULL: '1' }, argv: [command, '--vector', '--storage', '/tmp/store'] });
    assert.deepEqual(Array.from(result.spawned.args), [command, '--slim', '--vector', '--storage', '/tmp/store']);
  }
});

test('missing and mismatched packages fail precisely; Intel macOS is not advertised', () => {
  for (const settings of [{ installed: false }, { version: '9.0.0' }]) {
    const result = launch(settings);
    assert.equal(result.exit, 1);
    assert.match(result.error, /--include=optional/);
    assert.equal(result.spawned, undefined);
  }
  assert.ok(launch({ version: 'v1.2.3' }).spawned);
  const intel = launch({ platform: 'darwin', arch: 'x64' });
  assert.equal(intel.exit, 1);
  assert.match(intel.error, /unsupported platform/);
});

test('client installer pins scoped package and forwards mode switches in order', () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'engram-install-mode-'));
  try {
    const script = 'require(process.argv[1]).install(process.argv.slice(2))';
    const run = (...args) => spawnSync(process.execPath, ['-e', script, path.join(root, 'install.js'), ...args], { cwd: dir, encoding: 'utf8' });
    const result = run('--client', 'claude', '--storage', dir, '--slim', '--vector', '--no-vector');
    assert.equal(result.status, 0, result.stderr);
    const args = JSON.parse(fs.readFileSync(path.join(dir, '.mcp.json'), 'utf8')).mcpServers.engram.args;
    assert.equal(args[1], `@engramlabs/mcp@${require('../package.json').version}`);
    assert.deepEqual(args.slice(-3), ['--slim', '--vector', '--no-vector']);
    const before = fs.readFileSync(path.join(dir, '.mcp.json'), 'utf8');
    for (const invalid of [['--typo'], ['--storage'], ['--project', '--slim']]) {
      assert.equal(run('--client', 'claude', ...invalid).status, 1);
      assert.equal(fs.readFileSync(path.join(dir, '.mcp.json'), 'utf8'), before);
    }
  } finally { fs.rmSync(dir, { recursive: true, force: true }); }
});

test('release builds one embedding-capable artifact for every declared dependency', () => {
  const workflow = fs.readFileSync(path.join(root, '../../.github/workflows/release.yml'), 'utf8');
  const targets = [...workflow.matchAll(/- target: (\S+)/g)].map(match => match[1]);
  assert.equal(new Set(targets).size, targets.length);
  assert.deepEqual(targets.map(t => `@engramlabs/mcp-${t}`).sort(), Object.keys(require('../package.json').optionalDependencies).sort());
  assert.doesNotMatch(workflow, /matrix\.variant|--no-default-features|--full/);
  assert.match(workflow, /verify_embedding_modes\.py/);
  assert.match(workflow, /PACK=\$\(npm pack --json/);
  assert.match(workflow, /mv "\$PACK" artifacts\//);
  assert.doesNotMatch(workflow, /mv \*\.tgz artifacts/);
});


test('selected provider installation retains Helix identity and full launch scope', () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'engram-install-selection-'));
  const selected = ['--backend','helixdb','--helix-mode','service','--helix-store','owned-logical',
    '--helix-endpoint','http://127.0.0.1:9999','--helix-credential-env','OWNED_CREDENTIAL_REFERENCE',
    '--trusted-root',dir,'--org','org-selected','--domain','domain-selected','--subdomain','sub-selected',
    '--vector','--embedding-provider','none'];
  try {
    const result = spawnSync(process.execPath, ['-e', 'require(process.argv[1]).install(process.argv.slice(2))',
      path.join(root,'install.js'),'--client','claude','--storage',dir,'--project','project-selected',...selected],
      {cwd:dir,encoding:'utf8',env:{...process.env,OWNED_CREDENTIAL_REFERENCE:'PRIVATE_CREDENTIAL_VALUE'}});
    assert.equal(result.status,0,result.stderr);
    const config = fs.readFileSync(path.join(dir,'.mcp.json'),'utf8');
    const saved = JSON.parse(config).mcpServers.engram.args;
    assert.deepEqual(saved.slice(-selected.length),selected);
    assert.doesNotMatch(config + result.stdout + result.stderr,/PRIVATE_CREDENTIAL_VALUE/);
  } finally { fs.rmSync(dir,{recursive:true,force:true}); }
});


test('installer rejects conflicting selection and embedded credentials before config writes', () => {
  const dir=fs.mkdtempSync(path.join(os.tmpdir(),'engram-install-conflict-'));
  try {
    for(const flags of [
      ['--backend','sqlite','--helix-mode','embedded'],
      ['--backend','sqlite','--backend','helixdb'],
      ['--backend','helixdb','--helix-mode','service','--helix-store','owned','--helix-endpoint','http://user:SECRET_MARKER@localhost','--helix-credential-env','REFERENCE'],
    ]) {
      const result=spawnSync(process.execPath,['-e','require(process.argv[1]).install(process.argv.slice(2))',path.join(root,'install.js'),'--client','claude',...flags],{cwd:dir,encoding:'utf8'});
      assert.equal(result.status,1);
      assert.ok(!fs.existsSync(path.join(dir,'.mcp.json')));
      assert.doesNotMatch(result.stdout+result.stderr,/SECRET_MARKER/);
    }
  } finally {fs.rmSync(dir,{recursive:true,force:true});}
});


test('installer preserves explicit vector precedence and inactive provider secrets stay unpersisted', () => {
  const dir=fs.mkdtempSync(path.join(os.tmpdir(),'engram-install-environment-'));
  try {
    for(const backend of ['sqlite','pgvector']) {
      const result=spawnSync(process.execPath,['-e','require(process.argv[1]).install(process.argv.slice(2))',
        path.join(root,'install.js'),'--client','claude','--vector'],{cwd:dir,encoding:'utf8',env:{...process.env,
        ENGRAM_BACKEND:backend,ENGRAM_ENABLE_VECTOR:'false',ENGRAM_HELIX_MODE:'service',
        ENGRAM_HELIX_ENDPOINT:'http://INACTIVE_PRIVATE_MARKER',
        ENGRAM_PG_CONNECTION_STRING:'postgres://user:PRIVATE_PG_CREDENTIAL@localhost/db'}});
      assert.equal(result.status,0,result.stderr);
      const config=fs.readFileSync(path.join(dir,'.mcp.json'),'utf8');
      const args=JSON.parse(config).mcpServers.engram.args;
      assert.equal(args[args.indexOf('--backend')+1],backend);
      assert.ok(args.includes('--vector')); assert.ok(!args.includes('--no-vector'));
      assert.doesNotMatch(config+result.stdout+result.stderr,/PRIVATE_PG_CREDENTIAL|INACTIVE_PRIVATE_MARKER/);
    }
  } finally {fs.rmSync(dir,{recursive:true,force:true});}
});


function originInstallation(endpoint, expectValid) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'engram-install-origin-'));
  const file = path.join(dir, '.mcp.json');
  const before = JSON.stringify({mcpServers:{engram:{command:'existing-owned-entry'}},untouched:true})+'\n';
  fs.writeFileSync(file, before);
  const result = spawnSync(process.execPath, ['-e', 'require(process.argv[1]).install(process.argv.slice(2))',
    path.join(root, 'install.js'), '--client','claude','--storage',dir,
    '--backend','helixdb','--helix-mode','service','--helix-store','origin-owned',
    '--helix-endpoint',endpoint,'--helix-credential-env','OWNED_ORIGIN_REFERENCE',
    '--vector','--embedding-provider','none'], {cwd:dir,encoding:'utf8',env:{...process.env,
      ENGRAM_HELIX_PATH:'',OWNED_ORIGIN_REFERENCE:'ORIGIN_PRIVATE_CREDENTIAL_VALUE'}});
  const after = fs.readFileSync(file, 'utf8');
  try {
    if (expectValid) {
      assert.equal(result.status,0,result.stderr);
      const args=JSON.parse(after).mcpServers.engram.args;
      assert.equal(args[args.indexOf('--helix-endpoint')+1],endpoint);
      assert.equal(args[args.indexOf('--helix-credential-env')+1],'OWNED_ORIGIN_REFERENCE');
    } else {
      console.error(`ORIGIN_CONTROL root=${dir} status=${result.status} unchanged=${before===after}`);
      assert.equal(result.status,1,'invalid origin must reject before client configuration changes');
      assert.equal(after,before,'invalid origin changed existing client config');
      assert.doesNotMatch(result.stdout+result.stderr,/ORIGIN_PRIVATE_CREDENTIAL_VALUE|ORIGIN_SECRET_MARKER/);
    }
    assert.doesNotMatch(after,/ORIGIN_PRIVATE_CREDENTIAL_VALUE/);
  } catch (error) {
    console.error(`ORIGIN_FAILED_ROOT_RETAIN ${dir}`);
    throw error;
  }
  fs.rmSync(dir,{recursive:true,force:true});
}
test('origin rejects unsupported scheme before existing config changes', () => {
  originInstallation('ftp://localhost/',false);
});
test('origin rejects missing host paths credentials and even empty query or fragment', () => {
  for(const endpoint of ['file:///owned/path','http://','http://localhost/prefix',
    'http://ORIGIN_SECRET_MARKER:password@localhost/','http://localhost/?token=ORIGIN_SECRET_MARKER',
    'http://localhost/#ORIGIN_SECRET_MARKER','http://localhost/?','http://localhost/#',
    'http://localhost?','http://localhost#','http://localhost:8080/prefix/']) originInstallation(endpoint,false);
});
test('origin preserves valid HTTP HTTPS port and IPv6 forwarding', () => {
  for(const endpoint of ['http://localhost','http://127.0.0.1:9999/','https://example.invalid/',
    'https://example.invalid:8443','http://[::1]:9999/']) originInstallation(endpoint,true);
});
