# Review rubrics

Always require code, security and platform. Analyze every additional discipline;
record applicability, scope evidence and rationale in analysis.md. Re-evaluate
after build. The reviewer set freezes at evidence freeze: a later rebuild may
only add reviewers, and any other mid-round change requires blocking plus
human rework, which starts a new cycle.

| Discipline | Applies when | Lens |
| --- | --- | --- |
| code | Every job, including declarative changes | Business behavior, maintainability, configuration logic, tests, error paths, regression and dependency policy |
| security | Every job | Trust boundaries, execution identity, object/field/record permissions, tenant isolation, injection, secrets, audit and abuse |
| platform | Every job | Native extension fit, supported APIs, release/edition, configuration completeness, export fidelity, dependencies, upgrade and transport compatibility |
| ux | Journey, operator workflow or approval behavior changes | Success, denial, errors, recovery, discoverability and task completion |
| ui | Visual/interactive surface changes | Keyboard/focus, semantics, contrast, responsive layout, localization, loading/empty/error states and rendered evidence |
| backend | Logic, services, integrations, workflows or background work | Transactions, bulk behavior, concurrency, retry, idempotency, side effects and limits |
| data | Schema, records, mappings, migrations, reporting or retention changes | Ownership, lineage, reconciliation, integrity, sensitive data, replay and repair |
| operations | Runtime, infrastructure, release, monitoring or support changes | Capacity, alerting, health checks, runbooks, deployment and recovery |

Check authorization with denied and least-privileged identities, not only admins.
Treat low-code rules and workflow transitions as executable behavior. Client-side
restrictions do not prove server authorization. Use review.md with profile questions.

| Severity | Definition | Gate |
| --- | --- | --- |
| critical | Credible severe exposure, destructive behavior or systemic failure | CHANGES and immediate escalation |
| high | Material correctness, security, reliability or deployment defect | CHANGES |
| medium | Relevant defect with bounded impact | CHANGES unless authorized scoped exception |
| low | Nonblocking improvement | Follow-up; PASS may retain it |

Missing access/evidence is not a low-severity improvement. Use ERROR when review
cannot establish its conclusion. Exceptions require owner, rationale, scope,
compensating control, authorization evidence and expiry. Never accept a critical
finding through an agent-authored exception. Organization rules may be stricter.

Wait for all results. Any ERROR blocks capability recovery. Any CHANGES sends the
complete finding set to builder if rounds remain. Third-round CHANGES escalates.
Rebuilds create new snapshots and rerun all required reviewers. Preserve history.
