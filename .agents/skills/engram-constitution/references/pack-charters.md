# Pack charters — what each domain owns (merge/split per project reality)

Seven packs are the DEFAULT cut, not dogma. A small repo may merge
(build+release, ci+testing); a platform repo may split further. The test is
always: does this domain have its own decision authority?

| Pack | Prefix | Owns | Typical merge partner |
|---|---|---|---|
| product | PRD | what the software promises: scope, substrates, observable guarantees, capability honesty | — |
| architecture | ARC | how it is structured: seams, boundaries, ownership, component coherence | — |
| dev | DEV | how artifacts are made: toolchain, builds, vendoring, reproducibility | build |
| testing | TST | what proves it: fixtures, conformance, coverage discipline | ci |
| ci | CIE | enforcement view: which gate enforces which rule — owns NO other domain's rules | testing |
| security | SEC | trust boundaries: authz, supply chain, secrets, audit | — |
| release | REL | shipping: versioning, compatibility, publication, rollback | dev |

Rule: one rule, one home. ci/ only REGISTERS enforcement. Security rules live
where they constrain, cited with `security:` refs if a dedicated pack exists.


## Monorepo: the per-module×concern test

A module earns a sub-constitution pack only for concerns where it holds its
own decision authority. Run the test per module × per concern:

| Module type | Owns | Shares root |
|---|---|---|
| Microservice (own deploy, own data) | product, architecture, platform, release | dev, testing, ci, security |
| Monolith module (shared deploy) | product rules if a distinct capability | almost everything |
| Shared library | nothing | everything |

Root architecture owns module boundaries and dependency direction — never
module internals. Module architecture owns its seams. The two never invert.