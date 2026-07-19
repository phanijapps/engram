// Code Repository + IT SDLC ontology and taxonomy for the MCP server.
//
// Covers the concepts a coding agent encounters in a software repo:
// structural code entities (Repository → Package → Module → File → Function/Class),
// version control (Branch, Commit, PullRequest, Tag), and SDLC artifacts
// (Epic, Story, Task, Bug, Sprint, Pipeline, Build, Deployment, Environment,
// Release, ChangeRequest).
//
// Pure data builder — no side effects. The MCP startup seeder (seed.ts)
// persists these through the knowledge transport.

export type Record = { [key: string]: unknown };

export type CodeSdlcSample = {
  ontology: Record;
  classes: Record[];
  properties: Record[];
  axioms: Record[];
  schemes: Record[];
  concepts: Record[];
};

export type SeedOptions = {
  scope: unknown;
  policy: unknown;
  actor: unknown;
  now: string;
};

const ONTOLOGY_ID = "ontology-code-sdlc";
const SCHEME_WORK_ITEM_ID = "scheme-sdlc-work-items";
const SCHEME_ENVIRONMENT_ID = "scheme-sdlc-environments";
const SCHEME_PIPELINE_STAGE_ID = "scheme-sdlc-pipeline-stages";
const SCHEME_CHANGE_TYPE_ID = "scheme-sdlc-change-types";
const SCHEME_CODE_ENTITY_ID = "scheme-code-entity-kinds";

