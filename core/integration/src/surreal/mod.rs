//! SurrealDB backend wiring for the integration facade.
//!
//! This submodule holds ONLY the thin recipe wiring (`bootstrap_surreal`) that
//! returns an `EngramProvider`. The actual Surreal database operations — every
//! capability cell — live in the dedicated `engram-store-surreal` crate (one
//! crate per backend, ADR-0022 amended 2026-07-16). The wiring lives here, not
//! in that crate, because it returns the facade-owned `EngramProvider` and a
//! crate that did so would form a Cargo cycle with `EngramProvider::open`.
//!
//! Reach `bootstrap_surreal` via [`EngramProvider::open`](crate::EngramProvider::open)
//! with the `surreal` cargo feature.

mod bootstrap;

pub(crate) use bootstrap::bootstrap_surreal;
