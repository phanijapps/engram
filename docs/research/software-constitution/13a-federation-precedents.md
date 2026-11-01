# Research: Federated-constitution precedents
Date: 2026-11-01

## Summary
Federate by decision authority and concern, not by the tool that checks compliance: root owns governance and common invariants; domain documents own scoped obligations and local refinements. The five-domain cut is workable only if CI is an enforcement view rather than a second home for product/build/testing rules, and security, release/operations, and documentation have explicit coverage. Stable rule IDs, typed references, unique ownership, and a checked normative dependency DAG are recommendations for Engram—not universal requirements of these precedents.

## Findings: precedent evidence
1. **IETF — scope and specification identity are separate.** Working groups have IESG-approved charters specifying scope, deliverables, goals, milestones, sometimes exclusions; each sits within an area. RFC 2026 §2.1 says an Internet Standard gains an STD label while retaining its RFC number; §3.3 recommends modular technical specifications when reusable across applicability domains. Thus organizational areas are not a root-to-leaf hierarchy of normative specifications. **Sources:** [WG guide][1], [RFC 2026 §§2.1, 3.3][2]. **Support:** direct evidence. **Confidence:** high.
2. **IETF — distinguish dependency from context.** RFC 7322 §4.8.6 defines normative references as essential to implementing or understanding the RFC, versus informative references providing additional information. It also describes holding publication until referenced drafts are ready, then publishing together. **Interpretation:** reference classification and coordinated change are useful precedents; a universal prohibition on cycles is not established. **Source:** [RFC 7322 §4.8.6][3]. **Support:** direct evidence plus labeled interpretation. **Confidence:** high.
3. **Linux — common process plus local addenda, with explicit routing.** Maintainer Entry Profiles “supplement” top-level process documents with subsystem-local customs; their checklist is additional to the common checklist. Profiles may link to parent-subsystem profiles to expose submission-chain obligations. MAINTAINERS supplies file patterns, exclusions, reviewers, and profile pointers; its instructions say to seek the most precise area first. The documentation tree separately accommodates developer, user, architecture-specific, and other manuals. **Sources:** [entry-profile guide][4], [MAINTAINERS header][5], [documentation index][6]. **Support:** direct evidence. **Confidence:** high.
4. **Kubernetes — scoped charters inherit common governance and name exclusions.** SIG Architecture explicitly adheres to common SIG governance, owns design principles, API conventions and deprecation policy, but excludes nonarchitectural KEPs and release enhancement delivery. Common governance requires tech leads to resolve cross-SIG issues, tracks subprojects in `sigs.yaml` with OWNERS, and requires differing subproject processes to document how they differ. **Interpretation:** cross-cutting does not mean ownerless; policy design and delivery can have distinct owners. **Sources:** [Architecture charter][7], [SIG governance: Tech Lead, Subprojects][8]. **Support:** direct evidence plus labeled interpretation. **Confidence:** high.
5. **OPA — composition needs nonoverlapping write authority, not implicit override order.** Bundle manifests declare owned policy/data namespace roots; policy packages and data must fit those roots. Roots cannot overlap within one manifest, but that check is explicitly not enforced across bundles. OPA warns that multiple sources have no bundle-load ordering guarantee and conflicts may produce an error state; it recommends central aggregation where possible. **Interpretation:** a constitutional assembly step must validate ownership globally; “root loaded last wins” is not a sound precedent. Namespace ownership is not the same thing as constitutional precedence. **Source:** [Bundles: managing data from multiple sources][9]. **Support:** direct evidence plus labeled interpretation. **Confidence:** high.
6. **ISO/IEC/IEEE 42010 — organize around concerns and check correspondence.** The explanatory conceptual-model site describes viewpoints as conventions framing specific concerns and views as addressing concerns of one or more stakeholders. Correspondences and correspondence rules express/enforce refinement, consistency, traceability, dependency and obligations within/between descriptions. Its 2022 overview adds stakeholder perspectives grouping concerns and organizing viewpoints; the detailed historical model is labeled 2011. **Interpretation:** “concern × stakeholder” is a useful selection heuristic, not a requirement for one document per Cartesian-product cell. **Source:** [42010 conceptual model][10]. **Support:** direct evidence plus labeled interpretation. **Confidence:** high for the explanatory model; medium for transfer to constitutions.
7. **COBIT — separate direction, delivery and assurance without assuming disjoint concerns.** ISACA describes governance objectives under EDM, with management objectives under APO, BAI, DSS and MEA. DSS includes operational delivery/support and security; MEA covers monitoring and conformance. **Interpretation:** assurance observes obligations owned elsewhere; a development-only constitution needs explicit operational coverage or an explicit exclusion. This is a structural analogy, not proof of a rule-ID or no-cycles convention. **Source:** [ISACA: Governance and Management Objectives][11]. **Support:** direct evidence plus labeled interpretation. **Confidence:** high for domain structure; medium for analogy.

