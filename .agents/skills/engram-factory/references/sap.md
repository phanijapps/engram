# SAP profile

First identify the product and deployment: SAP is not one platform. Pin relevant
S/4HANA public/private/on-premise, BTP, ABAP or other product versions, clients and
landscape. Treat examples below as applicable questions, not universal availability.

## Analysis

Record business process and organizational scope, modules, master/transaction data,
released extension points/APIs, existing custom objects, integration consumers,
roles and segregation-of-duties constraints. Identify development/test/production
systems and clients, transports and ownership boundaries.

## Plan

Assess supported native/key-user, developer or side-by-side extension approaches
against clean-core objectives. Document released API/extension availability and
justify legacy/custom modifications explicitly. Specify data/business rules,
authorizations, transactions/locks, integration error/reprocessing and transport
mechanism/dependencies appropriate to the actual product and landscape.

## Tasks

Include configuration and extension objects, roles, interface contracts, dependent
transports, test fixtures, data migration, import/activation prerequisites and
operational documentation. Distinguish repository objects and configuration content.

## Verification

Use relevant unit/static checks, including ABAP Unit/ATC where applicable, business
process integration/regression, authorization and negative cases, data reconciliation,
transport dependency checks and target-system validation. Record product/client,
versions, identities, configuration baseline and run IDs. Do not assume tooling exists.

## Release

Record transport/package IDs, immutable export evidence, import order, target/client,
prerequisites, approval chain and post-import checks. Define process/data impact,
containment and corrective transports or restore strategy. Do not promise a generic
transport rollback or assume every SAP product uses the same transport mechanism.
