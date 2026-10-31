# Research: Spec craft — rigorous contracts, senior-grade reasoning

Date: 2026-10-31. Analysis only; no template or job edits.

## Summary

Engram should preserve v2's machine-readable contract spine while rewriting its authoring experience around the reader's decision. The precise “school project” smell is **rubric completion substituted for an engineering argument**: interchangeable tables reward filling slots, while causality, competing options, and consequences have nowhere substantial to live. Keep-and-rewrite brief, spec, and tasks; replace the plan's body with a design-led implementation narrative while retaining its metadata and traceability contract.

## Method and evidence limits

I read all four requested templates, job 011's spec/plan/tasks, job 002's v1 analysis, the index-mining hook, and the current artifact gate. References below use repository-relative paths and 1-based physical lines; T means `.agents/skills/engram-factory/assets/templates/`, J011 means `.engram-factory/jobs/011-en004-fail-closed/`, and J002 means `.engram-factory/jobs/002-expose-agent-skills/`.

External sources were fetched, not accepted from search summaries alone. Research angles: standards/RFC reasoning; customer-backwards narrative; design-document exemplars; lightweight repository-native proposals. Source-check retrieved relevant passages but returned “unclear” because automated semantic assessment was unavailable; I manually inspected original passages instead.

**Execution limitation:** no shell or engram MCP tool was exposed. I read `tools/hooks/index-mining.py`, but could not execute it, perform live recall/search, run greps, or run the strict index-evidence validator. This is **index unavailable**, not “index empty.” J002 records historical recall queries `q-1792996501127137760` and `q-179299653100728352`; those are secondary provenance in a read document, not two newly verified index results. The repository's live-index completion condition remains unmet.

## Findings: exemplar cultures and transferable lessons

