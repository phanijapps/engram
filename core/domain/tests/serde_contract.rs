use chrono::Utc;
use engram_domain::*;
use serde_json::json;

fn actor() -> Actor {
    Actor {
        id: Id::from("actor-1"),
        kind: ActorKind::Agent,
        display_name: Some("agent".to_owned()),
        metadata: None,
    }
}

fn scope() -> Scope {
    Scope {
        tenant: "tenant-a".to_owned(),
        subject: Some("subject-a".to_owned()),
        workspace: Some("workspace-a".to_owned()),
        session: None,
        environment: Some("test".to_owned()),
    }
}

fn policy() -> Policy {
    Policy {
        visibility: Visibility::Workspace,
        retention: Retention::Durable,
        sensitivity: Some(Sensitivity::Medium),
        allowed_uses: vec![AllowedUse::Retrieval, AllowedUse::Evaluation],
        expires_at: None,
        delete_mode: Some(DeleteMode::Tombstone),
    }
}

fn provenance(now: Timestamp) -> Provenance {
    Provenance {
        source: "test".to_owned(),
        actor: actor(),
        observed_at: now,
        evidence: Vec::new(),
        derivations: Vec::new(),
        confidence: Some(0.9),
        method: Some("manual".to_owned()),
    }
}

#[test]
fn memory_record_uses_contract_json_names() {
    let now = Utc::now();
    let record = MemoryRecord {
        id: Id::from("mem-1"),
        kind: MemoryKind::Fact,
        content: MemoryContent {
            text: "Rust owns deterministic behavior.".to_owned(),
            summary: None,
            entities: Vec::new(),
            language: Some("en".to_owned()),
            format: Some(MemoryContentFormat::Text),
            structured: None,
            hash: Some("sha256:test".to_owned()),
        },
        scope: scope(),
        provenance: provenance(now),
        policy: policy(),
        status: MemoryStatus::Active,
        links: Vec::new(),
        assertions: Vec::new(),
        created_at: now,
        updated_at: None,
        metadata: None,
    };

    let value = serde_json::to_value(record).expect("serialize memory record");

    assert_eq!(value["createdAt"], json!(now));
    assert_eq!(value["content"]["hash"], json!("sha256:test"));
    assert_eq!(value["kind"], json!("fact"));
    assert_eq!(
        value["policy"]["allowedUses"],
        json!(["retrieval", "evaluation"])
    );
    assert!(value.get("created_at").is_none());
    assert!(value["content"].get("structured").is_none());
}

#[test]
fn knowledge_entity_serializes_bi_temporal_validity() {
    // ADR-0019: KnowledgeEntity carries optional validFrom/validUntil.
    let now = Utc::now();
    let entity = KnowledgeEntity {
        id: Id::from("entity-1"),
        graph_id: None,
        kind: EntityKind::Function,
        name: "parse".to_owned(),
        aliases: Vec::new(),
        scope: scope(),
        source_refs: Vec::new(),
        concept_refs: Vec::new(),
        ontology_class_refs: Vec::new(),
        provenance: provenance(now),
        created_at: now,
        updated_at: None,
        valid_from: Some(now),
        valid_until: Some(now),
        metadata: None,
    };

    let value = serde_json::to_value(&entity).expect("serialize entity");
    assert_eq!(value["validFrom"], json!(now));
    assert_eq!(value["validUntil"], json!(now));
    assert!(
        value.get("valid_from").is_none(),
        "camelCase contract names only"
    );

    // Round-trips losslessly.
    let back: KnowledgeEntity = serde_json::from_value(value).expect("deserialize entity");
    assert_eq!(back, entity);

    // None omits the keys (skip_serializing_if = Option::is_none).
    let mut omitted = entity;
    omitted.valid_from = None;
    omitted.valid_until = None;
    let omitted_value = serde_json::to_value(&omitted).expect("serialize");
    assert!(omitted_value.get("validFrom").is_none());
    assert!(omitted_value.get("validUntil").is_none());
}

#[test]
fn entity_kind_new_symbol_kinds_serialize_snake_case() {
    // ADR-0020: code-structural symbol kinds round-trip as snake_case.
    for (kind, expected) in [
        (EntityKind::Struct, "struct"),
        (EntityKind::Interface, "interface"),
        (EntityKind::Trait, "trait"),
        (EntityKind::TypeAlias, "type_alias"),
        (EntityKind::Enum, "enum"),
        (EntityKind::Endpoint, "endpoint"),
    ] {
        let value = serde_json::to_value(&kind).expect("serialize kind");
        assert_eq!(value, json!(expected));
        let back: EntityKind = serde_json::from_value(value).expect("deserialize kind");
        assert_eq!(back, kind);
    }
}

