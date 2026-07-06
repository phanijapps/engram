# Engram Rust Integration Example

This example demonstrates how to embed Engram as a Rust library using the integration contract facade.

## What it demonstrates

1. **Configuration**: Creating an `EngramConfig` with storage path, trusted root, and capability policy
2. **Bootstrap**: Using `EngramProvider::bootstrap()` to initialize the provider facade
3. **Capability reporting**: Checking `CapabilityReport` for all 10 feature families
4. **Type-safe integration**: Using strongly-typed repository handles through the facade

## Running the example

```bash
cd examples/rust-integration
cargo run
```

## Key concepts

### Provider Facade
The `EngramProvider` is a single entry point that bundles repository handles with capability reporting:

```rust
let provider = EngramProvider::bootstrap(&config)?;
let report = provider.capabilities();
```

### Capability Reporting
Each feature family has a `CapabilityState` that indicates whether it's supported:

- `Supported`: Feature is ready to use
- `Unsupported`: Feature is not available (with reason code)
- `Degraded`: Feature works with limitations
- `RequiresMigration`: Storage needs migration
- `RequiresReindex`: Vector index needs rebuilding
- `Misconfigured`: Configuration issue

### Path Confinement
The provider validates that storage paths are within the trusted root directory for security.

### Capability Policy
The `FailClosed` policy ensures that unsupported operations return errors rather than silent failures.

## Integration contract

The integration contract enables external applications to:
- Embed Engram without adopting its storage layout
- Check capabilities at bootstrap time
- Use typed repository handles for supported features
- Maintain compatibility across releases through stable contracts
