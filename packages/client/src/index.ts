export { createEngramClient, EngramClient, type EngramClientOptions } from "./client.js";
export { createNativeEngramClient } from "./native.js";
/** Transport boundary consumed by `EngramClient` and JavaScript-side adapters. */
export type { EngramTransport } from "./transport.js";
