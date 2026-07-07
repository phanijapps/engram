---
name: lighter-review-on-demo-work
description: User wants lighter adversarial/QA review on demo and exploratory "art of the possible" work — few passes, not iterate-to-clean grinding.
metadata:
  type: feedback
---

On demo / exploratory / "art of the possible" work, keep review light: one adversarial pass, skip the quality-engineer grind, and do NOT iterate adversarial-review to a many-round "Clean" verdict. Keep the mechanical gates (typecheck / build / test) — those are cheap and objective.

**Why:** the exhaustive multi-pass adversarial + QE loop (which ran ~13 passes on the belief/source-assertion feature) is overkill for demo-layer UI/visual work where the payoff is speed and the cost of a minor miss is low. The user explicitly asked to "go lighter on Adversarial and QA" during the [[demo-reimagine]] spec.

**How to apply:** reserve the full iterate-to-clean adversarial + quality-engineer treatment for core contracts, domain types, security boundaries, and persistence. For demo/UI/visual/exploratory changes, one review pass is enough — apply the obvious fixes and move on. Distinct from [[string-op-guard-trim]] which is a code rule, not a process preference.
