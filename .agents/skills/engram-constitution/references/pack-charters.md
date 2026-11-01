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
