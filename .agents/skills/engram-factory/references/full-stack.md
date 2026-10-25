# Full-stack profile

Apply to browser/mobile clients, services, databases, messaging and infrastructure.
Combine with platform profiles for hybrid applications. Pin actual frameworks,
versions and hosting model. These are design questions, not preselected technology.

## Analysis

Record client/service/data boundaries, service ownership, existing API consumers,
authentication and tenant model, workload baseline, runtime/dependency versions,
hosting/network constraints and user accessibility needs. Identify systems of record.

## Plan

Specify UI states, server-side validation, API/event contracts and versioning,
transaction boundaries, consistency, cache invalidation, pagination, concurrency,
idempotency, database migration order, secrets, dependency provenance and infrastructure
changes. Link concrete threat/failure cases to tests. Define health and readiness signals.

## Tasks

Include vertical behavior slices, contract updates, permissions, database changes,
observability, dependency/configuration changes, automated checks, deployment assets
and recovery rehearsal. Isolate parallel writers by component and migration order.

## Verification

Cover unit/component, API contracts, integration and key end-to-end journeys;
authorization, tenant isolation, injection, bulk/concurrency, retry and downstream
failure; keyboard and rendered UI states; meaningful workload and migration checks.
State where mocks replace real services and what remains unverified.

## Release

Record immutable image/package digests, infrastructure revision, environment
configuration, backward compatibility, migration/activation order, feature flags,
canary or staged checks, monitoring, rollback/roll-forward and data repair. A previous
container image does not reverse a schema migration or external side effect.
