import { createNativeMemoryTransport, type NativeMemoryTransportOptions } from "@engram/node";

import { createEngramClient, type EngramClient } from "./client.js";

/** Creates an Engram client backed by the Rust native Node transport. */
export function createNativeEngramClient(
  options: NativeMemoryTransportOptions = {}
): EngramClient {
  return createEngramClient({ transport: createNativeMemoryTransport(options) });
}
