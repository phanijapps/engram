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
    "CodeByteRange": {
      "additionalProperties": false,
      "properties": {
        "end": {
          "minimum": 0,
          "type": "integer"
        },
        "start": {
          "minimum": 0,
          "type": "integer"
        }
      },
      "required": [
        "start",
        "end"
      ],
      "type": "object"
    },
    "CodeCalibrationArtifact": {
      "additionalProperties": false,
      "properties": {
        "configHash": {
          "minLength": 1,
          "type": "string"
        },
        "developmentLabelsSha256": {
          "pattern": "^[a-f0-9]{64}$",
          "type": "string"
        },
        "embeddingManifestId": {
          "minLength": 1,
          "type": "string"
        },
        "id": {
          "minLength": 1,
          "type": "string"
        },
        "passageRepresentationId": {
          "minLength": 1,
          "type": "string"
        },
        "queryRepresentationId": {
          "minLength": 1,
          "type": "string"
        },
        "schemaVersion": {
          "const": 1
        },
        "scoreSpace": {
          "enum": [
            "rrf",
            "cross_encoder"
          ]
        },
        "scoreTransformation": {
          "$ref": "#/$defs/CodeScoreTransformation"
        },
        "scorerManifestId": {
          "type": [
            "string",
            "null"
          ]
        },
        "threshold": {
          "type": "number"
        },
        "tokenizerIdentity": {
          "minLength": 1,
          "type": "string"
        }
      },
      "required": [
        "schemaVersion",
        "id",
        "configHash",
        "queryRepresentationId",
        "passageRepresentationId",
        "embeddingManifestId",
        "scorerManifestId",
        "tokenizerIdentity",
        "scoreTransformation",
        "scoreSpace",
        "threshold",
        "developmentLabelsSha256"
      ],
      "type": "object"
    },
    "CodeChunkDescriptor": {
      "additionalProperties": false,
      "properties": {
        "complete": {
          "type": "boolean"
        },
        "declarationRange": {
          "$ref": "#/$defs/CodeByteRange"
        },
        "file": {
          "$ref": "#/$defs/CodeFileKey"
        },
        "fingerprint": {
          "minLength": 1,
          "type": "string"
        },
        "generation": {
          "minLength": 1,
          "type": "string"
        },
        "parentUnit": {
          "minLength": 1,
          "type": "string"
        },
        "parseQuality": {
          "$ref": "#/$defs/CodeParseQuality"
        },
        "ranges": {
          "items": {
            "$ref": "#/$defs/CodeByteRange"
          },
          "minItems": 1,
          "type": "array"
        },
        "role": {
          "$ref": "#/$defs/CodeChunkRole"
        },
        "sliceSeparator": {
          "default": "",
          "type": "string"
        },
        "sourceHash": {
          "minLength": 1,
          "type": "string"
        },
        "unit": {
          "minLength": 1,
          "type": "string"
        },
        "unsupportedFeatures": {
          "items": {
            "minLength": 1,
            "type": "string"
          },
          "type": "array"
        }
      },
      "required": [
        "role",
        "file",
        "generation",
        "sourceHash",
        "fingerprint",
        "ranges",
        "complete",
        "parseQuality"
      ],
      "type": "object"
    },
    "CodeChunkRole": {
      "enum": [
        "source_snapshot",
        "symbol",
        "container_context",
        "residual"
      ],
      "type": "string"
    },
    "CodeConfigurationSnapshot": {
      "additionalProperties": false,
      "properties": {
        "effective": {
          "$ref": "#/$defs/CodeIntelligenceConfig"
        },
        "hash": {
          "pattern": "^[a-f0-9]{64}$",
          "type": "string"
        },
        "origins": {
          "additionalProperties": {
            "enum": [
              "profile",
              "file",
              "environment",
              "explicit"
            ],
            "type": "string"
          },
          "type": "object"
        }
      },
      "required": [
        "effective",
        "hash",
        "origins"
      ],
      "type": "object"
    },
    "CodeContextFact": {
      "additionalProperties": false,
      "properties": {
        "kind": {
          "enum": [
            "path",
            "symbol",
            "signature",
            "parent",
            "import",
            "unresolved_import"
          ]
        },
        "sources": {
          "items": {
            "$ref": "#/$defs/CodeEvidenceSourceRef"
          },
          "maxItems": 20,
          "minItems": 1,
          "type": "array"
        },
        "value": {
          "maxLength": 8192,
          "type": "string"
        }
      },
      "required": [
        "kind",
        "value",
        "sources"
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
        "routes_to",
        "registers"
      ],
      "type": "string"
    },
    "CodeEmbeddingSegment": {
      "additionalProperties": false,
      "properties": {
        "attemptId": {
          "minLength": 1,
          "type": "string"
        },
        "context": {
          "items": {
            "$ref": "#/$defs/CodeContextFact"
          },
          "maxItems": 20,
          "type": "array"
        },
        "continuation": {
          "type": "boolean"
        },
        "embeddingInput": {
          "$ref": "#/$defs/CodeTokenCoverage"
        },
        "file": {
          "$ref": "#/$defs/CodeFileKey"
        },
        "generation": {
          "minLength": 1,
          "type": "string"
        },
        "id": {
          "minLength": 1,
          "type": "string"
        },
        "parentChunkId": {
          "minLength": 1,
          "type": "string"
        },
        "parentUnitId": {
          "minLength": 1,
          "type": "string"
        },
        "ranges": {
          "items": {
            "$ref": "#/$defs/CodeByteRange"
          },
          "maxItems": 20,
          "minItems": 1,
          "type": "array"
        },
        "representationId": {
          "minLength": 1,
          "type": "string"
        },
        "sourceCategory": {
          "$ref": "#/$defs/CodeSourceCategory"
        },
        "sourceHash": {
          "minLength": 1,
          "type": "string"
        }
      },
      "required": [
        "id",
        "parentChunkId",
        "parentUnitId",
        "file",
        "generation",
        "attemptId",
        "sourceHash",
        "representationId",
        "ranges",
        "continuation",
        "sourceCategory",
        "context",
        "embeddingInput"
      ],
      "type": "object"
    },
    "CodeEvidenceDiagnostics": {
      "additionalProperties": false,
      "properties": {
        "calibrationId": {
          "type": [
            "string",
            "null"
          ]
        },
        "calibrationState": {
          "enum": [
            "validated",
            "unvalidated"
          ]
        },
        "candidateCounts": {
          "additionalProperties": {
            "maximum": 1000000,
            "minimum": 0,
            "type": "integer"
          },
          "type": "object"
        },
        "configHash": {
          "minLength": 1,
          "type": "string"
        },
        "configOrigins": {
          "additionalProperties": {
            "type": "string"
          },
          "type": "object"
        },
        "effectiveReranker": {
          "enum": [
            "none",
            "cross_encoder"
          ]
        },
        "omittedParentDecisions": {
          "maximum": 1000000,
          "minimum": 0,
          "type": "integer"
        },
        "omittedParents": {
          "maximum": 1000000,
          "minimum": 0,
          "type": "integer"
        },
        "omittedSegments": {
          "maximum": 1000000,
          "minimum": 0,
          "type": "integer"
        },
        "omittedUnits": {
          "maximum": 1000000,
          "minimum": 0,
          "type": "integer"
        },
        "parentDecisions": {
          "items": {
            "$ref": "#/$defs/CodeParentDecision"
          },
          "maxItems": 200,
          "type": "array"
        },
        "phaseMs": {
          "additionalProperties": {
            "minimum": 0,
            "type": "number"
          },
          "type": "object"
        },
        "profile": {
          "enum": [
            "balanced-v1",
            "fast-v1"
          ]
        },
        "queryRepresentationId": {
          "minLength": 1,
          "type": "string"
        },
        "reindexRequired": {
          "type": "boolean"
        },
        "requestedReranker": {
          "enum": [
            "none",
            "cross_encoder"
          ]
        },
        "scoredPairs": {
          "maximum": 1000000,
          "minimum": 0,
          "type": "integer"
        },
        "scorerManifestId": {
          "type": [
            "string",
            "null"
          ]
        },
        "scorerState": {
          "enum": [
            "ready",
            "disabled",
            "unavailable",
            "failed"
          ]
        },
        "thresholdScoreSpace": {
          "enum": [
            "rrf",
            "cross_encoder",
            null
          ]
        },
        "truncated": {
          "type": "boolean"
        }
      },
      "required": [
        "configHash",
        "profile",
        "requestedReranker",
        "effectiveReranker",
        "scorerState",
        "scorerManifestId",
        "calibrationState",
        "candidateCounts",
        "omittedUnits",
        "truncated",
        "reindexRequired",
        "configOrigins",
        "queryRepresentationId",
        "phaseMs",
        "omittedSegments",
        "omittedParents",
        "scoredPairs",
        "calibrationId",
        "thresholdScoreSpace"
      ],
      "type": "object"
    },
    "CodeEvidenceItem": {
      "additionalProperties": false,
      "properties": {
        "context": {
          "items": {
            "$ref": "#/$defs/CodeContextFact"
          },
          "maxItems": 20,
          "minItems": 0,
          "type": "array"
        },
        "excerpts": {
          "items": {
            "$ref": "#/$defs/CodeExcerptPart"
          },
          "maxItems": 20,
          "minItems": 1,
          "type": "array"
        },
        "fusionScore": {
          "type": "number"
        },
        "kind": {
          "const": "code"
        },
        "name": {
          "minLength": 1,
          "type": "string"
        },
        "parentUnitId": {
          "minLength": 1,
          "type": "string"
        },
        "partial": {
          "type": "boolean"
        },
        "rerankScore": {
          "type": [
            "number",
            "null"
          ]
        },
        "segments": {
          "items": {
            "$ref": "#/$defs/CodeSegmentEvidence"
          },
          "maxItems": 200,
          "minItems": 1,
          "type": "array"
        },
        "sourceCategory": {
          "const": "implementation"
        }
      },
      "required": [
        "parentUnitId",
        "name",
        "kind",
        "sourceCategory",
        "excerpts",
        "segments",
        "context",
        "partial",
        "fusionScore",
        "rerankScore"
      ],
      "type": "object"
    },
    "CodeEvidencePacket": {
      "additionalProperties": false,
      "properties": {
        "diagnostics": {
          "$ref": "#/$defs/CodeEvidenceDiagnostics"
        },
        "evidenceStatus": {
          "enum": [
            "candidates_available",
            "insufficient_evidence"
          ]
        },
        "items": {
          "items": {
            "$ref": "#/$defs/CodeEvidenceItem"
          },
          "maxItems": 100,
          "type": "array"
        },
        "sourceFailures": {
          "items": {
            "$ref": "#/$defs/RetrievalSourceFailure"
          },
          "maxItems": 100,
          "type": "array"
        },
        "supportingUnits": {
          "items": {
            "$ref": "#/$defs/CodeSupportingUnit"
          },
          "maxItems": 100,
          "minItems": 0,
          "type": "array"
        }
      },
      "required": [
        "items",
        "evidenceStatus",
        "diagnostics",
        "sourceFailures",
        "supportingUnits"
      ],
      "type": "object"
    },
    "CodeEvidenceQuery": {
      "additionalProperties": false,
      "properties": {
        "candidatesPerLane": {
          "maximum": 1000,
          "minimum": 10,
          "type": "integer"
        },
        "evidenceTokenBudget": {
          "maximum": 32768,
          "minimum": 256,
          "type": "integer"
        },
        "query": {
          "description": "Runtime enforces at most 16384 UTF-8 bytes before tokenization (schema length is code points).",
          "maxLength": 16384,
          "minLength": 1,
          "type": "string"
        },
        "repository": {
          "description": "Runtime enforces at most 2048 UTF-8 bytes before tokenization (schema length is code points).",
          "maxLength": 2048,
          "minLength": 1,
          "type": "string"
        },
        "resultLimit": {
          "maximum": 100,
          "minimum": 1,
          "type": "integer"
        }
      },
      "required": [
        "query"
      ],
      "type": "object"
    },
    "CodeEvidenceSourceRef": {
      "additionalProperties": false,
      "properties": {
        "endByte": {
          "maximum": 9007199254740991,
          "minimum": 0,
          "type": "integer"
        },
        "generation": {
          "description": "Exact opaque canonical source-generation identifier; never a numeric counter.",
          "minLength": 1,
          "type": "string"
        },
        "indexedRevision": {
          "type": [
            "string",
            "null"
          ]
        },
        "path": {
          "maxLength": 4096,
          "minLength": 1,
          "type": "string"
        },
        "repository": {
          "minLength": 1,
          "type": "string"
        },
        "representationId": {
          "minLength": 1,
          "type": "string"
        },
        "sourceHash": {
          "description": "Exact opaque canonical source hash, preserved without relabeling.",
          "minLength": 1,
          "type": "string"
        },
        "startByte": {
          "maximum": 9007199254740991,
          "minimum": 0,
          "type": "integer"
        },
        "unitId": {
          "description": "Canonical CodeChunkDescriptor.unit, distinct from KnowledgeChunk.id; never a chunk lookup key.",
          "minLength": 1,
          "type": "string"
        }
      },
      "required": [
        "repository",
        "path",
        "unitId",
        "sourceHash",
        "generation",
        "representationId",
        "startByte",
        "endByte",
        "indexedRevision"
      ],
      "type": "object"
    },
    "CodeExcerptPart": {
      "additionalProperties": false,
      "properties": {
        "source": {
          "$ref": "#/$defs/CodeEvidenceSourceRef"
        },
        "text": {
          "maxLength": 131072,
          "type": "string"
        }
      },
      "required": [
        "source",
        "text"
      ],
      "type": "object"
    },
    "CodeFileKey": {
      "additionalProperties": false,
      "properties": {
        "path": {
          "minLength": 1,
          "type": "string"
        },
        "repository": {
          "minLength": 1,
          "type": "string"
        },
        "scope": {
          "$ref": "#/$defs/Scope"
        }
      },
      "required": [
        "scope",
        "repository",
        "path"
      ],
      "type": "object"
    },
    "CodeIntelligenceConfig": {
      "additionalProperties": false,
      "description": "Fully resolved immutable snapshot; Rust profiles/field-wise overlays supply values.",
      "properties": {
        "evidence": {
          "additionalProperties": false,
          "properties": {
            "max_per_file": {
              "default": 4,
              "maximum": 100,
              "minimum": 1,
              "type": "integer"
            },
            "max_supporting_units": {
              "default": 20,
              "maximum": 100,
              "minimum": 0,
              "type": "integer"
            },
            "token_budget": {
              "default": 6000,
              "maximum": 32768,
              "minimum": 256,
              "type": "integer"
            }
          },
          "required": [
            "token_budget",
            "max_supporting_units",
            "max_per_file"
          ],
          "type": "object"
        },
        "fusion": {
          "allOf": [
            {
              "$ref": "#/$defs/RecallFusionConfig"
            },
            {
              "properties": {
                "rerank": {
                  "properties": {
                    "strategy": {
                      "enum": [
                        "none",
                        "cross_encoder"
                      ]
                    }
                  },
                  "type": "object"
                }
              },
              "type": "object"
            },
            {
              "properties": {
                "default_source_weight": {
                  "default": 1.0,
                  "description": "Weight applied to any lane source not named in `source_weights`. Default 1.0 (pure RRF when no overrides are set).",
                  "minimum": 0,
                  "type": "number"
                },
                "rerank": {
                  "additionalProperties": false,
                  "description": "Reranker selection + parameters. `strategy` reuses the domain RerankStrategy enum (snake_case); engram wires `none`/`mmr`/`cross_encoder` today (the other variants deserialize but are not dispatched).",
                  "properties": {
                    "lambda": {
                      "default": 0.5,
                      "description": "MMR relevance/diversity trade-off in [0, 1] (1 = pure relevance, 0 = max diversity). Only consulted for `strategy = \"mmr\"`.",
                      "maximum": 1,
                      "minimum": 0,
                      "type": "number"
                    },
                    "strategy": {
                      "default": "none",
                      "description": "Which reranker to apply between fusion and budget.",
                      "enum": [
                        "none",
                        "cross_encoder"
                      ],
                      "type": "string"
                    }
                  },
                  "required": [
                    "strategy",
                    "lambda"
                  ],
                  "title": "RerankConfig",
                  "type": "object"
                },
                "rrf_k": {
                  "default": 60,
                  "description": "RRF constant (>= 1). Lower values make top ranks dominate more aggressively; higher values flatten. Conventional default 60 (Cormack et al. 2009).",
                  "minimum": 1,
                  "type": "integer"
                },
                "source_weights": {
                  "additionalProperties": {
                    "minimum": 0,
                    "type": "number"
                  },
                  "default": {},
                  "description": "Per-lane-source weight overrides. Keys are the normalized lane source tags (the weight keys): `vector`, `lexical`, `graph`, `associative_graph`, `community_summary`, `temporal`, `facts`, `belief`. A zero weight keeps a source visible for traceability while contributing nothing to its fused score.",
                  "type": "object"
                }
              },
              "required": [
                "rrf_k",
                "default_source_weight",
                "source_weights",
                "rerank"
              ]
            },
            {
              "properties": {
                "source_weights": {
                  "propertyNames": {
                    "enum": [
                      "vector",
                      "lexical",
                      "graph",
                      "associative_graph",
                      "community_summary"
                    ]
                  },
                  "type": "object"
                }
              },
              "type": "object"
            }
          ]
        },
        "index": {
          "additionalProperties": false,
          "properties": {
            "body_token_target": {
              "default": 384,
              "maximum": 8192,
              "minimum": 64,
              "type": "integer"
            },
            "context_token_budget": {
              "default": 64,
              "maximum": 1024,
              "minimum": 0,
              "type": "integer"
            }
          },
          "required": [
            "body_token_target",
            "context_token_budget"
          ],
          "type": "object"
        },
        "local_scorer": {
          "additionalProperties": false,
          "properties": {
            "batch_size": {
              "default": 8,
              "maximum": 64,
              "minimum": 1,
              "type": "integer"
            },
            "max_candidates": {
              "default": 75,
              "maximum": 200,
              "minimum": 1,
              "type": "integer"
            },
            "max_parents": {
              "default": 50,
              "maximum": 200,
              "minimum": 1,
              "type": "integer"
            },
            "model_id": {
              "default": "jinaai/jina-reranker-v1-turbo-en",
              "minLength": 1,
              "type": "string"
            },
            "on_failure": {
              "default": "degrade",
              "enum": [
                "degrade",
                "error"
              ]
            },
            "pair_token_limit": {
              "default": 1024,
              "maximum": 8192,
              "minimum": 128,
              "type": "integer"
            },
            "query_token_limit": {
              "default": 128,
              "maximum": 2048,
              "minimum": 16,
              "type": "integer"
            }
          },
          "required": [
            "model_id",
            "query_token_limit",
            "pair_token_limit",
            "max_candidates",
            "batch_size",
            "on_failure"
          ],
          "type": "object"
        },
        "profile": {
          "default": "balanced-v1",
          "enum": [
            "balanced-v1",
            "fast-v1"
          ]
        },
        "query": {
          "additionalProperties": false,
          "description": "Query-only format; excluded from passage representation identity.",
          "properties": {
            "instruction": {
              "default": "none",
              "enum": [
                "legacy-v1",
                "none",
                "bge-retrieval-v1"
              ]
            }
          },
          "required": [
            "instruction"
          ],
          "type": "object"
        },
        "retrieval": {
          "additionalProperties": false,
          "properties": {
            "candidates_per_lane": {
              "default": 50,
              "maximum": 1000,
              "minimum": 10,
              "type": "integer"
            },
            "enabled_lanes": {
              "default": [
                "vector",
                "lexical"
              ],
              "items": {
                "enum": [
                  "vector",
                  "lexical",
                  "graph",
                  "associative_graph",
                  "community_summary"
                ]
              },
              "minItems": 1,
              "type": "array",
              "uniqueItems": true
            },
            "graph_hops": {
              "default": 1,
              "maximum": 2,
              "minimum": 0,
              "type": "integer"
            },
            "graph_neighbor_limit": {
              "default": 20,
              "maximum": 100,
              "minimum": 0,
              "type": "integer"
            },
            "result_limit": {
              "default": 10,
              "maximum": 100,
              "minimum": 1,
              "type": "integer"
            }
          },
          "required": [
            "candidates_per_lane",
            "result_limit",
            "enabled_lanes",
            "graph_hops",
            "graph_neighbor_limit"
          ],
          "type": "object"
        },
        "schema_version": {
          "const": 1,
          "default": 1
        }
      },
      "required": [
        "schema_version",
        "profile",
        "index",
        "retrieval",
        "fusion",
        "local_scorer",
        "evidence",
        "query"
      ],
      "type": "object"
    },
    "CodeIntelligenceConfigOverlay": {
      "additionalProperties": false,
      "description": "Partial overlay. Defaults shown are balanced-v1 annotations; shared profile expansion precedes explicit overrides.",
      "properties": {
        "evidence": {
          "additionalProperties": false,
          "properties": {
            "max_per_file": {
              "default": 4,
              "maximum": 100,
              "minimum": 1,
              "type": "integer"
            },
            "max_supporting_units": {
              "default": 20,
              "maximum": 100,
              "minimum": 0,
              "type": "integer"
            },
            "token_budget": {
              "default": 6000,
              "maximum": 32768,
              "minimum": 256,
              "type": "integer"
            }
          },
          "type": "object"
        },
        "fusion": {
          "allOf": [
            {
              "$ref": "#/$defs/RecallFusionConfig"
            },
            {
              "properties": {
                "rerank": {
                  "properties": {
                    "strategy": {
                      "enum": [
                        "none",
                        "cross_encoder"
                      ]
                    }
                  },
                  "type": "object"
                }
              },
              "type": "object"
            },
            {
              "properties": {
                "source_weights": {
                  "propertyNames": {
                    "enum": [
                      "vector",
                      "lexical",
                      "graph",
                      "associative_graph",
                      "community_summary"
                    ]
                  },
                  "type": "object"
                }
              },
              "type": "object"
            }
          ]
        },
        "index": {
          "additionalProperties": false,
          "properties": {
            "body_token_target": {
              "default": 384,
              "maximum": 8192,
              "minimum": 64,
              "type": "integer"
            },
            "context_token_budget": {
              "default": 64,
              "maximum": 1024,
              "minimum": 0,
              "type": "integer"
            }
          },
          "type": "object"
        },
        "local_scorer": {
          "additionalProperties": false,
          "properties": {
            "batch_size": {
              "default": 8,
              "maximum": 64,
              "minimum": 1,
              "type": "integer"
            },
            "max_candidates": {
              "default": 75,
              "maximum": 200,
              "minimum": 1,
              "type": "integer"
            },
            "max_parents": {
              "default": 50,
              "maximum": 200,
              "minimum": 1,
              "type": "integer"
            },
            "model_id": {
              "default": "jinaai/jina-reranker-v1-turbo-en",
              "minLength": 1,
              "type": "string"
            },
            "on_failure": {
              "default": "degrade",
              "enum": [
                "degrade",
                "error"
              ]
            },
            "pair_token_limit": {
              "default": 1024,
              "maximum": 8192,
              "minimum": 128,
              "type": "integer"
            },
            "query_token_limit": {
              "default": 128,
              "maximum": 2048,
              "minimum": 16,
              "type": "integer"
            }
          },
          "type": "object"
        },
        "profile": {
          "default": "balanced-v1",
          "enum": [
            "balanced-v1",
            "fast-v1"
          ]
        },
        "query": {
          "additionalProperties": false,
          "description": "Query-only format; excluded from passage representation identity.",
          "properties": {
            "instruction": {
              "default": "none",
              "enum": [
                "legacy-v1",
                "none",
                "bge-retrieval-v1"
              ]
            }
          },
          "type": "object"
        },
        "retrieval": {
          "additionalProperties": false,
          "properties": {
            "candidates_per_lane": {
              "default": 50,
              "maximum": 1000,
              "minimum": 10,
              "type": "integer"
            },
            "enabled_lanes": {
              "default": [
                "vector",
                "lexical"
              ],
              "items": {
                "enum": [
                  "vector",
                  "lexical",
                  "graph",
                  "associative_graph",
                  "community_summary"
                ]
              },
              "minItems": 1,
              "type": "array",
              "uniqueItems": true
            },
            "graph_hops": {
              "default": 1,
              "maximum": 2,
              "minimum": 0,
              "type": "integer"
            },
            "graph_neighbor_limit": {
              "default": 20,
              "maximum": 100,
              "minimum": 0,
              "type": "integer"
            },
            "result_limit": {
              "default": 10,
              "maximum": 100,
              "minimum": 1,
              "type": "integer"
            }
          },
          "type": "object"
        },
        "schema_version": {
          "const": 1,
          "default": 1
        }
      },
      "type": "object"
    },
    "CodeModelAsset": {
      "additionalProperties": false,
      "properties": {
        "path": {
          "minLength": 1,
          "type": "string"
        },
        "role": {
          "enum": [
            "onnx",
            "onnx_data",
            "tokenizer",
            "tokenizer_config",
            "special_tokens",
            "model_config"
          ]
        },
        "sha256": {
          "pattern": "^[a-f0-9]{64}$",
          "type": "string"
        }
      },
      "required": [
        "path",
        "sha256",
        "role"
      ],
      "type": "object"
    },
    "CodeModelManifest": {
      "additionalProperties": false,
      "properties": {
        "assets": {
          "items": {
            "$ref": "#/$defs/CodeModelAsset"
          },
          "maxItems": 20,
          "minItems": 1,
          "type": "array"
        },
        "id": {
          "minLength": 1,
          "type": "string"
        },
        "inputContractId": {
          "description": "Versioned input order, token preparation and expected output interpretation; included in manifest identity.",
          "minLength": 1,
          "type": "string"
        },
        "license": {
          "minLength": 1,
          "type": "string"
        },
        "modelId": {
          "minLength": 1,
          "type": "string"
        },
        "revision": {
          "minLength": 1,
          "type": "string"
        },
        "schemaVersion": {
          "const": 1
        },
        "scoreTransformation": {
          "$ref": "#/$defs/CodeScoreTransformation"
        },
        "tokenizerIdentity": {
          "minLength": 1,
          "type": "string"
        },
        "tokenizerWindow": {
          "maximum": 32768,
          "minimum": 1,
          "type": "integer"
        }
      },
      "required": [
        "schemaVersion",
        "id",
        "modelId",
        "revision",
        "license",
        "assets",
        "tokenizerIdentity",
        "tokenizerWindow",
        "scoreTransformation",
        "inputContractId"
      ],
      "type": "object"
    },
    "CodeParentDecision": {
      "additionalProperties": false,
      "properties": {
        "appliedThreshold": {
          "type": [
            "number",
            "null"
          ]
        },
        "fusionScore": {
          "type": "number"
        },
        "parentUnitId": {
          "minLength": 1,
          "type": "string"
        },
        "reason": {
          "enum": [
            "selected",
            "below_threshold",
            "scorer_pool",
            "result_limit",
            "per_file_limit",
            "evidence_budget",
            "invalid_evidence",
            "serialized_byte_limit"
          ]
        },
        "rerankScore": {
          "type": [
            "number",
            "null"
          ]
        },
        "retrievedSegments": {
          "maximum": 1000000,
          "minimum": 0,
          "type": "integer"
        },
        "scoredSegments": {
          "maximum": 1000000,
          "minimum": 0,
          "type": "integer"
        }
      },
      "required": [
        "parentUnitId",
        "reason",
        "fusionScore",
        "rerankScore",
        "appliedThreshold",
        "retrievedSegments",
        "scoredSegments"
      ],
      "type": "object"
    },
    "CodeParseQuality": {
      "enum": [
        "clean",
        "recovered",
        "fallback",
        "failed"
      ],
      "type": "string"
    },
    "CodeScoreTransformation": {
      "enum": [
        "raw_logit",
        "sigmoid",
        "none"
      ]
    },
    "CodeSegmentEvidence": {
      "additionalProperties": false,
      "properties": {
        "embeddingInput": {
          "$ref": "#/$defs/CodeTokenCoverage"
        },
        "fusionScore": {
          "type": "number"
        },
        "fusionTraces": {
          "items": {
            "$ref": "#/$defs/FusionTrace"
          },
          "maxItems": 10,
          "minItems": 1,
          "type": "array"
        },
        "rerankContext": {
          "items": {
            "$ref": "#/$defs/CodeContextFact"
          },
          "maxItems": 20,
          "type": "array"
        },
        "rerankInput": {
          "anyOf": [
            {
              "$ref": "#/$defs/CodeTokenCoverage"
            },
            {
              "type": "null"
            }
          ]
        },
        "rerankScore": {
          "type": [
            "number",
            "null"
          ]
        },
        "rerankSources": {
          "items": {
            "$ref": "#/$defs/CodeEvidenceSourceRef"
          },
          "maxItems": 20,
          "minItems": 1,
          "type": "array"
        },
        "segmentId": {
          "minLength": 1,
          "type": "string"
        },
        "sources": {
          "items": {
            "$ref": "#/$defs/CodeEvidenceSourceRef"
          },
          "maxItems": 20,
          "minItems": 1,
          "type": "array"
        }
      },
      "required": [
        "segmentId",
        "sources",
        "fusionTraces",
        "fusionScore",
        "rerankScore",
        "embeddingInput",
        "rerankInput"
      ],
      "type": "object"
    },
    "CodeSegmentInventory": {
      "additionalProperties": false,
      "properties": {
        "attemptId": {
          "minLength": 1,
          "type": "string"
        },
        "file": {
          "$ref": "#/$defs/CodeFileKey"
        },
        "generation": {
          "minLength": 1,
          "type": "string"
        },
        "representationId": {
          "minLength": 1,
          "type": "string"
        },
        "segmentIds": {
          "description": "Canonical parent inventory order, then ascending first source byte per parent; terminal continuations follow their range.",
          "items": {
            "minLength": 1,
            "type": "string"
          },
          "maxItems": 100000,
          "type": "array"
        },
        "state": {
          "$ref": "#/$defs/CodeSourceState"
        }
      },
      "required": [
        "file",
        "generation",
        "attemptId",
        "representationId",
        "segmentIds",
        "state"
      ],
      "type": "object"
    },
    "CodeSourceBundle": {
      "additionalProperties": false,
      "properties": {
        "chunks": {
          "items": {
            "$ref": "#/$defs/KnowledgeChunk"
          },
          "type": "array"
        },
        "contextEntities": {
          "default": [],
          "items": {
            "$ref": "#/$defs/KnowledgeEntity"
          },
          "type": "array"
        },
        "documents": {
          "items": {
            "$ref": "#/$defs/SourceDocument"
          },
          "type": "array"
        },
        "entities": {
          "items": {
            "$ref": "#/$defs/KnowledgeEntity"
          },
          "type": "array"
        },
        "graphs": {
          "items": {
            "$ref": "#/$defs/KnowledgeGraph"
          },
          "type": "array"
        },
        "manifest": {
          "$ref": "#/$defs/CodeSourceGeneration"
        },
        "relationships": {
          "items": {
            "$ref": "#/$defs/KnowledgeRelationship"
          },
          "type": "array"
        },
        "sources": {
          "items": {
            "$ref": "#/$defs/KnowledgeSource"
          },
          "type": "array"
        },
        "unresolvedReferences": {
          "items": {
            "$ref": "#/$defs/UnresolvedReference"
          },
          "type": "array"
        },
        "version": {
          "const": 1,
          "type": "integer"
        }
      },
      "required": [
        "version",
        "manifest",
        "sources",
        "documents",
        "chunks",
        "graphs",
        "entities",
        "relationships",
        "unresolvedReferences"
      ],
      "type": "object"
    },
    "CodeSourceCategory": {
      "enum": [
        "implementation",
        "test",
        "generated",
        "documentation",
        "unknown"
      ]
    },
    "CodeSourceContext": {
      "additionalProperties": false,
      "properties": {
        "readerVersion": {
          "const": 1,
          "type": "integer"
        },
        "requester": {
          "$ref": "#/$defs/Requester"
        },
        "scope": {
          "$ref": "#/$defs/Scope"
        }
      },
      "required": [
        "scope",
        "requester",
        "readerVersion"
      ],
      "type": "object"
    },
    "CodeSourceGeneration": {
      "additionalProperties": false,
      "properties": {
        "attemptId": {
          "minLength": 1,
          "type": "string"
        },
        "byteLength": {
          "minimum": 0,
          "type": "integer"
        },
        "documentId": {
          "minLength": 1,
          "type": "string"
        },
        "file": {
          "$ref": "#/$defs/CodeFileKey"
        },
        "fingerprint": {
          "minLength": 1,
          "type": "string"
        },
        "generation": {
          "minLength": 1,
          "type": "string"
        },
        "policy": {
          "$ref": "#/$defs/Policy"
        },
        "provenance": {
          "$ref": "#/$defs/Provenance"
        },
        "records": {
          "items": {
            "$ref": "#/$defs/CodeSourceRecord"
          },
          "type": "array"
        },
        "snapshotIds": {
          "items": {
            "minLength": 1,
            "type": "string"
          },
          "type": "array"
        },
        "sourceHash": {
          "minLength": 1,
          "type": "string"
        },
        "state": {
          "$ref": "#/$defs/CodeSourceState"
        }
      },
      "required": [
        "file",
        "generation",
        "attemptId",
        "sourceHash",
        "fingerprint",
        "state",
        "byteLength",
        "policy",
        "provenance"
      ],
      "type": "object"
    },
    "CodeSourceQuery": {
      "additionalProperties": false,
      "properties": {
        "generation": {
          "type": "string"
        },
        "maxBytes": {
          "default": 1048576,
          "minimum": 1,
          "type": "integer"
        },
        "path": {
          "minLength": 1,
          "type": "string"
        },
        "repository": {
          "minLength": 1,
          "type": "string"
        }
      },
      "required": [
        "repository",
        "path"
      ],
      "type": "object"
    },
    "CodeSourceReadRequest": {
      "additionalProperties": false,
      "properties": {
        "file": {
          "$ref": "#/$defs/CodeFileKey"
        },
        "generation": {
          "minLength": 1,
          "type": "string"
        },
        "maxBytes": {
          "default": 1048576,
          "minimum": 1,
          "type": "integer"
        },
        "requester": {
          "$ref": "#/$defs/Requester"
        }
      },
      "required": [
        "file",
        "requester"
      ],
      "type": "object"
    },
    "CodeSourceReadResult": {
      "additionalProperties": false,
      "properties": {
        "coverage": {
          "default": [],
          "items": {
            "$ref": "#/$defs/CodeChunkDescriptor"
          },
          "type": "array"
        },
        "manifest": {
          "$ref": "#/$defs/CodeSourceGeneration"
        },
        "text": {
          "type": "string"
        }
      },
      "required": [
        "manifest",
        "text"
      ],
      "type": "object"
    },
    "CodeSourceRecord": {
      "additionalProperties": false,
      "properties": {
        "id": {
          "minLength": 1,
          "type": "string"
        },
        "kind": {
          "$ref": "#/$defs/CodeSourceRecordKind"
        }
      },
      "required": [
        "kind",
        "id"
      ],
      "type": "object"
    },
    "CodeSourceRecordKind": {
      "enum": [
        "unresolved_reference",
        "source",
        "chunk",
        "entity",
        "relationship",
        "graph",
        "document"
      ],
      "type": "string"
    },
    "CodeSourceState": {
      "enum": [
        "updating",
        "incomplete",
        "active"
      ],
      "type": "string"
    },
    "CodeSupportingUnit": {
      "additionalProperties": false,
      "properties": {
        "excerpts": {
          "items": {
            "$ref": "#/$defs/CodeExcerptPart"
          },
          "maxItems": 20,
          "minItems": 1,
          "type": "array"
        },
        "kind": {
          "enum": [
            "code",
            "document"
          ]
        },
        "name": {
          "minLength": 1,
          "type": "string"
        },
        "partial": {
          "type": "boolean"
        },
        "sourceCategory": {
          "enum": [
            "implementation",
            "test",
            "generated",
            "documentation",
            "unknown"
          ]
        },
        "unitId": {
          "minLength": 1,
          "type": "string"
        }
      },
      "required": [
        "unitId",
        "name",
        "kind",
        "sourceCategory",
        "excerpts",
        "partial"
      ],
      "type": "object"
    },
    "CodeTokenCoverage": {
      "additionalProperties": false,
      "properties": {
        "codeTokens": {
          "maximum": 8192,
          "minimum": 0,
          "type": "integer"
        },
        "contextTokens": {
          "maximum": 1024,
          "minimum": 0,
          "type": "integer"
        },
        "queryTokens": {
          "maximum": 2048,
          "minimum": 0,
          "type": "integer"
        },
        "specialTokens": {
          "maximum": 64,
          "minimum": 0,
          "type": "integer"
        },
        "totalTokens": {
          "maximum": 16384,
          "minimum": 0,
          "type": "integer"
        },
        "truncated": {
          "const": false
        }
      },
      "required": [
        "codeTokens",
        "contextTokens",
        "queryTokens",
        "specialTokens",
        "totalTokens",
        "truncated"
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
    "DocumentHead": {
      "additionalProperties": false,
      "properties": {
        "attemptId": {
          "minLength": 1,
          "type": "string"
        },
        "chunkIds": {
          "items": {
            "minLength": 1,
            "type": "string"
          },
          "maxItems": 100000,
          "type": "array"
        },
        "documentId": {
          "type": [
            "string",
            "null"
          ]
        },
        "generation": {
          "minLength": 1,
          "type": "string"
        },
        "key": {
          "$ref": "#/$defs/DocumentHeadKey"
        },
        "policy": {
          "$ref": "#/$defs/Policy"
        },
        "provenance": {
          "$ref": "#/$defs/Provenance"
        },
        "sourceHash": {
          "minLength": 1,
          "type": "string"
        },
        "state": {
          "$ref": "#/$defs/CodeSourceState"
        }
      },
      "required": [
        "key",
        "generation",
        "attemptId",
        "state",
        "sourceHash",
        "documentId",
        "chunkIds",
        "policy",
        "provenance"
      ],
      "type": "object"
    },
    "DocumentHeadKey": {
      "additionalProperties": false,
      "properties": {
        "repository": {
          "minLength": 1,
          "type": "string"
        },
        "scope": {
          "$ref": "#/$defs/Scope"
        },
        "sourceKey": {
          "minLength": 1,
          "type": "string"
        }
      },
      "required": [
        "scope",
        "repository",
        "sourceKey"
      ],
      "type": "object"
    },
    "EmbeddingBackfillReport": {
      "additionalProperties": false,
      "properties": {
        "codeEmbedded": {
          "minimum": 0,
          "type": "integer"
        },
        "embedded": {
          "minimum": 0,
          "type": "integer"
        },
        "newEmbeddings": {
          "minimum": 0,
          "type": "integer"
        },
        "remaining": {
          "minimum": 0,
          "type": "integer"
        },
        "reused": {
          "minimum": 0,
          "type": "integer"
        }
      },
      "required": [
        "embedded",
        "newEmbeddings",
        "reused",
        "codeEmbedded",
        "remaining"
      ],
      "type": "object"
    },
    "EmbeddingBackfillRequest": {
      "additionalProperties": false,
      "properties": {
        "limit": {
          "default": 256,
          "maximum": 1024,
          "minimum": 1,
          "type": "integer"
        },
        "requester": {
          "$ref": "#/$defs/Requester"
        },
        "scope": {
          "$ref": "#/$defs/Scope"
        },
        "sourceFilter": {
          "type": "string"
        }
      },
      "required": [
        "scope",
        "requester"
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
    "EntityKind": {
      "enum": [
        "person",
        "organization",
        "project",
        "repository",
        "file",
        "module",
        "class",
        "function",
        "method",
        "variable",
        "struct",
        "interface",
        "trait",
        "type_alias",
        "enum",
        "endpoint",
        "api",
        "concept",
        "value_stream",
        "requirement",
        "task",
        "tool",
        "artifact",
        "unknown"
      ],
      "type": "string"
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
        "rank": {
          "minimum": 1,
          "type": "integer"
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
    "KnowledgeChunk": {
      "additionalProperties": false,
      "properties": {
        "code": {
          "$ref": "#/$defs/CodeChunkDescriptor"
        },
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
        "documentKind": {
          "$ref": "#/$defs/SourceDocumentKind"
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
    "KnowledgeEntity": {
      "additionalProperties": false,
      "properties": {
        "aliases": {
          "items": {
            "type": "string"
          },
          "type": "array"
        },
        "archivedAt": {
          "$ref": "#/$defs/Timestamp"
        },
        "conceptRefs": {
          "items": {
            "$ref": "#/$defs/ConceptRef"
          },
          "type": "array"
        },
        "createdAt": {
          "$ref": "#/$defs/Timestamp"
        },
        "graphId": {
          "$ref": "#/$defs/Identifier"
        },
        "id": {
          "$ref": "#/$defs/Identifier"
        },
        "kind": {
          "$ref": "#/$defs/EntityKind"
        },
        "metadata": {
          "$ref": "#/$defs/Metadata"
        },
        "name": {
          "type": "string"
        },
        "ontologyClassRefs": {
          "items": {
            "$ref": "#/$defs/Identifier"
          },
          "type": "array"
        },
        "provenance": {
          "$ref": "#/$defs/Provenance"
        },
        "scope": {
          "$ref": "#/$defs/Scope"
        },
        "sourceRefs": {
          "items": {
            "$ref": "#/$defs/EvidenceRef"
          },
          "type": "array"
        },
        "updatedAt": {
          "$ref": "#/$defs/Timestamp"
        },
        "validFrom": {
          "$ref": "#/$defs/Timestamp"
        },
        "validUntil": {
          "$ref": "#/$defs/Timestamp"
        }
      },
      "required": [
        "id",
        "scope",
        "kind",
        "name",
        "provenance",
        "createdAt"
      ],
      "type": "object"
    },
    "KnowledgeGraph": {
      "additionalProperties": false,
      "properties": {
        "createdAt": {
          "$ref": "#/$defs/Timestamp"
        },
        "id": {
          "$ref": "#/$defs/Identifier"
        },
        "metadata": {
          "$ref": "#/$defs/Metadata"
        },
        "name": {
          "type": "string"
        },
        "ontologyRefs": {
          "items": {
            "$ref": "#/$defs/OntologyRef"
          },
          "type": "array"
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
        "scope",
        "name",
        "policy",
        "provenance",
        "createdAt"
      ],
      "type": "object"
    },
    "KnowledgeRelationship": {
      "additionalProperties": false,
      "properties": {
        "archivedAt": {
          "$ref": "#/$defs/Timestamp"
        },
        "confidence": {
          "type": "number"
        },
        "createdAt": {
          "$ref": "#/$defs/Timestamp"
        },
        "evidence": {
          "items": {
            "$ref": "#/$defs/EvidenceRef"
          },
          "type": "array"
        },
        "graphId": {
          "$ref": "#/$defs/Identifier"
        },
        "id": {
          "$ref": "#/$defs/Identifier"
        },
        "object": {
          "$ref": "#/$defs/EntityRef"
        },
        "predicate": {
          "type": "string"
        },
        "provenance": {
          "$ref": "#/$defs/Provenance"
        },
        "scope": {
          "$ref": "#/$defs/Scope"
        },
        "subject": {
          "$ref": "#/$defs/EntityRef"
        },
        "updatedAt": {
          "$ref": "#/$defs/Timestamp"
        }
      },
      "required": [
        "id",
        "scope",
        "subject",
        "predicate",
        "object",
        "provenance",
        "createdAt"
      ],
      "type": "object"
    },
    "KnowledgeSource": {
      "additionalProperties": false,
      "properties": {
        "createdAt": {
          "$ref": "#/$defs/Timestamp"
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
    "OntologyRef": {
      "additionalProperties": false,
      "properties": {
        "id": {
          "$ref": "#/$defs/Identifier"
        },
        "uri": {
          "type": "string"
        },
        "version": {
          "type": "string"
        }
      },
      "required": [],
      "type": "object"
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
        "chunkKinds": {
          "items": {
            "$ref": "#/$defs/KnowledgeChunkKind"
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
    "RecallFusionConfig": {
      "additionalProperties": false,
      "description": "External recall-fusion configuration (RFC-0019). Operator-facing, serializable config loaded from a `[recall_fusion]` profile section or `.engram/recall.json`. Drives weighted reciprocal-rank fusion (vector + lexical + graph + facts + ...) and optional reranking in engram's unified recall. Absent config falls back to equal-weight RRF (today's behavior).",
      "properties": {
        "default_source_weight": {
          "default": 1.0,
          "description": "Weight applied to any lane source not named in `source_weights`. Default 1.0 (pure RRF when no overrides are set).",
          "minimum": 0,
          "type": "number"
        },
        "rerank": {
          "$ref": "#/$defs/RerankConfig",
          "description": "Optional reranker selection + parameters. When absent, no reranking is applied (fusion output is the final ranking)."
        },
        "rrf_k": {
          "default": 60,
          "description": "RRF constant (>= 1). Lower values make top ranks dominate more aggressively; higher values flatten. Conventional default 60 (Cormack et al. 2009).",
          "minimum": 1,
          "type": "integer"
        },
        "source_weights": {
          "additionalProperties": {
            "minimum": 0,
            "type": "number"
          },
          "default": {},
          "description": "Per-lane-source weight overrides. Keys are the normalized lane source tags (the weight keys): `vector`, `lexical`, `graph`, `associative_graph`, `community_summary`, `temporal`, `facts`, `belief`. A zero weight keeps a source visible for traceability while contributing nothing to its fused score.",
          "type": "object"
        }
      },
      "title": "RecallFusionConfig",
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
    "RerankConfig": {
      "additionalProperties": false,
      "description": "Reranker selection + parameters. `strategy` reuses the domain RerankStrategy enum (snake_case); engram wires `none`/`mmr`/`cross_encoder` today (the other variants deserialize but are not dispatched).",
      "properties": {
        "lambda": {
          "default": 0.5,
          "description": "MMR relevance/diversity trade-off in [0, 1] (1 = pure relevance, 0 = max diversity). Only consulted for `strategy = \"mmr\"`.",
          "maximum": 1,
          "minimum": 0,
          "type": "number"
        },
        "strategy": {
          "default": "none",
          "description": "Which reranker to apply between fusion and budget.",
          "enum": [
            "none",
            "mmr",
            "cross_encoder",
            "llm_judge",
            "policy_priority"
          ],
          "type": "string"
        }
      },
      "title": "RerankConfig",
      "type": "object"
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
  "type": "object",
  "x-spec": [
    "docs/specs/code-source-preservation/spec.md",
    "docs/specs/local-code-embeddings/spec.md"
  ]
} as const;
export const engramV1Definitions = engramV1Schema.$defs;
export type EngramV1DefinitionName = keyof typeof engramV1Definitions;
