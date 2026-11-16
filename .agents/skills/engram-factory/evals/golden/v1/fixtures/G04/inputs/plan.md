---
schema_version: 2
plan_format: "compact-v1"
size_tier: "S"
---
# Plan: Parse, validate and persist scoped import

## Owner summary
Deliver the scoped fixture change and prove the accepted outcomes without widening the assignment.

## Approach
Keep `read_rows(path)` as the CSV parser. Add `validate_rows(rows)`,
`persist_unique(valid_rows, destination)` and `import_csv(path, destination)`
in importer.py; no new service, database, CLI or source-file writes.

The destination is a caller-supplied in-memory dictionary mapping exact ID strings
to name strings. Do not trim or coerce IDs. A row is malformed when id or name is
missing/empty, or csv.DictReader exposes surplus columns under the None key.
Check surplus columns first, then id, then name. Report the physical data row
number starting at 2 and reason `unexpected columns`, `missing id` or `missing name`.
The fixture has no multiline cells; row numbering is over parsed records.

`validate_rows` returns (valid_rows, rejected), preserving input order. Valid rows
contain row, id and name; rejected entries contain row and reason. Do not mutate
parser output. `persist_unique` skips IDs already in destination, including those
inserted earlier in the batch, retaining the first value. Return (imported,
duplicates) as ordered lists of ID strings; duplicates includes each skipped
occurrence. `import_csv` composes those seams and returns a dictionary with keys
imported, duplicates and rejected. Only destination is mutated. For sample.csv
and an empty destination, expect {a: Ada, b: Bea}, imported [a,b], duplicates [a]
and rejected [{row:5, reason:missing id}]. Repeat import adds no entries.

Preserve source outside the explicit task ownership and retain failed proof.

## Steps
| ID | Action | Requirements | Depends on | Owned paths | Proof |
| --- | --- | --- | --- | --- | --- |
| STEP-001 | Implement the accepted fixture outcome | REQ-001 | none | importer.py, tests/test_importer.py | GATE-001 |

## Proof
| ID | Acceptance | Command or protocol | Expected |
| --- | --- | --- | --- |
| GATE-001 | AC-001, AC-002 | python3 -m unittest discover -s tests -v | Accepted positive and negative outcomes are observed |

## Rollback and risks
Revert the bounded task change if its required proof cannot pass; preserve evidence and escalate any change to an accepted contract.

## Decisions and evidence
LED-001 scopes this synthetic fixture; no production or external mutation is authorized. Owner Finish remains separate from Start.
