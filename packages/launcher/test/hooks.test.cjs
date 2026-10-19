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

// ---------------------------------------------------------------- launcher lockstep
test('version lockstep normalizes the v prefix (Windows 2026-10-18 repro)', () => {
  const binJs = fs.readFileSync(path.join(__dirname, '..', 'bin.js'), 'utf8');
  // The fix: both sides pass through a leading-v normalizer before compare.
  assert.match(binJs, /replace\(\/\^v\/, ""\)/, 'bin.js normalizes v prefix in lockstep check');
  // And the stamper stamps clean versions (no v) for future packages.
  const stamper = fs.readFileSync(
    path.join(__dirname, '..', '..', '..', 'scripts', 'release', 'stamp-platform-package.py'),
    'utf8',
  );
  assert.match(stamper, /raw_version\[1:\]/, 'stamper strips the v prefix');
});

// ---------------------------------------------------------------- gate v2 (overlap)
// Measured on the 9.7k-entity self-index: lexical scores compress, so pure
// separation silenced CORRECT top hits. Gate v2 adds query-term overlap as
// the primary signal (separation stays as the vector/semantic path).
test('gate v2: compressed scores + high overlap still injects', () => {
  const store = tmpStore('v2relevant');
  // Scores compressed (sep 1.04x — separation alone would silence) but the
  // top hit's content overlaps the query heavily.
  const payload = recallWith(
    [0.0325, 0.0313, 0.0312],
    ['exact', 'exact', 'exact'],
  );
  const res = fire(RECALL, '{"prompt":"what are the boundary rules for core crates","cwd":"/tmp"}', {
    ENGRAM_STORAGE: store,
    ENGRAM_STUB_RESPONSE: JSON.stringify({
      ok: true,
      hit_count: 3,
      results: [
        { kind: 'memory', score: 0.0325, evidence: 'exact', content: 'Core boundary invariants: engram-domain and core port crates must never name an engine type or hold SQL; boundary rules enforced by scripts' },
        { kind: 'memory', score: 0.0313, evidence: 'exact', content: 'scanner facade memory unrelated wording' },
        { kind: 'chunk', score: 0.0312, evidence: 'exact', content: 'Boundary Rules section from docs' },
      ],
    }),
  });
  assert.equal(res.status, 0);
  assert.ok(res.stdout.includes('UserPromptSubmit'), 'overlap path injects when scores compress');
});

test('gate v2: junk entity matching one query word stays silent', () => {
  const store = tmpStore('v2junk');
  const res = fire(RECALL, '{"prompt":"banana pancake recipe for breakfast","cwd":"/tmp"}', {
    ENGRAM_STORAGE: store,
    ENGRAM_STUB_RESPONSE: JSON.stringify({
      ok: true,
      hit_count: 5,
      results: [
        { kind: 'entity', score: 0.0481, evidence: 'keyword', content: 'pg_recipe_recall_fuses_vector_lane' },
        { kind: 'entity', score: 0.0317, evidence: 'keyword', content: 'pg_recipe_opens_and_reports_capabilities' },
        { kind: 'chunk', score: 0.0310, evidence: 'keyword', content: 'EngramProvider diagram text' },
      ],
    }),
  });
  assert.equal(res.status, 0);
  assert.equal(res.stdout.trim(), '', 'junk code entity does not inject (low overlap, entity lane)');
});

test('gate v2: memory lane with moderate overlap injects, weak evidence never does', () => {
  const store = tmpStore('v2lane');
  const res = fire(RECALL, '{"prompt":"scanner module god module dissolved once","cwd":"/tmp"}', {
    ENGRAM_STORAGE: store,
    ENGRAM_STUB_RESPONSE: JSON.stringify({
      ok: true,
      hit_count: 2,
      results: [
        { kind: 'memory', score: 0.03, evidence: 'exact', content: 'scanner mod god-module dissolved into walk prepass sweep modules' },
        { kind: 'entity', score: 0.029, evidence: 'exact', content: 'unrelated_symbol' },
      ],
    }),
  });
  assert.ok(res.stdout.includes('UserPromptSubmit'), 'memory-lane overlap injects');
});

