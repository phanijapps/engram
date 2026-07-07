---
type: ontology-phase
title: Phase 12 - Pilot Execution and Review
slug: it-sdlc-ontology-phase-12-pilot-execution-review
project: unified-knowledge-base
phase: 12
status: drafted
provenance: ai-assisted
created: 2026-06-27
modified: 2026-06-30
tags:
  - tiaa
  - unified-knowledge-base
  - it-sdlc-ontology
  - pilot
  - execution
  - review
---

## Purpose

Phase 12 defines how to run the first IT SDLC ontology pilot.

The pilot should prove that the ontology can support real solution-design and governance decisions before expanding to more domains, tools, sources, or federated ontologies.

This phase should answer:

> Can a small, governed slice of the ontology help a team understand a product/system landscape, identify reusable assets, surface applicable standards/NFRs/controls, and support a solution design approval conversation?

## Pilot Principle

Prove usefulness before scale.

The pilot should not try to ingest the enterprise. It should select one meaningful product/system slice and move it through enough ontology phases to answer the most important competency questions.

The goal is not complete coverage. The goal is to test whether the model helps humans make better design decisions.

## Pilot Candidate Shape

Choose a pilot candidate that has:

- A clear product, digital experience, or software-system anchor.
- Multiple deployable units or candidate components.
- At least one integration surface.
- At least one meaningful data entity or data classification concern.
- Some SDLC traceability.
- Some operational or production-readiness concern.
- At least one standard, NFR, control, regulation, risk, or exception to reason about.
- Accessible subject-matter experts.

Good candidates:

- A current solution design moving toward approval.
- A critical system with unclear integrations.
- A digital experience composed of micro-frontends, BFFs, backend services, and/or agentic apps.
- A system with known compliance, data, or operational constraints.
- A product where reuse decisions matter.

Avoid first:

- A purely theoretical architecture.
- A system with no available owners.
- A legacy estate too large to bound.
- A source landscape where no records can be shared or reviewed.

## Pilot Scope Template

```text
Pilot name:
Business sponsor:
Architecture owner:
Product/system owner:
Pilot target gate:
Primary competency questions:
Candidate Product:
Candidate DigitalExperience:
Candidate SoftwareSystem:
Candidate DeployableUnits:
Candidate InterfaceContracts:
Known DataEntities/DataAssets:
Known Standards/Policies/Controls/NFRs:
Known source systems:
Known gaps/risks:
Review participants:
```

## Recommended Pilot Target

The first pilot should target the **Solution Design Approval** gate.

Reason:

- It requires enough semantic detail to be useful.
- It does not require full production evidence.
- It pressures the model around product/system boundaries, reuse, integration, data, standards, NFRs, controls, and ownership.
- It is early enough in the lifecycle to create leverage.

Optional secondary target:

- Selectively test **Build Readiness** or **Deploy to Production** validation for one or two facts where evidence already exists.

## Pilot Execution Steps

### Step 1: Select the Pilot Slice

Choose one bounded product/system slice.

Output:

- Pilot scope record.
- Initial owner list.
- Target gate.
- Initial competency questions.

Review checkpoint:

- Is the slice small enough to finish?
- Does it include enough complexity to test the ontology?

### Step 2: Seed the Canonical Example

Manually seed a minimal canonical view:

- Product.
- DigitalExperience, if applicable.
- SoftwareSystem.
- DeployableUnits.
- InterfaceContracts.
- DataEntities or DataAssets.
- Owners.
- Standards, constraints, NFRs, controls, risks, and exceptions.

Output:

- Reviewed seed model.
- Known open questions.

Review checkpoint:

- Do humans understand the distinction between product, experience, system, unit, contract, and runtime?
- Are the seed records useful enough to discuss solution design?

### Step 3: Select Source Lanes

Use the accepted pilot source strategy:

> Start with one architecture/source-of-truth tool, one SDLC tool, one repository/CI source, one API/integration source if available, and one GRC/control source if controls are in scope.

Choose only the lanes needed for the pilot:

- Landscape lane.
- Integration lane.
- Delivery lane.
- Runtime lane.
- Governance lane.
- Knowledge lane.

Output:

- Selected lanes.
- Source candidates.
- Source owners.

Review checkpoint:

- Which lane is necessary for solution design?
- Which lane can wait?

### Step 4: Define Source Profiles

For each selected source, define:

- Source purpose.
- Mapped concepts.
- Identity keys.
- Authority scope.
- Conflict policy.
- Sample records.
- Review owner.

Output:

- Source profiles.
- Record authority matrix.

Review checkpoint:

- Which facts are authoritative?
- Which facts are context only?
- Which facts must be reviewed before use at a gate?

### Step 5: Create Source Assertions

Represent incoming source facts as assertions with provenance.

Output:

- Source assertions.
- Candidate facts.
- Inferred facts, if any.
- Disputed or duplicate facts.

Review checkpoint:

- Are assertions traceable to source?
- Are inferred facts clearly marked?

### Step 6: Normalize and Resolve

Map assertions to canonical concepts and relationships.

Resolve:

- Duplicate system/product names.
- Product versus software-system confusion.
- Deployable unit versus runtime service confusion.
- Interface contract versus concrete endpoint/channel confusion.
- Source conflicts.

Output:

- Candidate canonical view.
- Resolved identities.
- Conflict list.
- Rejected matches.

Review checkpoint:

- Are the canonical entities correct?
- Are false matches prevented?

