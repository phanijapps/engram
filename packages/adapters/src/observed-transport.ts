import type { EngramTransport } from "@engram/client";
import type {
  ContextPayload,
  ForgetRequest,
  ForgetResult,
  RetrievalRequest,
  WriteMemoryRequest,
  WriteMemoryResponse
} from "@engram/contracts";

import type { AdapterObserver, AdapterOperation } from "./events.js";
import { classifyError, errorMessage, retrievalTrace } from "./events.js";
import { emitSafely } from "./observer.js";

/** Construction options for `createObservedTransport`. */
export interface ObservedTransportOptions {
  transport: EngramTransport;
  observer?: AdapterObserver;
  now?: () => Date;
}

/** Wraps an Engram transport with framework-neutral observability events. */
export function createObservedTransport(options: ObservedTransportOptions): EngramTransport {
  return new ObservedEngramTransport(options);
}

class ObservedEngramTransport implements EngramTransport {
  readonly #transport: EngramTransport;
  readonly #observer: AdapterObserver | undefined;
  readonly #now: () => Date;

  constructor(options: ObservedTransportOptions) {
    this.#transport = options.transport;
    this.#observer = options.observer;
    this.#now = options.now ?? (() => new Date());
  }

  writeMemory(request: WriteMemoryRequest): Promise<WriteMemoryResponse> {
    return this.#observe("writeMemory", () => this.#transport.writeMemory(request));
  }

  retrieve(request: RetrievalRequest): Promise<ContextPayload> {
    return this.#observe("retrieve", () => this.#transport.retrieve(request), async (payload) => {
      await emitSafely(this.#observer, retrievalTrace(payload, this.#timestamp()));
    });
  }

  forget(request: ForgetRequest): Promise<ForgetResult> {
    return this.#observe("forget", () => this.#transport.forget(request));
  }

  async #observe<T>(
    operation: AdapterOperation,
    execute: () => Promise<T>,
    afterSuccess?: (result: T) => Promise<void>
  ): Promise<T> {
    const startedAt = this.#now();
    await emitSafely(this.#observer, {
      kind: "operation_started",
      operation,
      at: startedAt.toISOString()
    });

    try {
      const result = await execute();
      const completedAt = this.#now();
      await afterSuccess?.(result);
      await emitSafely(this.#observer, {
        kind: "operation_succeeded",
        operation,
        at: completedAt.toISOString(),
        durationMs: completedAt.getTime() - startedAt.getTime()
      });
      return result;
    } catch (error) {
      const failedAt = this.#now();
      await emitSafely(this.#observer, {
        kind: "operation_failed",
        operation,
        at: failedAt.toISOString(),
        durationMs: failedAt.getTime() - startedAt.getTime(),
        errorKind: classifyError(error),
        message: errorMessage(error)
      });
      throw error;
    }
  }

  #timestamp(): string {
    return this.#now().toISOString();
  }
}
