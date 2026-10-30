/** Main client factory, client class, options, and bound-code client exports. */
export { createEngramClient, EngramClient, type EngramClientOptions, EngramCodeBindingClient } from "./client.js";
/** Native-provider client factory. */
export { createNativeEngramClient } from "./native.js";

/** Transport abstraction for custom client transports. */
export type { EngramTransport } from "./transport.js";
/** Bound-code errors, helpers, requests, results, and generated contract types. */
export {
  EngramCodeBindingUnsupportedError,
  isBindingDenied,
  type CodeBindRequest,
  type CodeBinding,
  type CodeBindingCheckpoint,
  type CodeBindingStatus,
  type CodeFreshnessState,
  type CodeRefreshRequest,
  type CodeRegisterRequest,
  type CodeRefFreshness,
  type CodeRefObservation,
  type CodeScanFilter,
  type CodeScanRequest,
  type CodeScanResult,
  type BoundCodeScanResult,
  type LegacyDocumentScanResult,
  type CodeStatusRequest,
  type Scope
} from "./code-binding.js";
