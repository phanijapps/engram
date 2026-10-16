# T11 finalized diagnostic snapshot quality review

Read-only review of the focused observability wrapper, provider-builder composition, strengthened existing handle test, unchanged adapter/conformance expectations, T11 plan correction, and optional MCP HOW-TO addition. No benchmark/model runs, real-store operations or held-out inputs/results inspected.

## Verdict

Clean — ready to commit the observability regression fix. No actionable production quality/testability findings.

ProviderObservability delegates to the existing adapter and replaces only snapshot.capabilities with the finalized immutable provider report. The awaited inner result uses ?, preserving typed failures instead of synthesizing a successful snapshot on error. Counts, embedding configuration, versions and future diagnostic fields remain exactly as returned by the adapter. The wrapper owns no backend type, SQL, policy or storage behavior.

Composition occurs centrally in EngramProviderBuilder after both readiness fields are finalized, so all backend recipes and externally supplied observability handles follow the same rule. An absent handle stays absent. The wrapper holds a cloned report rather than a provider reference, so there is no Arc ownership cycle. Existing post-build calibration installation does not change readiness/capabilities and cannot stale the snapshot.

The strengthened existing handle regression is appropriate: it supplies deliberately different adapter capabilities, compares the complete returned report to the provider report and checks non-capability metadata survives. It tests observable behavior rather than asserting wrapper construction or weakening the original equality contract. Existing conformance/full-report assertions remain unchanged. The parent reports this focused test and original conformance test passing; the broader workspace retry remains separate. No new mirror-of-implementation test is necessary for the straightforward typed-error forwarding.

The HOW-TO correctly separates operation/scorer availability from retrieval acceptance, describes explicit local asset/config/calibration installation, retains source/history during reindex and explains unvalidated inspection status. One nonblocking wording correction was sent to the parent: packets expose profile/config hash/origins, not the entire effective configuration; the full settings are available through the provider configuration snapshot. Avoid describing the packet as containing all resolved settings.

git diff --check passes. This checkpoint does not establish T10 quality outcomes, T11 operational performance or overall feature shipment.
