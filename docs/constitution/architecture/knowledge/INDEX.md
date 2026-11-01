# Architecture — knowledge manifest

Progressive loading: read the fragment your task needs plus its prerequisites.
Full-read route: all fragments in order (~3651 tokens total).

| Fragment | Size | Prerequisites | Task routes |
|---|---|---|---|
| [How engram is built (summary)](knowledge/summary.md) | ~192 tok | none | all stages |
| [The memory pipeline](knowledge/pipeline.md) | ~326 tok | summary | all stages |
| [Layer responsibilities (core, adapters, integration, N-API)](knowledge/layers.md) | ~741 tok | pipeline | all stages |
| [Repo layout](knowledge/layout.md) | ~542 tok | layers | all stages |
| [Conventions across crates and packages](knowledge/conventions.md) | ~253 tok | layout | all stages |
| [Reference architecture (constraints, strategy, building blocks)](knowledge/golden-path.md) | ~1367 tok | layers | all stages |
