/**
 * Install rehearsal (job 002, AC-013): the automated fresh-directory journey
 * from the packed tarball to a loadable factory — pack the launcher, npm
 * install the tarball into a clean temp dir with --omit=optional (no
 * registry, no platform binaries), run the real `install --agent pi --scope
 * project` from the installed package, then prove engram-frame and
 * engram-factory landed whole and the factory controller executes.
 *
 * Mirrors release.md REL-002..REL-004 as TEST-013; TEST-016 is the operator
 * rerun of the same steps.
 */
'use strict';

const test = require('node:test');
const assert = require('node:assert');
const { spawnSync } = require('node:child_process');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');

const LAUNCHER = path.join(__dirname, '..');

test(
  'AC-013 fresh dir: tarball install delivers engram-frame + engram-factory; factory.py runs',
  { timeout: 300000 },
  () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'engram-rehearsal-'));
    const home = path.join(root, 'home');
    fs.mkdirSync(home);
    // Isolated npm: cache and config resolve under the temp HOME; no audit,
    // no funding, optional deps off — a local tarball with zero regular
    // dependencies installs fully offline.
    const npmEnv = {
      ...process.env,
      HOME: home,
      npm_config_audit: 'false',
      npm_config_fund: 'false',
      npm_config_update_notifier: 'false',
    };

    // 1. Pack the launcher (prepack generates the payload).
    const packDir = path.join(root, 'pack');
    fs.mkdirSync(packDir);
    const pack = spawnSync(
      'npm',
      ['pack', '--json', '--pack-destination', packDir],
      { cwd: LAUNCHER, encoding: 'utf8', timeout: 120000 },
    );
    assert.equal(pack.status, 0, `npm pack failed: ${pack.stderr}`);
    const jsonStart = pack.stdout.indexOf('[\n');
    const tarball = path.join(packDir, JSON.parse(pack.stdout.slice(jsonStart))[0].filename);
    assert.ok(fs.existsSync(tarball), `tarball missing: ${tarball}`);

    // 2. Fresh project dir with no prior state; install the tarball.
    const proj = path.join(root, 'proj');
    fs.mkdirSync(proj);
    const install = spawnSync(
      'npm',
      ['install', tarball, '--omit=optional', '--no-audit', '--no-fund', '--loglevel=error'],
      { cwd: proj, encoding: 'utf8', timeout: 180000, env: npmEnv },
    );
    assert.equal(install.status, 0, `npm install failed: ${install.stderr}`);
    const installedBin = path.join(proj, 'node_modules', '@engramlabs', 'mcp', 'bin.js');
    assert.ok(fs.existsSync(installedBin), 'installed launcher bin missing');

    // 3. Run the agent-bundle install from the INSTALLED package.
    const agentInstall = spawnSync(
      'node',
      [installedBin, 'install', '--agent', 'pi', '--scope', 'project'],
      { cwd: proj, encoding: 'utf8', timeout: 60000, env: { ...process.env, HOME: home } },
    );
    assert.equal(agentInstall.status, 0, `install --agent pi failed: ${agentInstall.stderr}`);
    assert.match(agentInstall.stdout, /wrote: .*engram-frame/, 'frame not in written set');
    assert.match(agentInstall.stdout, /wrote: .*engram-factory/, 'factory not in written set');

    // 4. engram-frame and engram-factory present, whole, loadable.
    for (const skill of ['engram-frame', 'engram-factory']) {
      const skillMd = path.join(proj, '.agents', 'skills', skill, 'SKILL.md');
      assert.ok(fs.existsSync(skillMd), `${skill}/SKILL.md missing`);
    }
    assert.ok(
      fs.existsSync(path.join(proj, '.agents', 'skills', 'engram-factory', 'scripts', 'factory.py')),
      'factory controller missing from delivered skill',
    );
    // Specialist dialects landed project-side.
    assert.ok(fs.existsSync(path.join(proj, '.pi', 'agents', 'engram-builder.md')));
    assert.ok(fs.existsSync(path.join(proj, '.pi', 'agents', 'engram-qa.md')));

    // 5. The factory controller executes from the installed location.
    const factory = spawnSync(
      'python3',
      [path.join(proj, '.agents', 'skills', 'engram-factory', 'scripts', 'factory.py'), '--help'],
      { cwd: proj, encoding: 'utf8', timeout: 60000 },
    );
    assert.equal(factory.status, 0, `factory.py --help failed: ${factory.stderr}`);
  },
);
