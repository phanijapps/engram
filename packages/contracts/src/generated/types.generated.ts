/* Generated from contracts/v1/schemas/engram-v1.schema.json. Do not edit. */

export type Identifier = string;
export type CodeParseQuality = "clean" | "recovered" | "fallback" | "failed";
export type CodeChunkRole = "source_snapshot" | "symbol" | "container_context" | "residual";
/**
 * Closed predicate vocabulary emitted by the engram-code extractor (RFC-0020 Phase 2, ADR-0028). KnowledgeRelationship.predicate stays an open string; this is the contracted code set.
 */
export type CodeEdgeKind = "calls" | "imports" | "contains" | "extends" | "implements" | "routes_to" | "registers";
export type Timestamp = string;
export type SourceDocumentKind =
  | "text"
  | "markdown"
  | "html"
  | "pdf"
  | "code"
  | "notebook"
  | "image"
  | "audio"
  | "video"
  | "structured_data"
  | "unknown";
export type KnowledgeChunkKind =
  | "document_section"
  | "paragraph"
  | "table"
  | "code_block"
  | "code_symbol"
  | "file"
  | "diff_hunk"
  | "api_reference"
  | "transcript_segment"
  | "structured_record";
export type DeleteMode = "delete" | "redact" | "tombstone" | "archive";
export type EvidenceRef = EvidenceRef1 & {
  location?: SourceLocation;
  quote?: string;
  targetId?: string;
  targetType: "memory" | "event" | "source" | "document" | "chunk" | "entity" | "concept" | "url";
  uri?: string;
};
export type EvidenceRef1 = {
  [k: string]: unknown;
};
export type EntityKind =
  | "person"
  | "organization"
  | "project"
  | "repository"
  | "file"
  | "module"
  | "class"
  | "function"
  | "method"
  | "variable"
  | "struct"
  | "interface"
  | "trait"
  | "type_alias"
  | "enum"
  | "endpoint"
  | "api"
  | "concept"
  | "value_stream"
  | "requirement"
  | "task"
  | "tool"
  | "artifact"
  | "unknown";
export type CodeSourceRecordKind =
  "unresolved_reference" | "source" | "chunk" | "entity" | "relationship" | "graph" | "document";
export type CodeSourceState = "updating" | "incomplete" | "active";
export type SourceKind = "filesystem" | "git_repository" | "url" | "upload" | "database" | "api" | "generated";
/**
 * Lifecycle of a recorded unresolved cross-file code reference. pending -> resolved (orphan sweep); pending -> failed (operator only); terminal states never change.
 */
export type UnresolvedReferenceStatus = "pending" | "resolved" | "failed";
export type RetrievalTargetType =
  | "memory"
  | "event"
  | "chunk"
  | "document"
  | "entity"
  | "relationship"
  | "concept"
  | "belief"
  | "contradiction"
  | "hierarchy_node"
  | "hierarchy_relation"
  | "rule"
  | "policy"
  | "axiom"
  | "decision_trace";
export type RetrievalMode = "temporal" | "cue" | "hierarchical" | "semantic" | "graph" | "keyword";
export type MemoryKind = "observation" | "fact" | "preference" | "episode" | "artifact" | "relationship" | "procedure";
export type MemoryEventKind =
  | "observed"
  | "written"
  | "updated"
  | "retrieved"
  | "consolidated"
  | "redacted"
  | "forgotten"
  | "expired"
  | "policy_changed"
  | "linked"
  | "unlinked"
  | "belief_synthesized"
  | "belief_retracted"
  | "contradiction_detected"
  | "hierarchy_built";
export type MemoryStatus = "active" | "archived" | "redacted" | "forgotten" | "expired";

