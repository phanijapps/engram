/**
 * Parity gate wiring tests (job 002, FIND-operations-001 / FIND-code-003):
 * the deterministic drift signal (scripts/check-skills-parity.cjs) must run
 * inside a recurring gate — every `npm test` (and thus every
 * `pnpm run test` / ci.yml run) executes it, so wrapper or payload drift
 * fails loudly instead of shipping undetected.
 *
 * The selftest spawns the real checker against temp fixtures only; nothing
 * here writes the repo or the real payload.
 */
'use strict';

const test = require('node:test');
const assert = require('node:assert');
const { spawnSync } = require('node:child_process');
const fs = require('node:fs');
const path = require('node:path');

const LAUNCHER = path.join(__dirname, '..');
const CHECKER = path.join(LAUNCHER, 'scripts', 'check-skills-parity.cjs');

test('FIND-operations-001 the parity gate runs inside npm test (recurring gate)', () => {
  const pkg = JSON.parse(fs.readFileSync(path.join(LAUNCHER, 'package.json'), 'utf8'));
  const script = pkg.scripts && pkg.scripts.test;
  assert.ok(script, 'launcher test script missing');
  assert.match(
    script,
    /check-skills-parity\.cjs/,
    'npm test does not run the parity check — drift would ship undetected',
  );
  assert.match(script, /node --test/, 'test suite invocation missing');
});

test('FIND-operations-001 parity --selftest detects every injected drift kind (exit 1 by design)', () => {
  const res = spawnSync('node', [CHECKER, '--selftest'], { encoding: 'utf8', timeout: 60000 });
  assert.equal(
    res.status,
    1,
    `selftest must exit 1 when detection works (got ${res.status}):\n${res.stdout}${res.stderr}`,
  );
  const kinds = ['set drift', 'hygiene drift', 'wrapper drift', 'orphan wrapper', 'payload symlink'];
  for (const kind of kinds) {
    assert.ok(res.stdout.includes(`detected: ${kind}`), `${kind} not detected`);
  }
});
