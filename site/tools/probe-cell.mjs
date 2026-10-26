#!/usr/bin/env node
/* Render probe cell fetcher (job 004, plan DEC-P03 / TASK-016).
 * Drives one built page at an EXACT viewport width (Emulation.
 * setDeviceMetricsOverride — new headless clamps --window-size to ~500px,
 * so AC-013's 360px check needs the CDP override) through the site's inert
 * ?probe=1 reporter, waits for a probe whose measurements are valid, and
 * writes the probe JSON to a file.
 * Hygiene (FIND-CODE-004): every temp profile directory is removed in a
 * finally block (round 0 leaked one per matrix cell), and the debug port
 * retries on bind failure instead of trusting a fixed pid-derived port.
 * Usage: node probe-cell.mjs <http-url-with-probe-params> <width> <out.json>
 */
import { spawn } from 'node:child_process';
import { existsSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const [url, width, out] = process.argv.slice(2);
if (!url || !width || !out) {
  console.error('usage: node probe-cell.mjs <url> <width> <out.json>');
  process.exit(2);
}

const BASE_PORT = 9800 + (process.pid % 400);
const PORT_ATTEMPTS = 5;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

/* Every spawned chrome and every created profile is tracked and cleaned in
 * the finally block — no leak on success, failure or retry. Cleanup WAITS
 * for each chrome to actually exit before removing its profile: chrome's
 * crashpad helper can re-create files under the profile directory after a
 * naive synchronous rm, so removal is verified and retried (FIND-CODE-004). */
const chromes = [];
const profiles = [];

async function cleanup() {
  for (const chrome of chromes) {
    try { chrome.kill(); } catch (e) { /* already gone */ }
  }
  await Promise.all(chromes.map((c) => new Promise((res) => {
    if (c.exitCode !== null) return res();
    c.once('exit', res);
    setTimeout(res, 3000); // never hang the cell on a stubborn process
  })));
  for (const profile of profiles) {
    for (let i = 0; i < 3; i++) {
      try { rmSync(profile, { recursive: true, force: true }); } catch (e) { /* retried below */ }
      if (!existsSync(profile)) break;
      await sleep(150);
    }
  }
}

function launchChrome(port) {
  const profile = mkdtempSync(join(tmpdir(), 'engram-probe-cdp-'));
  profiles.push(profile);
  const chrome = spawn('google-chrome', [
    '--headless=new',
    `--remote-debugging-port=${port}`,
    '--no-first-run',
    '--no-default-browser-check',
    '--disable-gpu',
    '--hide-scrollbars',
    `--user-data-dir=${profile}`,
    'about:blank',
  ], { stdio: 'ignore' });
  chromes.push(chrome);
  return chrome;
}

/* Resolve once a page target answers on the debug port; null = bind failure
 * or chrome death (caller retries on the next port). */
async function waitForEndpoint(port, deadlineMs) {
  const deadline = Date.now() + deadlineMs;
  while (Date.now() < deadline) {
    try {
      const res = await fetch(`http://127.0.0.1:${port}/json/list`);
      const targets = await res.json();
      const page = targets.find((t) => t.type === 'page');
      if (page) return page;
    } catch (e) { /* not up yet */ }
    await sleep(200);
  }
  return null;
}

function drive(page, wsUrl) {
  return new Promise(async (resolve, reject) => {
    let ws;
    try {
      ws = new WebSocket(wsUrl);
      let nextId = 1;
      const pending = new Map();
      ws.onmessage = (ev) => {
        const msg = JSON.parse(ev.data);
        if (msg.id && pending.has(msg.id)) {
          pending.get(msg.id)(msg);
          pending.delete(msg.id);
        }
      };
      const call = (method, params = {}) => new Promise((res) => {
        const id = nextId++;
        pending.set(id, res);
        ws.send(JSON.stringify({ id, method, params }));
      });
      const evaluate = async (expression) => {
        const r = await call('Runtime.evaluate', { expression, returnByValue: true });
        return r.result?.result?.value;
      };
      await new Promise((r, rej) => { ws.onopen = r; ws.onerror = () => rej(new Error('CDP websocket failed')); });

      await call('Page.enable');
      await call('Emulation.setDeviceMetricsOverride', {
        width: Number(width),
        height: 900,
        deviceScaleFactor: 1,
        mobile: false,
      });
      await call('Page.navigate', { url });

      // Wait for the probe element AND a layout pass that reports a real
      // clientWidth (a loaded page can never measure 0).
      let probe = null;
      for (let i = 0; i < 100; i++) {
        probe = await evaluate(`(() => {
          const pre = document.querySelector('#engram-probe');
          if (!pre) return null;
          if (document.documentElement.clientWidth <= 0) return null;
          return pre.textContent;
        })()`);
        if (probe) break;
        await sleep(150);
      }
      if (!probe) throw new Error('probe never produced a valid measurement');
      const json = probe.slice('ENGRAM-PROBE '.length);
      JSON.parse(json); // validate
      writeFileSync(out, json, 'utf-8');
      resolve(0);
    } catch (e) {
      reject(e);
    } finally {
      try { ws?.close(); } catch (e) { /* best effort */ }
    }
  });
}

async function run() {
  for (let attempt = 0; attempt < PORT_ATTEMPTS; attempt++) {
    const port = BASE_PORT + attempt * 37; // retry space; sequential cells re-roll via pid
    launchChrome(port);
    const page = await waitForEndpoint(port, 10_000);
    if (page) {
      try {
        return await drive(page, page.webSocketDebuggerUrl);
      } catch (e) {
        if (attempt === PORT_ATTEMPTS - 1) throw e;
        console.error(`probe-cell: attempt ${attempt + 1} failed (${e.message}); retrying on next port`);
      }
    } else if (attempt === PORT_ATTEMPTS - 1) {
      throw new Error(`debug port never answered after ${PORT_ATTEMPTS} attempts (base ${BASE_PORT})`);
    } else {
      console.error(`probe-cell: port ${port} did not come up; retrying on next port`);
    }
  }
  throw new Error('unreachable');
}

try {
  const code = await run();
  await cleanup();
  process.exit(code);
} catch (e) {
  console.error('probe-cell:', e.message);
  await cleanup();
  process.exit(1);
}