## Root versus domain: the division test
The following is **researcher recommendation**, derived principally from [4], [7]–[10], not a claim that those systems prescribe this exact structure.

- **Root:** authority/adoption, normative vocabulary, precedence, amendment and exception process, domain charters and escalation, global rule identity/reference semantics, and genuinely system-wide invariants.
- **Domain:** obligations whose meaning and tradeoffs have one accountable subject-matter owner; applicability, local refinements, required evidence, and links to upstream obligations. Routine commands, workflows and examples belong in referenced implementation/runbook documents, not constitutional prose.
- **Test A — authority:** if changing a statement changes how domains obtain or exercise authority, it belongs at root.
- **Test B — universality:** put an invariant at root only if it is intended to constrain the federation as a whole. Do not promote a build-specific obligation merely because five domains consume its output.
- **Test C — semantic owner:** ask who can justify changing the obligation without changing unrelated concerns. That domain owns it; reviewers and enforcement sites may be plural.
- **Test D — independent usefulness:** create a domain only when it has distinct concerns, decisions, accountable ownership and a meaningful body of rules. File length alone is not a charter.
- Root precedence must be explicit: local refinement cannot silently weaken root requirements. Exceptional departures need a root-governed, owner-approved, time-bounded record. This is a proposed Engram guard, not an inferred Linux/OPA guarantee.

## Auditable cross-references and circularity
**Recommended Engram convention** (inspired by normative/informative classification [3], scoped ownership [5,8,9], and correspondence rules [10]):

1. Assign stable namespaced IDs, e.g. `ROOT-004`, `ARCH-012`, `BUILD-007`, `TEST-003`, `CI-002`. An ID identifies one normative obligation; never reuse retired IDs. On a move, preserve identity via a registry/alias rather than silently minting a duplicate.
2. Maintain one canonical rule record: ID, text/home anchor, owner, applicability, status/revision, typed outgoing references, and evidence/gate mappings. Render human indexes from it; do not hand-maintain a second normative copy.
3. Normative citations target **rule ID plus resolvable anchor**, not only a filename. Document references remain appropriate for background/charters; external standards use document/version plus section where possible. Rule-level addressing is our stronger auditability choice, not an IETF requirement.
4. Define edge direction explicitly: `A requires/refines B` means A depends on B. Domain rules may depend on root or another domain's stable rule. Root may contain navigation links to domains but must not depend on domain rules for its own meaning.
5. Reject cycles in the **normative dependency graph**, including cross-domain cycles. Permit reciprocal informative links and `enforced_by`/`evidence_for` relationships; keep these out of DAG validation. A domain must not require a CI gate rule that itself requires that same domain rule.
6. When a cycle appears, extract the shared invariant to the lowest suitable common authority, or separate the underlying obligation from its evidence mapping. Do not relabel a genuine dependency “informative” to satisfy the lint.
7. Resolve conflicts through owners/root escalation, not file order. Amend producer rule and affected consumers/evidence mappings in one reviewed change; record the constitution release/commit for an auditable snapshot.

**Confidence:** medium for suitability to Engram; the principles have strong analogues, but this exact DAG/registry scheme is not demonstrated as universal practice.

## Duplication-drift failure modes and guards
These are **researcher-inferred risks and proposed controls**, not observed incidents unless stated.

