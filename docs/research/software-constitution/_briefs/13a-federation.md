# Brief: Federated-constitution precedents (gpt-6.1-sol)

ROLE: Research how large governance/rule systems decompose into domain-scoped
sub-documents with cross-references, so engram's constitution can split into
domain sub-constitutions (product / architecture / build / testing / ci)
WITHOUT duplication drift or circularity. Analysis only — write ONE file:
docs/research/software-constitution/13a-federation-precedents.md

## Study (fetch/skim, extract principles with citations)
- IETF: RFC series ← area/WG structure; how STDs cross-reference normatively
- Linux kernel: Documentation/ subsystem trees; maintainer-scope rules
- Kubernetes: SIG charter model — each SIG owns policies, cross-SIG deps
- Policy-as-code: OPA/bundle layering — root policy + domain policies
- ISO/IEC/IEEE 42010: viewpoint = concern×stakeholder (already partially used)
- Any constitution-like federated standard (e.g. COBIT/ITIL domain structure)

## Answer (with evidence)
1. What belongs in the ROOT vs a DOMAIN doc — the division test.
2. Cross-reference conventions that stay auditable (normative ref direction,
   ID namespacing, no-cycles rule, ref must point at rule-ID not file?).
3. The duplication-drift failure modes in federated rule systems + guards.
4. How systems keep "one rule, one home" when domains overlap (e.g. a build
   rule that CI enforces — who owns it?).
5. Verdict on the owner's 5-domain cut (product/architecture/build/testing/
   ci): what's missing (security? release/ops? docs?), what to merge.
Under ~200 lines, date 2026-11-01. No repo edits.
