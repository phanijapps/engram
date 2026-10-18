#!/usr/bin/env node
'use strict';
// Stub engram-mcp for hook evals. Reads the canned response from
// ENGRAM_STUB_RESPONSE (JSON string) or a fail mode from ENGRAM_STUB_MODE.
const mode = process.env.ENGRAM_STUB_MODE || 'ok';
if (mode === 'usage-error') { console.error('unknown argument'); process.exit(2); }
if (mode === 'crash') { process.exit(1); }
if (mode === 'hang') { setTimeout(() => {}, 10000); }
const payload = JSON.parse(process.env.ENGRAM_STUB_RESPONSE || '{"ok":true,"hit_count":0,"results":[]}');
console.log(JSON.stringify(payload));
