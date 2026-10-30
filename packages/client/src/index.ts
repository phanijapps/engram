export { createEngramClient, EngramClient, type EngramClientOptions, EngramCodeBindingClient } from "./client.js";
export { createNativeEngramClient } from "./native.js";

export type { EngramTransport } from "./transport.js";
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
