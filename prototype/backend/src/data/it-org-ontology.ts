// IT-organization sample ontology + taxonomy (RFC 0004 Slice 3).
//
// Pure data builder: constructs the v1 ontology/classes/properties/axioms and a
// service-tier / incident-severity taxonomy using the exact serde shapes Rust
// owns (camelCase fields; snake_case enum values). No network — the demo route
// persists these through the knowledge transport; the same objects are returned
// to the UI for browsing without separate list endpoints.

export type ItOrgRecord = Record<string, unknown>;
export type ItOrgSample = {
  ontology: ItOrgRecord;
  classes: ItOrgRecord[];
  properties: ItOrgRecord[];
  axioms: ItOrgRecord[];
  scheme: ItOrgRecord;
  concepts: ItOrgRecord[];
};

export type ItOrgOptions = {
  scope: unknown;
  policy: unknown;
  actor: unknown;
  now: string;
};

const ONTOLOGY_ID = "ontology-it-org";
const SCHEME_ID = "scheme-it-service-tiers";

/**
 * Builds the IT-org sample: classes (Person, SRE, Team, Service, Runbook,
 * Incident), properties (owns, depends_on, responds_to, authored, member_of,
 * manages), a transitive axiom on depends_on, and a service-tier / severity
 * taxonomy.
 */
export function buildItOrgOntology(opts: ItOrgOptions): ItOrgSample {
  const { scope, policy, actor, now } = opts;
  const provenance = {
    source: "demo:it-org-ontology",
    actor,
    observedAt: now,
    confidence: 1,
    method: "manual",
  };

  const ontology: ItOrgRecord = {
    id: ONTOLOGY_ID,
    uri: "urn:ontology:it-org",
    name: "IT Organization",
    scope,
    language: "owl",
    version: "1.0.0",
    status: "active",
    imports: [],
    policy,
    provenance,
    createdAt: now,
  };

  const cls = (
    id: string,
    label: string,
    parentClassIds: string[],
    description: string,
  ): ItOrgRecord => ({
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

  const classes: ItOrgRecord[] = [
    cls("class-person", "Person", [], "A person in the organization."),
    cls("class-sre", "SRE", ["class-person"], "A site reliability engineer."),
    cls("class-team", "Team", [], "A team that owns services."),
    cls("class-service", "Service", [], "A deployable service."),
    cls("class-runbook", "Runbook", [], "An operational runbook document."),
    cls("class-incident", "Incident", [], "An operational incident."),
  ];

  const prop = (
    id: string,
    label: string,
    domainClassId: string,
    rangeClassId: string,
  ): ItOrgRecord => ({
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

  const properties: ItOrgRecord[] = [
    prop("prop-owns", "owns", "class-team", "class-service"),
    prop("prop-depends-on", "depends_on", "class-service", "class-service"),
    prop("prop-responds-to", "responds_to", "class-sre", "class-incident"),
    prop("prop-authored", "authored", "class-person", "class-runbook"),
    prop("prop-member-of", "member_of", "class-person", "class-team"),
    prop("prop-manages", "manages", "class-sre", "class-service"),
  ];

  const axioms: ItOrgRecord[] = [
    {
      id: "axiom-depends-transitive",
      ontologyId: ONTOLOGY_ID,
      kind: "transitive",
      subjectClassId: "class-service",
      propertyId: "prop-depends-on",
      objectClassId: "class-service",
      expression: undefined,
      provenance,
      createdAt: now,
    },
  ];

  const scheme: ItOrgRecord = {
    id: SCHEME_ID,
    uri: "urn:scheme:it-service-tiers",
    name: "IT Service Tiers & Severity",
    scope,
    version: "1.0.0",
    provenance,
    policy,
    createdAt: now,
  };

  const concept = (id: string, label: string): ItOrgRecord => ({
    id,
    uri: `urn:concept:${id}`,
    schemeId: SCHEME_ID,
    prefLabel: { value: label, language: "en" },
    altLabels: [],
    status: "active",
    provenance,
    createdAt: now,
  });

  const concepts: ItOrgRecord[] = [
    concept("concept-tier-1", "Tier 1 — critical"),
    concept("concept-tier-2", "Tier 2 — standard"),
    concept("concept-tier-3", "Tier 3 — internal"),
    concept("concept-sev-1", "SEV 1 — incident"),
    concept("concept-sev-2", "SEV 2 — major"),
    concept("concept-sev-3", "SEV 3 — minor"),
    concept("concept-sev-4", "SEV 4 — noise"),
  ];

  return { ontology, classes, properties, axioms, scheme, concepts };
}

export const IT_ORG_ONTOLOGY_ID = ONTOLOGY_ID;
