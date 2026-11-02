/**
 * Typed client for the console's private loopback JSON API. The UI's ONLY
 * data path — no engine driver, no store queries, ever (NFR-007).
 */

/** One inventory row: a partition's exact scope tuple plus per-kind counts. */
export interface PartitionSummary {
  scope: {
    tenant: string;
    subject?: string | null;
    workspace?: string | null;
    session?: string | null;
    environment?: string | null;
  };
  memoryCount: number;
  entityCount: number;
  beliefCount: number;
}

/** Named response state: "ready" or one of the degraded states (NFR-005). */
export type InventoryState =
  | "ready"
  | "store-locked"
  | "backend-unreachable"
  | "store-invalid"
  | "configuration-invalid"
  | "engine-error";

export interface InventoryResponse {
  state: InventoryState;
  backend?: string;
  partitionCount?: number;
  partitions: PartitionSummary[];
  message?: string;
}

/**
 * URL-safe key for one partition tuple — byte-identical to the server's
 * Buffer.from(json, "utf8").toString("base64url") encoding. btoa() alone
 * throws InvalidCharacterError for any scope value outside Latin-1 (review r1
 * FIND-code-001), leaving listed non-ASCII partitions unopenable; encoding the
 * UTF-8 bytes fixes that.
 */
export function partitionKey(scope: PartitionSummary["scope"]): string {
  const bytes = new TextEncoder().encode(JSON.stringify(scope));
  let binary = "";
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return btoa(binary).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

async function getJson<T>(path: string): Promise<T> {
  const response = await fetch(path, { headers: { accept: "application/json" } });
  if (!response.ok) {
    throw new Error(`console api ${path} -> ${response.status}`);
  }
  return (await response.json()) as T;
}

export function fetchInventory(): Promise<InventoryResponse> {
  return getJson<InventoryResponse>("/api/inventory");
}

export interface PartitionRecords<T> {
  items: T[];
  nextOffset: number | null;
}

export interface MemoryRecordView {
  id: string;
  content: { text: string };
  createdAt?: string;
}

export interface EntityView {
  id: string;
  kind: string;
  name: string;
}

export interface RelationshipView {
  id: string;
  subject: { id?: string; name?: string };
  predicate: string;
  object: { id?: string; name?: string };
}

export function fetchPartitionMemories(
  scope: PartitionSummary["scope"],
  limit: number,
  offset: number
): Promise<PartitionRecords<MemoryRecordView>> {
  return getJson(`/api/partitions/${partitionKey(scope)}/memories?limit=${limit}&offset=${offset}`);
}

export function fetchPartitionEntities(
  scope: PartitionSummary["scope"],
  limit: number,
  offset: number
): Promise<PartitionRecords<EntityView>> {
  return getJson(`/api/partitions/${partitionKey(scope)}/entities?limit=${limit}&offset=${offset}`);
}

export function fetchPartitionRelationships(
  scope: PartitionSummary["scope"],
  limit: number,
  offset: number
): Promise<PartitionRecords<RelationshipView>> {
  return getJson(
    `/api/partitions/${partitionKey(scope)}/relationships?limit=${limit}&offset=${offset}`
  );
}

export function fetchPartitionRecent(
  scope: PartitionSummary["scope"],
  limit: number
): Promise<{ items: MemoryRecordView[] }> {
  return getJson(`/api/partitions/${partitionKey(scope)}/recent?limit=${limit}`);
}

export interface Anomaly {
  kind: "subject-partitions" | "entity-identity";
  title: string;
  partitions: PartitionSummary["scope"][];
  partitionCount: number;
}

export interface AnomaliesResponse {
  state: InventoryState;
  anomalies: Anomaly[];
  anomalyCount?: number;
  message?: string;
}

export function fetchAnomalies(): Promise<AnomaliesResponse> {
  return getJson<AnomaliesResponse>("/api/anomalies");
}

export interface BindingHealthLine {
  state: string;
  repo: string;
  branchRef?: string | null;
  indexedCommitOid?: string | null;
  freshnessState?: string;
  observedOid?: string | null;
  message?: string;
}

export function fetchBindingHealth(
  scope: PartitionSummary["scope"],
  repo: string
): Promise<BindingHealthLine> {
  return getJson(`/api/partitions/${partitionKey(scope)}/binding-health?repo=${encodeURIComponent(repo)}`);
}

export function fetchPartitionGraph(
  scope: PartitionSummary["scope"],
  limit: number
): Promise<{ entities: EntityView[]; relationships: RelationshipView[] }> {
  return getJson(`/api/partitions/${partitionKey(scope)}/graph?limit=${limit}`);
}
