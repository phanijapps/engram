# RFC-0021 evidence and review notes

Date: 2026-10-14. Proposal: [Structural code context](../0021-structural-code-context.md).
These notes record research and review of the Draft. RFC-0021 was subsequently
accepted by phanijapps on 2026-10-14; the evidence below remains a record of
what was measured, not a claim that the accepted design is implemented.

## Research checkpoint

The user's request to write an RFC for the existing audit, with future parser
support, supplies the drafting frame. Prior audit recommendations are carried
forward as provisional decisions during drafting. The user's subsequent
explicit approval accepts the RFC and the proposed next-step sequence.

| Decision | Repo evidence independently inspected | External backing | Recommendation and owner |
| --- | --- | --- | --- |
| D1: provider boundary | ADR-0028 explicitly deferred a parser port and names a second engine as its revisit trigger; current `core/code/src/lib.rs` exposes TreeSitterChunker rather than a normalized provider result. | Monaco provider/result shape and Tree-sitter positions/named nodes. | Normalize facts, not full syntax trees; compiled trusted registration. phanijapps, RFC review. |
| D2: source authority | Audit live reproduction and `has_declaration_descendant`; SourceDocument has no inline source; existing KnowledgeRepository can persist/read document chunks. | Tree-sitter exact source positions. | Snapshot plus semantic chunks, with typed roles and index exclusion. phanijapps, RFC review. |
| D3: context owner | Current `symbol_context` returns callers/callees/community; structural edges encode contains/import paths; chunk summary/metadata are unset during ingest. | Monaco symbol hierarchy/ranges establishes the structural vocabulary, not full dependency resolution. | Shared context hydration over facts and source; unsupported means unknown. phanijapps, RFC review. |
| D4: input budget | MCP `embed_pending` takes 8,192 characters; stored method text is unchanged; scan-side lexical feed uses entity name/kind. | LlamaIndex structural recursion/leaf fallback; tokenizer truncation/overflow API. | Structural segments with measured full-input limits. phanijapps, RFC review. |
| D5: derivation lifecycle | RFC-0020 and `extract_knowledge.ts` place generation in TS; domain provenance already has Summarization and evidence refs; policy allowed uses do not currently include a summarization-specific authorization. | W3C derivation/invalidation; OWASP indirect prompt-injection guidance. | TS generation with a specified Rust validation/authorization port and separate derived records. phanijapps, RFC review. |

The axes/options are in the RFC. Status quo is grounded in current Engram
behavior; other categories compare retention, abstraction, ownership, budget
outcome and artifact lifecycle independently. No claim is made that one cited
project implements all the proposed contracts.

## Polyglot spike

Riskiest structural assumption: downstream consumers can use normalized unit,
parent and source-range concepts rather than TypeScript-specific node kinds.
Used the existing live server, not a new provider implementation:

```bash
python3 scripts/dev/mcp_driver.py \
  --storage /tmp/engram-rfc21-polyglot-store \
  --project rfc21-polyglot --no-vector \
  call scan '{"target":"repo","path":"/tmp/engram-rfc21-polyglot"}'
```

The three files use equivalent service methods and multiline signatures:

```typescript
import { Database } from './db';
export class UserService {
  private db: Database;
  constructor(db: Database) { this.db = db; }
  async getUser(
    id: string,
  ) { return await this.db.query(id); }
}
```

```python
from db import Database
class UserService:
    def __init__(self, db: Database):
        self.db = db
    def get_user(
        self, user_id: str,
    ):
        return self.db.query(user_id)
```

```rust
use crate::db::Database;
struct UserService { db: Database }
impl UserService {
    fn get_user(
        &self, id: &str,
    ) -> String {
        self.db.query(id)
    }
}
```

Observed: three files ingested, 17 entities, 14 relationships, zero errors.
`scanned: 4` is the scanner's reported entry count, not four input files.
Read-only persisted-chunk inspection found complete multiline method texts:

| Language | Method anchor | Lines |
| --- | --- | --- |
| TypeScript | `fn UserService::getUser` | 5–7 |
| Python | `def UserService::get_user` | 5–8 |
| Rust | `fn UserService::get_user` | 4–8 |

TS/Python class chunks and the Rust impl chunk contained the container's first
line. The Rust struct retained its one-line field declaration. This difference
shows why the common model needs explicit field/context coverage, not an
assumption that every class-like chunk has equivalent context.

