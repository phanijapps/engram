/* Generated from contracts/v1/schemas/engram-v1.schema.json. Do not edit. */
export const engramV1Schema = {
  "$defs": {
    "Actor": {
      "additionalProperties": false,
      "properties": {
        "displayName": {
          "type": "string"
        },
        "id": {
          "$ref": "#/$defs/Identifier"
        },
        "kind": {
          "enum": [
            "user",
            "agent",
            "system",
            "service",
            "tool"
          ],
          "type": "string"
        },
        "metadata": {
          "$ref": "#/$defs/Metadata"
        }
      },
      "required": [
        "id",
        "kind"
      ],
      "type": "object"
    },
    "CodeEdgeKind": {
      "description": "Closed predicate vocabulary emitted by the engram-code extractor (RFC-0020 Phase 2, ADR-0028). KnowledgeRelationship.predicate stays an open string; this is the contracted code set.",
      "enum": [
        "calls",
        "imports",
        "contains",
        "extends",
        "implements",
        "routes_to"
      ],
      "type": "string"
    },
    "CodeEdgeResolution": {
      "additionalProperties": false,
      "description": "Typed resolution evidence stamped on code relationships (KnowledgeRelationship.codeResolution; absent on non-code relationships).",
      "properties": {
        "resolvedBy": {
          "description": "Stable, versioned producer id (e.g. ast-local-v1, scope-name-v1, cooccurrence-v1, orphan-sweep-v1, legacy-code-edge-v1).",
          "minLength": 1,
          "pattern": "^[A-Za-z0-9._-]+$",
          "type": "string"
        },
        "tier": {
          "$ref": "#/$defs/ResolutionTier"
        }
      },
      "required": [
        "tier",
        "resolvedBy"
      ],
      "type": "object"
    },
    "CodeGraphStaleness": {
      "additionalProperties": false,
      "description": "Code-graph freshness vs its repository. commitsBehind is local-ancestry-only; null means unknown \u2014 never a fabricated 0.",
      "properties": {
        "commitsBehind": {
          "minimum": 0,
          "type": "integer"
        },
        "currentRevision": {
          "type": "string"
        },
        "diagnostic": {
          "type": "string"
        },
        "dirty": {
          "type": "boolean"
        },
        "gitRef": {
          "description": "Git ref that was indexed. None for non-git.",
          "type": "string"
        },
        "indexedRevision": {
          "type": "string"
        },
        "lastSuccessfulScanAt": {
          "$ref": "#/$defs/Timestamp"
        },
        "scanInProgress": {
          "type": "boolean"
        },
        "sourceKey": {
          "type": "string"
        },
        "status": {
          "$ref": "#/$defs/StalenessStatus"
        }
      },
      "required": [
        "sourceKey",
        "status",
        "scanInProgress"
      ],
      "type": "object"
    },
    "ConceptRef": {
      "additionalProperties": false,
      "properties": {
        "id": {
          "$ref": "#/$defs/Identifier"
        },
        "label": {
          "type": "string"
        },
        "uri": {
          "type": "string"
        }
      },
      "type": "object"
    },
    "ContextBudget": {
      "additionalProperties": false,
      "properties": {
        "maxBytes": {
          "minimum": 1,
          "type": "integer"
        },
        "maxItems": {
          "minimum": 1,
          "type": "integer"
        },
        "maxTokens": {
          "minimum": 1,
          "type": "integer"
        }
      },
      "type": "object"
    },
    "ContextPayload": {
      "additionalProperties": false,
      "properties": {
        "budget": {
          "$ref": "#/$defs/ContextBudget"
        },
        "createdAt": {
          "$ref": "#/$defs/Timestamp"
        },
        "items": {
          "items": {
            "$ref": "#/$defs/RetrievalResult"
          },
          "type": "array"
        },
        "omitted": {
          "items": {
            "$ref": "#/$defs/OmittedResult"
          },
          "type": "array"
        },
        "sourceFailures": {
          "items": {
            "$ref": "#/$defs/RetrievalSourceFailure"
          },
          "type": "array"
        }
      },
      "required": [
        "items",
        "createdAt"
      ],
      "type": "object"
    },
    "Cue": {
      "additionalProperties": false,
      "properties": {
        "operator": {
          "enum": [
            "equals",
            "contains",
            "starts_with",
            "ends_with",
            "exists",
            "in",
            "range"
          ],
          "type": "string"
        },
        "slot": {
          "type": "string"
        },
        "value": true,
        "weight": {
          "type": "number"
        }
      },
      "required": [
        "slot",
        "value"
      ],
      "type": "object"
    },
    "DeleteMode": {
      "enum": [
        "delete",
        "redact",
        "tombstone",
        "archive"
      ],
      "type": "string"
    },
    "DerivationRef": {
      "additionalProperties": false,
      "properties": {
        "createdAt": {
          "$ref": "#/$defs/Timestamp"
        },
        "inputRefs": {
          "items": {
            "$ref": "#/$defs/EvidenceRef"
          },
          "type": "array"
        },
        "kind": {
          "enum": [
            "manual",
            "ingestion",
            "extraction",
            "summarization",
            "consolidation",
            "ranking",
            "taxonomy_evolution"
          ],
          "type": "string"
        },
        "model": {
          "type": "string"
        },
        "promptHash": {
          "type": "string"
        }
      },
      "required": [
        "kind",
        "createdAt"
      ],
      "type": "object"
    },
    "EmbeddingRef": {
      "additionalProperties": false,
      "properties": {
        "contentHash": {
          "type": "string"
        },
        "createdAt": {
          "$ref": "#/$defs/Timestamp"
        },
        "dimensions": {
          "minimum": 1,
          "type": "integer"
        },
        "id": {
          "$ref": "#/$defs/Identifier"
        },
        "model": {
          "type": "string"
        },
        "targetId": {
          "$ref": "#/$defs/Identifier"
        },
        "targetType": {
          "enum": [
            "memory",
            "chunk",
            "entity",
            "concept"
          ],
          "type": "string"
        }
      },
      "required": [
        "id",
        "model",
        "dimensions",
        "targetType",
        "targetId",
        "contentHash",
        "createdAt"
      ],
      "type": "object"
    },
    "EntityRef": {
      "additionalProperties": false,
      "properties": {
        "aliases": {
          "items": {
            "type": "string"
          },
          "type": "array"
        },
        "id": {
          "$ref": "#/$defs/Identifier"
        },
        "kind": {
          "type": "string"
        },
        "name": {
          "type": "string"
        }
      },
      "type": "object"
    },
    "EvaluationCase": {
      "additionalProperties": false,
      "properties": {
        "expect": {
          "$ref": "#/$defs/EvaluationExpectation"
        },
        "id": {
          "type": "string"
        },
        "request": {
          "$ref": "#/$defs/RetrievalRequest"
        }
      },
      "required": [
        "id",
        "request",
        "expect"
      ],
      "type": "object"
    },
    "EvaluationExpectation": {
      "additionalProperties": false,
      "properties": {
        "maxResults": {
          "minimum": 1,
          "type": "integer"
        },
        "minScore": {
          "maximum": 1,
          "minimum": 0,
          "type": "number"
        },
        "mustExclude": {
          "items": {
            "$ref": "#/$defs/ExpectedTarget"
          },
          "type": "array"
        },
        "mustInclude": {
          "items": {
            "$ref": "#/$defs/ExpectedTarget"
          },
          "type": "array"
        },
        "requiresExplanation": {
          "type": "boolean"
        }
      },
      "type": "object"
    },
    "EvaluationFixture": {
      "additionalProperties": false,
      "properties": {
        "cases": {
          "items": {
            "$ref": "#/$defs/EvaluationCase"
          },
          "type": "array"
        },
        "createdAt": {
          "$ref": "#/$defs/Timestamp"
        },
        "id": {
          "$ref": "#/$defs/Identifier"
        },
        "name": {
          "type": "string"
        },
        "scope": {
          "$ref": "#/$defs/Scope"
        },
        "setup": {
          "$ref": "#/$defs/EvaluationSetup"
        }
      },
      "required": [
        "id",
        "name",
        "scope",
        "setup",
        "cases",
        "createdAt"
      ],
      "type": "object"
    },
    "EvaluationSetup": {
      "additionalProperties": false,
      "properties": {
        "chunks": {
          "items": {
            "$ref": "#/$defs/KnowledgeChunk"
          },
          "type": "array"
        },
        "documents": {
          "items": {
            "$ref": "#/$defs/SourceDocument"
          },
          "type": "array"
        },
        "memories": {
          "items": {
            "$ref": "#/$defs/WriteMemoryRequest"
          },
          "type": "array"
        },
        "sources": {
          "items": {
            "$ref": "#/$defs/KnowledgeSource"
          },
          "type": "array"
        }
      },
      "type": "object"
    },
    "EvidenceRef": {
      "additionalProperties": false,
      "anyOf": [
        {
          "required": [
            "targetId"
          ]
        },
        {
          "required": [
            "uri"
          ]
        },
        {
          "required": [
            "location"
          ]
        }
      ],
      "properties": {
        "location": {
          "$ref": "#/$defs/SourceLocation"
        },
        "quote": {
          "maxLength": 500,
          "type": "string"
        },
        "targetId": {
          "type": "string"
        },
        "targetType": {
          "enum": [
            "memory",
            "event",
            "source",
            "document",
            "chunk",
            "entity",
            "concept",
            "url"
          ],
          "type": "string"
        },
        "uri": {
          "type": "string"
        }
      },
      "required": [
        "targetType"
      ],
      "type": "object"
    },
    "ExpectedTarget": {
      "additionalProperties": false,
      "properties": {
        "targetId": {
          "$ref": "#/$defs/Identifier"
        },
        "targetType": {
          "$ref": "#/$defs/RetrievalTargetType"
        }
      },
      "required": [
        "targetType",
        "targetId"
      ],
      "type": "object"
    },
    "ForgetRequest": {
      "additionalProperties": false,
      "properties": {
        "mode": {
          "$ref": "#/$defs/DeleteMode"
        },
        "reason": {
          "type": "string"
        },
        "requester": {
          "$ref": "#/$defs/Requester"
        },
        "scope": {
          "$ref": "#/$defs/Scope"
        },
        "targetId": {
          "$ref": "#/$defs/Identifier"
        },
        "targetType": {
          "enum": [
            "memory",
            "event",
            "source",
            "document",
            "chunk",
            "entity",
            "concept"
          ],
          "type": "string"
        }
      },
      "required": [
        "targetType",
        "targetId",
        "scope",
        "requester",
        "mode"
      ],
      "type": "object"
    },
    "ForgetResult": {
      "additionalProperties": false,
      "properties": {
        "event": {
          "$ref": "#/$defs/MemoryEvent"
        },
        "status": {
          "enum": [
            "deleted",
            "redacted",
            "tombstoned",
            "archived",
            "denied",
            "not_found"
          ],
          "type": "string"
        },
        "targetId": {
          "$ref": "#/$defs/Identifier"
        },
        "targetType": {
          "type": "string"
        }
      },
      "required": [
        "targetType",
        "targetId",
        "status"
      ],
      "type": "object"
    },
    "FusionTrace": {
      "additionalProperties": false,
      "properties": {
        "deduplicatedWith": {
          "items": {
            "type": "string"
          },
          "type": "array"
        },
        "fusionScore": {
          "type": "number"
        },
        "fusionStrategy": {
          "enum": [
            "none",
            "weighted_sum",
            "reciprocal_rank_fusion",
            "max_score",
            "learned_ranker"
          ],
          "type": "string"
        },
        "rerankScore": {
          "type": "number"
        },
        "rerankStrategy": {
          "enum": [
            "none",
            "mmr",
            "cross_encoder",
            "llm_judge",
            "policy_priority"
          ],
          "type": "string"
        },
        "source": {
          "type": "string"
        },
        "sourceRank": {
          "minimum": 1,
          "type": "integer"
        },
        "sourceScore": {
          "type": "number"
        }
      },
      "required": [
        "source"
      ],
      "type": "object"
    },
    "Identifier": {
      "minLength": 1,
      "type": "string"
    },
    "KnowledgeChange": {
      "additionalProperties": false,
      "description": "One committed knowledge mutation (invalidation metadata, never recallable content). seq is a decimal string on JSON surfaces.",
      "properties": {
        "graphId": {
          "$ref": "#/$defs/Identifier"
        },
        "occurredAt": {
          "$ref": "#/$defs/Timestamp"
        },
        "op": {
          "$ref": "#/$defs/KnowledgeChangeOp"
        },
        "scanRunId": {
          "type": "string"
        },
        "scope": {
          "$ref": "#/$defs/Scope"
        },
        "seq": {
          "pattern": "^[0-9]+$",
          "type": "string"
        },
        "stableSourceKey": {
          "type": "string"
        },
        "targetId": {
          "$ref": "#/$defs/Identifier"
        },
        "targetType": {
          "$ref": "#/$defs/KnowledgeChangeTargetType"
        }
      },
      "required": [
        "seq",
        "scope",
        "targetType",
        "targetId",
        "op",
        "occurredAt"
      ],
      "type": "object"
    },
    "KnowledgeChangeOp": {
      "description": "Knowledge change-log operation vocabulary.",
      "enum": [
        "upsert",
        "remove",
        "archive",
        "restore"
      ],
      "type": "string"
    },
    "KnowledgeChangeTargetType": {
      "description": "Knowledge target kinds appearing in the change log. Memories and proposals never appear.",
      "enum": [
        "graph",
        "entity",
        "relationship",
        "document",
        "chunk"
      ],
      "type": "string"
    },
    "KnowledgeChangesPage": {
      "additionalProperties": false,
      "description": "One bounded page of changes plus resume state. resetRequired means the cursor predates the retained window \u2014 take a fresh snapshot.",
      "properties": {
        "hasMore": {
          "type": "boolean"
        },
        "highWatermark": {
          "type": "string"
        },
        "items": {
          "items": {
            "$ref": "#/$defs/KnowledgeChange"
          },
          "type": "array"
        },
        "nextCursor": {
          "type": "string"
        },
        "resetRequired": {
          "type": "boolean"
        }
      },
      "required": [
        "items",
        "hasMore",
        "resetRequired"
      ],
      "type": "object"
    },
    "KnowledgeChunk": {
      "additionalProperties": false,
      "properties": {
        "concepts": {
          "items": {
            "$ref": "#/$defs/ConceptRef"
          },
          "type": "array"
        },
        "contentHash": {
          "type": "string"
        },
        "createdAt": {
          "$ref": "#/$defs/Timestamp"
        },
        "documentId": {
          "$ref": "#/$defs/Identifier"
        },
        "embeddingRefs": {
          "items": {
            "$ref": "#/$defs/EmbeddingRef"
          },
          "type": "array"
        },
        "entities": {
          "items": {
            "$ref": "#/$defs/EntityRef"
          },
          "type": "array"
        },
        "id": {
          "$ref": "#/$defs/Identifier"
        },
        "kind": {
          "$ref": "#/$defs/KnowledgeChunkKind"
        },
        "location": {
          "$ref": "#/$defs/SourceLocation"
        },
        "metadata": {
          "$ref": "#/$defs/Metadata"
        },
        "policy": {
          "$ref": "#/$defs/Policy"
        },
        "provenance": {
          "$ref": "#/$defs/Provenance"
        },
        "sourceId": {
          "$ref": "#/$defs/Identifier"
        },
        "summary": {
          "type": "string"
        },
        "text": {
          "type": "string"
        },
        "updatedAt": {
          "$ref": "#/$defs/Timestamp"
        }
      },
      "required": [
        "id",
        "documentId",
        "sourceId",
        "kind",
        "text",
        "contentHash",
        "provenance",
        "policy",
        "createdAt"
      ],
      "type": "object"
    },
    "KnowledgeChunkKind": {
      "enum": [
        "document_section",
        "paragraph",
        "table",
        "code_block",
        "code_symbol",
        "file",
        "diff_hunk",
        "api_reference",
        "transcript_segment",
        "structured_record"
      ],
      "type": "string"
    },
    "KnowledgeSource": {
      "additionalProperties": false,
      "properties": {
        "createdAt": {
          "$ref": "#/$defs/Timestamp"
        },
        "gitRef": {
          "description": "Git ref that was indexed (refs/heads/main). None for non-git sources.",
          "type": "string"
        },
        "id": {
          "$ref": "#/$defs/Identifier"
        },
        "kind": {
          "$ref": "#/$defs/SourceKind"
        },
        "metadata": {
          "$ref": "#/$defs/Metadata"
        },
        "name": {
          "type": "string"
        },
        "policy": {
          "$ref": "#/$defs/Policy"
        },
        "provenance": {
          "$ref": "#/$defs/Provenance"
        },
        "repositoryKey": {
          "description": "Canonical repository identity (commit-stable, branch-independent).",
          "type": "string"
        },
        "scope": {
          "$ref": "#/$defs/Scope"
        },
        "updatedAt": {
          "$ref": "#/$defs/Timestamp"
        },
        "uri": {
          "type": "string"
        },
        "version": {
          "type": "string"
        }
      },
      "required": [
        "id",
        "kind",
        "scope",
        "name",
        "policy",
        "provenance",
        "createdAt"
      ],
      "type": "object"
    },
    "MemoryContent": {
      "additionalProperties": false,
      "properties": {
        "entities": {
          "items": {
            "$ref": "#/$defs/EntityRef"
          },
          "type": "array"
        },
        "format": {
          "enum": [
            "text",
            "markdown",
            "json",
            "code",
            "structured"
          ],
          "type": "string"
        },
        "hash": {
          "type": "string"
        },
        "language": {
          "type": "string"
        },
        "structured": true,
        "summary": {
          "type": "string"
        },
        "text": {
          "type": "string"
        }
      },
      "required": [
        "text"
      ],
      "type": "object"
    },
    "MemoryEvent": {
      "additionalProperties": false,
      "properties": {
        "actor": {
          "$ref": "#/$defs/Actor"
        },
        "id": {
          "$ref": "#/$defs/Identifier"
        },
        "kind": {
          "$ref": "#/$defs/MemoryEventKind"
        },
        "memoryId": {
          "$ref": "#/$defs/Identifier"
        },
        "occurredAt": {
          "$ref": "#/$defs/Timestamp"
        },
        "payload": true,
        "provenance": {
          "$ref": "#/$defs/Provenance"
        },
        "recordedAt": {
          "$ref": "#/$defs/Timestamp"
        },
        "scope": {
          "$ref": "#/$defs/Scope"
        }
      },
      "required": [
        "id",
        "kind",
        "scope",
        "actor",
        "payload",
        "provenance",
        "occurredAt",
        "recordedAt"
      ],
      "type": "object"
    },
    "MemoryEventKind": {
      "enum": [
        "observed",
        "written",
        "updated",
        "retrieved",
        "consolidated",
        "redacted",
        "forgotten",
        "expired",
        "policy_changed",
        "linked",
        "unlinked",
        "belief_synthesized",
        "belief_retracted",
        "contradiction_detected",
        "hierarchy_built"
      ],
      "type": "string"
    },
    "MemoryKind": {
      "enum": [
        "observation",
        "fact",
        "preference",
        "episode",
        "artifact",
        "relationship",
        "procedure"
      ],
      "type": "string"
    },
    "MemoryLink": {
      "additionalProperties": false,
      "properties": {
        "provenance": {
          "$ref": "#/$defs/Provenance"
        },
        "rel": {
          "type": "string"
        },
        "targetId": {
          "type": "string"
        },
        "targetType": {
          "enum": [
            "memory",
            "event",
            "belief",
            "contradiction",
            "chunk",
            "document",
            "entity",
            "concept",
            "hierarchy_node",
            "source"
          ],
          "type": "string"
        }
      },
      "required": [
        "rel",
        "targetType",
        "targetId"
      ],
      "type": "object"
    },
    "MemoryRecord": {
      "additionalProperties": false,
      "properties": {
        "content": {
          "$ref": "#/$defs/MemoryContent"
        },
        "createdAt": {
          "$ref": "#/$defs/Timestamp"
        },
        "id": {
          "$ref": "#/$defs/Identifier"
        },
        "kind": {
          "$ref": "#/$defs/MemoryKind"
        },
        "links": {
          "items": {
            "$ref": "#/$defs/MemoryLink"
          },
          "type": "array"
        },
        "metadata": {
          "$ref": "#/$defs/Metadata"
        },
        "policy": {
          "$ref": "#/$defs/Policy"
        },
        "provenance": {
          "$ref": "#/$defs/Provenance"
        },
        "scope": {
          "$ref": "#/$defs/Scope"
        },
        "status": {
          "$ref": "#/$defs/MemoryStatus"
        },
        "updatedAt": {
          "$ref": "#/$defs/Timestamp"
        },
        "verdict": {
          "$ref": "#/$defs/Verdict",
          "description": "Optional assertion verdict word for this record.",
          "examples": [
            "asserted_by_agent"
          ]
        }
      },
      "required": [
        "id",
        "kind",
        "content",
        "scope",
        "provenance",
        "policy",
        "status",
        "createdAt"
      ],
      "type": "object"
    },
    "MemoryStatus": {
      "enum": [
        "active",
        "archived",
        "redacted",
        "forgotten",
        "expired"
      ],
      "type": "string"
    },
    "Metadata": {
      "additionalProperties": true,
      "type": "object"
    },
    "OmittedResult": {
      "additionalProperties": false,
      "properties": {
        "reason": {
          "enum": [
            "policy_denied",
            "budget_exceeded",
            "low_score",
            "expired",
            "redacted"
          ],
          "type": "string"
        },
        "targetId": {
          "$ref": "#/$defs/Identifier"
        },
        "targetType": {
          "$ref": "#/$defs/RetrievalTargetType"
        }
      },
      "required": [
        "targetType",
        "targetId",
        "reason"
      ],
      "type": "object"
    },
    "OperatorProposal": {
      "additionalProperties": false,
      "description": "Inert, reviewable pending write. Never a recall target; approval promotes through the policy-checked memory write path.",
      "properties": {
        "createdAt": {
          "$ref": "#/$defs/Timestamp"
        },
        "decision": {
          "additionalProperties": false,
          "properties": {
            "decidedAt": {
              "$ref": "#/$defs/Timestamp"
            },
            "reason": {
              "type": "string"
            },
            "reviewer": {
              "$ref": "#/$defs/Actor"
            }
          },
          "required": [
            "reviewer",
            "decidedAt"
          ],
          "type": "object"
        },
        "id": {
          "$ref": "#/$defs/Identifier"
        },
        "payload": {
          "$ref": "#/$defs/ProposalMemoryWrite"
        },
        "payloadFingerprint": {
          "minLength": 1,
          "type": "string"
        },
        "promotion": {
          "additionalProperties": false,
          "properties": {
            "appliedAt": {
              "$ref": "#/$defs/Timestamp"
            },
            "eventId": {
              "$ref": "#/$defs/Identifier"
            },
            "idempotencyKey": {
              "minLength": 1,
              "type": "string"
            },
            "memoryId": {
              "$ref": "#/$defs/Identifier"
            }
          },
          "required": [
            "memoryId",
            "eventId",
            "idempotencyKey",
            "appliedAt"
          ],
          "type": "object"
        },
        "scope": {
          "$ref": "#/$defs/Scope"
        },
        "status": {
          "$ref": "#/$defs/OperatorProposalStatus"
        },
        "submittedBy": {
          "$ref": "#/$defs/Actor"
        },
        "updatedAt": {
          "$ref": "#/$defs/Timestamp"
        },
        "version": {
          "minimum": 1,
          "type": "integer"
        }
      },
      "required": [
        "id",
        "scope",
        "payload",
        "status",
        "submittedBy",
        "payloadFingerprint",
        "version",
        "createdAt"
      ],
      "type": "object"
    },
    "OperatorProposalStatus": {
      "description": "Operator proposal queue lifecycle. pending -> applying (approval claim, CAS); applying -> approved (promotion receipt); pending -> rejected. applying is the visible in-progress/recovery state.",
      "enum": [
        "pending",
        "applying",
        "approved",
        "rejected"
      ],
      "type": "string"
    },
    "Policy": {
      "additionalProperties": false,
      "properties": {
        "allowedUses": {
          "items": {
            "enum": [
              "retrieval",
              "personalization",
              "evaluation",
              "consolidation",
              "debugging"
            ],
            "type": "string"
          },
          "type": "array",
          "uniqueItems": true
        },
        "deleteMode": {
          "$ref": "#/$defs/DeleteMode"
        },
        "expiresAt": {
          "$ref": "#/$defs/Timestamp"
        },
        "retention": {
          "enum": [
            "ephemeral",
            "session",
            "durable",
            "legal_hold"
          ],
          "type": "string"
        },
        "sensitivity": {
          "enum": [
            "low",
            "medium",
            "high",
            "restricted"
          ],
          "type": "string"
        },
        "visibility": {
          "enum": [
            "private",
            "workspace",
            "organization",
            "public"
          ],
          "type": "string"
        }
      },
      "required": [
        "visibility",
        "retention"
      ],
      "type": "object"
    },
    "ProposalMemoryWrite": {
      "additionalProperties": false,
      "description": "Typed proposed memory intent (first accepted slice: memory writes only). Cannot set status, reviewer, memory id, or permissions.",
      "properties": {
        "content": {
          "$ref": "#/$defs/MemoryContent"
        },
        "kind": {
          "$ref": "#/$defs/MemoryKind"
        },
        "links": {
          "items": {
            "$ref": "#/$defs/MemoryLink"
          },
          "type": "array"
        },
        "policy": {
          "$ref": "#/$defs/Policy"
        }
      },
      "required": [
        "kind",
        "content",
        "policy"
      ],
      "type": "object"
    },
    "Provenance": {
      "additionalProperties": false,
      "properties": {
        "actor": {
          "$ref": "#/$defs/Actor"
        },
        "confidence": {
          "maximum": 1,
          "minimum": 0,
          "type": "number"
        },
        "derivations": {
          "items": {
            "$ref": "#/$defs/DerivationRef"
          },
          "type": "array"
        },
        "evidence": {
          "items": {
            "$ref": "#/$defs/EvidenceRef"
          },
          "type": "array"
        },
        "method": {
          "type": "string"
        },
        "observedAt": {
          "$ref": "#/$defs/Timestamp"
        },
        "source": {
          "minLength": 1,
          "type": "string"
        }
      },
      "required": [
        "source",
        "actor",
        "observedAt"
      ],
      "type": "object"
    },
    "QueryFilter": {
      "additionalProperties": false,
      "properties": {
        "asOf": {
          "description": "Bi-temporal as-of filter: only entity versions valid at this instant (validFrom <= asOf < validUntil). Distinct from since/until which filter observed time. ADR-0021.",
          "format": "date-time",
          "type": "string"
        },
        "chunkKinds": {
          "items": {
            "$ref": "#/$defs/KnowledgeChunkKind"
          },
          "type": "array"
        },
        "codeIndexKeys": {
          "description": "Which code indexes contribute candidates. Qualified keys or configured aliases. Absent = workspace default; ['*'] = all.",
          "items": {
            "type": "string"
          },
          "type": "array"
        },
        "conceptIds": {
          "items": {
            "$ref": "#/$defs/Identifier"
          },
          "type": "array"
        },
        "entityIds": {
          "items": {
            "$ref": "#/$defs/Identifier"
          },
          "type": "array"
        },
        "includeArchived": {
          "type": "boolean"
        },
        "includeCodeArtifacts": {
          "description": "Include code-artifact entities (Variable/Module/File/Repository - import-path stubs, file stubs, structural anchors) in ranked retrieval results. Absent/false = suppressed: they still participate fully in the graph (seeds, walk/transit nodes, edges, community structure) but are not eligible as results. Indexing is unchanged either way.",
          "type": "boolean"
        },
        "memoryKinds": {
          "items": {
            "$ref": "#/$defs/MemoryKind"
          },
          "type": "array"
        },
        "minConfidence": {
          "maximum": 1,
          "minimum": 0,
          "type": "number"
        },
        "since": {
          "$ref": "#/$defs/Timestamp"
        },
        "sourceKinds": {
          "items": {
            "$ref": "#/$defs/SourceKind"
          },
          "type": "array"
        },
        "until": {
          "$ref": "#/$defs/Timestamp"
        }
      },
      "type": "object"
    },
    "RecallOutcome": {
      "additionalProperties": false,
      "description": "Attributed post-action outcome for one recall (F2 outcome loop).",
      "properties": {
        "configVersion": {
          "type": "string"
        },
        "occurredAt": {
          "$ref": "#/$defs/Timestamp"
        },
        "omittedTargets": {
          "items": {
            "$ref": "#/$defs/Identifier"
          },
          "type": "array"
        },
        "outcome": {
          "$ref": "#/$defs/TaskOutcome"
        },
        "queryId": {
          "minLength": 1,
          "type": "string"
        },
        "recalledTargets": {
          "items": {
            "$ref": "#/$defs/Identifier"
          },
          "type": "array"
        },
        "scope": {
          "$ref": "#/$defs/Scope"
        }
      },
      "required": [
        "queryId",
        "scope",
        "outcome",
        "occurredAt"
      ],
      "type": "object"
    },
    "Requester": {
      "additionalProperties": false,
      "properties": {
        "actor": {
          "$ref": "#/$defs/Actor"
        },
        "onBehalfOf": {
          "$ref": "#/$defs/Actor"
        },
        "permissions": {
          "items": {
            "type": "string"
          },
          "type": "array"
        },
        "roles": {
          "items": {
            "type": "string"
          },
          "type": "array"
        }
      },
      "required": [
        "actor"
      ],
      "type": "object"
    },
    "ResolutionTier": {
      "description": "Mechanism that established a code edge's endpoint binding (edge-evidence adoption 2026-10). parsed = direct syntax/structural fact; scoped_name = deterministic name-table binding (not compiler proof); heuristic = inferential binding (receiver capitalization, co-occurrence, framework synthesis); legacy_unknown = pre-evidence edge, conservative floor. The tier describes the binding, not extraction effort; a name-only parsed edge shows resolved=false separately.",
      "enum": [
        "parsed",
        "scoped_name",
        "heuristic",
        "legacy_unknown"
      ],
      "type": "string"
    },
    "RetrievalExplanation": {
      "additionalProperties": false,
      "properties": {
        "matchedCues": {
          "items": {
            "$ref": "#/$defs/Cue"
          },
          "type": "array"
        },
        "matchedTerms": {
          "items": {
            "type": "string"
          },
          "type": "array"
        },
        "path": {
          "items": {
            "type": "string"
          },
          "type": "array"
        },
        "reason": {
          "type": "string"
        },
        "sourceSummary": {
          "type": "string"
        }
      },
      "required": [
        "reason"
      ],
      "type": "object"
    },
    "RetrievalMode": {
      "enum": [
        "temporal",
        "cue",
        "hierarchical",
        "semantic",
        "graph",
        "keyword"
      ],
      "type": "string"
    },
    "RetrievalRequest": {
      "additionalProperties": false,
      "properties": {
        "budget": {
          "$ref": "#/$defs/ContextBudget"
        },
        "cues": {
          "items": {
            "$ref": "#/$defs/Cue"
          },
          "type": "array"
        },
        "filters": {
          "$ref": "#/$defs/QueryFilter"
        },
        "includeExplanations": {
          "type": "boolean"
        },
        "limit": {
          "minimum": 1,
          "type": "integer"
        },
        "modes": {
          "items": {
            "$ref": "#/$defs/RetrievalMode"
          },
          "type": "array"
        },
        "query": {
          "type": "string"
        },
        "requester": {
          "$ref": "#/$defs/Requester"
        },
        "scope": {
          "$ref": "#/$defs/Scope"
        }
      },
      "required": [
        "query",
        "scope",
        "requester"
      ],
      "type": "object"
    },
    "RetrievalResult": {
      "additionalProperties": false,
      "properties": {
        "content": {
          "type": "string"
        },
        "explanation": {
          "$ref": "#/$defs/RetrievalExplanation"
        },
        "fusionTrace": {
          "$ref": "#/$defs/FusionTrace"
        },
        "id": {
          "$ref": "#/$defs/Identifier"
        },
        "metadata": {
          "$ref": "#/$defs/Metadata"
        },
        "policy": {
          "$ref": "#/$defs/Policy"
        },
        "provenance": {
          "$ref": "#/$defs/Provenance"
        },
        "score": {
          "$ref": "#/$defs/RetrievalScore"
        },
        "targetId": {
          "$ref": "#/$defs/Identifier"
        },
        "targetType": {
          "$ref": "#/$defs/RetrievalTargetType"
        }
      },
      "required": [
        "id",
        "targetType",
        "targetId",
        "content",
        "score",
        "provenance",
        "policy"
      ],
      "type": "object"
    },
    "RetrievalScore": {
      "additionalProperties": false,
      "properties": {
        "confidence": {
          "maximum": 1,
          "minimum": 0,
          "type": "number"
        },
        "cueMatch": {
          "maximum": 1,
          "minimum": 0,
          "type": "number"
        },
        "hierarchicalFit": {
          "maximum": 1,
          "minimum": 0,
          "type": "number"
        },
        "policyFit": {
          "maximum": 1,
          "minimum": 0,
          "type": "number"
        },
        "recency": {
          "maximum": 1,
          "minimum": 0,
          "type": "number"
        },
        "relevance": {
          "maximum": 1,
          "minimum": 0,
          "type": "number"
        },
        "total": {
          "maximum": 1,
          "minimum": 0,
          "type": "number"
        }
      },
      "required": [
        "total"
      ],
      "type": "object"
    },
    "RetrievalSourceFailure": {
      "additionalProperties": false,
      "properties": {
        "degraded": {
          "type": "boolean"
        },
        "message": {
          "type": "string"
        },
        "mode": {
          "$ref": "#/$defs/RetrievalMode"
        },
        "reason": {
          "type": "string"
        },
        "severity": {
          "enum": [
            "info",
            "warning",
            "error"
          ],
          "type": "string"
        },
        "source": {
          "type": "string"
        }
      },
      "required": [
        "source",
        "severity",
        "reason",
        "degraded"
      ],
      "type": "object"
    },
    "RetrievalTargetType": {
      "enum": [
        "memory",
        "event",
        "chunk",
        "document",
        "entity",
        "relationship",
        "concept",
        "belief",
        "contradiction",
        "hierarchy_node",
        "hierarchy_relation",
        "rule",
        "policy",
        "axiom",
        "decision_trace"
      ],
      "type": "string"
    },
    "ScanCheckpoint": {
      "additionalProperties": false,
      "description": "Truthful scan state for one (scope, stable source key). indexedRevision is a FULL git OID.",
      "properties": {
        "completedAt": {
          "$ref": "#/$defs/Timestamp"
        },
        "dirty": {
          "type": "boolean"
        },
        "errors": {
          "minimum": 0,
          "type": "integer"
        },
        "gitRef": {
          "description": "Git ref that was indexed. None for non-git.",
          "type": "string"
        },
        "indexedRevision": {
          "type": "string"
        },
        "scanRoot": {
          "type": "string"
        },
        "scanRunId": {
          "minLength": 1,
          "type": "string"
        },
        "scope": {
          "$ref": "#/$defs/Scope"
        },
        "stableSourceKey": {
          "minLength": 1,
          "type": "string"
        },
        "startedAt": {
          "$ref": "#/$defs/Timestamp"
        },
        "status": {
          "$ref": "#/$defs/ScanStatus"
        }
      },
      "required": [
        "scope",
        "stableSourceKey",
        "scanRunId",
        "startedAt",
        "status",
        "errors"
      ],
      "type": "object"
    },
    "ScanStatus": {
      "enum": [
        "in_progress",
        "completed",
        "partial"
      ],
      "type": "string"
    },
    "Scope": {
      "additionalProperties": false,
      "properties": {
        "environment": {
          "type": "string"
        },
        "session": {
          "type": "string"
        },
        "subject": {
          "type": "string"
        },
        "tenant": {
          "minLength": 1,
          "type": "string"
        },
        "workspace": {
          "type": "string"
        }
      },
      "required": [
        "tenant"
      ],
      "type": "object"
    },
    "SourceDocument": {
      "additionalProperties": false,
      "properties": {
        "contentHash": {
          "type": "string"
        },
        "createdAt": {
          "$ref": "#/$defs/Timestamp"
        },
        "id": {
          "$ref": "#/$defs/Identifier"
        },
        "kind": {
          "$ref": "#/$defs/SourceDocumentKind"
        },
        "language": {
          "type": "string"
        },
        "metadata": {
          "$ref": "#/$defs/Metadata"
        },
        "mimeType": {
          "type": "string"
        },
        "path": {
          "type": "string"
        },
        "policy": {
          "$ref": "#/$defs/Policy"
        },
        "provenance": {
          "$ref": "#/$defs/Provenance"
        },
        "sourceId": {
          "$ref": "#/$defs/Identifier"
        },
        "title": {
          "type": "string"
        },
        "updatedAt": {
          "$ref": "#/$defs/Timestamp"
        },
        "uri": {
          "type": "string"
        },
        "version": {
          "type": "string"
        }
      },
      "required": [
        "id",
        "sourceId",
        "kind",
        "contentHash",
        "provenance",
        "policy",
        "createdAt"
      ],
      "type": "object"
    },
    "SourceDocumentKind": {
      "enum": [
        "text",
        "markdown",
        "html",
        "pdf",
        "code",
        "notebook",
        "image",
        "audio",
        "video",
        "structured_data",
        "unknown"
      ],
      "type": "string"
    },
    "SourceKind": {
      "enum": [
        "filesystem",
        "git_repository",
        "url",
        "upload",
        "database",
        "api",
        "generated"
      ],
      "type": "string"
    },
    "SourceLocation": {
      "additionalProperties": false,
      "properties": {
        "anchor": {
          "type": "string"
        },
        "endLine": {
          "minimum": 1,
          "type": "integer"
        },
        "endOffset": {
          "minimum": 0,
          "type": "integer"
        },
        "path": {
          "type": "string"
        },
        "startLine": {
          "minimum": 1,
          "type": "integer"
        },
        "startOffset": {
          "minimum": 0,
          "type": "integer"
        }
      },
      "type": "object"
    },
    "StalenessStatus": {
      "enum": [
        "current",
        "behind",
        "unknown",
        "unindexed",
        "diverged",
        "partial"
      ],
      "type": "string"
    },
    "TaskOutcome": {
      "additionalProperties": false,
      "description": "Host-verified task result. Association data \u2014 never causal proof.",
      "properties": {
        "capability": {
          "type": "string"
        },
        "success": {
          "type": "boolean"
        },
        "verifier": {
          "$ref": "#/$defs/Actor"
        }
      },
      "required": [
        "success",
        "verifier"
      ],
      "type": "object"
    },
    "Timestamp": {
      "format": "date-time",
      "type": "string"
    },
    "UnresolvedReference": {
      "additionalProperties": false,
      "description": "Recorded best-effort-failed cross-file code reference (the code-resolution honesty ledger). Same lifecycle as relationships: retracted with the referring document on re-ingest, re-attempted by the orphan sweep.",
      "properties": {
        "candidates": {
          "items": {
            "$ref": "#/$defs/Identifier"
          },
          "type": "array"
        },
        "createdAt": {
          "$ref": "#/$defs/Timestamp"
        },
        "fromEntityId": {
          "$ref": "#/$defs/Identifier"
        },
        "graphId": {
          "$ref": "#/$defs/Identifier"
        },
        "id": {
          "$ref": "#/$defs/Identifier"
        },
        "line": {
          "minimum": 1,
          "type": "integer"
        },
        "path": {
          "minLength": 1,
          "type": "string"
        },
        "referenceName": {
          "minLength": 1,
          "type": "string"
        },
        "scope": {
          "$ref": "#/$defs/Scope"
        },
        "status": {
          "$ref": "#/$defs/UnresolvedReferenceStatus"
        },
        "updatedAt": {
          "$ref": "#/$defs/Timestamp"
        }
      },
      "required": [
        "id",
        "fromEntityId",
        "referenceName",
        "status",
        "path",
        "scope",
        "createdAt"
      ],
      "type": "object"
    },
    "UnresolvedReferenceStatus": {
      "description": "Lifecycle of a recorded unresolved cross-file code reference. pending -> resolved (orphan sweep); pending -> failed (operator only); terminal states never change.",
      "enum": [
        "pending",
        "resolved",
        "failed"
      ],
      "type": "string"
    },
    "Verdict": {
      "description": "Optional assertion verdict word carried by MemoryRecord.verdict. Closed set of three snake_case words; 'derived' is reserved for the future derivation layer and is emitted by nothing in v1 (write surfaces reject it as input).",
      "enum": [
        "asserted_by_agent",
        "grounded_in_source",
        "derived"
      ],
      "type": "string"
    },
    "WriteMemoryRequest": {
      "additionalProperties": false,
      "properties": {
        "content": {
          "$ref": "#/$defs/MemoryContent"
        },
        "idempotencyKey": {
          "type": "string"
        },
        "kind": {
          "$ref": "#/$defs/MemoryKind"
        },
        "links": {
          "items": {
            "$ref": "#/$defs/MemoryLink"
          },
          "type": "array"
        },
        "policy": {
          "$ref": "#/$defs/Policy"
        },
        "provenance": {
          "$ref": "#/$defs/Provenance"
        },
        "requester": {
          "$ref": "#/$defs/Requester"
        },
        "scope": {
          "$ref": "#/$defs/Scope"
        },
        "verdict": {
          "$ref": "#/$defs/Verdict",
          "description": "Optional assertion verdict stamped onto the written record (DEC-P06); absent means no verdict on this write surface.",
          "examples": [
            "grounded_in_source"
          ]
        }
      },
      "required": [
        "kind",
        "content",
        "scope",
        "requester",
        "provenance",
        "policy"
      ],
      "type": "object"
    },
    "WriteMemoryResponse": {
      "additionalProperties": false,
      "properties": {
        "deduplicated": {
          "type": "boolean"
        },
        "event": {
          "$ref": "#/$defs/MemoryEvent"
        },
        "record": {
          "$ref": "#/$defs/MemoryRecord"
        }
      },
      "required": [
        "record",
        "event"
      ],
      "type": "object"
    }
  },
  "$id": "https://engram.local/contracts/v1/schemas/engram-v1.schema.json",
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "title": "Engram v1 contract definitions",
  "type": "object"
} as const;
export const engramV1Definitions = engramV1Schema.$defs;
export type EngramV1DefinitionName = keyof typeof engramV1Definitions;
