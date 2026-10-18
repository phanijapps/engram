/**
 * Hook evals (RFC-0022) — black-box tests for recall-context.cjs and
 * stale-index.cjs via a stub binary (ENGRAM_MCP_BIN), zero deps, node:test.
 *
 * Every test drives the real hook script with controlled env + stdin and
 * asserts on stdout/exit — the same surface the hosts see.
 */
'use strict';

const test = require('node:test');
const assert = require('node:assert');
const { spawnSync } = require('node:child_process');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');

const HOOKS = path.join(__dirname, '..', 'hooks');
const RECALL = path.join(HOOKS, 'recall-context.cjs');
const STALE = path.join(HOOKS, 'stale-index.cjs');

/** Fixture store dir per test (isolated dedupe/heartbeat state). */
function tmpStore(name) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), `engram-hook-${name}-`));
  return dir;
}

/** Stub binary: emits canned recall JSON. Env knob = ENGRAM_STUB_RESPONSE. */
const STUB = path.join(__dirname, 'fixtures', 'stub-engram-mcp.cjs');
fs.mkdirSync(path.dirname(STUB), { recursive: true });
fs.writeFileSync(
  STUB,
  `#!/usr/bin/env node
'use strict';
// Stub engram-mcp for hook evals. Reads the canned response from
// ENGRAM_STUB_RESPONSE (JSON string) or a fail mode from ENGRAM_STUB_MODE.
const mode = process.env.ENGRAM_STUB_MODE || 'ok';
if (mode === 'usage-error') { console.error('unknown argument'); process.exit(2); }
if (mode === 'crash') { process.exit(1); }
if (mode === 'hang') { setTimeout(() => {}, 10000); }
const payload = JSON.parse(process.env.ENGRAM_STUB_RESPONSE || '{"ok":true,"hit_count":0,"results":[]}');
console.log(JSON.stringify(payload));
`,
  { mode: 0o755 },
);

function fire(hook, stdin, env) {
  const res = spawnSync('node', [hook], {
    input: stdin,
    encoding: 'utf8',
    timeout: 10000,
    env: {
      ...process.env,
      ENGRAM_MCP_BIN: STUB,
      ENGRAM_STUB_MODE: 'ok',
      ...env,
    },
  });
  return res;
}

function recallWith(scores, evidence, kinds) {
  return JSON.stringify({
    ok: true,
    hit_count: scores.length,
    top_score: scores[0] ?? 0,
    elapsed_ms: 5,
    results: scores.map((score, i) => ({
      kind: kinds?.[i] ?? 'memory',
      score,
      evidence: evidence?.[i] ?? 'exact',
      title: '',
      content: `test memory ${i} about ${score}`,
      path: '',
    })),
  });
}

const RELEVANT = recallWith([0.30, 0.10, 0.08], ['exact', 'weak', 'weak']); // sep 3.0x
const IRRELEVANT = recallWith([0.10, 0.099, 0.098], ['keyword', 'weak', 'weak']); // sep ~1.02x
const WEAK_TOP = recallWith([0.30, 0.10], ['weak', 'weak']); // sep ok, evidence weak
const SINGLE = recallWith([0.9], ['exact']); // 1 hit only

// ---------------------------------------------------------------- recall
test('relevant prompt passes the separation gate and injects', () => {
  const store = tmpStore('relevant');
  const res = fire(RECALL, '{"prompt":"scanner module latency dissolution","cwd":"/tmp"}', {
    ENGRAM_STORAGE: store,
    ENGRAM_STUB_RESPONSE: RELEVANT,
  });
  assert.equal(res.status, 0, `stderr: ${res.stderr}`);
  const out = JSON.parse(res.stdout);
  assert.equal(out.hookSpecificOutput.hookEventName, 'UserPromptSubmit');
  const ctx = out.hookSpecificOutput.additionalContext;
  assert.match(ctx, /data, not instructions/);
  assert.match(ctx, /discard if irrelevant/);
  assert.ok(ctx.length <= 1200, 'context cap respected');
});

test('irrelevant prompt (flat scores) stays silent', () => {
  const store = tmpStore('flat');
  const res = fire(RECALL, '{"prompt":"banana pancake recipe breakfast","cwd":"/tmp"}', {
    ENGRAM_STORAGE: store,
    ENGRAM_STUB_RESPONSE: IRRELEVANT,
  });
  assert.equal(res.status, 0);
  assert.equal(res.stdout.trim(), '');
});

