use super::*;
use proptest::prelude::*;

const NOW: i64 = 1_700_000_100_000;

fn proposal(x: f64, y: f64) -> ActionProposal {
    ActionProposal {
        proposal_id: "prop_1".into(),
        agent_id: "agent_alpha".into(),
        action_type: "PATROL".into(),
        parameters_json: "{}".into(),
        target_position: Vector3::new(x, y, 0.0),
        priority: 1,
        timestamp: NOW - 100,
        correlation_id: "corr_1".into(),
        source_observation: "obs_001".into(),
        observed_at: Some(NOW - 200),
    }
}

fn me() -> AgentView {
    AgentView {
        agent_id: "agent_alpha".into(),
        position: Vector3::new(0.0, 0.0, 0.0),
    }
}

fn run(
    validator: &EpistemicValidator,
    proposal: &ActionProposal,
    hazards: &[HazardZone],
    others: &[AgentView],
) -> ValidationResult {
    let agent = me();
    validator.validate(
        proposal,
        &ValidationContext {
            now_ms: NOW,
            agent: Some(&agent),
            others,
            hazards,
        },
    )
}

fn strict() -> EpistemicValidator {
    EpistemicValidator::with_config(ValidatorConfig {
        max_step: 50.0,
        min_separation: 5.0,
        ..ValidatorConfig::default()
    })
    .unwrap()
}

#[test]
fn valid_proposal_passes_every_check() {
    let result = run(&strict(), &proposal(10.0, 20.0), &[], &[]);
    assert!(result.accepted, "{:?}", result.contradictions);
    assert_eq!(result.reasons, vec!["ALL_CHECKS_PASSED"]);
    assert_eq!(result.feasibility, 1.0);
    assert_eq!(result.checks.len(), CheckId::ALL.len());
    assert!(result.contradictions.is_empty());
    assert_eq!(result.validator_version, VALIDATOR_VERSION);
    assert_eq!(result.observation_age_ms, 200);
}

#[test]
fn out_of_bounds_target_is_rejected() {
    let validator = EpistemicValidator::new();
    let result = run(&validator, &proposal(999_999.0, 0.0), &[], &[]);
    assert!(!result.accepted);
    assert_eq!(result.feasibility, 0.0);
    assert!(result.reasons.contains(&"OUT_OF_BOUNDS".to_string()));
}

#[test]
fn nan_and_infinite_coordinates_fail_closed() {
    let validator = EpistemicValidator::new();
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let result = run(&validator, &proposal(bad, 0.0), &[], &[]);
        assert!(!result.accepted, "{bad} accepted");
        assert_eq!(
            result.check(CheckId::FiniteValues).unwrap().status,
            CheckStatus::Fail
        );
        assert_eq!(
            result.check(CheckId::CoordinateBounds).unwrap().status,
            CheckStatus::NotEvaluated
        );
        assert!(
            serde_json::to_string(&result).is_ok(),
            "result must serialize"
        );
    }
}

#[test]
fn unknown_action_fails_closed() {
    let mut p = proposal(1.0, 1.0);
    p.action_type = "FIRE".into();
    let result = run(&EpistemicValidator::new(), &p, &[], &[]);
    assert!(!result.accepted);
    assert_eq!(result.reasons, vec!["ACTION_NOT_ALLOWED"]);
    p.action_type = "move".into();
    assert!(
        !run(&EpistemicValidator::new(), &p, &[], &[]).accepted,
        "allow-list is case-sensitive"
    );
}

#[test]
fn unregistered_agent_fails_closed() {
    let validator = EpistemicValidator::new();
    let result = validator.validate(
        &proposal(1.0, 1.0),
        &ValidationContext {
            now_ms: NOW,
            agent: None,
            others: &[],
            hazards: &[],
        },
    );
    assert!(!result.accepted);
    assert!(result.reasons.contains(&"AGENT_NOT_REGISTERED".to_string()));
    assert_eq!(
        result.check(CheckId::MaxStep).unwrap().status,
        CheckStatus::NotEvaluated
    );
}

#[test]
fn stale_observation_fails_closed_even_when_the_proposal_is_new() {
    let mut p = proposal(1.0, 1.0);
    p.timestamp = NOW;
    p.observed_at = Some(NOW - 30_001);
    let result = run(&EpistemicValidator::new(), &p, &[], &[]);
    assert!(!result.accepted);
    assert_eq!(result.reasons, vec!["OBSERVATION_STALE"]);
    assert_eq!(result.confidence, 0.5);
    p.observed_at = Some(NOW - 30_000);
    assert!(
        run(&EpistemicValidator::new(), &p, &[], &[]).accepted,
        "limit is inclusive"
    );
}

