# Large-graph visualization & viz-backend transport — applied survey

> Discipline: applied (practitioner-pattern survey)

**Scope:** decision-grade prior art for the engram-viz overhaul — performant
browser rendering of a ~170k-node / ~227k-edge knowledge graph (423k text
chunks in a 2 GB SQLite+vector store), and the Node.js backend transport over
a Rust N-API binding. Prepared 2026-08-03 to ground the overhaul brief/spec.

## Bottom line

- **Rendering:** do not render 170k raw nodes. Render the **community
  hierarchy** (Leiden/Louvain meta-nodes) at the overview; drill into a
  neighborhood on demand with a hard cap. WebGL is mandatory past ~10k
  elements (SVG/Canvas2D break down). **deck.gl** is the safest production
  choice (1M items @ 60 FPS, best perf docs); **Cosmos.gl/Cosmograph** is the
  graph-native GPU alternative (vendor-sourced ceiling claims).
- **Transport:** a **thin Hono HTTP/SSE Backend-for-Frontend** that loads the
  Rust `.node` N-API addon **in-process** and exposes structured-JSON over
  REST + streaming. This beats both bridging engram-mcp (stdio + Debug-text
  blobs + not a UI transport) and exposing the browser directly to the native
  binding (unreachable). MCP stays the LLM-agent surface; the browser never
  speaks MCP.
- **Data motion:** **keyset/cursor pagination** (offset degrades O(n)), server-
  side aggregation before push, and **SSE over WebSocket** for unidirectional
  server→client streams (browser-native reconnection, simpler).

## Findings

### F1 — Rendering-tech ceilings [high]
SVG smooth to ~1–5k DOM nodes; Canvas2D ~5–10k (stretching to ~50k with
culling); WebGL 100k–1M+ at 60 FPS; WebGPU 1M+ but no production graph lib is
WebGPU-first yet. Horak et al. (2018) measured SVG/Canvas parity until
~10k elements. Rule of thumb: SVG dies ~1–2k, Canvas ~10k, WebGL above.
[Horak 2018]; [yWorks blog]; [deck.gl perf docs].

### F2 — Library shortlist for 100k+ nodes [moderate] *(vendor-bias flagged)*
| Library | Renderer | Credible ceiling | Caveat |
|---|---|---|---|
| **deck.gl** (Scatterplot + Arc layers) | WebGL | **1M @ 60 FPS** | Not graph-native; compose layers. Best falsifiable docs. |
| **Cosmos.gl / Cosmograph** | WebGL (GPU layout+render) | ~1M claimed; 133k/321k demo | Vendor claim, no independent benchmark. |
| **sigma.js** + graphology | WebGL | ~100k edges; layout-bound past ~10k nodes | Pair with pre-computed layout. |
| **Cytoscape.js** (WebGL preview, Jan 2025) | Canvas + WebGL preview | ~5× FPS uplift in preview | Preview, not benchmarked at 100k+. |
| react-force-graph (three.js) | WebGL (3D) | below 100k interactive | Matches zbot's Graph-tab aesthetic but not this scale. |

[deck.gl perf]; [OpenJSF cosmos.gl]; [Cytoscape WebGL preview]; [sigma.js];
[Nightingale "million nodes"]. Survivorship bias: Cosmos/yFiles/Tom Sawyer
numbers are vendor marketing; deck.gl docs cite concrete hardware specs.

### F3 — Level-of-detail = render the community hierarchy [high]
The dominant strategy at this scale is **never render raw nodes at overview**.
Pre-compute communities server-side; render meta-nodes; drill on click.
Microsoft **GraphRAG** is canonical: extract KG → **Leiden hierarchical
communities** → summaries per community per level → map-reduce query. **Leiden
over Louvain** (Traag 2019): Leiden guarantees well-connected communities.
**Shneiderman's mantra** (1996): overview → zoom/filter → details-on-demand.
[GraphRAG docs + arXiv 2404.16130]; [Traag 2019 PMC6435756]; [Shneiderman 1996].

### F4 — Server-side pre-aggregation + on-demand neighborhoods [high]
Heavy metrics (centrality, communities, bridge/betweenness) pre-computed and
materialized server-side; browser gets small summaries (community meta-nodes +
sizes + top-K bridges) and fetches raw neighborhoods on demand with hard caps
(K≈50–100/expand). **Local-fit:** engram already ships `engram-graph-analytics`
(PageRank, betweenness, communities, reachability) + a `community-summary`
GraphRAG adapter — the server primitives partly exist; the gap is browser
composition. [Tom Sawyer]; [GraphRAG]; [Neo4j NVL/Bloom on-demand expansion].

### F5 — Pagination & streaming [high]
**Keyset/cursor pagination is mandatory** — offset degrades O(n) (StackSync:
468µs @ OFFSET 0 → 87ms @ OFFSET 1,000,000; 17× speedup at depth). For
server→client push, **SSE > WebSocket** when the stream is unidirectional:
browser-native reconnection, standard-HTTP, one fewer protocol (Shopify BFCM
went from 10s min delay → ms-level push). Server aggregates before push.
[StackSync keyset]; [Shopify SSE].

### F6 — Transport: thin BFF + in-process N-API wins [high]
| Axis | (a) in-process N-API | (b) MCP stdio bridge | (c) HTTP/SSE BFF → (a) |
|---|---|---|---|
| Latency to Rust | lowest (fn call) | worst (spawn + JSON-RPC/stdio) | lowest (BFF calls (a) in-proc) |
| Data shape | structured JSON | `content:[{type:"text"}]` text blobs | structured JSON over REST |
| Browser-reachable | no | no (stdio) | **yes** |
| WS/SSE streaming | ugly to surface | stdio only | **native** |