### Step 7: Apply Gate-Aware Validation

Run the **Solution Design Approval** validation profile.

Focus on:

- Product/system/experience boundaries.
- Owners.
- Required design attributes.
- Key interface contracts.
- Data classification and ownership.
- Applicable standards, constraints, NFRs, controls, risks, and exceptions.
- Planned evidence.
- Open design risks.

Output:

- Validation findings.
- Missing facts.
- Advisory gaps.
- Blocking gaps.
- Exception candidates.

Review checkpoint:

- Which missing facts block solution design approval?
- Which can be accepted as open risks?
- Which planned evidence must move to build readiness?

### Step 8: Review With Stakeholders

Review the ontology-backed pilot view with:

- Product owner.
- Solution architect.
- Domain architect.
- Engineering lead.
- Integration/API owner.
- Data owner/steward.
- Risk/control representative.
- Operations/support representative, if production impact exists.

Output:

- Corrections.
- Accepted facts.
- Disputed facts.
- New competency questions.
- Model changes.
- Source mapping changes.

Review checkpoint:

- Did the ontology improve the solution-design conversation?
- Did it surface gaps earlier than the normal process?

### Step 9: Promote Reviewed Facts

Promote reviewed facts to the governed view.

Keep other facts as:

- Candidate.
- Inferred.
- Disputed.
- Deprecated.
- Rejected.

Output:

- Governed pilot view.
- Decision log.
- Open issue list.
- Evidence plan.

Review checkpoint:

- Which facts are ready for reuse?
- Which facts require more source mapping or governance?

### Step 10: Retrospective and Expansion Decision

Assess whether to expand.

Output:

- Pilot retrospective.
- Metrics.
- Lessons learned.
- Model changes.
- Source mapping changes.
- Next pilot recommendation.

Review checkpoint:

- Expand to another system?
- Add another source lane?
- Add a federated ontology extension?
- Tighten validation rules?
- Move toward implementation architecture?

## Pilot Success Criteria

The pilot is successful if it can:

- Answer at least 8-10 priority competency questions.
- Show product/system/experience/deployable-unit/interface-contract distinctions clearly.
- Identify reusable systems, units, contracts, or data assets.
- Surface applicable standards, constraints, NFRs, controls, risks, and exceptions.
- Preserve provenance for reviewed facts.
- Distinguish source, candidate, inferred, reviewed, authoritative, disputed, and rejected facts.
- Support a solution-design approval conversation.
- Produce actionable gaps or remediation items.
- Demonstrate which source lanes should be added next.

## Pilot Metrics

| Metric | Target |
|--------|--------|
| Priority competency questions answered | 8-10 for first pilot |
| Seed canonical entities reviewed | Product/system/unit/contract/data/control core slice |
| Source profiles created | 3-5 |
| Gate validation findings classified | 100% of findings classified as blocking/advisory/info |
| Facts with provenance | 100% of imported/reviewed facts |
| Disputed facts resolved or owned | 100% assigned owner |
| Stakeholder corrections captured | 100% captured as model/source changes or rejected changes |
| Decision usefulness | Stakeholders agree it improved or accelerated design review |

## Expansion Criteria

Do not expand just because the pilot is interesting. Expand when:

- The model helped answer real competency questions.
- Stakeholders understood the canonical distinctions.
- The source profiles produced useful facts.
- Validation findings were actionable.
- The governance workflow was not too slow.
- Open issues are known and owned.

Expansion options:

- Add another product/system slice.
- Add a new source lane.
- Add a federated ontology extension, such as data, agentic systems, or risk/control.
- Add a new gate profile, such as build readiness or deploy to production.
- Add more source systems only after current mappings prove useful.

## Phase 12 Deliverables

The reviewed output of this phase should be:

- Pilot scope record.
- Seed canonical example.
- Selected ingestion lanes.
- Source profile set.
- Record authority matrix for the pilot.
- Candidate canonical view.
- Gate validation findings.
- Stakeholder review notes.
- Promoted governed facts.
- Pilot retrospective.
- Expansion recommendation.

## Review Questions

- Which product/system slice should be the first pilot?
- Is Solution Design Approval the right first target gate?
- Which competency questions are must-answer in the pilot?
- Which ingestion lanes are required for the first pilot?
- Which sources are available enough to map?
- Who must review and approve promoted facts?
- What would make the pilot credible to stakeholders?

## Research Anchors for Progressive Disclosure

Use these anchors when explaining pilot execution choices.

| Anchor | Use When |
|--------|----------|
| [SHACL](https://www.w3.org/TR/shacl/) | Explaining gate-aware validation and rule findings. |
| [PROV-O](https://www.w3.org/TR/prov-overview/) | Explaining source assertions, provenance, and promoted facts. |
| [Backstage System Model](https://backstage.io/docs/features/software-catalog/system-model/) | Explaining developer-catalog style seed records and ownership. |
| [OpenAPI Specification](https://spec.openapis.org/oas/latest.html) | Explaining interface-contract evidence for APIs. |
| [AsyncAPI Specification](https://www.asyncapi.com/docs/reference/specification/v3.0.0) | Explaining event/message contract evidence. |
| [COBIT](https://www.isaca.org/resources/cobit) | Explaining control, assurance, risk, and governance participation. |

Progressive-disclosure rule:

- Start with the pilot question and target gate.
- Bring in provenance and validation anchors only when discussing trust or gate evidence.
- Bring in catalog/contract anchors only when discussing source lanes or integration evidence.
