# Maximo profile

Pin Maximo product/deployment and Manage/Application Suite version. Distinguish
configuration migration from application-source migration; available mechanisms
and limitations vary by version and artifact type.

## Analysis

Inventory affected objects/attributes, domains, relationships, workflows, automation
scripts/launch points, escalations, integration object structures, security groups,
applications and user surfaces. Record organization/site scope, asset/work-order
business rules, volumes, mobile/offline needs if present and source environment.

## Plan

Explain native configuration versus scripts/extensions. Specify event/launch-point
context, transaction/validation behavior, recursion, bulk processing, site/org
authorization and integration error/reprocessing. Map each artifact type to a
supported export/migration route; include database configuration impacts and any
maintenance or activation dependencies established for the installed version.

## Tasks

Include object/configuration changes, scripts, permissions, workflow/integration,
source exports, package definitions/dependencies, application-source changes where
relevant, data reconciliation and operational runbooks. Do not hide manual steps.

## Verification

Exercise business transitions, validation, scripts/launch points, security groups
and site/org restrictions, integration replay/error paths, volume and regressions.
Record product version, environment, configuration baseline and execution evidence.
Validate package completeness and target compatibility with supported tooling.

## Release

Record package/application export IDs and hashes, artifact-specific migration route,
dependency/import order, database/configuration activation prerequisites, downtime
if required, reconciliation and post-change checks. Explicitly identify configuration
or UI artifacts excluded from the chosen migration route. Define supported restore,
corrective package or roll-forward procedures and business-data repair.
