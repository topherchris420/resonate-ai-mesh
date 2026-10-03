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

/// How a provider produces its answers. Recorded in provenance and used to
/// decide whether a model was involved and whether replay must substitute it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    /// A pure function of the evidence package, defined in this repository.
    Deterministic,
    /// Returns envelopes captured in an earlier run.
    Recorded,
    /// A probabilistic model reached over the network.
    RemoteModel,
    /// No judgment is produced.
    Disabled,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ProviderDescriptor {
    pub name: String,
    pub model: String,
    pub kind: ProviderKind,
    /// True when evaluating may open a network connection.
    pub networked: bool,
}

impl ProviderDescriptor {
    pub fn deterministic(name: &str, model: &str) -> Self {
        Self {
            name: name.to_string(),
            model: model.to_string(),
            kind: ProviderKind::Deterministic,
            networked: false,
        }
    }

    /// True when a probabilistic model produced the answers.
    pub fn model_involved(&self) -> bool {
        matches!(self.kind, ProviderKind::RemoteModel)
    }
}

/// A judgment provider evaluates an immutable evidence package and returns data.
/// It has no write handle to authoritative state, the event bus, or actuators.
#[async_trait]
pub trait JudgmentProvider: Send + Sync {
    async fn evaluate(&self, request: &JudgmentRequest) -> JudgmentEnvelope;

    fn descriptor(&self) -> ProviderDescriptor {
        ProviderDescriptor::deterministic("unnamed", "unknown")
    }
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