(c) wins on every axis that matters. N-API loads the `.node` addon in-process —
zero IPC, ABI-stable. Sidecars/subprocesses add real overhead (MeshInsight:
up to 2.7× latency worst-case) and the practitioner consensus is in-process
libraries win for latency-sensitive local workloads.
[Sam Newman BFF]; [AWS BFF]; [Node.js Node-API]; [60devs Node IPC];
[MeshInsight SOCC'23].

### F7 — MCP is not a UI transport [high]
Per modelcontextprotocol.io, MCP's host/client/server model targets an **"AI
application like Claude Code"** exposing tools to a model; tools are
**"model-controlled"** with a **"human in the loop"** trust warning — semantics
that make no sense for a trusted viz backend. Tool results default to text
content blocks; `structuredContent`/`outputSchema` (2025-06-18) are new and
"unrelated to LLM structured outputs." Stdio is "direct process communication
between local processes" — not browser-reachable. **Keep MCP for the agent
surface; back it with the same in-process N-API SDK the BFF uses** (matches the
project's own ADR-0022 surface-parity rule: one Rust capability, multiple
transports). [MCP Architecture]; [MCP Tools spec]; [MCP Schema spec].

### F8 — Keep the BFF thin [moderate]
BFF literature is dominated by cross-team scale stories (SoundCloud/Netflix/
Shopify/AWS). For a **single UI over one local store**, the BFF should be
thin: shaping + streaming + pagination + auth boundary — not microservice
fan-out (WunderGraph: for one client a BFF "usually adds more overhead than it
removes"). Don't over-build. [WunderGraph BFF lessons].

## Survivorship-bias flags
- Cosmos/yFiles/Tom Sawyer performance numbers are vendor marketing — treat as
  upper bounds, not independent measurements.
- deck.gl docs are vendor (vis.gl/Uber) but cite concrete hardware + falsifiable
  tuning guidance — more trustworthy than round-number vendor claims.
- BFF scale stories reflect multi-team deployments; a single-user local viz
  needs a far thinner gateway.
- MeshInsight's 2.7× sidecar overhead is a worst-case at scale; at low RPS the
  argument against MCP-bridging is ergonomic/semantic (text blobs, no
  streaming, wrong protocol), not just latency.

## Known unknowns
- **Known-unknown:** independent benchmark of Cosmos.gl/Cosmograph at ~170k
  nodes (only vendor demos exist). Would be closed by running a headless
  frame-rate probe on a 170k-node sample before committing to it over deck.gl.
- **Known-unknown:** whether engram's `community-summary` adapter + Leiden are
  wired for the `agentzero` store today (hierarchy tables are empty —
  `hierarchy_build` has not been run). Would be closed by running
  `hierarchy_build` and inspecting output cardinality.
- **Unknowable from research:** the right K (neighborhood-expand cap) and
  community-count target for smooth interaction — depends on the real edge
  distribution and target hardware; decide empirically during the build.
- **Known-unknown:** process-isolation value of running Rust out-of-process
  (an N-API panic crashes the Node host). Would be closed by surveying the
  Rust core's `unsafe` surface / panic history; if material, run the binding in
  a worker_thread or child process despite the IPC cost.

## Sources
- deck.gl Performance Optimization — https://deck.gl/docs/developer-guide/performance
- Horak et al. 2018, Comparing Rendering Performance — https://imld.de/cnt/uploads/Horak-2018-Graph-Performance.pdf
- yWorks, SVG/Canvas/WebGL — https://www.yworks.com/blog/svg-canvas-webgl
- OpenJSF, Introducing cosmos.gl — https://openjsf.org/blog/introducing-cosmos-gl
- Nightingale, Visualizing a graph with a million nodes — https://nightingaledvs.com/how-to-visualize-a-graph-with-a-million-nodes/
- Cytoscape.js WebGL preview (Jan 2025) — https://blog.js.cytoscape.org/2025/01/13/webgl-preview/
- sigma.js — https://www.sigmajs.org/
- Microsoft GraphRAG — https://microsoft.github.io/graphrag/ ; arXiv 2404.16130 — https://arxiv.org/html/2404.16130v2
- Traag et al. 2019, Louvain to Leiden — https://pmc.ncbi.nlm.nih.gov/articles/PMC6435756/
- Shneiderman 1996, The Eyes Have It — https://www.cs.umd.edu/~ben/papers/Shneiderman1996eyes.pdf
- Tom Sawyer, Large-Scale Graph Visualization — https://blog.tomsawyer.com/large-scale-graph-visualization
- Sam Newman, BFF pattern — https://samnewman.io/patterns/architectural/bff/
- AWS, Backends for Frontends — https://aws.amazon.com/blogs/mobile/backends-for-frontends-pattern/
- WunderGraph, 7 BFF lessons — https://wundergraph.com/blog/7-key-lessons-i-learned-while-building-bffs
- Node.js Node-API — https://nodejs.org/api/node-api.html
- 60devs, Node.js IPC performance — https://60devs.com/performance-of-inter-process-communications-in-nodejs.html
- MeshInsight, sidecar overhead (SOCC'23) — https://foci.uw.edu/papers/socc23-meshinsight.pdf
- MCP Architecture — https://modelcontextprotocol.io/docs/concepts/architecture
- MCP Tools spec (2025-06-18) — https://modelcontextprotocol.io/specification/2025-06-18/server/tools
- MCP Schema spec (structuredContent) — https://modelcontextprotocol.io/specification/2025-06-18/schema
- StackSync, keyset pagination — https://www.stacksync.com/blog/keyset-cursors-postgres-pagination-fast-accurate-scalable
- Shopify, SSE for real-time streaming — https://shopify.engineering/server-sent-events-data-streaming
