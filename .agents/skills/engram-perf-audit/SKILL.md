---
name: engram-perf-audit
description: Use when asked to make engram faster, before optimizing anything, or when a path is "slow" without attributed cause. Triggers on "performance", "optimize", "why is X slow", "make it faster", "profile this". Enforces measure-before-blame: instrument phases, compare SQL-in-CLI vs SQL-in-server, and only then change code. Born of the session where the blamed cost (json_extract) was 60x innocent and the real cost (Rust-side record deserialization) was invisible until measured.
---

# Skill: engram-perf-audit

The rule: **no optimization ships without a measured attribution.** Every
perf claim needs three numbers: before, attribution, after.

## Procedure

### 1. Isolate the phase before naming the cost

Time the phases separately. For a slow tool call, temporarily instrument:

```rust
let t0 = std::time::Instant::now();
let x = expensive_a();
let t_a = t0.elapsed();
let y = expensive_b();
eprintln!("PERF a={:.2}s b={:.2}s", t_a.as_secs_f32(), t0.elapsed().as_secs_f32() - t_a.as_secs_f32());
```

Then `scripts/dev/mcp_driver.py` (stderr is printed). Remove the
instrumentation before committing.

Zero-cost trick for SQL paths: run the SAME query in the sqlite3 CLI on the
same store (`file:...?mode=ro`). The 2026-08-28 case: a "1.1s listing query"
ran in **19ms** in the CLI — proving the cost was Rust-side record
deserialization around it, not the SQL.

### 2. Check the known attribution traps before believing a theory

- **First-call vs steady-state**: boot/model-load/lazy-init inflates the
  first timed call. Measure the SECOND call (send two, time the second).
- **Inference dominates embed paths**: ~60ms/chunk through BGE-small;
  listing is ~0.2s. Don't optimize listing when the model is 95% of the call.
- **Durable dedup changes the math**: any embed measurement must report the
  `new + reused` split — a "256-chunk" call may only run 80 inferences.
- **Watch for double-work regressions**: a leftover loop + trailing flush
  once made every embed exactly 2× — totals that are suspiciously round
  multiples of a cap are a smell.

### 3. Fix, then prove with the same measurement

Same store, same call shape, same cap. Report
`before → after (x.y×)` in the commit. If the win is below noise, say so and
revert rather than shipping complexity (the abandoned STORED-generated-column
attempt is the canonical example — SQLite cannot ALTER-ADD them, and the
failed migration silently took the store down).

### 4. Guard the regression class, not just the number

Each perf fix in this repo came with a semantics trap nearby (loose scope
predicate widening visibility 19k chunks; camelCase path probes returning
empty). After any query change, verify the COUNT invariant — e.g. pending-set
size before/after a listing rewrite must be identical.

## Recording

Benchmarks and methodology live under `docs/perf/`. Backlog perf items carry
their measured numbers so the next audit starts from data, not folklore.
