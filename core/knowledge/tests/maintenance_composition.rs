//! Declared in-memory collaborator tests canonical semantics; real provider proof is separate.
use engram_domain::*;
use engram_knowledge::maintenance_composition::{self as c, MaintenanceStorage};
use engram_runtime::CoreResult;
use std::{cell::RefCell, collections::BTreeMap};
#[derive(Default)]
struct Store {
    entities: RefCell<BTreeMap<String, KnowledgeEntity>>,
    relations: RefCell<BTreeMap<String, KnowledgeRelationship>>,
}
impl MaintenanceStorage for Store {
    fn visible(&self, a: &Scope, b: &Scope) -> bool {
        a == b
    }
    fn entity(&self, id: &EntityId, s: &Scope) -> CoreResult<Option<KnowledgeEntity>> {
        Ok(self
            .entities
            .borrow()
            .get(id.as_str())
            .filter(|v| &v.scope == s)
            .cloned())
    }
    fn relationship(
        &self,
        id: &RelationshipId,
        s: &Scope,
    ) -> CoreResult<Option<KnowledgeRelationship>> {
        Ok(self
            .relations
            .borrow()
            .get(id.as_str())
            .filter(|v| &v.scope == s)
            .cloned())
    }
    fn entities(&self, s: &Scope) -> CoreResult<Vec<KnowledgeEntity>> {
        Ok(self
            .entities
            .borrow()
            .values()
            .filter(|v| &v.scope == s)
            .cloned()
            .collect())
    }
    fn relationships(&self, s: &Scope) -> CoreResult<Vec<KnowledgeRelationship>> {
        Ok(self
            .relations
            .borrow()
            .values()
            .filter(|v| &v.scope == s)
            .cloned()
            .collect())
    }
    fn write_entity(&self, v: &KnowledgeEntity) -> CoreResult<()> {
        self.entities
            .borrow_mut()
            .insert(v.id.to_string(), v.clone());
        Ok(())
    }
    fn write_relationship(&self, v: &KnowledgeRelationship) -> CoreResult<()> {
        self.relations
            .borrow_mut()
            .insert(v.id.to_string(), v.clone());
        Ok(())
    }
    fn delete_entity(&self, id: &EntityId) -> CoreResult<()> {
        self.entities.borrow_mut().remove(id.as_str());
        Ok(())
    }
    fn delete_relationship(&self, id: &RelationshipId) -> CoreResult<()> {
        self.relations.borrow_mut().remove(id.as_str());
        Ok(())
    }
    fn duplicate_count(&self, _: &Scope) -> CoreResult<u32> {
        Ok(0)
    }
}
fn entity(id: &str) -> KnowledgeEntity {
    serde_json::from_value(serde_json::json!({"id":id,"name":id,"kind":"concept","scope":{"tenant":"t"},"createdAt":"2026-11-17T00:00:00Z","policy":{"visibility":"public","retention":"durable"},"provenance":{"source":"fixture","actor":{"id":"actor","kind":"agent"},"observedAt":"2026-11-17T00:00:00Z"}})).unwrap()
}
#[test]
fn preview_is_inert_and_archive_restore_alias_are_idempotent() {
    let store = Store::default();
    let e = entity("e");
    store.write_entity(&e).unwrap();
    let plan = MaintenancePlan::new(
        None,
        e.scope.clone(),
        vec![MaintenanceMutation::Archive {
            target: MaintenanceTarget::Entity(e.id.clone()),
        }],
        MaintenancePolicy::default(),
        e.provenance.actor.clone(),
    );
    assert_eq!(
        c::apply_plan(&store, &plan, ApplyMode::Preview)
            .unwrap()
            .applied,
        0
    );
    assert_eq!(store.entity(&e.id, &e.scope).unwrap(), Some(e.clone()));
    assert_eq!(
        c::apply_plan(&store, &plan, ApplyMode::Apply)
            .unwrap()
            .applied,
        1
    );
    assert_eq!(
        c::apply_plan(&store, &plan, ApplyMode::Apply)
            .unwrap()
            .unchanged,
        1
    );
    let restore = MaintenancePlan::new(
        None,
        e.scope.clone(),
        vec![
            MaintenanceMutation::Restore {
                target: MaintenanceTarget::Entity(e.id.clone()),
            },
            MaintenanceMutation::AddAlias {
                entity: e.id.clone(),
                alias: "alias".into(),
            },
        ],
        MaintenancePolicy::default(),
        e.provenance.actor.clone(),
    );
    assert_eq!(
        c::apply_plan(&store, &restore, ApplyMode::Apply)
            .unwrap()
            .applied,
        2
    );
    let after = store.entity(&e.id, &e.scope).unwrap().unwrap();
    assert_eq!(after.aliases, vec!["alias"]);
    assert!(after.archived_at.is_none());
    assert_eq!(after.source_refs, e.source_refs);
    assert_eq!(
        c::apply_plan(&store, &restore, ApplyMode::Apply)
            .unwrap()
            .unchanged,
        2
    );
}