| Failure mode | Guard and precedent |
|---|---|
| Copied MUST text evolves differently in root and domain | One canonical home; elsewhere cite its ID or label a generated summary nonnormative. Linux local addenda supplement common process [4]. |
| Multiple domains claim the same obligation | One accountable owner, explicit in/out-of-scope charters, consultation/escalation; validate ownership globally [7–9]. Multiple reviewers are not multiple normative homes. |
| “CI requires X” restates and gradually changes the build/test standard | Gate-to-rule traceability; gate configuration cannot amend policy. Separate delivery from conformance [7,11]. |
| A root-level OPA namespace is assumed to merge/override child bundles safely | Use disjoint owned namespaces and explicit composition; check the whole assembled set. Actual OPA docs warn of conflict and nondeterministic load order [9]. |
| A file move or rename silently breaks applicability | Stable IDs/aliases, anchor-resolution lint, tombstones for retired rules, inbound-reference checks; explicit routing has precedent in MAINTAINERS [5]. |
| Child addenda weaken common obligations unnoticed | Check refinements/waivers against root precedence; declare differences and their approval, rather than infer precedence [4,8]. |
| Normative dependency cycles make interpretation recursive | DAG lint on typed rule edges; shared-invariant extraction. This intentionally goes beyond the IETF publication precedent [3]. |
| Too many domains leave rules unowned or documents stale | Minimal charters, named owners, coverage review by stakeholder/concern and lifecycle—not just folder count [1,8,10,11]. |

## One rule, one home when domains overlap
**Recommendation:** ownership follows the reason the requirement exists; enforcement follows the mechanism used to observe it.

Example: `BUILD-007` owns “release artifacts must be reproducible under the specified inputs.” Testing owns a distinct reproducibility verification method/fixture requirement if needed; CI owns generic obligations about executing gates, retaining evidence, and handling unavailable checks. A CI gate declares `enforces: BUILD-007` and reports its result; it must not copy or redefine reproducibility. Build-rule changes require the build owner plus affected testing/CI reviewers. No reverse normative dependency from the build obligation to its gate is necessary.

For security of released artifacts, a security owner owns the threat-driven signing/integrity obligation; build/release owns how artifacts are produced/distributed; CI records evidence. A stakeholder–concern–rule–evidence matrix supplies the cross-view consistency check [10]. If a proposed “shared rule” contains independently changeable obligations, split it and link the parts rather than assigning two canonical homes.

## Verdict on product / architecture / build / testing / ci
**Conditionally endorse, with changes (researcher inference; medium confidence).** The list mixes enduring subject-matter concerns with an execution mechanism, so five equal sovereign documents would encourage drift.

| Proposed scope | Verdict / boundary |
|---|---|
| Product | Keep: user-visible semantics, compatibility and capability promises. Product defines the promise; architecture owns structural means, testing owns verification obligations. |
| Architecture | Keep: module/contract boundaries, dependency rules, neutrality and transport consistency constraints. Consult product for externally visible effects. |
| Build | Keep; extend to **build and release** initially: reproducibility, dependencies, packaging, provenance, versioned outputs and publication/rollback obligations. |
| Testing | Keep if sufficiently substantial: correctness, determinism, regression/evaluation methods, coverage/evidence expectations. Avoid owning the product promise itself. |
| CI | Prefer **assurance/enforcement**, a thin cross-domain gate/evidence registry plus genuinely CI-specific integrity/availability rules. Merge into build/verification if it has no independent policy content. |

**Missing coverage, not automatically three more documents:**
- **Security/privacy:** explicitly assign threat modeling, authorization, secrets, supply-chain integrity, data isolation/retention and vulnerability response. Add a security domain when substantive; do not scatter unowned requirements across CI/build/architecture.
- **Release/operations:** include upgrade/migration, distribution, support/deprecation, recovery and incident ownership as applicable. If Engram does not operate a hosted service, declare that boundary; still cover library release and data migration. COBIT distinguishes delivery/support from build [11].
- **Documentation/contracts governance:** source-of-truth, generated-artifact reproducibility, rule/contract lifecycle, and provenance need homes. Put global authority/change rules at root; assign specific contract/document obligations to architecture/product/build. Create a docs domain only with independent scope and ownership.
- **Governance itself:** the root must supply adoption, conflict resolution and exceptions; it is not a sixth bag of miscellaneous technical rules.