    fn descriptor(&self) -> ProviderDescriptor {
        ProviderDescriptor::deterministic("deterministic_mock", "deterministic-mock")
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

    fn descriptor(&self) -> ProviderDescriptor {
        ProviderDescriptor {
            name: "disabled".to_string(),
            model: "none".to_string(),
            kind: ProviderKind::Disabled,
            networked: false,
        }
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

    fn descriptor(&self) -> ProviderDescriptor {
        ProviderDescriptor::deterministic(&self.provider_name, "none")
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
        let missing = |reason: &str| {
            let mut envelope = draft_envelope(
                request,
                "recorded",
                "none",
                ProviderStatus::MissingFields,
                Vec::new(),
            );
            envelope.evaluation_mode = EvaluationMode::RecordedJudgment;
            envelope.disposition = Disposition::Unavailable;
            envelope.reason_codes = vec![reason.to_string()];
            envelope
        };
        match self.by_proposal.get(&request.proposal_id) {
            // A recording answers the evidence it was given. Different evidence
            // (a different state hash) gets no recorded answer.
            Some(recorded) if recorded.state_hash != request.prepared.state_hash => {
                missing("RECORDED_JUDGMENT_STATE_MISMATCH")
            }
            Some(recorded) => {
                let mut copy = recorded.clone();
                copy.evaluation_mode = EvaluationMode::RecordedJudgment;
                copy.correlation_id = request.correlation_id.clone();
                copy.causation_id = request.causation_id.clone();
                copy.proposal_id = request.proposal_id.clone();
                copy
            }
            None => missing("RECORDED_JUDGMENT_MISSING"),
        }
    }

    fn descriptor(&self) -> ProviderDescriptor {
        ProviderDescriptor {
            name: "recorded".to_string(),
            model: "recorded".to_string(),
            kind: ProviderKind::Recorded,
            networked: false,
        }
    }
}

/// Deterministic stand-in for a judge that responds to evidence quality.
///
/// This is not a model and makes no claim to approximate one. It exists so
/// experiments can measure what a bounded judgment stage does to a run (extra
/// withholds, human reviews, disagreement with deterministic validation)
/// without network access. Rules (`evidence-heuristic-v1`):
///
/// * staleness `s = clamp((1 - deterministic_confidence) / 0.5, 0, 1)`
/// * signal quality `q` = operator `signal_quality`, or 1.0 when absent
/// * evidence score `= clamp(3 - 2s - (1 - q), 0, 3)`
/// * support: `supported` if score >= 2, `mixed` if score >= 1, else `insufficient_evidence`
/// * Choice/Score confidence `= 0.95 - 0.5s`
/// * contradiction noul `= 0.9` if deterministic contradictions exist, else `0.05`
/// * scope noul `= 0.1 + 0.6s`
/// * human-review noul `= 0.1 + 0.5·[priority >= 5] + 0.3s`
pub struct EvidenceHeuristicJudge;

pub const EVIDENCE_HEURISTIC_MODEL: &str = "evidence-heuristic-v1";

#[async_trait]
impl JudgmentProvider for EvidenceHeuristicJudge {
    async fn evaluate(&self, request: &JudgmentRequest) -> JudgmentEnvelope {
        draft_envelope(
            request,
            "deterministic_mock",
            EVIDENCE_HEURISTIC_MODEL,
            ProviderStatus::Ok,
            evidence_heuristic_answers(&request.prepared),
        )
    }

    fn descriptor(&self) -> ProviderDescriptor {
        ProviderDescriptor::deterministic("deterministic_mock", EVIDENCE_HEURISTIC_MODEL)
    }
}

pub fn evidence_heuristic_answers(prepared: &PreparedJudgment) -> Vec<JudgmentAnswer> {
    let state = &prepared.state;
    let staleness = ((1.0 - state.validation.deterministic_confidence) / 0.5).clamp(0.0, 1.0);
    let quality = state.operator.signal_quality.unwrap_or(1.0).clamp(0.0, 1.0);
    let score = round2((3.0 - 2.0 * staleness - (1.0 - quality)).clamp(0.0, 3.0));
    let selected = if score >= 2.0 {
        SUPPORT_SUPPORTED
    } else if score >= 1.0 {
        SUPPORT_MIXED
    } else {
        SUPPORT_INSUFFICIENT
    };
    let confidence = round2(0.95 - 0.5 * staleness);
    let contradiction = if state.validation.contradictions.is_empty() {
        0.05
    } else {
        0.9
    };
    let scope = round2(0.1 + 0.6 * staleness);
    let high_priority = if state.proposal.priority >= 5 {
        0.5
    } else {
        0.0
    };
    let human = round2(0.1 + high_priority + 0.3 * staleness);
    let top = round2(0.7 + 0.2 * (1.0 - staleness));
    let rest = round2((1.0 - top) / 3.0);
    answers_with(
        selected,
        top,
        rest,
        Some(confidence),
        contradiction,
        scope,
        human,
        score,
        Some(confidence),
    )
}

/// Deterministic judge that disagrees with every third proposal (by a stable
/// hash of its id) with high confidence. Used to measure how the kernel
/// handles judgment that contradicts deterministic validation.
pub struct ContrarianJudge;

pub const CONTRARIAN_MODEL: &str = "contrarian-v1";

#[async_trait]
impl JudgmentProvider for ContrarianJudge {
    async fn evaluate(&self, request: &JudgmentRequest) -> JudgmentEnvelope {
        let disagree = stable_bucket(&request.proposal_id, 3) == 0;
        let answers = if disagree {
            answers_with(
                SUPPORT_UNSUPPORTED,
                0.9,
                0.03,
                Some(0.9),
                0.2,
                0.2,
                0.2,
                2.5,
                Some(0.9),
            )
        } else {
            supported_answers(0.88, 0.86)
        };
        draft_envelope(
            request,
            "deterministic_mock",
            CONTRARIAN_MODEL,
            ProviderStatus::Ok,
            answers,
        )
    }

    fn descriptor(&self) -> ProviderDescriptor {
        ProviderDescriptor::deterministic("deterministic_mock", CONTRARIAN_MODEL)
    }
}

/// FNV-1a over the id, reduced modulo `buckets`. Stable across platforms and releases.
pub fn stable_bucket(id: &str, buckets: u64) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in id.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash % buckets.max(1)
}

fn round2(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
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
    answers_with(
        selected,
        0.82,
        0.06,
        choice_confidence,
        contradiction,
        scope,
        human,
        evidence_score,
        evidence_confidence,
    )
}

#[allow(clippy::too_many_arguments)]
fn answers_with(
    selected: &str,
    selected_probability: f64,
    other_probability: f64,
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
            if option == selected {
                selected_probability
            } else {
                other_probability
            },
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
    async fn recorded_judgment_for_different_evidence_is_not_reused() {
        let mut recorded = draft_envelope(
            &request(),
            "typesafe",
            "jev-latest",
            ProviderStatus::Ok,
            scripted_answers(MockScenario::Supported).1,
        );
        recorded.state_hash = "sha256:other-evidence".into();
        let provider = RecordedJudgmentProvider::new(vec![recorded]);
        let replayed = provider.evaluate(&request()).await;
        assert_eq!(replayed.disposition, Disposition::Unavailable);
        assert_eq!(
            replayed.reason_codes,
            vec!["RECORDED_JUDGMENT_STATE_MISMATCH"]
        );
        assert!(replayed.answers.is_empty());
    }

    fn request_with_confidence(confidence: f64, priority: i32) -> JudgmentRequest {
        let mut req = request();
        let case = JudgmentCase {
            proposal_id: "prop_heuristic".into(),
            agent_id: "agent_alpha".into(),
            action_type: "MOVE".into(),
            target: [1.0, 2.0, 0.0],
            priority,
            source_observation: "obs_001".into(),
            observation_simulated: true,
            validation_feasibility: 1.0,
            validation_accepted: true,
            constraints_checked: vec!["coordinate_bounds".into()],
            contradictions: vec![],
            deterministic_confidence: confidence,
            validation_provenance: "pordenone.validator.v2:obs_001".into(),
            adaptive: None,
            operator_raw: serde_json::json!({"is_simulated": true, "operator_load_index": 0.2}),
            spatial_coordinate_system: "local_sim".into(),
            within_declared_bounds: true,
            correlation_id: "corr".into(),
            causation_id: "val".into(),
            simulation_status: "SIMULATED".into(),
            source_event_ids: vec![],
        };
        req.prepared = prepare_case(&case, &StateLimits::default()).unwrap();
        req.proposal_id = "prop_heuristic".into();
        req
    }

    #[tokio::test]
    async fn evidence_heuristic_passes_fresh_evidence_and_revises_stale_evidence() {
        let policy = JudgmentPolicy::default();
        let judge = EvidenceHeuristicJudge;
        let fresh = judge.evaluate(&request_with_confidence(1.0, 1)).await;
        assert_eq!(policy.evaluate(&fresh).0, Disposition::Pass);
        let stale = judge.evaluate(&request_with_confidence(0.6, 1)).await;
        assert_ne!(policy.evaluate(&stale).0, Disposition::Pass);
        let urgent = judge.evaluate(&request_with_confidence(1.0, 6)).await;
        assert_eq!(policy.evaluate(&urgent).0, Disposition::HumanReview);
        assert_eq!(judge.descriptor().kind, ProviderKind::Deterministic);
        assert!(!judge.descriptor().networked);
        for answer in &fresh.answers {
            if !answer.probabilities.is_empty() {
                let sum: f64 = answer.probabilities.values().sum();
                assert!((0.95..=1.05).contains(&sum), "{sum}");
            }
        }
    }

    #[tokio::test]
    async fn contrarian_disagrees_with_a_stable_subset() {
        let judge = ContrarianJudge;
        let policy = JudgmentPolicy::default();
        let mut disagreements = 0;
        for index in 0..30 {
            let mut req = request();
            req.proposal_id = format!("prop-{index:04}");
            let envelope = judge.evaluate(&req).await;
            let again = judge.evaluate(&req).await;
            assert_eq!(envelope.answers, again.answers);
            if policy.evaluate(&envelope).0 != Disposition::Pass {
                disagreements += 1;
            }
        }
        assert!(disagreements > 0 && disagreements < 30);
        assert_eq!(stable_bucket("abc", 7), stable_bucket("abc", 7));
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