export function buildCodeSdlcOntology(opts: SeedOptions): CodeSdlcSample {
  const { scope, policy, actor, now } = opts;

  const provenance = {
    source: "mcp:code-sdlc-ontology",
    actor,
    observedAt: now,
    confidence: 1,
    method: "manual",
  };

  // ── Ontology root ─────────────────────────────────────────────────────────

  const ontology: Record = {
    id: ONTOLOGY_ID,
    uri: "urn:ontology:code-sdlc",
    name: "Code Repository & IT SDLC",
    scope,
    language: "owl",
    version: "1.0.0",
    status: "active",
    imports: [],
    policy,
    provenance,
    createdAt: now,
  };

  // ── Class builder ─────────────────────────────────────────────────────────

  const cls = (
    id: string,
    label: string,
    parentClassIds: string[],
    description: string,
  ): Record => ({
    id,
    ontologyId: ONTOLOGY_ID,
    uri: `urn:cls:${id.replace("class-", "")}`,
    label,
    description,
    parentClassIds,
    conceptRefs: [],
    status: "active",
    provenance,
    createdAt: now,
  });

  // ── Code structure classes ────────────────────────────────────────────────

  const classes: Record[] = [
    // VCS
    cls("class-repository", "Repository", [], "A version-controlled code repository."),
    cls("class-branch", "Branch", [], "A named ref pointing to a commit history."),
    cls("class-commit", "Commit", [], "An immutable snapshot of the repository tree."),
    cls("class-pull-request", "PullRequest", [], "A proposed merge of one branch into another."),
    cls("class-tag", "Tag", [], "An immutable named ref, typically marking a release."),

    // Code structure
    cls("class-file", "File", [], "A source file tracked in version control."),
    cls("class-module", "Module", [], "A logical grouping of related files (directory or namespace)."),
    cls("class-package", "Package", [], "A distributable unit — npm package, crate, pip package, etc."),
    cls("class-function", "Function", [], "A named callable unit of code."),
    cls("class-code-class", "Class", [], "An object-oriented class definition."),
    cls("class-interface", "Interface", [], "A structural contract (TypeScript interface, Rust trait, Java interface)."),
    cls("class-component", "Component", [], "A coarse architectural unit — microservice, library, plugin."),
    cls("class-dependency", "Dependency", [], "An external package or service this codebase depends on."),
    cls("class-test", "Test", ["class-function"], "A test function or test suite."),
    cls("class-config-file", "ConfigFile", ["class-file"], "Build, lint, CI, or environment configuration file."),

    // SDLC planning
    cls("class-epic", "Epic", [], "A large body of work decomposed into stories."),
    cls("class-story", "Story", [], "A user-facing feature or capability."),
    cls("class-task", "Task", [], "A concrete unit of engineering work."),
    cls("class-bug", "Bug", ["class-task"], "A defect to be fixed."),
    cls("class-spike", "Spike", ["class-task"], "A time-boxed investigation or research task."),
    cls("class-sprint", "Sprint", [], "A time-boxed development iteration."),
    cls("class-milestone", "Milestone", [], "A named checkpoint or delivery target."),

    // CI/CD
    cls("class-pipeline", "Pipeline", [], "An automated CI/CD workflow definition."),
    cls("class-build", "Build", [], "A single execution of a CI pipeline."),
    cls("class-deployment", "Deployment", [], "A release of a component to an environment."),
    cls("class-environment", "Environment", [], "A named runtime environment (dev, staging, prod)."),
    cls("class-release", "Release", [], "A versioned, deployable artifact set."),
    cls("class-change-request", "ChangeRequest", [], "A formal request to change a production system."),
    cls("class-incident", "Incident", [], "An unplanned interruption or degradation of a service."),
    cls("class-runbook", "Runbook", [], "A step-by-step operational procedure document."),
  ];

  // ── Property builder ──────────────────────────────────────────────────────

  const prop = (
    id: string,
    label: string,
    domainClassId: string,
    rangeClassId: string,
  ): Record => ({
    id,
    ontologyId: ONTOLOGY_ID,
    uri: `urn:prop:${label}`,
    label,
    kind: "object",
    domainClassId,
    rangeClassId,
    datatype: undefined,
    inversePropertyId: undefined,
    status: "active",
    provenance,
    createdAt: now,
  });

  const properties: Record[] = [
    // Code structure
    prop("prop-contains-branch", "contains_branch", "class-repository", "class-branch"),
    prop("prop-contains-file", "contains_file", "class-module", "class-file"),
    prop("prop-contains-module", "contains_module", "class-package", "class-module"),
    prop("prop-part-of", "part_of", "class-file", "class-module"),
    prop("prop-defines", "defines", "class-file", "class-function"),
    prop("prop-defines-class", "defines", "class-file", "class-code-class"),
    prop("prop-calls", "calls", "class-function", "class-function"),
    prop("prop-implements", "implements", "class-code-class", "class-interface"),
    prop("prop-depends-on-pkg", "depends_on", "class-component", "class-dependency"),
    prop("prop-depends-on-svc", "depends_on", "class-component", "class-component"),

    // VCS relationships
    prop("prop-merges-into", "merges_into", "class-pull-request", "class-branch"),
    prop("prop-based-on", "based_on", "class-branch", "class-branch"),
    prop("prop-points-to", "points_to", "class-tag", "class-commit"),
    prop("prop-authored", "authored", "class-commit", "class-file"),

    // SDLC relationships
    prop("prop-decomposes-into", "decomposes_into", "class-epic", "class-story"),
    prop("prop-tracked-in", "tracked_in", "class-story", "class-sprint"),
    prop("prop-resolves", "resolves", "class-pull-request", "class-task"),
    prop("prop-fixes", "fixes", "class-pull-request", "class-bug"),
    prop("prop-belongs-to-milestone", "belongs_to", "class-story", "class-milestone"),

    // CI/CD relationships
    prop("prop-triggers", "triggers", "class-pull-request", "class-pipeline"),
    prop("prop-produces", "produces", "class-build", "class-release"),
    prop("prop-deploys-to", "deploys_to", "class-deployment", "class-environment"),
    prop("prop-deployed-in", "deployed_in", "class-release", "class-environment"),
    prop("prop-governs", "governs", "class-change-request", "class-deployment"),
    prop("prop-documents", "documents", "class-runbook", "class-incident"),
  ];

  // ── Axioms ────────────────────────────────────────────────────────────────

  const axioms: Record[] = [
    {
      id: "axiom-depends-on-transitive",
      ontologyId: ONTOLOGY_ID,
      kind: "transitive",
      subjectClassId: "class-component",
      propertyId: "prop-depends-on-svc",
      objectClassId: "class-component",
      expression: undefined,
      provenance,
      createdAt: now,
    },
    {
      id: "axiom-calls-transitive",
      ontologyId: ONTOLOGY_ID,
      kind: "transitive",
      subjectClassId: "class-function",
      propertyId: "prop-calls",
      objectClassId: "class-function",
      expression: undefined,
      provenance,
      createdAt: now,
    },
  ];

  // ── Taxonomy schemes + concepts ───────────────────────────────────────────

  const scheme = (id: string, uri: string, name: string): Record => ({
    id,
    uri,
    name,
    scope,
    version: "1.0.0",
    provenance,
    policy,
    createdAt: now,
  });

  const concept = (
    id: string,
    schemeId: string,
    label: string,
    broader?: string,
  ): Record => ({
    id,
    uri: `urn:concept:${id}`,
    schemeId,
    prefLabel: { value: label, language: "en" },
    altLabels: [],
    broaderConceptId: broader ?? undefined,
    status: "active",
    provenance,
    createdAt: now,
  });

  const schemes: Record[] = [
    scheme(SCHEME_WORK_ITEM_ID, "urn:scheme:sdlc-work-items", "SDLC Work Item Types"),
    scheme(SCHEME_ENVIRONMENT_ID, "urn:scheme:sdlc-environments", "Deployment Environments"),
    scheme(SCHEME_PIPELINE_STAGE_ID, "urn:scheme:sdlc-pipeline-stages", "CI/CD Pipeline Stages"),
    scheme(SCHEME_CHANGE_TYPE_ID, "urn:scheme:sdlc-change-types", "Change Types"),
    scheme(SCHEME_CODE_ENTITY_ID, "urn:scheme:code-entity-kinds", "Code Entity Kinds"),
  ];

  const concepts: Record[] = [
    // Work item types
    concept("wi-epic", SCHEME_WORK_ITEM_ID, "Epic"),
    concept("wi-story", SCHEME_WORK_ITEM_ID, "Story", "wi-epic"),
    concept("wi-task", SCHEME_WORK_ITEM_ID, "Task", "wi-story"),
    concept("wi-bug", SCHEME_WORK_ITEM_ID, "Bug", "wi-task"),
    concept("wi-spike", SCHEME_WORK_ITEM_ID, "Spike", "wi-task"),
    concept("wi-subtask", SCHEME_WORK_ITEM_ID, "Subtask", "wi-task"),

    // Environments
    concept("env-local", SCHEME_ENVIRONMENT_ID, "Local"),
    concept("env-dev", SCHEME_ENVIRONMENT_ID, "Development"),
    concept("env-integration", SCHEME_ENVIRONMENT_ID, "Integration"),
    concept("env-staging", SCHEME_ENVIRONMENT_ID, "Staging"),
    concept("env-production", SCHEME_ENVIRONMENT_ID, "Production"),
    concept("env-dr", SCHEME_ENVIRONMENT_ID, "Disaster Recovery", "env-production"),

    // Pipeline stages
    concept("stage-source", SCHEME_PIPELINE_STAGE_ID, "Source"),
    concept("stage-build", SCHEME_PIPELINE_STAGE_ID, "Build", "stage-source"),
    concept("stage-test", SCHEME_PIPELINE_STAGE_ID, "Test", "stage-build"),
    concept("stage-security-scan", SCHEME_PIPELINE_STAGE_ID, "Security Scan", "stage-test"),
    concept("stage-package", SCHEME_PIPELINE_STAGE_ID, "Package", "stage-test"),
    concept("stage-deploy-staging", SCHEME_PIPELINE_STAGE_ID, "Deploy to Staging", "stage-package"),
    concept("stage-integration-test", SCHEME_PIPELINE_STAGE_ID, "Integration Test", "stage-deploy-staging"),
    concept("stage-deploy-prod", SCHEME_PIPELINE_STAGE_ID, "Deploy to Production", "stage-integration-test"),

    // Change types
    concept("change-feature", SCHEME_CHANGE_TYPE_ID, "Feature"),
    concept("change-bugfix", SCHEME_CHANGE_TYPE_ID, "Bug Fix"),
    concept("change-hotfix", SCHEME_CHANGE_TYPE_ID, "Hotfix"),
    concept("change-refactor", SCHEME_CHANGE_TYPE_ID, "Refactor"),
    concept("change-chore", SCHEME_CHANGE_TYPE_ID, "Chore"),
    concept("change-release", SCHEME_CHANGE_TYPE_ID, "Release"),
    concept("change-rollback", SCHEME_CHANGE_TYPE_ID, "Rollback"),
    concept("change-config", SCHEME_CHANGE_TYPE_ID, "Configuration Change"),
    concept("change-dependency", SCHEME_CHANGE_TYPE_ID, "Dependency Update"),

    // Code entity kinds (mirrors EntityKind values from the knowledge graph)
    concept("ek-function", SCHEME_CODE_ENTITY_ID, "Function"),
    concept("ek-class", SCHEME_CODE_ENTITY_ID, "Class"),
    concept("ek-interface", SCHEME_CODE_ENTITY_ID, "Interface"),
    concept("ek-module", SCHEME_CODE_ENTITY_ID, "Module"),
    concept("ek-constant", SCHEME_CODE_ENTITY_ID, "Constant"),
    concept("ek-type", SCHEME_CODE_ENTITY_ID, "Type"),
    concept("ek-enum", SCHEME_CODE_ENTITY_ID, "Enum"),
    concept("ek-trait", SCHEME_CODE_ENTITY_ID, "Trait"),
    concept("ek-struct", SCHEME_CODE_ENTITY_ID, "Struct"),
    concept("ek-endpoint", SCHEME_CODE_ENTITY_ID, "Endpoint"),
  ];

  return { ontology, classes, properties, axioms, schemes, concepts };
}

export const CODE_SDLC_ONTOLOGY_ID = ONTOLOGY_ID;
