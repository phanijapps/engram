use engram_domain::Scope;
use engram_runtime::effect::*;
fn identity() -> EffectIdentity {
    EffectIdentity {
        actor: "actor".into(),
        call: "call-1".into(),
        session: "session-1".into(),
        backend: "helixdb".into(),
        mode: "embedded".into(),
        store: "store".into(),
        scope: Scope {
            tenant: "tenant".into(),
            subject: None,
            workspace: Some("work".into()),
            session: None,
            environment: None,
        },
        item: "item-1".into(),
    }
}
#[test]
fn exact_identity_and_actual_commit_cannot_be_replaced() {
    let id = identity();
    let frame = EffectFrame::new(id.clone());
    frame
        .bind_commit(&id, "step-1", "original-actual-uuid")
        .unwrap();
    for change in 0..8 {
        let mut wrong = id.clone();
        match change {
            0 => wrong.actor.push('x'),
            1 => wrong.call.push('x'),
            2 => wrong.session.push('x'),
            3 => wrong.backend.push('x'),
            4 => wrong.mode.push('x'),
            5 => wrong.store.push('x'),
            6 => wrong.scope.environment = Some("other".into()),
            _ => wrong.item.push('x'),
        }
        assert!(frame.observation(&wrong).is_err());
        assert!(frame.bind_commit(&wrong, "step-1", "replacement").is_err());
    }
    assert!(
        frame
            .bind_commit(&id, "wrong-step", "original-actual-uuid")
            .is_err()
    );
    assert!(frame.bind_commit(&id, "step-1", "replacement").is_err());
    assert!(
        frame
            .finish(
                &id,
                "step-1",
                EffectOutcome::Unknown {
                    operation_id: "replacement".into()
                },
                CleanupCertainty::Unconfirmed
            )
            .is_err()
    );
    let nested = frame.clone();
    nested
        .finish(
            &id,
            "step-1",
            EffectOutcome::Unknown {
                operation_id: "original-actual-uuid".into(),
            },
            CleanupCertainty::Unconfirmed,
        )
        .unwrap();
    let observed = frame.observation(&id).unwrap();
    assert_eq!(observed.commit_id.as_deref(), Some("original-actual-uuid"));
    assert_eq!(observed.cleanup, CleanupCertainty::Unconfirmed);
    assert!(frame.bind_commit(&id, "step-2", "new").is_err());
    assert!(
        frame
            .finish(
                &id,
                "step-1",
                EffectOutcome::Acknowledged,
                CleanupCertainty::Confirmed
            )
            .is_err()
    );
}
#[test]
fn blocked_admission_has_prior_id_without_new_step_or_commit() {
    let id = identity();
    let frame = EffectFrame::new(id.clone());
    frame
        .blocked(&id, "prior-actual-uuid", CleanupCertainty::Confirmed)
        .unwrap();
    let o = frame.observation(&id).unwrap();
    assert_eq!(
        o.outcome,
        EffectOutcome::BlockedByPriorUnknown {
            operation_id: "prior-actual-uuid".into()
        }
    );
    assert_eq!(o.commit_id, None);
    assert_eq!(o.step, None);
    assert!(
        frame
            .blocked(&id, "replacement-prior", CleanupCertainty::Confirmed)
            .is_err()
    );
    assert!(frame.bind_commit(&id, "step", "replacement").is_err());
}
#[test]
fn definite_failure_and_cleanup_are_independent() {
    let id = identity();
    let frame = EffectFrame::new(id.clone());
    frame.bind_commit(&id, "step", "actual").unwrap();
    frame
        .finish(
            &id,
            "step",
            EffectOutcome::DefiniteFailure,
            CleanupCertainty::Unconfirmed,
        )
        .unwrap();
    let o = frame.observation(&id).unwrap();
    assert_eq!(o.outcome, EffectOutcome::DefiniteFailure);
    assert_eq!(o.cleanup, CleanupCertainty::Unconfirmed);
    assert!(
        frame
            .finish(
                &id,
                "step",
                EffectOutcome::Acknowledged,
                CleanupCertainty::Confirmed
            )
            .is_err()
    );
}

#[test]
fn staged_frames_are_never_acknowledged_until_actual_commit_and_cancel_is_terminal() {
    let id = identity();
    let frame = EffectFrame::new(id.clone());
    assert_eq!(
        frame.observation(&id).unwrap().outcome,
        EffectOutcome::Staged
    );
    frame.bind_step(&id, "actual-step").unwrap();
    let staged = frame.observation(&id).unwrap();
    assert_eq!(staged.step.as_deref(), Some("actual-step"));
    assert_eq!(staged.commit_id, None);
    assert!(frame.bind_step(&id, "other-step").is_err());
    frame.cancel(&id, "actual-step").unwrap();
    assert_eq!(
        frame.observation(&id).unwrap().outcome,
        EffectOutcome::Cancelled
    );
    assert!(
        frame
            .bind_commit(&id, "actual-step", "fabricated-after-cancel")
            .is_err()
    );
    let frame = EffectFrame::new(id.clone());
    frame.bind_step(&id, "actual-step").unwrap();
    frame
        .bind_commit(&id, "actual-step", "actual-commit")
        .unwrap();
    frame
        .finish(
            &id,
            "actual-step",
            EffectOutcome::Acknowledged,
            CleanupCertainty::Confirmed,
        )
        .unwrap();
    frame.cancel(&id, "actual-step").unwrap();
    assert_eq!(
        frame.observation(&id).unwrap().outcome,
        EffectOutcome::Acknowledged
    );
}
