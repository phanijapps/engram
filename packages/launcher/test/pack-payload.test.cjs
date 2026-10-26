/**
 * Pack payload tests (job 002, RISK-001): drive `npm pack --json` (which
 * runs the prepack generator) and assert the tarball members carry exactly
 * the authored skill set with hygiene — a stale or empty payload fails the
 * release chain here, mirroring scripts/release/verify-launcher-package.py.
 */
'use strict';

const test = require('node:test');
const assert = require('node:assert');
const { spawnSync } = require('node:child_process');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');

const LAUNCHER = path.join(__dirname, '..');
const REPO = path.join(LAUNCHER, '..', '..');
const AUTHORED = path.join(REPO, '.agents', 'skills');
const AUTHORED_SKILLS = fs
  .readdirSync(AUTHORED, { withFileTypes: true })
  .filter((d) => d.isDirectory() && fs.existsSync(path.join(AUTHORED, d.name, 'SKILL.md')))
  .map((d) => d.name)
  .sort();

/** A private launcher copy staged like a dev checkout (launcher + authored
 *  tree at the ../../ position the generator resolves), so parallel test
 *  files never race over the real packages/launcher/skills payload. */
function launcherCopy(tag) {
  const tmpRoot = fs.mkdtempSync(path.join(os.tmpdir(), `engram-launcher-${tag}-`));
  // Stage at <root>/packages/launcher: the generator resolves the authored
  // tree at ../../.agents/skills relative to the launcher dir.
  const pkg = path.join(tmpRoot, 'packages', 'launcher');
  fs.cpSync(LAUNCHER, pkg, {
    recursive: true,
    filter: (src) =>
      src !== path.join(LAUNCHER, 'skills') && src !== path.join(LAUNCHER, 'test'),
  });
  fs.cpSync(AUTHORED, path.join(tmpRoot, '.agents', 'skills'), { recursive: true });
  return { tmpRoot, pkg };
}

/** Pack a launcher dir into its own temp dir; returns { tarball, members }. */
function packDir(pkg, extraArgs = []) {
  const dest = path.join(path.resolve(pkg, '..', '..'), 'pack');
  fs.mkdirSync(dest, { recursive: true });
  const res = spawnSync(
    'npm',
    ['pack', '--json', '--pack-destination', dest, ...extraArgs],
    { cwd: pkg, encoding: 'utf8', timeout: 120000 },
  );
  assert.equal(res.status, 0, `npm pack failed: ${res.stderr}`);
  // npm mixes prepack stdout with the JSON document; the payload starts at
  // the first top-level JSON array.
  const jsonStart = res.stdout.indexOf('[\n');
  const files = JSON.parse(res.stdout.slice(jsonStart));
  const tarball = path.join(dest, files[0].filename);
  assert.ok(fs.existsSync(tarball), `tarball missing: ${tarball}`);
  const list = spawnSync('tar', ['-tzf', tarball], { encoding: 'utf8', timeout: 60000 });
  assert.equal(list.status, 0, `tar -tzf failed: ${list.stderr}`);
  const members = list.stdout.split('\n').filter(Boolean);
  return { tarball, members };
}

/** Pack the REAL launcher dir (prepack regenerates the real payload). Only
 *  this file touches it, and tests within a file run serially. */
function packReal() {
  const dest = fs.mkdtempSync(path.join(os.tmpdir(), 'engram-pack-payload-'));
  const res = spawnSync(
    'npm',
    ['pack', '--json', '--pack-destination', dest],
    { cwd: LAUNCHER, encoding: 'utf8', timeout: 120000 },
  );
  assert.equal(res.status, 0, `npm pack failed: ${res.stderr}`);
  const jsonStart = res.stdout.indexOf('[\n');
  const files = JSON.parse(res.stdout.slice(jsonStart));
  const tarball = path.join(dest, files[0].filename);
  assert.ok(fs.existsSync(tarball), `tarball missing: ${tarball}`);
  const list = spawnSync('tar', ['-tzf', tarball], { encoding: 'utf8', timeout: 60000 });
  assert.equal(list.status, 0, `tar -tzf failed: ${list.stderr}`);
  const members = list.stdout.split('\n').filter(Boolean);
  return { tarball, members };
}