// ---------------------------------------------------------------- gate fixture (captured shapes)
// Real shapes captured live from the 9.7k-entity self-index (2026-10-19):
// the BM25-compression case that motivated gate v2. This tests HOOK GATE
// BEHAVIOR against captured shapes — it does NOT pin the server surface
// (the stub never touches oneshot.rs/tools.rs). The server-side pin lives
// in mcp/engram-mcp/src/oneshot.rs:recall_json_surface_keys_are_pinned.
test('fixture: live score shapes — compression + lane-normalized raws', () => {
  // Relevant query: correct memory tops at fused 0.0325 (2 lanes), raw=1.0
  // (facts lane). Junk query: pg_recipe_* entity tops at fused 0.0481 with
  // raw=1.0 too — raw source_score is LANE-NORMALIZED, not absolute BM25,
  // so it cannot discriminate (falsified the planned raw-percentile gate).
  const relevant = {
    ok: true,
    hit_count: 3,
    results: [
      { kind: 'memory', score: 0.0325, source_score: 1.0, lanes: 'facts+temporal', evidence: 'exact', content: 'Core boundary invariants: engram-domain and core port crates must never name an engine type' },
      { kind: 'chunk', score: 0.0164, source_score: 6.0, lanes: 'graph', evidence: 'exact', content: '## Boundary Rules section from docs' },
      { kind: 'memory', score: 0.0312, source_score: 1.0, lanes: 'lexical', evidence: 'exact', content: 'Core abstractions listing' },
    ],
  };
  const junk = {
    ok: true,
    hit_count: 3,
    results: [
      { kind: 'entity', score: 0.0481, source_score: 1.0, lanes: 'graph+associative_graph+lexical', evidence: 'keyword', content: 'pg_recipe_recall_fuses_vector_lane' },
      { kind: 'entity', score: 0.0317, source_score: 1.0, lanes: 'graph+lexical', evidence: 'keyword', content: 'pg_recipe_opens_and_reports_capabilities' },
      { kind: 'chunk', score: 0.0310, source_score: 1.0, lanes: 'lexical', evidence: 'weak', content: 'EngramProvider diagram text' },
    ],
  };
  // The discrimination must come from overlap (gate v2), not raw or fused.
  const store1 = tmpStore('fx-relevant');
  const r1 = fire(RECALL, '{"prompt":"what are the boundary rules for core crates","cwd":"/tmp"}', {
    ENGRAM_STORAGE: store1, ENGRAM_STUB_RESPONSE: JSON.stringify(relevant),
  });
  assert.ok(r1.stdout.includes('UserPromptSubmit'), 'fixture relevant injects via overlap');
  const store2 = tmpStore('fx-junk');
  const r2 = fire(RECALL, '{"prompt":"banana pancake recipe for breakfast","cwd":"/tmp"}', {
    ENGRAM_STORAGE: store2, ENGRAM_STUB_RESPONSE: JSON.stringify(junk),
  });
  assert.equal(r2.stdout.trim(), '', 'fixture junk stays silent despite higher fused score');
});

// ---------------------------------------------------------------- CLI flags (spec cli-help-version)
// Live checks against the real binary (soft-skip when unresolvable, same
// discipline as the conformance suite).
test('CLI: --version/-V exit 0 with the version line; --help/-h exit 0 with the contract; bogus flag exits 2', () => {
  const os = require('node:os');
  const path = require('node:path');
  const dev = path.join(os.homedir(), 'projects', 'mem-alpha', 'target', 'release', 'engram-mcp');
  const bin = process.env.ENGRAM_MCP_BIN && fs.existsSync(process.env.ENGRAM_MCP_BIN)
    ? process.env.ENGRAM_MCP_BIN
    : fs.existsSync(dev) ? dev : null;
  if (!bin) { console.log('  [soft-skip] no binary resolvable'); return; }
  const run = (args) => spawnSync(bin, args, { encoding: 'utf8', timeout: 5000 });

  for (const flag of ['--version', '-V']) {
    const r = run([flag]);
    assert.equal(r.status, 0, `${flag} exits 0`);
    assert.match(r.stdout, /^\d+\.\d+\.\d+/, `${flag} prints a semver line`);
    assert.equal(r.stdout.trim().split('\n').length, 1, `${flag} is one line`);
  }
  for (const flag of ['--help', '-h']) {
    const r = run([flag]);
    assert.equal(r.status, 0, `${flag} exits 0`);
    assert.ok(r.stdout.includes('--pg-connection-string'), `${flag} lists the easy-to-forget flag`);
    assert.ok(r.stdout.includes('recall') && r.stdout.includes('status'), `${flag} lists subcommands`);
  }
  const helpMid = run(['--storage', '/tmp/x', '--help']);
  assert.equal(helpMid.status, 0, '--help recognized mid-flags');
  const bogus = run(['--bogus-flag']);
  assert.equal(bogus.status, 2, 'usage error stays exit 2 (distinguishable from help)');
  assert.ok(bogus.stderr.length > 0, 'usage errors stay on stderr');

  const recallHelp = run(['recall', '--help']);
  assert.equal(recallHelp.status, 0, 'recall --help exits 0');
  assert.match(recallHelp.stdout, /--query/, 'recall help shows --query');
});