export interface EngramV1Types {
  Actor: Actor;
  CodeByteRange: CodeByteRange;
  CodeChunkDescriptor: CodeChunkDescriptor;
  CodeChunkRole: CodeChunkRole;
  CodeEdgeKind: CodeEdgeKind;
  CodeFileKey: CodeFileKey;
  CodeParseQuality: CodeParseQuality;
  CodeSourceBundle: CodeSourceBundle;
  CodeSourceContext: CodeSourceContext;
  CodeSourceGeneration: CodeSourceGeneration;
  CodeSourceQuery: CodeSourceQuery;
  CodeSourceReadRequest: CodeSourceReadRequest;
  CodeSourceReadResult: CodeSourceReadResult;
  CodeSourceRecord: CodeSourceRecord;
  CodeSourceRecordKind: CodeSourceRecordKind;
  CodeSourceState: CodeSourceState;
  ConceptRef: ConceptRef;
  ContextBudget: ContextBudget;
  ContextPayload: ContextPayload;
  Cue: Cue;
  DeleteMode: DeleteMode;
  DerivationRef: DerivationRef;
  EmbeddingBackfillReport: EmbeddingBackfillReport;
  EmbeddingBackfillRequest: EmbeddingBackfillRequest;
  EmbeddingRef: EmbeddingRef;
  EntityKind: EntityKind;
  EntityRef: EntityRef;
  EvaluationCase: EvaluationCase;
  EvaluationExpectation: EvaluationExpectation;
  EvaluationFixture: EvaluationFixture;
  EvaluationSetup: EvaluationSetup;
  EvidenceRef: EvidenceRef;
  ExpectedTarget: ExpectedTarget;
  ForgetRequest: ForgetRequest;
  ForgetResult: ForgetResult;
  FusionTrace: FusionTrace;
  Identifier: Identifier;
  KnowledgeChunk: KnowledgeChunk;
  KnowledgeChunkKind: KnowledgeChunkKind;
  KnowledgeEntity: KnowledgeEntity;
  KnowledgeGraph: KnowledgeGraph;
  KnowledgeRelationship: KnowledgeRelationship;
  KnowledgeSource: KnowledgeSource;
  MemoryContent: MemoryContent;
  MemoryEvent: MemoryEvent;
  MemoryEventKind: MemoryEventKind;
  MemoryKind: MemoryKind;
  MemoryLink: MemoryLink;
  MemoryRecord: MemoryRecord;
  MemoryStatus: MemoryStatus;
  Metadata: Metadata;
  OmittedResult: OmittedResult;
  OntologyRef: OntologyRef;
  Policy: Policy;
  Provenance: Provenance;
  QueryFilter: QueryFilter;
  Requester: Requester;
  RetrievalExplanation: RetrievalExplanation;
  RetrievalMode: RetrievalMode;
  RetrievalRequest: RetrievalRequest;
  RetrievalResult: RetrievalResult;
  RetrievalScore: RetrievalScore;
  RetrievalSourceFailure: RetrievalSourceFailure;
  RetrievalTargetType: RetrievalTargetType;
  Scope: Scope;
  SourceDocument: SourceDocument;
  SourceDocumentKind: SourceDocumentKind;
  SourceKind: SourceKind;
  SourceLocation: SourceLocation;
  Timestamp: Timestamp;
  UnresolvedReference: UnresolvedReference;
  UnresolvedReferenceStatus: UnresolvedReferenceStatus;
  WriteMemoryRequest: WriteMemoryRequest;
  WriteMemoryResponse: WriteMemoryResponse;
}
export interface Actor {
  displayName?: string;
  id: Identifier;
  kind: "user" | "agent" | "system" | "service" | "tool";
  metadata?: Metadata;
}
export interface Metadata {
  [k: string]: unknown;
}
export interface CodeByteRange {
  end: number;
  start: number;
}
export interface CodeChunkDescriptor {
  complete: boolean;
  declarationRange?: CodeByteRange;
  file: CodeFileKey;
  fingerprint: string;
  generation: string;
  parentUnit?: string;
  parseQuality: CodeParseQuality;
  /**
   * @minItems 1
   */
  ranges: [CodeByteRange, ...CodeByteRange[]];
  role: CodeChunkRole;
  sliceSeparator?: string;
  sourceHash: string;
  unit?: string;
  unsupportedFeatures?: string[];
}
export interface CodeFileKey {
  path: string;
  repository: string;
  scope: Scope;
}
export interface Scope {
  environment?: string;
  session?: string;
  subject?: string;
  tenant: string;
  workspace?: string;
}
export interface CodeSourceBundle {
  chunks: KnowledgeChunk[];
  contextEntities?: KnowledgeEntity[];
  documents: SourceDocument[];
  entities: KnowledgeEntity[];
  graphs: KnowledgeGraph[];
  manifest: CodeSourceGeneration;
  relationships: KnowledgeRelationship[];
  sources: KnowledgeSource[];
  unresolvedReferences: UnresolvedReference[];
  version: 1;
}
export interface KnowledgeChunk {
  code?: CodeChunkDescriptor;
  concepts?: ConceptRef[];
  contentHash: string;
  createdAt: Timestamp;
  documentId: Identifier;
  documentKind?: SourceDocumentKind;
  embeddingRefs?: EmbeddingRef[];
  entities?: EntityRef[];
  id: Identifier;
  kind: KnowledgeChunkKind;
  location?: SourceLocation;
  metadata?: Metadata;
  policy: Policy;
  provenance: Provenance;
  sourceId: Identifier;
  summary?: string;
  text: string;
  updatedAt?: Timestamp;
}
export interface ConceptRef {
  id?: Identifier;
  label?: string;
  uri?: string;
}
export interface EmbeddingRef {
  contentHash: string;
  createdAt: Timestamp;
  dimensions: number;
  id: Identifier;
  model: string;
  targetId: Identifier;
  targetType: "memory" | "chunk" | "entity" | "concept";
}
export interface EntityRef {
  aliases?: string[];
  id?: Identifier;
  kind?: string;
  name?: string;
}
export interface SourceLocation {
  anchor?: string;
  endLine?: number;
  endOffset?: number;
  path?: string;
  startLine?: number;
  startOffset?: number;
}
export interface Policy {
  allowedUses?: ("retrieval" | "personalization" | "evaluation" | "consolidation" | "debugging")[];
  deleteMode?: DeleteMode;
  expiresAt?: Timestamp;
  retention: "ephemeral" | "session" | "durable" | "legal_hold";
  sensitivity?: "low" | "medium" | "high" | "restricted";
  visibility: "private" | "workspace" | "organization" | "public";
}
export interface Provenance {
  actor: Actor;
  confidence?: number;
  derivations?: DerivationRef[];
  evidence?: EvidenceRef[];
  method?: string;
  observedAt: Timestamp;
  source: string;
}
export interface DerivationRef {
  createdAt: Timestamp;
  inputRefs?: EvidenceRef[];
  kind: "manual" | "ingestion" | "extraction" | "summarization" | "consolidation" | "ranking" | "taxonomy_evolution";
  model?: string;
  promptHash?: string;
}
export interface KnowledgeEntity {
  aliases?: string[];
  archivedAt?: Timestamp;
  conceptRefs?: ConceptRef[];
  createdAt: Timestamp;
  graphId?: Identifier;
  id: Identifier;
  kind: EntityKind;
  metadata?: Metadata;
  name: string;
  ontologyClassRefs?: Identifier[];
  provenance: Provenance;
  scope: Scope;
  sourceRefs?: EvidenceRef[];
  updatedAt?: Timestamp;
  validFrom?: Timestamp;
  validUntil?: Timestamp;
}
export interface SourceDocument {
  contentHash: string;
  createdAt: Timestamp;
  id: Identifier;
  kind: SourceDocumentKind;
  language?: string;
  metadata?: Metadata;
  mimeType?: string;
  path?: string;
  policy: Policy;
  provenance: Provenance;
  sourceId: Identifier;
  title?: string;
  updatedAt?: Timestamp;
  uri?: string;
  version?: string;
}
export interface KnowledgeGraph {
  createdAt: Timestamp;
  id: Identifier;
  metadata?: Metadata;
  name: string;
  ontologyRefs?: OntologyRef[];
  policy: Policy;
  provenance: Provenance;
  scope: Scope;
  updatedAt?: Timestamp;
  uri?: string;
  version?: string;
}
export interface OntologyRef {
  id?: Identifier;
  uri?: string;
  version?: string;
}
export interface CodeSourceGeneration {
  attemptId: string;
  byteLength: number;
  documentId?: string;
  file: CodeFileKey;
  fingerprint: string;
  generation: string;
  policy: Policy;
  provenance: Provenance;
  records?: CodeSourceRecord[];
  snapshotIds?: string[];
  sourceHash: string;
  state: CodeSourceState;
}
export interface CodeSourceRecord {
  id: string;
  kind: CodeSourceRecordKind;
}
export interface KnowledgeRelationship {
  archivedAt?: Timestamp;
  confidence?: number;
  createdAt: Timestamp;
  evidence?: EvidenceRef[];
  graphId?: Identifier;
  id: Identifier;
  object: EntityRef;
  predicate: string;
  provenance: Provenance;
  scope: Scope;
  subject: EntityRef;
  updatedAt?: Timestamp;
}
export interface KnowledgeSource {
  createdAt: Timestamp;
  id: Identifier;
  kind: SourceKind;
  metadata?: Metadata;
  name: string;
  policy: Policy;
  provenance: Provenance;
  scope: Scope;
  updatedAt?: Timestamp;
  uri?: string;
  version?: string;
}
/**
 * Recorded best-effort-failed cross-file code reference (the code-resolution honesty ledger). Same lifecycle as relationships: retracted with the referring document on re-ingest, re-attempted by the orphan sweep.
 */
