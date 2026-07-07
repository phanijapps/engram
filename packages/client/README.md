# @engram/client

Typed client facade for Engram v1 operations.

The client delegates behavior to an injected `EngramTransport`. It does not
implement memory semantics, policy enforcement, retrieval, or storage.

```ts
import { createEngramClient } from "@engram/client";

const client = createEngramClient({ transport });
```
