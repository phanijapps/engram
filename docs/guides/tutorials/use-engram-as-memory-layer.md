# Add memory to a Rust agent with Engram

> At the end of this tutorial you'll have a running Rust program that writes a memory through Engram and reads it back — the core loop of an agent with durable memory.

This tutorial is for someone using Engram for the first time as a memory layer. If you already have Engram integrated and want a specific operation — switching embedding models, building a knowledge graph, querying beliefs — this page sets up the foundation; the linked guides cover the rest.

## Before you begin

You need:

- Rust 1.85 or later, with Cargo
- A working knowledge of `async`/`await` in Rust
- The Engram source, cloned locally (Engram is consumed from its workspace today; it is not yet published to crates.io)

If async Rust is new, skim the [async book](https://rust-lang.github.io/async-book/) and come back.

## Step 1 — Add the Engram crates

From your clone of Engram, point your project at the workspace crates with path dependencies. In your `Cargo.toml`:

```toml
[dependencies]
engram-conformance = { path = "../path/to/engram/adapters/integration" }
engram-integration = { path = "../path/to/engram/core/integration" }
engram-domain = { path = "../path/to/engram/core/domain" }
tokio = { version = "1", features = ["full"] }
chrono = "0.4"
```

The shortest path to a working program is the bundled example, which already has these wired:

```bash
cargo run --package engram-integration-example
```

You should see:

```
memory capability: Supported
wrote memory id: memory-000001
retrieved 1 item(s):
  - The user prefers concise answers with code examples.
```

> If you get a path-resolution error, confirm the `path =` lines point at your Engram clone. The example lives at `examples/rust-integration/` inside the Engram repo.

## Step 2 — Configure and bootstrap

Engram is reached through one entry point: `bootstrap_provider`. It takes a configuration, constructs every storage adapter, runs a conformance check for each capability family, and hands back a provider whose handles are usable only where the check passed.

```rust
use engram_conformance::bootstrap_provider;
use engram_domain::types::ScopeMappingStrategy;
use engram_integration::{CapabilityPolicy, EmbeddingProviderConfig, EngramConfig, MigrationMode};

let storage_path = std::env::temp_dir().join("engram-getting-started");
let config = EngramConfig::new(
    storage_path,
    std::env::temp_dir(),          // trusted_root: must contain storage_path
    ScopeMappingStrategy::Strict,  // enforce tenant/workspace boundaries
    EmbeddingProviderConfig {
        provider_type: "fastembed".to_string(),
        model: "bge-small-en-v1.5".to_string(),
        dimensions: 384,
        prompt_profile: "query".to_string(),
        normalization: None,
    },
    MigrationMode::DryRun,
    CapabilityPolicy::FailClosed,
);

let provider = bootstrap_provider(&config)?;
```

The configuration carries five things that shape every later operation:

- **`storage_path`** — where the SQLite databases live. Each capability family gets its own file (`memory.db`, `knowledge.db`, …).
- **`trusted_root`** — Engram refuses any storage path that escapes this directory. It is a confinement guard, not a suggestion.
- **`ScopeMappingStrategy`** — `Strict` means a query in one tenant never sees another tenant's records.
- **`EmbeddingProviderConfig`** — which embedding model the vector index is built for. Dimensions are part of the identity, not just a size check; see Step 5.
- **`MigrationMode`** and **`CapabilityPolicy`** — `DryRun` + `FailClosed` means imports validate without writing, and unsupported operations return errors instead of silent empties.

You should see the provider build without error. Check the memory capability before you use it:

```rust
println!("memory capability: {:?}", provider.capabilities().memory);
```

```
memory capability: Supported
```

> If a family reports `Unsupported`, its handle will be `None` and the reason code tells you why. A capability is only `Supported` when its conformance check passed — you never reach a broken adapter through the facade.

## Step 3 — Write a memory

Pull the memory handle off the provider and write one observation.

```rust
use chrono::Utc;
use engram_domain::{
    Actor, ActorKind, AllowedUse, DeleteMode, Id, MemoryContent, MemoryKind, Policy,
    Provenance, Requester, Retention, Scope, Sensitivity, Visibility, WriteMemoryRequest,
};

let memory = provider.memory().expect("memory is Supported");

let scope = Scope {
    tenant: "my-agent".to_string(),
    subject: Some("session-1".to_string()),
    workspace: None,
    session: None,
    environment: Some("dev".to_string()),
};
let requester = Requester {
    actor: Actor {
        id: Id::from("agent-1"),
        kind: ActorKind::Agent,
        display_name: Some("MyAgent".to_string()),
        metadata: None,
    },
    roles: Vec::new(),
    permissions: vec!["memory.write".to_string(), "memory.retrieve".to_string()],
    on_behalf_of: None,
};

let written = memory
    .write_memory(WriteMemoryRequest {
        kind: MemoryKind::Observation,
        content: MemoryContent {
            text: "The user prefers concise answers with code examples.".to_string(),
            summary: None,
            entities: Vec::new(),
            language: Some("en".to_string()),
            format: None,
            structured: None,
            hash: None,
        },
        scope: scope.clone(),
        requester: requester.clone(),
        provenance: Provenance {
            source: "getting-started".to_string(),
            actor: requester.actor.clone(),
            observed_at: Utc::now(),
            evidence: Vec::new(),
            derivations: Vec::new(),
            confidence: None,
            method: None,
        },
        policy: Policy {
            visibility: Visibility::Workspace,
            retention: Retention::Durable,
            sensitivity: Some(Sensitivity::Low),
            allowed_uses: vec![AllowedUse::Retrieval],
            expires_at: None,
            delete_mode: Some(DeleteMode::Tombstone),
        },
        links: Vec::new(),
        idempotency_key: None,
    })
    .await?;
```

You should see:

```
wrote memory id: memory-000001
```

A few fields deserve a moment, because they shape how the memory is stored and who can see it:

- **`scope`** — the tenant, subject, workspace, session, and environment the memory belongs to. Retrieval is filtered by scope; a memory written under tenant `my-agent` is invisible to tenant `other-agent`.
- **`policy.visibility`** — `Workspace` makes it retrievable within the workspace; `Private` restricts it further.
- **`policy.retention`** — `Durable` persists across restarts.
- **`provenance`** — where the memory came from and when. Engram keeps provenance on every record for audit and consolidation.

## Step 4 — Retrieve the memory back

Read with a retrieval request against the same scope.

```rust
use engram_domain::RetrievalRequest;

let context = memory
    .retrieve(RetrievalRequest {
        query: "user preferences".to_string(),
        scope: scope.clone(),
        requester: requester.clone(),
        modes: Vec::new(),
        filters: None,
        cues: Vec::new(),
        limit: Some(5),
        budget: None,
        include_explanations: None,
    })
    .await?;

println!("retrieved {} item(s):", context.items.len());
for item in &context.items {
    println!("  - {}", item.content);
}
```

You should see:

```
retrieved 1 item(s):
  - The user prefers concise answers with code examples.
```

That round-trip is the whole memory loop: write under a scope, retrieve against the same scope, get the relevant records back. Everything else Engram does — knowledge graphs, beliefs, hierarchy — builds on this substrate.

## Step 5 — Use Ollama for embeddings

The `EmbeddingProviderConfig` in Step 2 named FastEmbed. To generate embeddings with a local Ollama daemon instead, change two things.

First, enable the `ollama` feature on the integration crate so the Ollama provider is compiled in:

```toml
[dependencies]
engram-integration = { path = "../path/to/engram/core/integration", features = ["ollama"] }
```

Then point the configuration at your Ollama model. The dimensions must match what that model actually produces.

```rust
let config = EngramConfig::new(
    storage_path,
    std::env::temp_dir(),
    ScopeMappingStrategy::Strict,
    EmbeddingProviderConfig {
        provider_type: "ollama".to_string(),
        model: "nomic-embed-text".to_string(),
        dimensions: 768,               // nomic-embed-text produces 768-dim vectors
        prompt_profile: "query".to_string(),
        normalization: None,
    },
    MigrationMode::DryRun,
    CapabilityPolicy::FailClosed,
);
```

Common Ollama embedding models and their dimensions:

| model | dimensions |
| --- | --- |
| `nomic-embed-text` | 768 |
| `mxbai-embed-large` | 1024 |
| `all-minilm` | 384 |

With Ollama running on `http://localhost:11434`, `bootstrap_provider` constructs a vector index keyed to that model's embedding space. Embedding calls hit the daemon at query time.

> If you change the model after vectors already exist, the provider reports `RequiresReindex` for the vectors family rather than mixing incompatible embeddings. Rebuild the index against the new model.

Engram identifies an embedding space by provider + model + dimensions + prompt profile, not dimensions alone. Two 768-dimensional models are not interchangeable; the guard prevents silently searching one model's vectors with another's queries.

## What you built

You have a Rust program that configures Engram, bootstraps it through one facade, writes a scoped memory, and reads it back. The data persists in SQLite under your storage path, confined to the trusted root, with scope isolation enforced on every read. Swap the embedding provider in configuration and the vector index follows.

## Next steps

- To store structured knowledge — entities, relationships, graphs: <!-- TODO: link to knowledge-graph how-to -->
- To track beliefs and detect contradictions: <!-- TODO: link to beliefs how-to -->
- To organize memories hierarchically: <!-- TODO: link to hierarchy how-to -->
- To understand how Engram turns a code repository into a knowledge graph automatically: [How repos get indexed](../explanation/how-repos-get-indexed.md)
- To import existing data with dry-run validation: <!-- TODO: link to migration how-to -->

## See also

- The runnable example this tutorial is built from: `examples/rust-integration/` in the Engram repo
