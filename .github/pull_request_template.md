## Summary

Describe the change.

## Contract Impact

- [ ] No accepted contract impact
- [ ] Compatible v1 addition
- [ ] Draft extension change
- [ ] Breaking change requiring future version

## Validation

- [ ] `python3 scripts/validate_contracts.py`
- [ ] `.codex/hooks/check-contracts.sh`
- [ ] `.codex/hooks/check-docs.sh`
- [ ] `cargo fmt --all --check`
- [ ] `cargo check --workspace`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `cargo test --workspace`
- [ ] `cargo check -p engram-store-vector --features fastembed-tests --tests`
- [ ] `cargo clippy -p engram-store-vector --features fastembed-tests --tests -- -D warnings`
- [ ] `pnpm run typecheck`
- [ ] `pnpm run test`
- [ ] `pnpm run build`

## Notes

Add review notes, deferred work, or compatibility concerns.
