---
name: string-op-guard-trim
description: When guarding string values for Contains/StartsWith/EndsWith operators, use trim().is_empty() not is_empty() — whitespace-only strings still over-match.
metadata:
  type: feedback
---

Use `target.trim().is_empty()` not `target.is_empty()` when guarding cue/filter string values before Contains/StartsWith/EndsWith comparisons.

**Why:** `" ".is_empty()` is false, so a whitespace-only value passes the guard and then matches every field via `f.contains(" ")` (any multi-word entity name contains a space). The fix is to trim first, guard on the trimmed value, and compare against the trimmed lowercased target.

**How to apply:** Any time implementing string operator matching (Contains, StartsWith, EndsWith) on user-supplied filter or cue values, apply this pattern:
```rust
let target = raw.trim();
if target.is_empty() { return false; }
let t = target.to_lowercase();
```
