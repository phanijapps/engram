/**
 * `@engram/runtime` — the TypeScript operational layer for engram's three
 * deployable modules (RFC-0017: ingest / maintenance / mcp). Each module lives
 * in its own sub-directory and composes the Phase A provider facade
 * (`createNativeProviderTransport` from `@engram/node`); shared config/scope
 * helpers live here.
 *
 * This package-root entry is a narrow facade re-exporting the shared helpers
 * (and, once the ingest module lands, `runIngest`).
 */
export { buildEngramConfig, buildScope, type ScanSummary } from "./shared/config.js";
export { runIngest } from "./ingest/cli.js";
export { createMcpServer, startMcpHttpServer } from "./mcp/server.js";
export { runMaintain, type ConsolidationRun } from "./maintenance/cli.js";
