# Plan: dashboard-tenant-view

### T1 — `/knowledge/stats` backend route
- **Tests:** goal-based — curl smoke.
- **Approach:** Aggregate from `listGraphs` + `listEntities` + `listRelationships` + `listChunks`. Group entities by `graphId` → per-repo counts. Return `{ tenant, repos: [{name, gitRemote, branch, sha, entityCount, relCount, lastUpdated}], totalDocs, totalChunks }`.

### T2 — Dashboard panel UI
- **Tests:** goal-based — frontend build.
- **Approach:** Collapsible panel or `/dashboard` route. shadcn `Card` per repo with the stats. Aggregate counts at the top.
