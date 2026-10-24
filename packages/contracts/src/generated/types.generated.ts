/* Generated from contracts/v1/schemas/engram-v1.schema.json. Do not edit. */

export type Identifier = string;
/**
 * Closed predicate vocabulary emitted by the engram-code extractor (RFC-0020 Phase 2, ADR-0028). KnowledgeRelationship.predicate stays an open string; this is the contracted code set.
 */
export type CodeEdgeKind = "calls" | "imports" | "contains" | "extends" | "implements" | "routes_to";
/**
 * Mechanism that established a code edge's endpoint binding (edge-evidence adoption 2026-10). parsed = direct syntax/structural fact; scoped_name = deterministic name-table binding (not compiler proof); heuristic = inferential binding (receiver capitalization, co-occurrence, framework synthesis); legacy_unknown = pre-evidence edge, conservative floor. The tier describes the binding, not extraction effort; a name-only parsed edge shows resolved=false separately.
 */
export type ResolutionTier = "parsed" | "scoped_name" | "heuristic" | "legacy_unknown";
export type Timestamp = string;
export type StalenessStatus = "current" | "behind" | "unknown" | "unindexed" | "diverged" | "partial";
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
export type MemoryKind = "observation" | "fact" | "preference" | "episode" | "artifact" | "relationship" | "procedure";
export type SourceKind = "filesystem" | "git_repository" | "url" | "upload" | "database" | "api" | "generated";
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
/**
 * Knowledge change-log operation vocabulary.
 */
export type KnowledgeChangeOp = "upsert" | "remove" | "archive" | "restore";
/**
 * Knowledge target kinds appearing in the change log. Memories and proposals never appear.
 */
export type KnowledgeChangeTargetType = "graph" | "entity" | "relationship" | "document" | "chunk";
export type MemoryStatus = "active" | "archived" | "redacted" | "forgotten" | "expired";
/**
 * Operator proposal queue lifecycle. pending -> applying (approval claim, CAS); applying -> approved (promotion receipt); pending -> rejected. applying is the visible in-progress/recovery state.
 */
export type OperatorProposalStatus = "pending" | "applying" | "approved" | "rejected";
export type ScanStatus = "in_progress" | "completed" | "partial";
/**
 * Lifecycle of a recorded unresolved cross-file code reference. pending -> resolved (orphan sweep); pending -> failed (operator only); terminal states never change.
 */
export type UnresolvedReferenceStatus = "pending" | "resolved" | "failed";