export interface UnresolvedReference {
  candidates?: Identifier[];
  createdAt: Timestamp;
  fromEntityId: Identifier;
  graphId?: Identifier;
  id: Identifier;
  line?: number;
  path: string;
  referenceName: string;
  scope: Scope;
  status: UnresolvedReferenceStatus;
  updatedAt?: Timestamp;
}
export interface CodeSourceContext {
  readerVersion: 1;
  requester: Requester;
  scope: Scope;
}
export interface Requester {
  actor: Actor;
  onBehalfOf?: Actor;
  permissions?: string[];
  roles?: string[];
}
export interface CodeSourceQuery {
  generation?: string;
  maxBytes?: number;
  path: string;
  repository: string;
}
export interface CodeSourceReadRequest {
  file: CodeFileKey;
  generation?: string;
  maxBytes?: number;
  requester: Requester;
}
export interface CodeSourceReadResult {
  coverage?: CodeChunkDescriptor[];
  manifest: CodeSourceGeneration;
  text: string;
}
export interface ContextBudget {
  maxBytes?: number;
  maxItems?: number;
  maxTokens?: number;
}
export interface ContextPayload {
  budget?: ContextBudget;
  createdAt: Timestamp;
  items: RetrievalResult[];
  omitted?: OmittedResult[];
  sourceFailures?: RetrievalSourceFailure[];
}
export interface RetrievalResult {
  content: string;
  explanation?: RetrievalExplanation;
  fusionTrace?: FusionTrace;
  id: Identifier;
  metadata?: Metadata;
  policy: Policy;
  provenance: Provenance;
  score: RetrievalScore;
  targetId: Identifier;
  targetType: RetrievalTargetType;
}
export interface RetrievalExplanation {
  matchedCues?: Cue[];
  matchedTerms?: string[];
  path?: string[];
  reason: string;
  sourceSummary?: string;
}
export interface Cue {
  operator?: "equals" | "contains" | "starts_with" | "ends_with" | "exists" | "in" | "range";
  slot: string;
  value: unknown;
  weight?: number;
}
export interface FusionTrace {
  deduplicatedWith?: string[];
  fusionScore?: number;
  fusionStrategy?: "none" | "weighted_sum" | "reciprocal_rank_fusion" | "max_score" | "learned_ranker";
  rerankScore?: number;
  rerankStrategy?: "none" | "mmr" | "cross_encoder" | "llm_judge" | "policy_priority";
  source: string;
  sourceRank?: number;
  sourceScore?: number;
}
export interface RetrievalScore {
  confidence?: number;
  cueMatch?: number;
  hierarchicalFit?: number;
  policyFit?: number;
  recency?: number;
  relevance?: number;
  total: number;
}
export interface OmittedResult {
  reason: "policy_denied" | "budget_exceeded" | "low_score" | "expired" | "redacted";
  targetId: Identifier;
  targetType: RetrievalTargetType;
}
export interface RetrievalSourceFailure {
  degraded: boolean;
  message?: string;
  mode?: RetrievalMode;
  reason: string;
  severity: "info" | "warning" | "error";
  source: string;
}
export interface EmbeddingBackfillReport {
  codeEmbedded: number;
  embedded: number;
  newEmbeddings: number;
  remaining: number;
  reused: number;
}
export interface EmbeddingBackfillRequest {
  limit?: number;
  requester: Requester;
  scope: Scope;
  sourceFilter?: string;
}
export interface EvaluationCase {
  expect: EvaluationExpectation;
  id: string;
  request: RetrievalRequest;
}
export interface EvaluationExpectation {
  maxResults?: number;
  minScore?: number;
  mustExclude?: ExpectedTarget[];
  mustInclude?: ExpectedTarget[];
  requiresExplanation?: boolean;
}
export interface ExpectedTarget {
  targetId: Identifier;
  targetType: RetrievalTargetType;
}
export interface RetrievalRequest {
  budget?: ContextBudget;
  cues?: Cue[];
  filters?: QueryFilter;
  includeExplanations?: boolean;
  limit?: number;
  modes?: RetrievalMode[];
  query: string;
  requester: Requester;
  scope: Scope;
}
export interface QueryFilter {
  chunkKinds?: KnowledgeChunkKind[];
  conceptIds?: Identifier[];
  entityIds?: Identifier[];
  includeArchived?: boolean;
  memoryKinds?: MemoryKind[];
  minConfidence?: number;
  since?: Timestamp;
  sourceKinds?: SourceKind[];
  until?: Timestamp;
}
export interface EvaluationFixture {
  cases: EvaluationCase[];
  createdAt: Timestamp;
  id: Identifier;
  name: string;
  scope: Scope;
  setup: EvaluationSetup;
}
export interface EvaluationSetup {
  chunks?: KnowledgeChunk[];
  documents?: SourceDocument[];
  memories?: WriteMemoryRequest[];
  sources?: KnowledgeSource[];
}
export interface WriteMemoryRequest {
  content: MemoryContent;
  idempotencyKey?: string;
  kind: MemoryKind;
  links?: MemoryLink[];
  policy: Policy;
  provenance: Provenance;
  requester: Requester;
  scope: Scope;
}
export interface MemoryContent {
  entities?: EntityRef[];
  format?: "text" | "markdown" | "json" | "code" | "structured";
  hash?: string;
  language?: string;
  structured?: unknown;
  summary?: string;
  text: string;
}
export interface MemoryLink {
  provenance?: Provenance;
  rel: string;
  targetId: string;
  targetType:
    | "memory"
    | "event"
    | "belief"
    | "contradiction"
    | "chunk"
    | "document"
    | "entity"
    | "concept"
    | "hierarchy_node"
    | "source";
}
export interface ForgetRequest {
  mode: DeleteMode;
  reason?: string;
  requester: Requester;
  scope: Scope;
  targetId: Identifier;
  targetType: "memory" | "event" | "source" | "document" | "chunk" | "entity" | "concept";
}
export interface ForgetResult {
  event?: MemoryEvent;
  status: "deleted" | "redacted" | "tombstoned" | "archived" | "denied" | "not_found";
  targetId: Identifier;
  targetType: string;
}
export interface MemoryEvent {
  actor: Actor;
  id: Identifier;
  kind: MemoryEventKind;
  memoryId?: Identifier;
  occurredAt: Timestamp;
  payload: unknown;
  provenance: Provenance;
  recordedAt: Timestamp;
  scope: Scope;
}
export interface MemoryRecord {
  content: MemoryContent;
  createdAt: Timestamp;
  id: Identifier;
  kind: MemoryKind;
  links?: MemoryLink[];
  metadata?: Metadata;
  policy: Policy;
  provenance: Provenance;
  scope: Scope;
  status: MemoryStatus;
  updatedAt?: Timestamp;
}
export interface WriteMemoryResponse {
  deduplicated?: boolean;
  event: MemoryEvent;
  record: MemoryRecord;
}
