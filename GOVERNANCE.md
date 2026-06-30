# Governance

Engram is a small pre-1.0 open-source project. Governance is lightweight and
contract-first: accepted contracts, ADRs, RFCs, and specs carry durable
technical authority; maintainers decide how changes move through those records.

## Maintainer Responsibilities

Maintainers are responsible for:

- protecting accepted v1 contracts from unreviewed breaking changes,
- keeping Rust, TypeScript, generated contracts, and documentation aligned,
- requiring specs for non-trivial behavior changes,
- enforcing release gates before publishing crates, npm packages, or tags,
- triaging issues and pull requests in public when safe,
- moving security and conduct reports into private maintainer channels.

## Decision Records

Use the narrowest durable record that fits the decision:

- ADRs under `docs/adr/` record accepted architecture decisions.
- RFCs under `docs/rfcs/` record proposals that need discussion before they are
  accepted.
- Specs under `docs/specs/` record implementation acceptance criteria for a
  concrete slice.
- `docs/domain-data-model.md` remains the domain source of truth until an ADR
  moves that authority to generated Rust/domain contracts.
- `contracts/v1/` contains accepted machine-readable v1 contract artifacts.

Roadmap entries are intent, not acceptance. A roadmap item becomes accepted
behavior only after its spec is implemented, validated, and marked shipped.

## Contract Changes

Maintainers should classify public contract changes before merge:

- compatible v1 addition,
- draft extension only,
- breaking change requiring a future version.

Breaking changes must not rewrite accepted v1 contracts in place. They require
an ADR or RFC, migration notes, and a future versioned contract package.

## Pull Request Decisions

Maintainers may merge a pull request when:

- the change has an issue, spec, ADR, or RFC when the scope is non-trivial,
- CI and required local gates pass,
- contract impact is explicit,
- documentation and examples are updated when public behavior changes,
- review findings are resolved or explicitly deferred in project docs.

Maintainers may close or request changes on pull requests that bypass contract
rules, mix unrelated concerns, introduce god modules, or make unsupported
production, security, or performance claims.

## Releases

Engram is not production-ready. Release candidates must satisfy
`docs/release-checklist.md` before publishing any crate, npm package, or tag.

Release notes should include:

- public API and contract changes,
- compatible and breaking changes,
- adapter and binding impacts,
- known limitations,
- migration steps when relevant.

## Disputes

Technical disputes should move from pull request comments to an RFC when:

- the decision changes a public contract,
- the decision changes crate or package boundaries,
- the decision affects compatibility, security, or release claims,
- maintainers cannot reach consensus in the pull request.

Until an RFC or ADR changes the rule, the existing accepted contract and ADRs
win over new implementation preferences.

## Security And Conduct

Security reports follow `SECURITY.md` and must not be handled in public issues
when they involve data leakage, policy bypass, credential exposure, or unsafe
deletion behavior.

Conduct reports follow `CODE_OF_CONDUCT.md`. Maintainers should prioritize
project safety and contributor trust over keeping a discussion open.

## Maintainer Changes

New maintainers should demonstrate sustained contributions across contracts,
Rust behavior, TypeScript bindings, tests, and documentation. Adding or removing
maintainers should be recorded in an ADR or governance update once the project
has more than one active maintainer.
