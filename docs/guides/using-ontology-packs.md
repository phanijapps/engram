# How to Make Best Use of Ontology Packs

This guide follows ASD-STE100 Simplified Technical English. Each sentence
holds one idea. Each sentence has 25 words or fewer. Each paragraph has six
sentences or fewer. The same word keeps the same meaning through this guide.

## What an ontology pack is

An ontology pack is a standard vocabulary for one domain. Engram holds 33
pinned packs. FIBO covers banking. schema.org covers travel and commerce.
Each pack holds classes, properties, and a hierarchy. Each pack carries a
sha256 pin and a license record. The server never changes a pin.

## When to install a pack

Install a pack when your project belongs to a domain. Examples are banking,
travel, geospatial work, and sensor data. Do not install a pack for a pure
software project. A pure software project keeps the default software
taxonomy. The behavior of a project without a pack does not change.

## The setup flow

1. Scan the repository. Read the `README` file and the package names.
2. List the candidate packs. Run `engram-mcp pack list`.
3. Pick one pack. Give a reason for your pick. Wait for the owner to confirm.
4. Install the pack. Run `engram-mcp pack install <id> --storage <dir> --project <p>`.
5. Restart the server. The server reads the pack layer at start only.
6. Ingest and distill. Run `scan_repo`, then use the distill flow.

An autonomous run does not wait for confirmation. An autonomous run logs the
pick and the reason. You stay accountable for an autonomous pick.

## What the install report tells you

The install report shows two counts. One count is the sliced classes. One
count is the available classes. Example: `classes 150/2449 available`. The
slice keeps the most used terms. The reduction is never silent. A mismatch
against the pin is a hard error. Never edit a pin to make a fetch pass.

## How to distill under a pack

Read the active vocabulary first. Call `ontology_read`, then
`taxonomy_read`. Classify each entity against the pack terms. Example: use
`Loan` and `DepositAccount` from FIBO. Use the declared predicates only.
Stamp a verdict on each fact. Use `grounded_in_source` when a document quote
supports the fact. Use `asserted_by_agent` for your own inference. The word
`derived` is reserved. Each write surface rejects the word `derived`.

## What to do when a term is missing

Check the counts in the install report. Call `pack widen <id>` when a term
you need is absent. Widen regenerates the launch slice offline. Widen never
fetches. Restart the server after a widen. Read the vocabulary again.

## How docs and code share one store

Run `scan_repo` on the repository path. The scan indexes the documents and
the code. The code symbols join the same store as the distilled facts. One
query can return both halves. Call `recall` for fused results. Call
`symbol_context` for callers and callees of one symbol. Each project scope
keeps its own packs and its own graph.

## Query patterns that work

Ask precise questions. Give the symbol name for a code question. Give the
domain term for a concept question. Check the verdict on each fact in the
answer. A green verdict means a source supports the fact. An orange verdict
means the agent inferred the fact. Read the source reference before you act
on an orange fact.

## Limits you must know

The server never calls an LLM. You do the extraction, the classification,
and the bridging. The pack layer gives vocabulary only. The pack layer gives
no formal reasoning. Widening needs a server restart. First install of a
pack needs the network once. Every later operation runs offline.