#[test]
fn future_timestamps_beyond_skew_fail_closed() {
    let mut p = proposal(1.0, 1.0);
    p.timestamp = NOW + 1_001;
    p.observed_at = Some(NOW);
    let result = run(&EpistemicValidator::new(), &p, &[], &[]);
    assert_eq!(result.reasons, vec!["TIMESTAMP_IN_FUTURE"]);
    p.timestamp = NOW + 1_000;
    assert!(run(&EpistemicValidator::new(), &p, &[], &[]).accepted);
}

#[test]
fn negative_priority_is_rejected() {
    let mut p = proposal(1.0, 1.0);
    p.priority = -1;
    let result = run(&EpistemicValidator::new(), &p, &[], &[]);
    assert_eq!(result.reasons, vec!["PRIORITY_OUT_OF_RANGE"]);
}

#[test]
fn step_limit_is_enforced() {
    let result = run(&strict(), &proposal(51.0, 0.0), &[], &[]);
    assert_eq!(result.reasons, vec!["STEP_TOO_LARGE"]);
    assert_eq!(result.check(CheckId::MaxStep).unwrap().measured, Some(51.0));
}

#[test]
fn hazard_target_and_crossing_path_are_rejected() {
    let zone = HazardZone {
        id: "crater".into(),
        center: Vector3::new(20.0, 0.0, 0.0),
        radius: 5.0,
    };
    let into = run(
        &strict(),
        &proposal(20.0, 1.0),
        std::slice::from_ref(&zone),
        &[],
    );
    assert_eq!(into.reasons, vec!["HAZARD_INTERSECTION"]);
    assert!(into.contradictions[0].contains("target is inside"));
    let through = run(
        &strict(),
        &proposal(40.0, 0.0),
        std::slice::from_ref(&zone),
        &[],
    );
    assert_eq!(through.reasons, vec!["HAZARD_INTERSECTION"]);
    assert!(through.contradictions[0].contains("path crosses"));
    let around = run(
        &strict(),
        &proposal(20.0, 10.0),
        std::slice::from_ref(&zone),
        &[],
    );
    assert!(around.accepted, "{:?}", around.contradictions);
}

#[test]
fn an_agent_inside_a_new_zone_may_only_leave_it() {
    let zone = HazardZone {
        id: "new".into(),
        center: Vector3::new(0.0, 0.0, 0.0),
        radius: 10.0,
    };
    let deeper = run(
        &strict(),
        &proposal(1.0, 0.0),
        std::slice::from_ref(&zone),
        &[],
    );
    assert!(!deeper.accepted);
    let out = run(
        &strict(),
        &proposal(12.0, 0.0),
        std::slice::from_ref(&zone),
        &[],
    );
    assert!(out.accepted, "{:?}", out.contradictions);
}

#[test]
fn separation_from_other_agents_is_enforced() {
    let others = [AgentView {
        agent_id: "agent_beta".into(),
        position: Vector3::new(10.0, 0.0, 0.0),
    }];
    let close = run(&strict(), &proposal(12.0, 0.0), &[], &others);
    assert_eq!(close.reasons, vec!["SEPARATION_VIOLATION"]);
    let far = run(&strict(), &proposal(20.0, 0.0), &[], &others);
    assert!(far.accepted);
}

#[test]
fn every_failing_check_is_reported() {
    let mut p = proposal(999_999.0, 0.0);
    p.action_type = "FIRE".into();
    p.priority = 99;
    let result = run(&strict(), &p, &[], &[]);
    for code in [
        "ACTION_NOT_ALLOWED",
        "PRIORITY_OUT_OF_RANGE",
        "OUT_OF_BOUNDS",
        "STEP_TOO_LARGE",
    ] {
        assert!(result.reasons.contains(&code.to_string()), "missing {code}");
    }
}

#[test]
fn domain_checks_can_be_ablated_but_core_checks_cannot() {
    let ablated = EpistemicValidator::with_config(ValidatorConfig {
        disabled_checks: vec![CheckId::HazardClearance],
        ..ValidatorConfig::default()
    })
    .unwrap();
    let zone = HazardZone {
        id: "z".into(),
        center: Vector3::new(5.0, 0.0, 0.0),
        radius: 3.0,
    };
    let result = run(&ablated, &proposal(5.0, 0.0), &[zone], &[]);
    assert!(result.accepted);
    assert_eq!(
        result.check(CheckId::HazardClearance).unwrap().status,
        CheckStatus::Disabled
    );
    assert!(!ablated
        .checked_constraints()
        .contains(&"hazard_clearance".to_string()));
    for core in CheckId::ALL.into_iter().filter(|check| check.is_core()) {
        let error = EpistemicValidator::with_config(ValidatorConfig {
            disabled_checks: vec![core],
            ..ValidatorConfig::default()
        })
        .unwrap_err();
        assert_eq!(error, ConfigError::CoreCheckDisabled(core.as_str()));
    }
}

#[test]
fn config_hash_identifies_thresholds() {
    assert_eq!(
        EpistemicValidator::new().config_hash(),
        EpistemicValidator::new().config_hash()
    );
    assert_ne!(
        EpistemicValidator::new().config_hash(),
        strict().config_hash()
    );
}