/// Deliberately violates the storage-read contract to test the composer's own visibility backstop.
struct Leaky<'a>(&'a Store);
impl MaintenanceStorage for Leaky<'_> {
    fn visible(&self, a: &Scope, b: &Scope) -> bool {
        a == b
    }
    fn entity(&self, id: &EntityId, _: &Scope) -> CoreResult<Option<KnowledgeEntity>> {
        Ok(self.0.entities.borrow().get(id.as_str()).cloned())
    }
    fn relationship(
        &self,
        id: &RelationshipId,
        _: &Scope,
    ) -> CoreResult<Option<KnowledgeRelationship>> {
        Ok(self.0.relations.borrow().get(id.as_str()).cloned())
    }
    fn entities(&self, s: &Scope) -> CoreResult<Vec<KnowledgeEntity>> {
        self.0.entities(s)
    }
    fn relationships(&self, s: &Scope) -> CoreResult<Vec<KnowledgeRelationship>> {
        self.0.relationships(s)
    }
    fn write_entity(&self, v: &KnowledgeEntity) -> CoreResult<()> {
        self.0.write_entity(v)
    }
    fn write_relationship(&self, v: &KnowledgeRelationship) -> CoreResult<()> {
        self.0.write_relationship(v)
    }
    fn delete_entity(&self, id: &EntityId) -> CoreResult<()> {
        self.0.delete_entity(id)
    }
    fn delete_relationship(&self, id: &RelationshipId) -> CoreResult<()> {
        self.0.delete_relationship(id)
    }
    fn duplicate_count(&self, s: &Scope) -> CoreResult<u32> {
        self.0.duplicate_count(s)
    }
}
#[test]
fn composer_does_not_trust_an_unfiltered_point_reader() {
    let store = Store::default();
    let e = entity("hidden");
    store.write_entity(&e).unwrap();
    let mut scope = e.scope.clone();
    scope.subject = Some("foreign".into());
    for mutation in [
        MaintenanceMutation::Archive {
            target: MaintenanceTarget::Entity(e.id.clone()),
        },
        MaintenanceMutation::Restore {
            target: MaintenanceTarget::Entity(e.id.clone()),
        },
        MaintenanceMutation::Delete {
            target: MaintenanceTarget::Entity(e.id.clone()),
        },
        MaintenanceMutation::AddAlias {
            entity: e.id.clone(),
            alias: "a".into(),
        },
        MaintenanceMutation::RemoveAlias {
            entity: e.id.clone(),
            alias: "a".into(),
        },
        MaintenanceMutation::Merge {
            survivor: e.id.clone(),
            absorbed: vec![e.id.clone()],
        },
    ] {
        let plan = c::build_plan(
            &Leaky(&store),
            MaintenancePlanRequest {
                graph_id: None,
                scope: scope.clone(),
                mutations: vec![mutation],
                policy: MaintenancePolicy::default(),
                actor: e.provenance.actor.clone(),
            },
        )
        .unwrap();
        assert!(
            plan.previews
                .iter()
                .all(|v| v.before.is_empty() && v.after.is_empty())
        );
        assert_eq!(
            c::apply_plan(&Leaky(&store), &plan, ApplyMode::Apply)
                .unwrap()
                .applied,
            0
        );
    }
    assert_eq!(store.entity(&e.id, &e.scope).unwrap(), Some(e));
}
