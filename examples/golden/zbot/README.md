# Golden eval fixture: zbot (mixed-shape project)

A deliberately mixed project exercising every golden case: Python classes
with methods, nested functions, underscore symbols, external calls, test
files calling production code, an unknown-receiver `.iter()` trap, a
type-classed SQL schema with a `type TEXT,` column (the EN-15 trap), a
TypeScript module with an interface + arrow binding + artifact-kind
variable, and a Markdown doc (the mixed-mission control).

Intentionally NOT a git repo — the scanner labels the repository
`engram-mcp-scan`. Do not add files without updating
`../cases.json` (or adding a case that covers the new shape).