#[test]
fn applicability_rule_round_trips() {
    // RFC-0013 D2: ApplicabilityRule binds a target under a condition.
    let now = Utc::now();
    let rule = ApplicabilityRule {
        id: Id::from("rule-1"),
        condition: "segment == retail".to_owned(),
        target: RuleTarget::Entity(EntityRef {
            id: Some(Id::from("cap-account-opening")),
            kind: None,
            name: Some("Account Opening".to_owned()),
            aliases: Vec::new(),
        }),
        binding: Some("requires_kyc".to_owned()),
        scope: scope(),
        policy: policy(),
        provenance: provenance(now),
        valid_from: Some(now),
        valid_until: None,
        created_at: now,
    };

    let value = serde_json::to_value(&rule).expect("serialize rule");
    assert_eq!(value["id"], json!("rule-1"));
    assert_eq!(
        value["target"]["entity"]["id"],
        json!("cap-account-opening")
    );
    assert!(value.get("validUntil").is_none(), "None omits the key");

    let back: ApplicabilityRule = serde_json::from_value(value).expect("deserialize rule");
    assert_eq!(back, rule);
}

#[test]
fn decision_trace_round_trips() {
    // RFC-0013 D4: DecisionTrace is a candidate-only agent run record.
    let now = Utc::now();
    let trace = DecisionTrace {
        id: Id::from("trace-1"),
        scope: scope(),
        agent: actor(),
        items_consulted: Vec::new(),
        traversal_path: vec!["fn-open-account".to_owned(), "fn-create-ledger".to_owned()],
        policy_applied: None,
        precedent: None,
        output: "approved".to_owned(),
        provenance: provenance(now),
        created_at: now,
    };

    let value = serde_json::to_value(&trace).expect("serialize trace");
    assert_eq!(value["agent"]["id"], json!("actor-1"));
    assert_eq!(
        value["traversalPath"],
        json!(["fn-open-account", "fn-create-ledger"])
    );
    assert!(value.get("policyApplied").is_none());
    assert!(value.get("itemsConsulted").is_none(), "empty Vec skips");

    let back: DecisionTrace = serde_json::from_value(value).expect("deserialize trace");
    assert_eq!(back, trace);
}

#[test]
fn context_subgraph_round_trips() {
    // RFC-0013 D1: ContextSubgraph is the connected-subgraph packet shape.
    let now = Utc::now();
    let subgraph = ContextSubgraph {
        nodes: Vec::new(),
        edges: Vec::new(),
        omitted: Vec::new(),
        budget: None,
        created_at: now,
    };

    let value = serde_json::to_value(&subgraph).expect("serialize subgraph");
    // Empty Vecs and None budget all skip; only createdAt remains.
    assert!(value.get("nodes").is_none());
    assert!(value.get("edges").is_none());
    assert!(value.get("omitted").is_none());
    assert!(value.get("budget").is_none());
    assert_eq!(value["createdAt"], json!(now));

    let back: ContextSubgraph = serde_json::from_value(value).expect("deserialize subgraph");
    assert_eq!(back, subgraph);
}

#[test]
fn knowledge_entity_ontology_class_refs_skip_when_empty() {
    // RFC-0013 D3: ontologyClassRefs is optional, skips when empty.
    let now = Utc::now();
    let entity = KnowledgeEntity {
        id: Id::from("entity-ocr"),
        graph_id: None,
        kind: EntityKind::Function,
        name: "parse".to_owned(),
        aliases: Vec::new(),
        scope: scope(),
        source_refs: Vec::new(),
        concept_refs: Vec::new(),
        ontology_class_refs: Vec::new(),
        provenance: provenance(now),
        created_at: now,
        updated_at: None,
        valid_from: None,
        valid_until: None,
        metadata: None,
    };

    let value = serde_json::to_value(&entity).expect("serialize");
    assert!(
        value.get("ontologyClassRefs").is_none(),
        "empty Vec skips serialization"
    );

    // Populated round-trips losslessly.
    let mut with_class = entity;
    with_class.ontology_class_refs = vec![Id::from("oc-capability")];
    let v2 = serde_json::to_value(&with_class).expect("serialize");
    assert_eq!(v2["ontologyClassRefs"], json!(["oc-capability"]));
    let back: KnowledgeEntity = serde_json::from_value(v2).expect("deserialize");
    assert_eq!(back, with_class);
}

