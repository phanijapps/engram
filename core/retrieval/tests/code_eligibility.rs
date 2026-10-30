use engram_domain::{Id, Scope};
use engram_retrieval::{CodeEligibilityToken, authorize_before_selection};

mod task_004 {
    use super::*;
    fn token() -> CodeEligibilityToken {
        CodeEligibilityToken {
            scope: Scope {
                tenant: "t".into(),
                subject: None,
                workspace: Some("w".into()),
                session: None,
                environment: None,
            },
            epoch: 3,
            generation_ids: vec!["g".into()],
            allowed_targets: vec![Id::from("allowed")],
        }
    }
    #[test]
    fn eligibility_required_before_selection() {
        assert!(authorize_before_selection(None, 10).is_err());
        assert!(authorize_before_selection(Some(&token()), 0).is_err());
        assert!(authorize_before_selection(Some(&token()), 10).is_ok());
    }
    #[test]
    fn lane_lifecycle_contract() {
        let t = token();
        assert_eq!(t.epoch, 3);
        assert_eq!(t.generation_ids, ["g"]);
        assert_eq!(t.allowed_targets, [Id::from("allowed")]);
        let missing = CodeEligibilityToken {
            allowed_targets: vec![],
            ..t
        };
        assert!(authorize_before_selection(Some(&missing), 10).is_err());
    }
}