export interface EngramV1Types {
  Actor: Actor;
  CodeEdgeKind: CodeEdgeKind;
  CodeEdgeResolution: CodeEdgeResolution;
  CodeGraphStaleness: CodeGraphStaleness;
  ConceptRef: ConceptRef;
  ContextBudget: ContextBudget;
  ContextPayload: ContextPayload;
  Cue: Cue;
  DeleteMode: DeleteMode;
  DerivationRef: DerivationRef;
  EmbeddingRef: EmbeddingRef;
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
  KnowledgeChange: KnowledgeChange;
  KnowledgeChangeOp: KnowledgeChangeOp;
  KnowledgeChangeTargetType: KnowledgeChangeTargetType;
  KnowledgeChangesPage: KnowledgeChangesPage;
  KnowledgeChunk: KnowledgeChunk;
  KnowledgeChunkKind: KnowledgeChunkKind;
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
  OperatorProposal: OperatorProposal;
  OperatorProposalStatus: OperatorProposalStatus;
  Policy: Policy;
  ProposalMemoryWrite: ProposalMemoryWrite;
  Provenance: Provenance;
  QueryFilter: QueryFilter;
  RecallOutcome: RecallOutcome;
  Requester: Requester;
  ResolutionTier: ResolutionTier;
  RetrievalExplanation: RetrievalExplanation;
  RetrievalMode: RetrievalMode;
  RetrievalRequest: RetrievalRequest;
  RetrievalResult: RetrievalResult;
  RetrievalScore: RetrievalScore;
  RetrievalSourceFailure: RetrievalSourceFailure;
  RetrievalTargetType: RetrievalTargetType;
  ScanCheckpoint: ScanCheckpoint;
  ScanStatus: ScanStatus;
  Scope: Scope;
  SourceDocument: SourceDocument;
  SourceDocumentKind: SourceDocumentKind;
  SourceKind: SourceKind;
  SourceLocation: SourceLocation;
  StalenessStatus: StalenessStatus;
  TaskOutcome: TaskOutcome;
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
/**
 * Typed resolution evidence stamped on code relationships (KnowledgeRelationship.codeResolution; absent on non-code relationships).
 */
export interface CodeEdgeResolution {
  /**
   * Stable, versioned producer id (e.g. ast-local-v1, scope-name-v1, cooccurrence-v1, orphan-sweep-v1, legacy-code-edge-v1).
   */
  resolvedBy: string;
  tier: ResolutionTier;
}
/**
 * Code-graph freshness vs its repository. commitsBehind is local-ancestry-only; null means unknown — never a fabricated 0.
 */
export interface CodeGraphStaleness {
  commitsBehind?: number;
  currentRevision?: string;
  diagnostic?: string;
  dirty?: boolean;
  indexedRevision?: string;
  lastSuccessfulScanAt?: Timestamp;
  scanInProgress: boolean;
  sourceKey: string;
  status: StalenessStatus;
}
export interface ConceptRef {
  id?: Identifier;
  label?: string;
  uri?: string;
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
export interface SourceLocation {
  anchor?: string;
  endLine?: number;
  endOffset?: number;
  path?: string;
  startLine?: number;
  startOffset?: number;
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
  /**
   * Bi-temporal as-of filter: only entity versions valid at this instant (validFrom <= asOf < validUntil). Distinct from since/until which filter observed time. ADR-0021.
   */
  asOf?: string;
  chunkKinds?: KnowledgeChunkKind[];
  conceptIds?: Identifier[];
  entityIds?: Identifier[];
  includeArchived?: boolean;
  /**
   * Include code-artifact entities (Variable/Module/File/Repository - import-path stubs, file stubs, structural anchors) in ranked retrieval results. Absent/false = suppressed: they still participate fully in the graph (seeds, walk/transit nodes, edges, community structure) but are not eligible as results. Indexing is unchanged either way.
   */
  includeCodeArtifacts?: boolean;
  memoryKinds?: MemoryKind[];
  minConfidence?: number;
  since?: Timestamp;
  sourceKinds?: SourceKind[];
  until?: Timestamp;
}
export interface Requester {
  actor: Actor;
  onBehalfOf?: Actor;
  permissions?: string[];
  roles?: string[];
}
export interface Scope {
  environment?: string;
  session?: string;
  subject?: string;
  tenant: string;
  workspace?: string;
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
export interface KnowledgeChunk {
  concepts?: ConceptRef[];
  contentHash: string;
  createdAt: Timestamp;
  documentId: Identifier;
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
/**
 * One committed knowledge mutation (invalidation metadata, never recallable content). seq is a decimal string on JSON surfaces.
 */
export interface KnowledgeChange {
  graphId?: Identifier;
  occurredAt: Timestamp;
  op: KnowledgeChangeOp;
  scanRunId?: string;
  scope: Scope;
  seq: string;
  stableSourceKey?: string;
  targetId: Identifier;
  targetType: KnowledgeChangeTargetType;
}
/**
 * One bounded page of changes plus resume state. resetRequired means the cursor predates the retained window — take a fresh snapshot.
 */
export interface KnowledgeChangesPage {
  hasMore: boolean;
  highWatermark?: string;
  items: KnowledgeChange[];
  nextCursor?: string;
  resetRequired: boolean;
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
/**
 * Inert, reviewable pending write. Never a recall target; approval promotes through the policy-checked memory write path.
 */
export interface OperatorProposal {
  createdAt: Timestamp;
  decision?: {
    decidedAt: Timestamp;
    reason?: string;
    reviewer: Actor;
  };
  id: Identifier;
  payload: ProposalMemoryWrite;
  payloadFingerprint: string;
  promotion?: {
    appliedAt: Timestamp;
    eventId: Identifier;
    idempotencyKey: string;
    memoryId: Identifier;
  };
  scope: Scope;
  status: OperatorProposalStatus;
  submittedBy: Actor;
  updatedAt?: Timestamp;
  version: number;
}
/**
 * Typed proposed memory intent (first accepted slice: memory writes only). Cannot set status, reviewer, memory id, or permissions.
 */
export interface ProposalMemoryWrite {
  content: MemoryContent;
  kind: MemoryKind;
  links?: MemoryLink[];
  policy: Policy;
}
/**
 * Attributed post-action outcome for one recall (F2 outcome loop).
 */
export interface RecallOutcome {
  configVersion?: string;
  occurredAt: Timestamp;
  omittedTargets?: Identifier[];
  outcome: TaskOutcome;
  queryId: string;
  recalledTargets?: Identifier[];
  scope: Scope;
}
/**
 * Host-verified task result. Association data — never causal proof.
 */
export interface TaskOutcome {
  capability?: string;
  success: boolean;
  verifier: Actor;
}
/**
 * Truthful scan state for one (scope, stable source key). indexedRevision is a FULL git OID.
 */
export interface ScanCheckpoint {
  completedAt?: Timestamp;
  dirty?: boolean;
  errors: number;
  indexedRevision?: string;
  scanRoot?: string;
  scanRunId: string;
  scope: Scope;
  stableSourceKey: string;
  startedAt: Timestamp;
  status: ScanStatus;
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
export interface WriteMemoryResponse {
  deduplicated?: boolean;
  event: MemoryEvent;
  record: MemoryRecord;
}
