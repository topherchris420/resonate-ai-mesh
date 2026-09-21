use crate::envelope::{
    new_judgment_id, now_ms, Disposition, EvaluationMode, JudgmentAnswer, JudgmentEnvelope,
    ProviderStatus, ENVELOPE_SCHEMA_VERSION,
};
use crate::questions::{
    proposal_question_set, QUESTION_SET_VERSION, Q_CONTRADICTION, Q_EVIDENCE_QUALITY,
    Q_HUMAN_REVIEW, Q_PROPOSAL_SUPPORT, Q_SCOPE, SUPPORT_INSUFFICIENT, SUPPORT_MIXED,
    SUPPORT_SUPPORTED, SUPPORT_UNSUPPORTED,
};
use crate::state::{PreparedJudgment, STATE_SCHEMA_VERSION};
use async_trait::async_trait;
use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct JudgmentRequest {
    pub prepared: PreparedJudgment,
    pub proposal_id: String,
    pub correlation_id: String,
    pub causation_id: String,
}

/// A judgment provider evaluates an immutable evidence package and returns data.
/// It has no write handle to authoritative state, the event bus, or actuators.
#[async_trait]
pub trait JudgmentProvider: Send + Sync {
    async fn evaluate(&self, request: &JudgmentRequest) -> JudgmentEnvelope;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MockScenario {
    Supported,
    Unsupported,
    InsufficientEvidence,
    Mixed,
    ScopeViolation,
    Contradiction,
    HumanReview,
    LowConfidence,
    MissingConfidence,
    UnsupportedLowConfidence,
    Timeout,
    Malformed,
    Unauthorized,
    Disabled,
}

pub struct DeterministicMockJudgmentProvider {
    pub scenario: MockScenario,
}

impl DeterministicMockJudgmentProvider {
    pub fn supported() -> Self {
        Self {
            scenario: MockScenario::Supported,
        }
    }
}

#[async_trait]
impl JudgmentProvider for DeterministicMockJudgmentProvider {
    async fn evaluate(&self, request: &JudgmentRequest) -> JudgmentEnvelope {
        let (status, answers) = scripted_answers(self.scenario);
        draft_envelope(
            request,
            "deterministic_mock",
            "deterministic-mock",
            status,
            answers,
        )
    }
}

pub struct DisabledJudgmentProvider;

#[async_trait]
impl JudgmentProvider for DisabledJudgmentProvider {
    async fn evaluate(&self, request: &JudgmentRequest) -> JudgmentEnvelope {
        let mut envelope = draft_envelope(
            request,
            "disabled",
            "none",
            ProviderStatus::Disabled,
            Vec::new(),
        );
        envelope.evaluation_mode = EvaluationMode::Disabled;
        envelope.provider_model_version = "none".to_string();
        envelope
    }
}

pub struct StaticStatusProvider {
    pub status: ProviderStatus,
    pub provider_name: String,
}

impl StaticStatusProvider {
    pub fn new(status: ProviderStatus) -> Self {
        Self {
            status,
            provider_name: "unconfigured".to_string(),
        }
    }
}

#[async_trait]
impl JudgmentProvider for StaticStatusProvider {
    async fn evaluate(&self, request: &JudgmentRequest) -> JudgmentEnvelope {
        draft_envelope(
            request,
            &self.provider_name,
            "none",
            self.status,
            Vec::new(),
        )
    }
}

pub struct RecordedJudgmentProvider {
    by_proposal: HashMap<String, JudgmentEnvelope>,
    network_calls: Arc<AtomicUsize>,
}

impl RecordedJudgmentProvider {
    pub fn new(envelopes: Vec<JudgmentEnvelope>) -> Self {
        let mut by_proposal = HashMap::new();
        for envelope in envelopes {
            by_proposal.insert(envelope.proposal_id.clone(), envelope);
        }
        Self {
            by_proposal,
            network_calls: Arc::new(AtomicUsize::new(0)),
        }
    }

    pub fn network_call_count(&self) -> usize {
        self.network_calls.load(Ordering::SeqCst)
    }