#[test]
fn retrieval_target_type_new_variants_serialize_snake_case() {
    // RFC-0013 D2/D4: rule/policy/axiom/decision_trace variants.
    for (variant, expected) in [
        (RetrievalTargetType::Rule, "rule"),
        (RetrievalTargetType::Policy, "policy"),
        (RetrievalTargetType::Axiom, "axiom"),
        (RetrievalTargetType::DecisionTrace, "decision_trace"),
    ] {
        let value = serde_json::to_value(&variant).expect("serialize variant");
        assert_eq!(value, json!(expected));
        let back: RetrievalTargetType = serde_json::from_value(value).expect("deserialize variant");
        assert_eq!(back, variant);
    }
}

#[test]
fn applicability_rule_concept_target_round_trips() {
    // RFC-0013 D2: the Concept arm of RuleTarget (distinct externally-tagged path).
    let now = Utc::now();
    let rule = ApplicabilityRule {
        id: Id::from("rule-2"),
        condition: "domain == deposits".to_owned(),
        target: RuleTarget::Concept(ConceptRef {
            id: Some(Id::from("cap-account-opening")),
            uri: None,
            label: Some("Account Opening".to_owned()),
        }),
        binding: None,
        scope: scope(),
        policy: policy(),
        provenance: provenance(now),
        valid_from: None,
        valid_until: None,
        created_at: now,
    };

    let value = serde_json::to_value(&rule).expect("serialize rule");
    assert_eq!(
        value["target"]["concept"]["id"],
        json!("cap-account-opening")
    );
    assert!(value.get("binding").is_none());
    assert!(value.get("validFrom").is_none());

    let back: ApplicabilityRule = serde_json::from_value(value).expect("deserialize rule");
    assert_eq!(back, rule);
}

#[test]
fn context_subgraph_populated_round_trips() {
    // RFC-0013 D1: a populated subgraph (node + edge + omitted + budget) round-trips.
    let now = Utc::now();
    let node = RetrievalResult {
        id: "result-1".to_owned(),
        target_type: RetrievalTargetType::Rule,
        target_id: "rule-1".to_owned(),
        content: "segment == retail".to_owned(),
        score: RetrievalScore {
            total: 0.9,
            relevance: Some(0.9),
            recency: None,
            confidence: None,
            cue_match: None,
            hierarchical_fit: None,
            policy_fit: None,
        },
        provenance: provenance(now),
        policy: policy(),
        explanation: None,
        fusion_trace: None,
        metadata: None,
    };
    let edge = KnowledgeRelationship {
        id: Id::from("rel-1"),
        graph_id: None,
        subject: EntityRef {
            id: Some(Id::from("fn-a")),
            kind: None,
            name: None,
            aliases: Vec::new(),
        },
        predicate: "calls".to_owned(),
        object: EntityRef {
            id: Some(Id::from("fn-b")),
            kind: None,
            name: None,
            aliases: Vec::new(),
        },
        scope: scope(),
        evidence: Vec::new(),
        confidence: Some(0.8),
        provenance: provenance(now),
        created_at: now,
        updated_at: None,
    };
    let subgraph = ContextSubgraph {
        nodes: vec![node],
        edges: vec![edge],
        omitted: vec![OmittedResult {
            target_type: RetrievalTargetType::Policy,
            target_id: "policy-1".to_owned(),
            reason: OmittedReason::BudgetExceeded,
        }],
        budget: Some(ContextBudget {
            max_items: Some(10),
            max_tokens: None,
            max_bytes: None,
        }),
        created_at: now,
    };

    let value = serde_json::to_value(&subgraph).expect("serialize subgraph");
    assert_eq!(value["nodes"][0]["targetType"], json!("rule"));
    assert_eq!(value["edges"][0]["predicate"], json!("calls"));
    assert_eq!(value["omitted"][0]["reason"], json!("budget_exceeded"));
    assert_eq!(value["budget"]["maxItems"], json!(10));

    let back: ContextSubgraph = serde_json::from_value(value).expect("deserialize subgraph");
    assert_eq!(back, subgraph);
}