1. **Claim:** Durable proposals need both precise behavior and a reasoned explanation of why that behavior is worth choosing. **Sources:** [PEP 1](https://peps.python.org/pep-0001/), [RFC 7322](https://www.rfc-editor.org/rfc/rfc7322.txt). **Support:** direct evidence. **Confidence:** high.
   PEP 1 distinguishes specification, motivation, rationale, compatibility, rejected ideas, and open issues; rationale includes alternatives and objections. RFC 7322 requires motivational context and explicit security considerations alongside formal publication structure. Interpretation: neither culture treats metadata or imperative keywords as substitutes for an argument. The brief's problem→constraints→options→decision sequence is a useful synthesis, not a universal prescribed IETF section order.

2. **Claim:** Customer-backwards prose is a thinking tool, not an instruction to inflate every document to six pages. **Sources:** [Amazon shareholder letter](https://www.aboutamazon.com/news/company-news/amazon-ceo-andy-jassy-2024-letter-to-shareholders), [Working Backwards excerpt](https://www.aboutamazon.com/news/workplace/an-insider-look-at-amazons-culture-and-processes). **Support:** direct evidence plus interpretation. **Confidence:** high.
   Amazon separately describes six-page-body narratives and PR/FAQ documents. The latter start with customer experience and examine practical difficulty; the excerpt explicitly rejects showing all one's work instead of distilling it. Researcher recommendation: use connected prose for the core causal argument and reserve tables for genuinely enumerable comparisons or evidence. The fetched official passages do not establish a universal “no bullets anywhere” or “tables only for numbers” rule.

3. **Claim:** A design document earns its length by explaining tradeoffs and making a system understandable from overview to detail. **Sources:** [Design Docs at Google](https://www.industrialempathy.com/posts/design-docs-at-google/), [Design docs — A design doc](https://www.industrialempathy.com/posts/design-doc-a-design-doc/), [Go type-parameters proposal](https://github.com/golang/proposal/blob/master/design/43651-type-parameters.md). **Support:** direct evidence. **Confidence:** high for these documents, medium for organization-wide generalization.
   Ubl's practitioner account explicitly permits adaptable form, real non-goals, alternative designs, and 1–3-page mini-docs; its public meta-exemplar demonstrates context, design rationale, and rejected approaches. The Google-origin Go proposal adds a concrete technical exemplar: overview, progressive examples, detailed semantics, limitations, and comparisons. These are historical public examples, not proof of a binding current Google-wide template.

4. **Claim:** Decision-oriented is not synonymous with short, table-free, or mechanically uniform. **Sources:** [Rust RFC 2005](https://github.com/rust-lang/rfcs/blob/master/text/2005-match-ergonomics.md), [Rust RFC 114](https://github.com/rust-lang/rfcs/blob/master/text/0114-closures.md). **Support:** direct evidence. **Confidence:** high.
   RFC 2005 shows the user-facing before/after before specifying binding rules, compatibility reasoning, drawbacks, and substantial alternative analysis. RFC 114 explains goals and competing closure designs, admits additional complexity, and leaves unresolved questions explicit. Both are lighter in bookkeeping than v2, but neither is merely a one-paragraph decision receipt.

5. **Claim:** Lightweight practice reduces the cost of changing one's mind while preserving ownership and reasoning. **Sources:** [Oxide RFD 1](https://rfd.shared.oxide.computer/rfd/0001), [README Driven Development](https://tom.preston-werner.com/2010/08/23/readme-driven-development.html). **Support:** direct evidence. **Confidence:** high for described practices.
   Oxide combines author/state/discussion metadata with viable options, data, reasoning, and a determination, scaling rigor to urgency. README-first argues for a usable introduction and interface before implementation, between exhaustive specification and none. Researcher inference: a one-pager should be a completeness-of-thought constraint, not a quota; small engram jobs can have very short narratives without discarding their structured records. No unambiguous PRE-specific source was established, so none is presented as a separate verified culture.

## Principles by artifact

These are researcher recommendations distilled from the sources, not claims that the exemplar organizations use engram's four-stage pipeline. Narrative carries reasoning; structure carries enumerable obligations, ownership, dependencies, and evidence.

### Brief — nine principles

1. Open with the affected person/system, today's harm, and the decision the owner is asking for; make the document intelligible before its first ID.
2. Preserve the owner's words as provenance, then separately synthesize the problem; do not confuse verbatim capture with explanation.
3. Distinguish symptom, hypothesized cause, and proposed mechanism; identify what is observed versus inferred.
4. Describe success as an observable changed experience with a judgment moment, not a component to build.
5. Explain why acting now beats waiting, including the cost of doing nothing; do not invent urgency or numerical precision.
6. State real constraints with their authority and consequence; connect them in prose instead of accumulating unexplained rule names.
7. Describe scope as a coherent commitment, with plausible adjacent goals deliberately excluded; MoSCoW rows are supporting records, not the story.
8. Name consequential owner choices, delegated freedoms, and unresolved questions; offer credible framing options without prematurely choosing implementation.
9. End with an explicit ratifiable boundary and owner decision; scale to a short memo, retaining only populated authority and outcome records.

### Spec — nine principles

1. Begin with a short situation→failure→desired behavior narrative that a reviewer can understand without following the trace graph.
2. Make each normative requirement independently testable; keep stable REQ IDs and separate product obligations from implementation choices.
3. Introduce a concrete user-visible example early, including denial/failure where material; explain the invariant it demonstrates.
4. Use evidence to justify claims, distinguish proven/likely/unknown, and cite precise sources rather than treating a citation cell as proof.
5. Carry rationale in paragraphs: relevant constraints, actual options, selection criterion, objection, and accepted cost.
6. Preserve dissent and uncertainty honestly; a rejected alternative must be viable enough that its rejection teaches the reader something.
7. Explain the scope and compatibility boundary in ordinary language, then map it to IDs; do not make readers reconstruct meaning from IDs alone.
8. Bind AC IDs to falsifiable outcomes and named gates; explain what each gate cannot establish as well as what evidence would count.
9. Separate risk reasoning from reviewer routing; assign ownership and revisit triggers without reducing a failure scenario to a discipline checkbox.

### Plan — nine principles

1. Lead with the chosen design and why it fits the spec's constraints; link decisions without requiring a DEC lookup for every meaningful sentence.
2. Explain the existing seam being changed, the new control/data flow, and what remains invariant; add a diagram only when it clarifies relationships.
3. Order the explanation overview→mechanism→boundary cases, not in the accidental order files will be edited.
4. Compare credible implementation approaches and their costs; distinguish newly authorized decisions from already binding ones.
5. Justify sequencing through dependencies and uncertainty reduction; “edit file A, edit file B” is not a strategy by itself.
6. Group steps into independently verifiable outcomes with concrete paths, REQ links, and gates; keep execution bookkeeping subordinate to the design.
7. Explain compatibility, consumers, migration, and both supported transports when touched; for no surface change, one evidenced sentence is enough.
8. Describe blast radius as a failure mechanism plus concrete dependents, not merely a count or a generic “low risk” adjective.
9. Explain safe rollout, partial-failure state, and recovery at the boundary that actually changes; distinguish source revert from data recovery and avoid ceremonial per-edit rollback text.

### Tasks — nine principles

1. Optimize for the implementer's next action and completion judgment, not another retelling of the proposal; checklists and tables are appropriate here.
2. Give each task a bounded observable outcome, stable TASK ID, and unambiguous STEP/REQ links.
3. Include necessary tests/docs/surface wiring as work, not as hoped-for consequences; group by independently verifiable changes rather than file count.
4. Specify actual dependencies and permit safe parallelism; do not invent a serial chain or require a commit for every task.
5. Name stop/escalation conditions that protect authority boundaries and avoid silently working around failed gates.
6. Bind tasks to every gate they must run and to exact raw evidence anchors; inventory ownership must agree with the task execution contract.
7. Keep a total REQ×task×AC coverage matrix, but review whether the linked action actually proves the obligation; presence is not coverage.
8. Track planned/executed/verified state truthfully, preserving failed and superseded evidence rather than turning a checkbox into an unsupported success claim.
9. Add delivery and review-finding records only when applicable or after findings exist; keep state inventories derived or reconciled rather than manually duplicating them.

## Per-template diagnosis with line evidence

### Brief: a questionnaire where a commissioning argument should be

**Direct evidence:** `T/brief.md:24–41` supplies a quote instruction, three labeled problem bullets, and an outcome placeholder, but no connected account of the affected experience or reason to act. `:43–53` pre-allocates all four MoSCoW buckets; `:55–66` requires constraint/decision rows whether or not real content exists. `:68–72` asks the reader to consume scope as outcome/MoSCoW/ledger IDs, and `:74–83` finishes with another form and handoff bullets.

**Interpretation:** each individual prompt is sensible; their aggregation has the voice of a worksheet. Placeholder rows such as `<fixed standard, deadline, boundary>` invite generic nouns where an owner-facing explanation belongs. The outcome's judgment-moment discipline and verbatim authority are strengths to preserve. **Verdict: keep-and-rewrite** — retain outcome, authority, constraints, and ratification records; put a concise problem/outcome/scope argument before them and allow genuinely empty categories to disappear.

### Spec: a traceability database mistaken for a specification's whole reading experience

**Direct evidence:** `T/spec.md:30–48` moves directly from intent mapping to evidence to a five-column decision row; the decision gets `<one choice>`, `<one alternative>`, and `<observable effect>`, but no room explicitly reserved for the argument connecting them. `:50–60` provides useful REQ/AC contracts. `:62–78` makes scope ID-led and combines “Risk / reviewer discipline,” conflating engineering reasoning with routing. `:43–48` and `:74–78` give decisions and risks the same placeholder-table treatment as facts.

**Interpretation:** **IDs masquerading as content** occurs when “included as REQ-001” is the only offered meaning, not whenever an ID appears. **Schema-shaped prose** occurs when every topic is squeezed into a claim/status/owner tuple, regardless of whether the reader needs a causal explanation. **Verdict: keep-and-rewrite** — preserve requirements, acceptance, gates, and trace links, but restore a motivating story and decision/risk paragraphs; compact factual registers can follow.

### Plan: a work breakdown structure wearing a design-document title

**Direct evidence:** `T/plan.md:27–30` does explicitly request a narrative — the defect is not literal absence of that instruction. But it limits it to “One concise design narrative” and says “reference by ID,” while `:32–60` allocates the body to five tables. `:38–42` assigns rollback to every step and `:62–64` repeats rollback afterward. The “New decisions” row (`:32–36`) contains no explicit rejected-alternative field or fuller tradeoff prompt.

**Interpretation:** the visual and cognitive budget says that enumeration is the main deliverable and design reasoning is a preamble. The ban on restating REQs is useful against duplication but harmful if interpreted as a ban on explaining the invariant in human language. **Verdict: replace the body, preserve its contract** — design/context/alternatives/sequence/recovery prose first; a compact step and coverage schedule afterward. No ID, frontmatter, or constitutional obligation needs to be removed to do this.

### Tasks: the right medium, contaminated by dummy content and duplicated state

**Direct evidence:** `T/tasks.md:28–32` is an appropriate actionable task table; `:41–51` usefully records coverage and gates. However, `:53–59` pre-fills every applicability row with “no” and placeholders. `:61–65` supplies a “none yet / pending review” row under a heading expressly marked “post-review only.” `:34–39` includes authoring policy about commits inside the artifact body.

**Interpretation:** task lists should not be converted into six-page essays. Their homework smell is **filler as apparent completion**: default rows look like assessed facts and review records exist before review. **Verdict: keep-and-rewrite** — retain the executable list and coverage matrix; remove dummy records and reconcile gate/state duplication.

### Cross-template smells: precise boundaries of the claim

**Direct evidence:** all four templates contain visible-in-source HTML authoring comments (`T/brief.md:12–21`, `T/spec.md:18–25`, `T/plan.md:16–22`, `T/tasks.md:15–23`) and angle-bracket placeholder rows. Those comments are hidden in rendered Markdown but exposed to raw-file readers and agents. Job 011's supplied outputs remove those comments and placeholders; it would be false to accuse those completed files of retaining them.

**Interpretation:** template scaffolding is necessary during drafting; the smell is failing to distinguish author instructions, illustrative rows, and ratified content. Repetition across phases makes every artifact feel like the same school assignment with renamed columns. Move author guidance out of the deliverable body; judge readiness by both structured integrity and the quality of the populated explanation, without inventing a mandatory paragraph quota.

## Real-output contrast: what the templates actually produce

**Direct evidence:** J011's spec (`spec.md:20–38`) opens with trace/evidence/requirements and no explanatory lead. It does state concrete behavior: a missing declared path must fail and name the path; healthy behavior must remain unchanged. The plan's Approach (`plan.md:22–24`) is a useful, short mechanism explanation, and the output omits several irrelevant template sections rather than filling every row. These are genuine strengths for an S-size fix, not evidence of carelessness.

**Interpretation:** a reader can still miss the core rationale: a missing configured scan surface currently disappears from inspection, so a clean result may mean “nothing inspected,” not “boundary intact.” This is the researcher's synthesis of the artifact's evidence, not a newly reproduced gate failure. Two or three well-written sentences would bridge the gap; requiring a long options essay for this fix would make the process worse.

**Direct evidence:** J011's tasks table (`tasks.md:22–26`) lists GATE-001 for both tasks, while the gate inventory (`:40–44`) assigns GATE-002 to TASK-001. Its STEP-002 and TASK-002 link only REQ-002 even though their two-case test includes the missing-path failure central to REQ-001 (`plan.md:26–31`; `tasks.md:22–36`). The coverage matrix says “total.”

**Interpretation:** these are incomplete/inconsistent execution links, not proof that behavior was left untested. They illustrate **traceability theater**: syntactically present identifiers can suggest a stronger semantic guarantee than the records support. Also, J011's spec names code/security/platform in frontmatter while its intent/risk text discusses “red-forman only”; these may describe different routing layers, but the artifact does not explain the distinction. Record the ambiguity, not an invented process violation.

**Direct evidence:** J002's v1 analysis supplies a causal Story (`analysis.md:48–52`) and decisions with context, severity reasoning, rejected alternatives, and consequences. Its frontmatter, evidence register, REQ IDs, and acceptance table coexist with that prose. It also contains extensive ledger repetition, a word-budget defense, and an oversized environment/profile appendix.

**Interpretation:** v1 demonstrates that rigorous traceability never required abandoning narrative; its senior quality comes from explaining why bounded damage can still cross a trust boundary and change a decision. Do not restore v1 wholesale: retain its reasoning pattern, not its administrative bulk or stale claims embedded in an old job.

## The real tension: machine truth versus human judgment

**What must stay structured:** versioned frontmatter and artifact identity; owner/status/ratification; source references and hashes; stable REQ/AC/GATE identifiers; STEP/TASK relationships; the coverage matrix; gate command, expected result, execution state, and raw evidence location; authority/deviation records needed by the constitutional contract. These are enumerable records with cross-document integrity needs. Their continued necessity is a repository-specific recommendation, not an external culture's demand for these exact identifiers.

**What must become explanatory prose:** problem and why now; coherent scope and exclusions; approach and control flow; decision rationale and plausible alternatives; risk mechanisms and accepted tradeoffs; compatibility/rollout/recovery explanations. A structured DEC or RISK identifier can anchor several paragraphs. A table can summarize alternatives after the author has explained the decision. Plain-English mention of an invariant is not a second canonical requirement.

**Direct gate evidence:** `tools/ci/check_job_artifacts.py:22–23` discovers IDs using regex; `:46–64` checks reference existence and whether REQ strings appear anywhere in tasks. `:65–69` detects TBD/TODO/FIXME, not general angle-bracket placeholders. It does not establish that an AC is proved, a gate owns the right task, an ID denotes a definition rather than a mention, or an alternative is honestly assessed. This is source inspection, not an executed gate result. The spec/task template comments advertise stronger integrity/coverage language than this implementation alone establishes.

**Recommendation/inference:** separate two review passes: deterministic validation of records and a human/agent read-through of reasoning. Generate or reconcile redundant inventories where practical; do not prescribe a new schema or gate without an implementation proposal. Add a small-job rule in the later rewrite: minimum sufficient narrative, no invented alternatives, no filler sections. Machine checks should protect linkage and honest state, not enforce essay-shaped busywork.

A practical quality test: after hiding IDs, can a reviewer explain the problem, chosen boundary, rejected option (if any), accepted risk, and success condition? After hiding narrative, can the machine still resolve obligations, work, and verification? Excellent artifacts pass both; neither view replaces the other.

## Contradictions

- The brief's prose-first aspiration is valid, but a universal no-bullets rule is not established by the fetched Amazon evidence and would contradict the useful goals lists, examples, and alternatives in the Google/Rust exemplars. Adopt prose for arguments, not prose everywhere.
- The plan already asks for narrative, and real job 011 already removes irrelevant sections. Therefore “v2 forbids prose” and “every real output keeps filler” are unsupported diagnoses; the stronger claim is that the default allocation of attention privileges bookkeeping.
- “Coverage totality” in template guidance is stronger than the current regex gate's semantic guarantees; job 011 shows why that distinction matters. No gate execution or full controller validation audit was performed, so this is not a claim about every validation layer.
- Lightweight docs and normative obligations are compatible: Oxide's metadata and Rust's formal semantics coexist with adaptable explanation. None of the exemplars justifies removing engram's owner authority or constitutional constraints.

## Missing evidence and residual risks

- Live index mining and strict index-evidence validation could not run; historical recall citations were not independently refreshed.
- One small v2 job and one v1 delta analysis are insufficient to establish a repository-wide frequency or prove that templates alone caused writing quality differences.
- Public Google guidance is a practitioner's account plus public examples, not an audited internal organization-wide standard. Historical examples establish craft, not present-day policy.
- No specific PRE definition/exemplar was verified. No empirical productivity comparison or universal Amazon table/list prohibition is claimed.
- Exact controller/parser compatibility of a replacement plan body requires separate read-only validator review before any rewrite. This report changes no templates, schemas, gates, or existing jobs.

## Sources

Kept sources were fetched and inspected: PEP 1 and RFC 7322 for normative clarity and rationale; Amazon's official letter and hosted Working Backwards excerpt for customer framing and distillation; Ubl's Google account and public meta-design exemplar for context/design/alternatives; the Go proposal for progressive technical exposition; Rust RFCs 114/2005 for concrete decision reasoning; Oxide RFD 1 for lightweight lifecycle with structured metadata; README Driven Development for reader-first interface framing. All URLs appear in the findings above.

Local sources kept: `T/{brief,spec,plan,tasks}.md`, `J011/{spec,plan,tasks}.md`, `J002/analysis.md`, `tools/ci/check_job_artifacts.py`, and `tools/hooks/index-mining.py` — direct evidence for diagnoses and limits.

Rejected/deprioritized: generic SEO design-doc templates and third-party PR/FAQ summaries, because original sources were available; the Spearhead RFD mirror, because Oxide's own document was stronger; RFC 7322 HTML extraction failed and was replaced with its official text; an attempted Oxide raw GitHub path returned 404 and was replaced with the public RFD site. No local source was silently substituted.

## Next steps

Before rewriting, refresh two relevant index queries and run the strict evidence validator using a shell-enabled session. Then test one narrative-first small-job sample against existing parsing/validation, reviewing both its authority trace and whether a fresh reader can explain its reasoning. Do not expand this into a new process framework before demonstrating that small improvement.