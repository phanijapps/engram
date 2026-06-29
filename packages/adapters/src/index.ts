export {
  classifyError,
  errorMessage,
  retrievalTrace,
  type AdapterErrorKind,
  type AdapterEvent,
  type AdapterObserver,
  type AdapterOperation,
  type OperationFailedEvent,
  type OperationStartedEvent,
  type OperationSucceededEvent,
  type RetrievalTraceEvent
} from "./events.js";
export { createObservedTransport, type ObservedTransportOptions } from "./observed-transport.js";
