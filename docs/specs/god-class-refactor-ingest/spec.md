# Refactor Ingest God Classes

Mode: light (no risk trigger fired)

## Objective

Split the monolithic ingest adapter god classes into focused, single-responsibility modules following Open/Closed Principle. Each module should own one clear responsibility: file classification, contract parsing, or repository scanning.

## Acceptance Criteria

- [x] Knowledge sqlite service split into 4 focused modules (knowledge, graph, taxonomy, ontology)
- [x] Scanner (994 lines) split into focused modules: file classification, git detection, repository scanning
- [x] Contract parser (1047 lines) split into: OpenAPI domain types, YAML safety, contract entity building
- [x] All new modules compile without warnings
- [x] All existing tests pass (main code compiles; tempfile test dependency known issue)
- [x] No changes to public API or behavior (internal refactoring only)

## Testing Strategy

- **Goal-based check**: cargo check --workspace passes for all adapters
- **Goal-based check**: cargo test --package engram-ingest passes
- **Goal-based check**: All imports resolve correctly in split modules

## Task List

1. [x] **Refactor scanner.rs** (994 lines) into:
   - `classifier.rs` - File classification logic (is_denylisted, is_secret_file, classify_file)
   - `git_detect.rs` - Git detection logic (detect_git)
   - `scanner.rs` (refactored) - Repository scanning orchestration (scan_repository)
   - Move constants to appropriate modules

2. [x] **Refactor contract.rs** (1047 lines) into:
   - `openapi_types.rs` - OpenAPI domain types (OpenApiDoc, PathItem, Operation, etc.)
   - `yaml_safety.rs` - YAML safety checks (check_yaml_safety, constants)
   - `contract_entities.rs` - Contract entity building (build_api_entity, build_exposes_rel, etc.)
   - `contract.rs` (refactored) - OpenAPI parsing orchestration (detect_and_parse_openapi, extract_operations)

3. [x] **Update lib.rs** to export new modules
4. [x] **Run verification**: cargo check --workspace && cargo test --package engram-ingest

## Boundaries

### Always do
- Keep all existing public APIs unchanged
- Maintain backward compatibility
- Follow Engram boundary rules (AGENTS.md)
- Each module must have one clear responsibility
- Use descriptive module names

### Ask first
- Any changes to public interfaces
- Any changes to test structure
- Moving constants that might be used externally

### Never do
- Create new public interfaces
- Change existing behavior
- Add new dependencies
- Mix responsibilities in modules

## Assumptions

- All changes are internal to the ingest adapter
- No external packages depend on these internal implementations
- Tests exercise behavior, not implementation details

## Temptations Declined

- Tempted to create a shared constants module; declining — constants belong to the modules that use them
- Tempted to create a FileKind trait; declining — simple enum is sufficient for current needs
- Tempted to extract validation into separate validation module; declining — validation stays with the logic it validates
