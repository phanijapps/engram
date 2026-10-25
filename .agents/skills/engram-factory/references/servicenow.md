# ServiceNow profile

Pin release, scope, installed applications/plugins, deployment mechanism and actual
instance capabilities. Verify current official documentation and local standards;
do not assume global and scoped customizations have identical lifecycle behavior.

## Analysis

Inventory tables/fields, business rules, Script Includes, flows/subflows, catalog
items, ACLs/roles and user surfaces affected. Record application scope, cross-scope
access, domain separation if used, source-of-truth instance, application dependencies
and expected record volumes. Identify administrative and end-user identities.

## Plan

Choose supported native configuration or scripted extension with rationale. Specify
server/client enforcement, rule timing/order, flow triggers, recursion prevention,
asynchronous work, query/bulk behavior and audit. Select an appropriate application
delivery route; record dependency order, captured changes, environment properties,
data movement and manual steps separately. Do not assume an update set captures all state.

## Tasks

Create explicit native-object, permission, integration, automated verification,
package/export and target-preview tasks. Include dependency/plugin checks and
unpackaged configuration steps. Preserve object IDs and export provenance for review.

## Verification

Use supported Automated Test Framework checks where appropriate, plus business,
permission, rule/flow, bulk, integration and regression cases. Record instance,
release, test identity and run evidence. Verify package contents and target conflicts;
do not run destructive tests in production.

## Release

Record application version or update-set identifiers/digests, dependency order,
preview/conflict resolution, activation/manual steps, environment settings, data
reconciliation and post-import tests. State recovery limits and required data repair;
do not equate source revert with instance restoration.
