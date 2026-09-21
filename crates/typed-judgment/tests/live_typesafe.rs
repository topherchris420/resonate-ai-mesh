use std::time::Duration;
use typed_judgment::{
    prepare_case, HttpTypeSafeTransport, JudgmentCase, JudgmentProvider, JudgmentRequest,
    ProviderStatus, StateLimits, TypeSafeJudgmentProvider,
};

/// Manual live check. Not part of CI.
///
/// ```text
/// TYPESAFE_API_KEY=... cargo test -p typed-judgment --test live_typesafe -- --ignored
/// ```
#[tokio::test]
#[ignore]
async fn live_typesafe_smoke() {
    let api_key = std::env::var("TYPESAFE_API_KEY").expect("TYPESAFE_API_KEY required");
    let transport =
        HttpTypeSafeTransport::new("https://api.typesafe.ai", api_key, Duration::from_secs(30))
            .expect("transport");
    let provider = TypeSafeJudgmentProvider::new("jev-latest", transport);
    let case = JudgmentCase {
        proposal_id: "prop_live".into(),
        agent_id: "agent_alpha".into(),
        action_type: "PATROL".into(),
        target: [12.0, 4.0, 0.0],
        priority: 1,
        source_observation: "obs_live".into(),
        observation_simulated: true,
        validation_feasibility: 1.0,
        validation_accepted: true,
        constraints_checked: vec![
            "coordinate_bounds".into(),
            "action_allowlist".into(),
            "observation_freshness".into(),
        ],
        contradictions: vec![],
        deterministic_confidence: 0.95,
        validation_provenance: "Validator:epistemic-v1:obs_live".into(),
        adaptive: None,
        operator_raw: serde_json::json!({
            "cognitive_load": 0.31,
            "signal_quality": 0.84,
            "state_confidence": 0.9,
            "is_simulated": true
        }),
        spatial_coordinate_system: "local_sim".into(),
        within_declared_bounds: true,
        correlation_id: "corr_live".into(),
        causation_id: "val_live".into(),
        simulation_status: "SIMULATED".into(),
        source_event_ids: vec!["evt_live".into()],
    };
    let request = JudgmentRequest {
        prepared: prepare_case(&case, &StateLimits::default()).expect("state"),
        proposal_id: case.proposal_id.clone(),
        correlation_id: case.correlation_id.clone(),
        causation_id: case.causation_id,
    };
    let envelope = provider.evaluate(&request).await;
    assert_eq!(envelope.provider_status, ProviderStatus::Ok);
    assert!(envelope.provider_model_version.starts_with("jev-"));
    assert_eq!(envelope.answers.len(), 5);
    let contradiction = envelope
        .answers
        .iter()
        .find(|answer| answer.question_id == "contradiction_present")
        .expect("noul");
    assert!(contradiction.confidence.is_none());
    assert!(contradiction.noul.is_some());
}