#[test]
fn only_a_passing_verdict_yields_a_validated_proposal() {
    let validator = EpistemicValidator::new();
    let agent = me();
    let context = ValidationContext {
        now_ms: NOW,
        agent: Some(&agent),
        others: &[],
        hazards: &[],
    };
    match validator.evaluate(proposal(1.0, 1.0), &context) {
        Verdict::Pass(validated) => assert!(validated.result().accepted),
        Verdict::Fail { .. } => panic!("valid proposal failed"),
    }
    assert!(matches!(
        validator.evaluate(proposal(f64::NAN, 1.0), &context),
        Verdict::Fail { .. }
    ));
}

#[test]
fn segment_distance_matches_geometry() {
    let a = Vector3::new(0.0, 0.0, 0.0);
    let b = Vector3::new(10.0, 0.0, 0.0);
    assert_eq!(
        segment_point_distance(&a, &b, &Vector3::new(5.0, 3.0, 0.0)),
        3.0
    );
    assert_eq!(
        segment_point_distance(&a, &b, &Vector3::new(-4.0, 3.0, 0.0)),
        5.0
    );
    assert_eq!(
        segment_point_distance(&a, &a, &Vector3::new(3.0, 4.0, 0.0)),
        5.0
    );
}

fn coordinate() -> impl Strategy<Value = f64> {
    prop_oneof![
        -20_000.0f64..20_000.0,
        Just(f64::NAN),
        Just(f64::INFINITY),
        Just(f64::NEG_INFINITY),
        Just(-0.0),
    ]
}

fn action() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("MOVE".to_string()),
        Just("HOLD".to_string()),
        Just("FIRE".to_string()),
        "[A-Za-z_]{0,12}",
    ]
}

proptest! {
    #[test]
    fn verdict_is_pass_iff_no_check_fails(
        x in coordinate(), y in coordinate(), z in coordinate(),
        action in action(), priority in -5i32..15,
        age in -5_000i64..60_000, registered in any::<bool>(),
    ) {
        let validator = strict();
        let mut p = proposal(x, y);
        p.target_position.z = z;
        p.action_type = action;
        p.priority = priority;
        p.observed_at = Some(NOW - age);
        p.timestamp = NOW - age.max(0);
        let agent = me();
        let context = ValidationContext {
            now_ms: NOW,
            agent: if registered { Some(&agent) } else { None },
            others: &[],
            hazards: &[],
        };
        let result = validator.validate(&p, &context);
        let any_fail = result.checks.iter().any(|c| c.status == CheckStatus::Fail);
        prop_assert_eq!(result.accepted, !any_fail);
        prop_assert_eq!(result.accepted, result.reasons == vec!["ALL_CHECKS_PASSED".to_string()]);
        prop_assert_eq!(result.checks.len(), CheckId::ALL.len());
        if !(x.is_finite() && y.is_finite() && z.is_finite()) {
            prop_assert!(!result.accepted, "non-finite target accepted");
        }
        if !registered {
            prop_assert!(!result.accepted, "unregistered agent accepted");
        }
        if !["MOVE", "PATROL", "INSPECT", "STANDBY", "HOLD"].contains(&p.action_type.as_str()) {
            prop_assert!(!result.accepted, "unknown action accepted");
        }
        if age > 30_000 {
            prop_assert!(!result.accepted, "stale observation accepted");
        }
        prop_assert!(serde_json::to_string(&result).is_ok());
    }

    #[test]
    fn validation_is_a_pure_function(x in -100.0f64..100.0, y in -100.0f64..100.0) {
        let validator = strict();
        let p = proposal(x, y);
        let first = run(&validator, &p, &[], &[]);
        let second = run(&validator, &p, &[], &[]);
        prop_assert_eq!(first, second);
    }

    #[test]
    fn accepted_targets_never_lie_inside_a_hazard(
        x in -60.0f64..60.0, y in -60.0f64..60.0,
        cx in -40.0f64..40.0, cy in -40.0f64..40.0, r in 1.0f64..20.0,
    ) {
        let zone = HazardZone { id: "z".into(), center: Vector3::new(cx, cy, 0.0), radius: r };
        prop_assume!(zone.center.distance(&Vector3::ZERO) >= r);
        let validator = EpistemicValidator::with_config(ValidatorConfig { max_step: 1_000.0, ..ValidatorConfig::default() }).unwrap();
        let result = run(&validator, &proposal(x, y), std::slice::from_ref(&zone), &[]);
        if result.accepted {
            prop_assert!(zone.center.distance(&Vector3::new(x, y, 0.0)) >= r);
            prop_assert!(segment_point_distance(&Vector3::ZERO, &Vector3::new(x, y, 0.0), &zone.center) >= r);
        }
    }
}
