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
});
