#!/usr/bin/env node
/* Render probe cell fetcher (job 004, plan DEC-P03 / TASK-016).
 * Drives one built page at an EXACT viewport width (Emulation.
 * setDeviceMetricsOverride — new headless clamps --window-size to ~500px,
 * so AC-013's 360px check needs the CDP override) through the site's inert
 * ?probe=1 reporter, waits for a probe whose measurements are valid, and
 * writes the probe JSON to a file.
 * Usage: node probe-cell.mjs <http-url-with-probe-params> <width> <out.json>
 */
import { spawn } from 'node:child_process';
import { writeFileSync, mkdtempSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const [url, width, out] = process.argv.slice(2);
if (!url || !width || !out) {
  console.error('usage: node probe-cell.mjs <url> <width> <out.json>');
  process.exit(2);
}

const port = 9800 + (process.pid % 400);
const profile = mkdtempSync(join(tmpdir(), 'engram-probe-cdp-'));
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

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

function run() {
  return new Promise(async (resolve, reject) => {
    try {
      let targets;
      for (let i = 0; i < 50; i++) {
        try {
          const res = await fetch(`http://127.0.0.1:${port}/json/list`);
          targets = await res.json();
          if (targets.some((t) => t.type === 'page')) break;
        } catch (e) { /* chrome not up yet */ }
        await sleep(200);
      }
      const page = targets.find((t) => t.type === 'page');
      const ws = new WebSocket(page.webSocketDebuggerUrl);
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
      await new Promise((r) => { ws.onopen = r; });

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
    }
  });
}

run()
  .then((code) => { chrome.kill(); process.exit(code); })
  .catch((e) => { console.error('probe-cell:', e.message); chrome.kill(); process.exit(1); });
