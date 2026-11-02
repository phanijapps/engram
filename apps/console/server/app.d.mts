/**
 * Type declarations for the plain-JS server core (kept dependency-free JS so
 * the runtime needs no build step; typed here for the TS tests/UI).
 */
import type { Server } from "node:http";

export declare const LOOPBACK_HOSTS: Set<string>;

export declare function assertLoopbackHost(host: string): void;

export declare function partitionKey(scope: {
  tenant: string;
  subject?: string | null;
  workspace?: string | null;
  session?: string | null;
  environment?: string | null;
}): string;

export declare function parsePartitionKey(key: string): {
  tenant: string;
  subject?: string | null;
  workspace?: string | null;
  session?: string | null;
  environment?: string | null;
};

export declare function classifyDegradedState(errorText: unknown): {
  state: string;
  message: string;
};

export declare function createConsoleServer(options: {
  provider: unknown;
  backend: string;
  storage: string;
  uiDir?: string;
}): {
  server: Server;
  listen(port?: number, host?: string): Promise<Server>;
};