## Contradictions
- **No-cycles is not an IETF-derived universal.** RFC 7322 allows coordinated publication of dependent documents; adopt a DAG as Engram's stricter local simplicity rule, not as historical fact [3].
- **OPA is not a hierarchy of override authority.** Its nonoverlapping ownership model and cross-bundle validation gap oppose naive root-plus-child overlay claims [9].
- **Scopes need not be orthogonal:** SIG Architecture owns conformance definitions as well as design principles [7], and 42010 allows concerns across views [10]. Therefore ownership can be unique while review/evidence views overlap.

## Missing evidence / residual risks
- This is precedent research, not an inventory of Engram's existing rails, owner capacity or gate implementation; exact allocation/merge choices remain unverified.
- The 42010 source is an explanatory site, not the paid normative standard; it distinguishes 2011 detail from 2022 changes. No clause-level compliance claim is made.
- RFC 2026/7322 and COBIT 2019 are historical precedents, not claims about the latest complete processes. Mutable Kubernetes/Linux/OPA pages were inspected without pinning commits; long-term audit should preserve snapshots.
- `source_check` retrieved OPA/RFC passages but reported automated semantic assessment unavailable. Important claims were checked manually against fetched OPA content and RFC plain text; no automatic validation verdict is claimed.

## Sources
Kept (all fetched and inspected; Linux MAINTAINERS header skimmed, not its entire roster):
- [1] [IETF Working Groups][1] — charter, area, scope and escalation evidence.
- [2] [RFC 2026][2] — durable RFC/STD identity and modular reuse; historical process source.
- [3] [RFC 7322][3] — §4.8.6 reference categories and coordinated publication; fetched as plain text after HTML extraction failed.
- [4] [Linux Maintainer Entry Profile][4] — common/local process division.
- [5] [Linux MAINTAINERS][5] — explicit scope patterns, exclusions and profile pointers.
- [6] [Linux documentation index][6] — audience/subsystem-oriented documentation tree.
- [7] [Kubernetes SIG Architecture charter][7] — scope, exclusions and inherited governance.
- [8] [Kubernetes SIG governance][8] — roles, cross-SIG decisions, subproject ownership and declared differences.
- [9] [OPA Bundles][9] — namespace ownership, overlap validation limits and composition warnings.
- [10] [42010 conceptual model][10] — concern/viewpoint/correspondence model; HTTP fetch succeeded after HTTPS failed.
- [11] [ISACA: Employing COBIT 2019][11] — governance/management domain decomposition; authoritative publisher commentary, not full normative framework.

Rejected/deprioritized: search summaries (discovery only); version-specific Linux search hits (used canonical guide instead); IEEE 1471 paper (superseded for this purpose by the fetched 42010 model); RFC 3967/downref material (maturity exceptions not needed for the narrower decomposition recommendation); latest-version claims unsupported by the historical sources.

## Next steps
Inventory existing rule IDs against proposed accountable owners, stakeholder concerns and lifecycle stages; use that matrix to decide whether testing/assurance or build/CI should merge. Before migration, prototype only the rule-ID resolver, unique-home check, typed normative-cycle check and rule-to-gate coverage report; preserve the current semantics during the split.

[1]: https://www.ietf.org/process/wgs/
[2]: https://www.rfc-editor.org/rfc/rfc2026.txt
[3]: https://www.rfc-editor.org/rfc/rfc7322.txt
[4]: https://docs.kernel.org/maintainer/maintainer-entry-profile.html
[5]: https://raw.githubusercontent.com/torvalds/linux/master/MAINTAINERS
[6]: https://docs.kernel.org/index.html
[7]: https://www.kubernetes.dev/community/community-groups/sigs/architecture/charter/
[8]: https://raw.githubusercontent.com/kubernetes/community/master/committee-steering/governance/sig-governance.md
[9]: https://www.openpolicyagent.org/docs/management-bundles
[10]: http://www.iso-architecture.org/42010/cm/
[11]: https://www.isaca.org/resources/news-and-trends/industry-news/2019/employing-cobit-2019-for-enterprise-governance-strategy
