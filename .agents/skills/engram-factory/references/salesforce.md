# Salesforce profile

Pin edition, API version, enabled features, namespace/package model and org strategy.
Verify security behavior and deployment requirements for that version. Avoid fixed
coverage or governor-limit numbers without a job-specific official source.

## Analysis

Inventory objects/fields, relationships, record types, validation rules, flows,
Apex/triggers, Lightning components, permissions/sharing and integrations. Record
execution identities, org aliases, data volumes, package dependencies and metadata
source of truth. Distinguish metadata, business data and environment configuration.

## Plan

Explain declarative versus Apex choice, order of execution, recursion control,
bulk/transaction behavior, asynchronous processing and integration retry. Specify
record sharing, object permissions and field security separately, using explicit
execution/access context appropriate to the pinned version. Define metadata/package
composition, dependency and destructive-change order, data migration and recovery.

## Tasks

Include native metadata, least-privilege permissions, execution-context checks,
bulk/limit cases, UI behavior, package/export reconciliation, deployment validation,
data scripts and environment-specific manual steps. Link changes to actual native IDs.

## Verification

Plan relevant Apex, component, flow and integration checks; run denied-identity,
record/object/field access, bulk and regression scenarios. Record test level, org,
API version, deployment-validation run and applicable coverage evidence. Coverage
is a platform gate, not a substitute for behavior assertions.

## Release

Record immutable metadata package/manifest, validation run, target org baseline,
dependency/destructive order, permissions, activation, data reconciliation and smoke
checks. Verify what is and is not reversible; code or metadata redeployment does
not repair changed business records or undo external effects.
