# Code Documentation Standards

## Rust

- Every Rust source module under `crates/*/src` starts with `//!` module docs.
- Public traits and public free functions require `///` docs.
- Module docs must explain ownership, boundaries, and what does not belong in
  the module. One-line labels are not acceptable.
- Public trait/function docs must explain caller expectations or implementor
  obligations. One-line labels are not acceptable.
- Trait docs should explain the boundary, caller expectations, and what adapter
  implementations must preserve.
- Public data models may rely on `docs/domain-data-model.md` for field-by-field
  semantics, but module docs must explain the model group.
- Avoid comments that narrate obvious code. Use comments for invariants,
  policy, compatibility, safety, and adapter obligations.

## TypeScript

- Public package entry points require JSDoc on exported functions, classes,
  interfaces, and type aliases.
- Generated files must be clearly marked as generated and should be excluded
  from manual documentation requirements.
- SDK docs should describe caller behavior and errors, not Rust internals.

## Review Standard

The documentation review hook should pass before handoff. If it fails, fix the
docs unless the code is generated or deliberately private.