Conclusion: current parsers already produce comparable location/anchor/text
concepts across these idioms. This supports the proposal, but does not validate
its new trait, detection registry, exact-byte snapshots, tokenizer segmentation
or Silver summaries. A test-only new provider and polyglot source-reconstruction
fixtures are required follow-up gates. No additional grammar dependency was
installed; vectors and LLM generation were disabled/not invoked.

The earlier [audit](../../research/ast-aware-code-chunking-audit.md) supplies
nested-function source-loss evidence and 11 passing adapter chunker tests.
The current spike is supplementary evidence, not an end-to-end semantic test.

## Citation integrity

All listed pages were fetched and their borrowed claims checked on 2026-10-14.
No upstream source was copied into the RFC.

| Source | Confirmed content | Use / limit |
| --- | --- | --- |
| [Tree-sitter introduction](https://tree-sitter.github.io/tree-sitter/) | Concrete syntax trees; recovery in the presence of errors; language parsers. | Parse availability does not establish semantic completeness. |
| [Tree-sitter basic parsing](https://tree-sitter.github.io/tree-sitter/using-parsers/2-basic-parsing.html) | Byte and row/column positions; named/anonymous nodes and parent/child traversal. | Exact source spans and structural extraction; not import resolution. |
| [Monaco provider](https://microsoft.github.io/monaco-editor/typedoc/interfaces/editor_editor_api.languages.DocumentSymbolProvider.html) and [result](https://microsoft.github.io/monaco-editor/typedoc/interfaces/editor_editor_api.languages.DocumentSymbol.html) | Providers return DocumentSymbol arrays; results have kind/name/range/selectionRange and children. | Shared output boundary precedent; not a claim that this is sufficient for Engram parsing. |
| [LlamaIndex CodeSplitter](https://github.com/run-llama/llama_index/blob/main/llama-index-core/llama_index/core/node_parser/text/code.py) | Parser injection, character/token modes, recursive child splitting and oversized leaf splitting in fetched source. | Pattern precedent only; no dependency or guarantee adopted. |
| [Tokenizers v0.20.3](https://huggingface.co/docs/tokenizers/v0.20.3/api/tokenizer) | enable_truncation has max_length and overflow stride. | Versioned conceptual reference. Latest unversioned API page lacked these details, so it is not cited for them. |
| [PROV-DM](https://www.w3.org/TR/prov-dm/) | Entity derivation, source usage/generation and invalidation concepts. | Provenance/lifecycle vocabulary; no wholesale PROV implementation proposed. |
| [OWASP LLM01](https://genai.owasp.org/llmrisk/llm01-prompt-injection/) | Indirect injection from files, validation, content separation and privilege restriction. | Source-comment/model boundary; no claim that delimiting alone prevents injection. |

Repository citations were checked against the actual files rather than stale
spec checkboxes. Charter statements about backend/embedding availability are
historical and were not used to describe current runtime availability.

## Review record

The configured adversarial/security roles failed to start because their fixed
model was unsupported on this account. Fresh Luna agents performed the two
read-only review lenses instead, consistent with the user's model preference.

- **Adversarial, first pass:** requested concrete generation authority/read
  filtering and independent options for port timing/placement and validation
  ownership. The RFC now defines a knowledge-adapter manifest/CAS, updating
  source exclusion, per-lane generation filtering and idempotent recovery;
  the options table includes those decision axes.
- **Security, first pass:** requested destination-bound authorization, exact
  derived scope, strict bounded output/evidence validation, non-agentic
  completion and enforceable parser memory/cancellation limits. All were
  incorporated. Enrichment with incompatible scopes/policies is denied.
- **Security, second pass:** requested requester/job-bound tickets and durable
  budget consumption, plus provider retention/deletion terms in authorization.
  The RFC now specifies job-wide attempt reservations and profile-bound grants;
  it does not claim provider-copy deletion or remote exactly-once execution.
- **Final independent passes:** both Luna reviewers reported
  `Clean — ready to commit.` after reviewing the amendments. No runtime code
  was implemented, and no RFC acceptance or commit is implied by that verdict.

Handoff checks: named approver, five recommended decisions, current/do-nothing
option, two owned open questions, verified references and linked polyglot spike.
Documentation gates and local-link/whitespace checks passed. Provider extension,
vector recall and real summary-quality gates remain future implementation tests,
as the RFC explicitly states.