    pub fn stored(&self, proposal_id: &str) -> Option<&JudgmentEnvelope> {
        self.by_proposal.get(proposal_id)
    }
}

#[async_trait]
impl JudgmentProvider for RecordedJudgmentProvider {
    async fn evaluate(&self, request: &JudgmentRequest) -> JudgmentEnvelope {
        match self.by_proposal.get(&request.proposal_id) {
            Some(recorded) => {
                let mut copy = recorded.clone();
                copy.evaluation_mode = EvaluationMode::RecordedJudgment;
                copy.correlation_id = request.correlation_id.clone();
                copy.causation_id = request.causation_id.clone();
                copy.proposal_id = request.proposal_id.clone();
                copy
            }
            None => {
                let mut envelope = draft_envelope(
                    request,
                    "recorded",
                    "none",
                    ProviderStatus::MissingFields,
                    Vec::new(),
                );
                envelope.evaluation_mode = EvaluationMode::RecordedJudgment;
                envelope.disposition = Disposition::Unavailable;
                envelope.reason_codes = vec!["RECORDED_JUDGMENT_MISSING".to_string()];
                envelope
            }
        }
    }
}

pub fn scripted_answers(scenario: MockScenario) -> (ProviderStatus, Vec<JudgmentAnswer>) {
    match scenario {
        MockScenario::Supported => (ProviderStatus::Ok, supported_answers(0.88, 0.86)),
        MockScenario::Unsupported => (
            ProviderStatus::Ok,
            with_support(
                SUPPORT_UNSUPPORTED,
                Some(0.84),
                0.07,
                0.11,
                0.16,
                2.4,
                Some(0.86),
            ),
        ),
        MockScenario::InsufficientEvidence => (
            ProviderStatus::Ok,
            with_support(
                SUPPORT_INSUFFICIENT,
                Some(0.80),
                0.07,
                0.11,
                0.16,
                0.4,
                Some(0.83),
            ),
        ),
        MockScenario::Mixed => (
            ProviderStatus::Ok,
            with_support(SUPPORT_MIXED, Some(0.77), 0.07, 0.11, 0.16, 2.2, Some(0.80)),
        ),
        MockScenario::ScopeViolation => (
            ProviderStatus::Ok,
            with_support(
                SUPPORT_SUPPORTED,
                Some(0.90),
                0.07,
                0.82,
                0.16,
                2.5,
                Some(0.87),
            ),
        ),
        MockScenario::Contradiction => (
            ProviderStatus::Ok,
            with_support(
                SUPPORT_SUPPORTED,
                Some(0.90),
                0.91,
                0.11,
                0.16,
                2.5,
                Some(0.87),
            ),
        ),
        MockScenario::HumanReview => (
            ProviderStatus::Ok,
            with_support(
                SUPPORT_SUPPORTED,
                Some(0.90),
                0.07,
                0.11,
                0.83,
                2.5,
                Some(0.87),
            ),
        ),
        MockScenario::LowConfidence => (
            ProviderStatus::Ok,
            with_support(
                SUPPORT_SUPPORTED,
                Some(0.42),
                0.07,
                0.11,
                0.16,
                2.4,
                Some(0.86),
            ),
        ),
        MockScenario::MissingConfidence => (
            ProviderStatus::Ok,
            with_support(SUPPORT_SUPPORTED, None, 0.07, 0.11, 0.16, 2.4, Some(0.86)),
        ),
        MockScenario::UnsupportedLowConfidence => (
            ProviderStatus::Ok,
            with_support(
                SUPPORT_UNSUPPORTED,
                Some(0.40),
                0.07,
                0.11,
                0.16,
                2.4,
                Some(0.86),
            ),
        ),
        MockScenario::Timeout => (ProviderStatus::Timeout, Vec::new()),
        MockScenario::Malformed => (ProviderStatus::MalformedResponse, Vec::new()),
        MockScenario::Unauthorized => (ProviderStatus::Unauthorized, Vec::new()),
        MockScenario::Disabled => (ProviderStatus::Disabled, Vec::new()),
    }
}

fn supported_answers(choice_confidence: f64, score_confidence: f64) -> Vec<JudgmentAnswer> {
    with_support(
        SUPPORT_SUPPORTED,
        Some(choice_confidence),
        0.07,
        0.11,
        0.16,
        2.4,
        Some(score_confidence),
    )
}

fn with_support(
    selected: &str,
    choice_confidence: Option<f64>,
    contradiction: f64,
    scope: f64,
    human: f64,
    evidence_score: f64,
    evidence_confidence: Option<f64>,
) -> Vec<JudgmentAnswer> {
    let mut probabilities = BTreeMap::new();
    for option in [
        SUPPORT_SUPPORTED,
        SUPPORT_MIXED,
        SUPPORT_UNSUPPORTED,
        SUPPORT_INSUFFICIENT,
    ] {
        probabilities.insert(
            option.to_string(),
            if option == selected { 0.82 } else { 0.06 },
        );
    }
    let mut legend = BTreeMap::new();
    legend.insert("0".to_string(), "insufficient evidence".to_string());
    legend.insert("1".to_string(), "weak or indirect".to_string());
    legend.insert("2".to_string(), "adequate but limited".to_string());
    legend.insert("3".to_string(), "strong and directly relevant".to_string());
    let mut score_probabilities = BTreeMap::new();
    score_probabilities.insert("0".to_string(), 0.05);
    score_probabilities.insert("1".to_string(), 0.10);
    score_probabilities.insert("2".to_string(), 0.50);
    score_probabilities.insert("3".to_string(), 0.35);
    vec![
        JudgmentAnswer::choice(
            Q_PROPOSAL_SUPPORT,
            selected,
            probabilities,
            choice_confidence,
        ),
        JudgmentAnswer::score(
            Q_EVIDENCE_QUALITY,
            evidence_score,
            score_probabilities,
            legend,
            evidence_confidence,
        ),
        JudgmentAnswer::noul(Q_CONTRADICTION, contradiction),
        JudgmentAnswer::noul(Q_SCOPE, scope),
        JudgmentAnswer::noul(Q_HUMAN_REVIEW, human),
    ]
}

pub fn draft_envelope(
    request: &JudgmentRequest,
    provider: &str,
    model: &str,
    status: ProviderStatus,
    answers: Vec<JudgmentAnswer>,
) -> JudgmentEnvelope {
    let requested_at = now_ms();
    JudgmentEnvelope {
        schema_version: ENVELOPE_SCHEMA_VERSION.to_string(),
        judgment_id: new_judgment_id(),
        provider: provider.to_string(),
        model: model.to_string(),
        provider_model_version: model.to_string(),
        question_set_version: QUESTION_SET_VERSION.to_string(),
        state_hash: request.prepared.state_hash.clone(),
        state_schema_version: STATE_SCHEMA_VERSION.to_string(),
        truncated: request.prepared.truncated,
        proposal_id: request.proposal_id.clone(),
        correlation_id: request.correlation_id.clone(),
        causation_id: request.causation_id.clone(),
        answers,
        disposition: Disposition::Pending,
        reason_codes: Vec::new(),
        policy_version: String::new(),
        requested_at,
        completed_at: requested_at,
        latency_ms: 0,
        provider_status: status,
        evaluation_mode: EvaluationMode::Live,
        simulation_label: request.prepared.simulation_label.clone(),
        provider_request_id: None,
        input_tokens: None,
        output_tokens: None,
    }
}

pub fn local_failure_envelope(
    proposal_id: &str,
    correlation_id: &str,
    causation_id: &str,
    status: ProviderStatus,
) -> JudgmentEnvelope {
    let requested_at = now_ms();
    JudgmentEnvelope {
        schema_version: ENVELOPE_SCHEMA_VERSION.to_string(),
        judgment_id: new_judgment_id(),
        provider: "kernel".to_string(),
        model: "none".to_string(),
        provider_model_version: "none".to_string(),
        question_set_version: proposal_question_set().version,
        state_hash: "sha256:unavailable".to_string(),
        state_schema_version: STATE_SCHEMA_VERSION.to_string(),
        truncated: false,
        proposal_id: proposal_id.to_string(),
        correlation_id: correlation_id.to_string(),
        causation_id: causation_id.to_string(),
        answers: Vec::new(),
        disposition: Disposition::Pending,
        reason_codes: Vec::new(),
        policy_version: String::new(),
        requested_at,
        completed_at: requested_at,
        latency_ms: 0,
        provider_status: status,
        evaluation_mode: EvaluationMode::Live,
        simulation_label: "SIMULATED".to_string(),
        provider_request_id: None,
        input_tokens: None,
        output_tokens: None,
    }
}

pub fn commit_allowed(disposition: Disposition) -> bool {
    matches!(disposition, Disposition::Pass | Disposition::Skipped)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::JudgmentPolicy;
    use crate::state::{prepare_case, JudgmentCase, StateLimits};

    fn request() -> JudgmentRequest {
        let case = JudgmentCase {
            proposal_id: "prop_recorded".into(),
            agent_id: "agent_alpha".into(),
            action_type: "MOVE".into(),
            target: [1.0, 2.0, 0.0],
            priority: 1,
            source_observation: "obs_001".into(),
            observation_simulated: true,
            validation_feasibility: 1.0,
            validation_accepted: true,
            constraints_checked: vec!["coordinate_bounds".into()],
            contradictions: vec![],
            deterministic_confidence: 0.95,
            validation_provenance: "Validator:epistemic-v1:obs_001".into(),
            adaptive: None,
            operator_raw: serde_json::json!({"is_simulated": true, "cognitive_load": 0.2}),
            spatial_coordinate_system: "local_sim".into(),
            within_declared_bounds: true,
            correlation_id: "corr_recorded".into(),
            causation_id: "val_recorded".into(),
            simulation_status: "SIMULATED".into(),
            source_event_ids: vec![],
        };
        let prepared = prepare_case(&case, &StateLimits::default()).unwrap();
        JudgmentRequest {
            prepared,
            proposal_id: "prop_recorded".into(),
            correlation_id: "corr_recorded".into(),
            causation_id: "val_new".into(),
        }
    }

    #[tokio::test]
    async fn replay_uses_recorded_judgment_and_makes_no_network_call() {
        let mut recorded = draft_envelope(
            &request(),
            "typesafe",
            "jev-latest",
            ProviderStatus::Ok,
            scripted_answers(MockScenario::Supported).1,
        );
        recorded.provider_model_version = "jev-1.13.0".into();
        recorded.disposition = Disposition::Pass;
        recorded.reason_codes = vec!["ELIGIBLE_PASS".into()];
        recorded.evaluation_mode = EvaluationMode::Live;
        recorded.causation_id = "val_original".into();
        let original_choice = recorded.answers[0].choice.clone();
        let provider = RecordedJudgmentProvider::new(vec![recorded]);
        let replayed = provider.evaluate(&request()).await;
        assert_eq!(provider.network_call_count(), 0);
        assert_eq!(replayed.evaluation_mode, EvaluationMode::RecordedJudgment);
        assert_eq!(replayed.disposition, Disposition::Pass);
        assert_eq!(replayed.provider_model_version, "jev-1.13.0");
        assert_eq!(replayed.answers[0].choice, original_choice);
        assert_eq!(replayed.causation_id, "val_new");
        let stored = provider.stored("prop_recorded").unwrap();
        assert_eq!(stored.evaluation_mode, EvaluationMode::Live);
        assert_eq!(stored.causation_id, "val_original");
        assert_eq!(stored.disposition, Disposition::Pass);
    }

    #[tokio::test]
    async fn mock_answers_are_stable() {
        let provider = DeterministicMockJudgmentProvider::supported();
        let first = provider.evaluate(&request()).await;
        let second = provider.evaluate(&request()).await;
        assert_eq!(first.answers, second.answers);
        assert_eq!(first.provider_status, ProviderStatus::Ok);
        let noul = first
            .answers
            .iter()
            .find(|answer| answer.question_id == Q_CONTRADICTION)
            .unwrap();
        assert!(noul.confidence.is_none());
        JudgmentPolicy::default().evaluate(&first);
    }
}
