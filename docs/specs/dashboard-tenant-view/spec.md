# Spec: dashboard-tenant-view (tenant + repos + docs overview)

- **Status:** Draft
- **Shape:** ui
- **Constrained by:** demo-ui-shell
- **Contract:** none

## Objective

A dashboard panel showing the operational state of the knowledge platform:
tenant identity, indexed code repositories (name, git remote, branch, SHA,
last updated, entity/relationship counts), and indexed documents (count,
chunk count, total text size). This is the "what do I have?" view, separate
from the graph exploration view.

## Acceptance Criteria

- [ ] Shows the current tenant (tenant-demo / engram / local).
- [ ] Lists each indexed repo with: name, git remote URL, branch, SHA, last ingested timestamp, entity count.
- [ ] Shows aggregate document + chunk counts.
- [ ] A new `/knowledge/stats` backend route aggregates this from the store.