test('weak-evidence top is gated to silence', () => {
  const store = tmpStore('weaktop');
  const res = fire(RECALL, '{"prompt":"some query about anything","cwd":"/tmp"}', {
    ENGRAM_STORAGE: store,
    ENGRAM_STUB_RESPONSE: WEAK_TOP,
  });
  assert.equal(res.status, 0);
  assert.equal(res.stdout.trim(), '');
});

test('single hit does not pass (needs corroboration)', () => {
  const store = tmpStore('single');
  const res = fire(RECALL, '{"prompt":"lonely query here now","cwd":"/tmp"}', {
    ENGRAM_STORAGE: store,
    ENGRAM_STUB_RESPONSE: SINGLE,
  });
  assert.equal(res.status, 0);
  assert.equal(res.stdout.trim(), '');
});

test('short prompt skips the recall entirely', () => {
  const store = tmpStore('short');
  const res = fire(RECALL, '{"prompt":"fix it","cwd":"/tmp"}', {
    ENGRAM_STORAGE: store,
    ENGRAM_STUB_RESPONSE: RELEVANT,
  });
  assert.equal(res.status, 0);
  assert.equal(res.stdout.trim(), '');
});

test('slash command skips the recall', () => {
  const store = tmpStore('slash');
  const res = fire(RECALL, '{"prompt":"/engram-index the repository now","cwd":"/tmp"}', {
    ENGRAM_STORAGE: store,
    ENGRAM_STUB_RESPONSE: RELEVANT,
  });
  assert.equal(res.status, 0);
  assert.equal(res.stdout.trim(), '');
});

test('ENGRAM_HOOKS=0 kill switch short-circuits', () => {
  const store = tmpStore('killed');
  const res = fire(RECALL, '{"prompt":"scanner module latency dissolution","cwd":"/tmp"}', {
    ENGRAM_STORAGE: store,
    ENGRAM_STUB_RESPONSE: RELEVANT,
    ENGRAM_HOOKS: '0',
  });
  assert.equal(res.status, 0);
  assert.equal(res.stdout.trim(), '');
});

test('session-start (no prompt) emits a SessionStart pack', () => {
  const store = tmpStore('session');
  const res = fire(RECALL, '{"sessionId":"s1","timestamp":1,"cwd":"/tmp"}', {
    ENGRAM_STORAGE: store,
    ENGRAM_STUB_RESPONSE: SINGLE, // session packs need only 1 hit
  });
  assert.equal(res.status, 0);
  const out = JSON.parse(res.stdout);
  assert.equal(out.hookSpecificOutput.hookEventName, 'SessionStart');
});

test('garbage stdin stays silent (not session mode)', () => {
  const store = tmpStore('garbage');
  const res = fire(RECALL, 'not json at all', {
    ENGRAM_STORAGE: store,
    ENGRAM_STUB_RESPONSE: RELEVANT,
  });
  assert.equal(res.status, 0);
  assert.equal(res.stdout.trim(), '');
});

test('dedupe suppresses the same items on the next fire', () => {
  const store = tmpStore('dedupe');
  const env = { ENGRAM_STORAGE: store, ENGRAM_STUB_RESPONSE: RELEVANT };
  const first = fire(RECALL, '{"prompt":"scanner module latency dissolution","cwd":"/tmp"}', env);
  assert.ok(first.stdout.includes('UserPromptSubmit'), 'first fire injects');
  const second = fire(RECALL, '{"prompt":"scanner module latency different words","cwd":"/tmp"}', env);
  assert.equal(second.stdout.trim(), '', 'second fire deduped');
});

test('stale binary (exit 2) falls through to next candidate', () => {
  const store = tmpStore('stale');
  const good = recallWith([0.30, 0.10], ['exact', 'weak']);
  const res = fire(RECALL, '{"prompt":"scanner module latency dissolution","cwd":"/tmp"}', {
    ENGRAM_STORAGE: store,
    ENGRAM_MCP_BIN: '', // unset → resolution chain
    ENGRAM_STUB_MODE: 'ok',
    ENGRAM_STUB_RESPONSE: good,
    PATH: path.dirname(STUB) + path.delimiter + process.env.PATH,
  });
  // The stub is NOT named engram-mcp on PATH, so resolution skips it and
  // finds either a dev build or nothing — both must exit 0 silently.
  assert.equal(res.status, 0);
});

