use engram_domain::{CodeBinding, RepoId, Scope};
use engram_knowledge::{
    AcquisitionMode, CodeEligibility, ConstrainedCodeQuery, RepositoryRegistration,
};

mod task_003 {
    use super::*;
    #[test]
    fn registration_generation_source_reset_ports() {
        let registration = RepositoryRegistration {
            repo_id: RepoId("opaque-id".into()),
            locator_id: "locator-ref".into(),
            acquisition: AcquisitionMode::LocalOnly,
        };
        assert_eq!(registration.repo_id.0, "opaque-id");
        assert_ne!(registration.locator_id, registration.repo_id.0);
        let scope = Scope {
            tenant: "tenant".into(),
            subject: None,
            workspace: Some("project".into()),
            session: None,
            environment: None,
        };
        let binding = CodeBinding {
            scope: scope.clone(),
            repo_id: registration.repo_id.clone(),
            branch_ref: "refs/heads/main".into(),
            version: 1,
            active_generation: None,
            epoch: 0,
        };
        binding.validate().unwrap();
        let eligibility = CodeEligibility {
            scope,
            epoch: 0,
            generations: vec![],
            repository_ids: vec![],
        };
        assert_eq!(eligibility.epoch, binding.epoch);
        let query = ConstrainedCodeQuery {
            eligibility,
            target_ids: vec![],
            limit: 10,
        };
        assert!(query.target_ids.is_empty());
        assert_eq!(query.limit, 10);
    }

    #[test]
    fn mandatory_query_eligibility() {
        fn requires_eligibility(_: &ConstrainedCodeQuery) {}
        let value = ConstrainedCodeQuery {
            eligibility: CodeEligibility {
                scope: Scope {
                    tenant: "t".into(),
                    subject: None,
                    workspace: None,
                    session: None,
                    environment: None,
                },
                epoch: 1,
                generations: vec!["g1".into()],
                repository_ids: vec!["r1".into()],
            },
            target_ids: vec!["eligible".into()],
            limit: 1,
        };
        requires_eligibility(&value);
        assert_eq!(value.eligibility.generations, ["g1"]);
        assert_eq!(value.target_ids, ["eligible"]);
    }
}
