use engram_domain::*;
use engram_knowledge::{CodeEligibility, code_storage_policy::*};
fn view(generations: Vec<String>) -> CodeEligibility {
    CodeEligibility {
        scope: Scope {
            tenant: "tenant".into(),
            subject: None,
            workspace: None,
            session: None,
            environment: None,
        },
        epoch: 7,
        generations,
        repository_ids: vec!["repo".into()],
    }
}
#[test]
fn canonical_eligibility_preserves_empty_explicit_wildcard_budget_and_order() {
    assert!(query_budget(0).is_err());
    assert!(explicit_maintenance(&[]).is_err());
    let ids = vec!["b".into(), "a".into(), "b".into()];
    let allowed = vec!["a".into(), "b".into()];
    assert_eq!(
        constrain_query(view(vec!["g".into()]), &ids, &allowed, 1)
            .unwrap()
            .target_ids,
        ids
    );
    assert_eq!(
        constrain_query(view(vec!["g".into()]), &[], &allowed, 1)
            .unwrap()
            .target_ids,
        vec!["a"]
    );
    assert!(
        constrain_query(view(vec![]), &[], &[], 1)
            .unwrap()
            .target_ids
            .is_empty()
    );
    let error = constrain_query(view(vec![]), &["a".into(), "b".into()], &[], 1)
        .unwrap_err()
        .to_string();
    assert!(error.ends_with("a, b"));
    assert!(
        constrain_query(
            view(vec!["g".into()]),
            &["a".into(), "foreign".into()],
            &allowed,
            10
        )
        .is_err()
    );
}
#[test]
fn canonical_error_markers_remain_portable() {
    assert!(
        code_error(CodeBindingError::StaleGeneration)
            .to_string()
            .contains("GenerationChanged")
    );
    assert!(
        code_error(CodeBindingError::BindingRequired)
            .to_string()
            .contains("BindingRequired")
    );
    assert!(
        code_error(CodeBindingError::BindingConflict)
            .to_string()
            .contains("BindingConflict")
    );
}