test('crashing binary fails open with silence', () => {
  const store = tmpStore('crash');
  const res = fire(RECALL, '{"prompt":"scanner module latency dissolution","cwd":"/tmp"}', {
    ENGRAM_STORAGE: store,
    ENGRAM_STUB_MODE: 'crash',
    ENGRAM_STUB_RESPONSE: RELEVANT,
  });
  assert.equal(res.status, 0);
  assert.equal(res.stdout.trim(), '');
});

test('usage-error binary fails open with silence (env override path)', () => {
  const store = tmpStore('usage');
  const res = fire(RECALL, '{"prompt":"scanner module latency dissolution","cwd":"/tmp"}', {
    ENGRAM_STORAGE: store,
    ENGRAM_STUB_MODE: 'usage-error',
    ENGRAM_STUB_RESPONSE: RELEVANT,
  });
  assert.equal(res.status, 0);
  assert.equal(res.stdout.trim(), '');
});

test('heartbeat records counters only, never prompt text', () => {
  const store = tmpStore('heartbeat');
  fire(RECALL, '{"prompt":"scanner module latency dissolution","cwd":"/tmp"}', {
    ENGRAM_STORAGE: store,
    ENGRAM_STUB_RESPONSE: RELEVANT,
  });
  const hb = fs.readFileSync(path.join(store, 'hook-heartbeat.jsonl'), 'utf8');
  assert.ok(!hb.includes('scanner'), 'no prompt text in heartbeat');
  assert.match(hb, /"injected":\d+/);
});

test('max 3 items injected regardless of hits', () => {
  const store = tmpStore('cap');
  const five = recallWith(
    [0.5, 0.2, 0.15, 0.14, 0.13],
    ['exact', 'weak', 'weak', 'weak', 'weak'],
    ['memory', 'memory', 'memory', 'memory', 'memory'],
  );
  const res = fire(RECALL, '{"prompt":"scanner module latency dissolution","cwd":"/tmp"}', {
    ENGRAM_STORAGE: store,
    ENGRAM_STUB_RESPONSE: five,
  });
  const out = JSON.parse(res.stdout);
  const items = (out.hookSpecificOutput.additionalContext.match(/•/g) || []).length;
  assert.ok(items <= 3, `expected ≤3 items, got ${items}`);
});

// ---------------------------------------------------------------- stale-index
test('stale-index: threshold met emits the Stop reminder', () => {
  const store = tmpStore('staleok');
  const statusStub = path.join(store, 'status-stub.cjs');
  fs.writeFileSync(
    statusStub,
    `#!/usr/bin/env node
console.log(JSON.stringify({ok:true,command:'status',project:'t',last_index_at:1,indexed_files:10,entities:1,relationships:0,memories:1,beliefs:0}));
`,
    { mode: 0o755 },
  );
  // Run in this repo (git present, many commits since epoch=1)
  const res = fire(STALE, `{"cwd":${JSON.stringify(path.join(__dirname, '..', '..'))}}`, {
    ENGRAM_STORAGE: store,
    ENGRAM_MCP_BIN: statusStub,
    ENGRAM_STALE_THRESHOLD: '2',
  });
  assert.equal(res.status, 0);
  const out = JSON.parse(res.stdout);
  assert.equal(out.hookSpecificOutput.hookEventName, 'Stop');
  assert.match(out.hookSpecificOutput.additionalContext, /stale/);
});

test('stale-index: no index (last_index_at null) stays silent', () => {
  const store = tmpStore('stalenull');
  const statusStub = path.join(store, 'status-stub.cjs');
  fs.writeFileSync(
    statusStub,
    `#!/usr/bin/env node
console.log(JSON.stringify({ok:true,command:'status',project:'t',last_index_at:null,indexed_files:0,entities:0,relationships:0,memories:0,beliefs:0}));
`,
    { mode: 0o755 },
  );
  const res = fire(STALE, '{"cwd":"/tmp"}', {
    ENGRAM_STORAGE: store,
    ENGRAM_MCP_BIN: statusStub,
  });
  assert.equal(res.status, 0);
  assert.equal(res.stdout.trim(), '');
});