test('AC-004 tarball members equal the authored skill set exactly', () => {
  const { members } = packReal();
  const packed = [...new Set(
    members
      .filter((m) => m.startsWith('package/skills/') && m.split('/').length > 2)
      .map((m) => m.split('/')[2]),
  )].sort();
  assert.deepEqual(packed, AUTHORED_SKILLS, 'packed skill set != authored set');
  for (const skill of AUTHORED_SKILLS) {
    assert.ok(
      members.includes(`package/skills/${skill}/SKILL.md`),
      `${skill}/SKILL.md not packed`,
    );
  }
});

test('AC-004 zero __pycache__ or *.pyc entries in the tarball (REQ-011)', () => {
  const { members } = packDir(launcherCopy('hygiene').pkg);
  const hygiene = members.filter((m) => m.includes('__pycache__') || m.endsWith('.pyc'));
  assert.deepEqual(hygiene, [], `hygiene violations packed: ${hygiene.slice(0, 5)}`);
});

test('AC-004 the factory skill payload packs whole: dialects + controller travel', () => {
  const { members } = packDir(launcherCopy('dialects').pkg);
  const dialects = members.filter(
    (m) =>
      m.startsWith('package/skills/engram-factory/agents/') &&
      (/^package\/skills\/engram-factory\/agents\/engram-.+\.(claude|pi|copilot)\.md$/.test(m) ||
        /^package\/skills\/engram-factory\/agents\/engram-.+\.codex\.yaml$/.test(m)),
  );
  assert.equal(dialects.length, 56, `expected 56 packed dialect files, found ${dialects.length}`);
  assert.ok(
    members.includes('package/skills/engram-factory/scripts/factory.py'),
    'factory controller not packed',
  );
  // The launcher's own agent bundle (red-forman) still ships.
  for (const f of ['red-forman.claude.md', 'red-forman.pi.md', 'red-forman.copilot.md', 'red-forman.codex.yaml']) {
    assert.ok(members.includes(`package/agents/${f}`), `package/agents/${f} not packed`);
  }
});

test('RISK-001 stale payload aborts: verify-launcher-package.py asserts the payload', () => {
  // The release verifier must carry the payload assertion — prove the check
  // exists and fails on an empty-payload tarball built in an isolated
  // launcher copy (prepack bypassed via --ignore-scripts; no real-tree
  // mutation, so parallel test files cannot race).
  const verifier = path.join(REPO, 'scripts', 'release', 'verify-launcher-package.py');
  const source = fs.readFileSync(verifier, 'utf8');
  assert.match(source, /assert_payload/, 'payload assertion present in the release verifier');
  const { pkg } = launcherCopy('stale');
  const { tarball } = packDir(pkg, ['--ignore-scripts']);
  // A minimal fake platform package so the verifier reaches the payload
  // assertion before binary resolution: same name/version as the manifest.
  const manifest = JSON.parse(
    spawnSync('tar', ['-xzf', tarball, '-O', 'package/package.json'], { encoding: 'utf8' }).stdout,
  );
  const platformDir = fs.mkdtempSync(path.join(os.tmpdir(), 'engram-platform-'));
  const platformName = '@engramlabs/mcp-linux-x64-gnu';
  fs.mkdirSync(path.join(platformDir, 'package'), { recursive: true });
  fs.writeFileSync(
    path.join(platformDir, 'package', 'package.json'),
    JSON.stringify({ name: platformName, version: manifest.version, os: ['linux'], cpu: ['x64'] }),
  );
  fs.writeFileSync(
    path.join(platformDir, 'package', 'engram-mcp'),
    '#!/bin/sh\n[ "$1" = "--version" ] && echo "$ENGRAM_FAKE_VERSION" && exit 0\nexit 1\n',
    { mode: 0o755 },
  );
  const platformTar = path.join(platformDir, 'platform.tgz');
  spawnSync('tar', ['-czf', platformTar, '-C', platformDir, 'package'], { timeout: 60000 });
  const verify = spawnSync(
    'python3',
    [verifier, '--launcher', tarball, '--platform-package', platformTar],
    {
      encoding: 'utf8',
      timeout: 60000,
      env: { ...process.env, ENGRAM_FAKE_VERSION: manifest.version },
    },
  );
  assert.notEqual(verify.status, 0, 'verifier must fail on an empty payload');
  assert.match(verify.stderr, /packed skill set != authored set/, `unexpected failure: ${verify.stderr}`);
});
