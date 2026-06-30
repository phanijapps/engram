# @engram/adapters

Framework-neutral adapter utilities for JavaScript-side integrations.

Adapter code must depend on `@engram/contracts` and must not redefine domain
types.

## Observable Transport

`createObservedTransport` wraps any `EngramTransport` and emits structured
operation events. Runtime packages can forward those events to logs, telemetry,
gateway traces, or tests without changing memory behavior.

```ts
import { createObservedTransport } from "@engram/adapters";

const transport = createObservedTransport({
  transport: baseTransport,
  observer: {
    emit(event) {
      console.log(event);
    }
  }
});
```
