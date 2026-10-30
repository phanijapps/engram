export { createEngramClient, EngramClient, type EngramClientOptions, EngramCodeBindingClient } from "./client.js";
export { createNativeEngramClient } from "./native.js";

export type { EngramTransport } from "./transport.js";
export {
  EngramCodeBindingUnsupportedError,
  isBindingDenied,
  type CodeBindRequest,
  type CodeBindingCheckpoint,
  type CodeBindingRecord,
  type CodeBindingStatusResult,
  type CodeFreshnessState,
  type CodeRefreshRequest,
  type CodeRegisterRequest,
  type CodeRefFreshness,
  type CodeRefObservation,
  type CodeScanRequest,
  type CheckpointGeneration
} from "./code-binding.js";
