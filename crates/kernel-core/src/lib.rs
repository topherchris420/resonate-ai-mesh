use epistemic_validator::{ActionProposal, EpistemicValidator, ValidationResult};
use event_bus::{EventBus, EventEnvelope, EVENT_SCHEMA_V1_1};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use spatial_state::{SpatialEntity, SpatialIndex, Vector3 as SpatialVector3};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info, warn};
use typed_judgment::{
    commit_allowed, local_failure_envelope, prepare_case, AdaptiveSnapshot,
    DisabledJudgmentProvider, Disposition, EvaluationMode, JudgmentCase, JudgmentEnvelope,
    JudgmentPolicy, JudgmentProvider, JudgmentRequest, PrepareError, ProviderStatus,
    RecordedJudgmentProvider, StateLimits, QUESTION_SET_VERSION,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AgentStatus {
    Idle,
    Busy,
    Executing,
    Failed,
    Completed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthoritativeAgentState {
    pub agent_id: String,
    pub status: AgentStatus,
    pub capabilities: Vec<String>,
    pub current_task: Option<String>,
    pub priority: i32,
    pub position: SpatialVector3,
    pub velocity: SpatialVector3,
    pub confidence: f64,
    pub last_updated: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum CognitiveLoadPolicyLevel {
    Normal,
    Elevated,
    High,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyConfiguration {
    pub level: CognitiveLoadPolicyLevel,
    pub update_frequency_ms: u64,
    pub visual_density_scale: f64,
    pub aggregate_alerts: bool,
    pub defer_background_tasks: bool,
}

impl Default for PolicyConfiguration {
    fn default() -> Self {
        Self {
            level: CognitiveLoadPolicyLevel::Normal,
            update_frequency_ms: 100,
            visual_density_scale: 1.0,
            aggregate_alerts: false,
            defer_background_tasks: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PipelineOutcome {
    pub validation: ValidationResult,
    pub judgment: Option<JudgmentEnvelope>,
    pub committed: bool,
}

#[derive(Debug, Clone)]
pub struct HumanReviewDecision {
    pub proposal_id: String,
    pub approve: bool,
    pub operator_ref: String,
    pub note: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum JudgmentMode {
    Disabled,
    Live,
    Replay,
}

struct PendingReview {
    proposal: ActionProposal,
    validation: ValidationResult,
    judgment: JudgmentEnvelope,
    validation_event_id: String,
    judgment_event_id: String,
}

pub struct KernelEngine {
    event_bus: EventBus,
    validator: EpistemicValidator,
    agent_states: Arc<RwLock<HashMap<String, AuthoritativeAgentState>>>,
    spatial_index: Arc<RwLock<SpatialIndex>>,
    policy_config: Arc<RwLock<PolicyConfiguration>>,
    judgment_provider: Arc<dyn JudgmentProvider>,
    judgment_policy: JudgmentPolicy,
    judgment_mode: JudgmentMode,
    state_limits: StateLimits,
    operator_raw: Arc<RwLock<Value>>,
    adaptive: Arc<RwLock<Option<AdaptiveSnapshot>>>,
    simulation_status: Arc<RwLock<String>>,
    pending_reviews: Arc<RwLock<HashMap<String, PendingReview>>>,
}

impl KernelEngine {
    pub fn new(event_bus: EventBus) -> Self {
        Self::assemble(
            event_bus,
            Arc::new(DisabledJudgmentProvider),
            JudgmentPolicy::default(),
            JudgmentMode::Disabled,
            StateLimits::default(),
        )
    }

    pub fn with_judgment(
        event_bus: EventBus,
        provider: Arc<dyn JudgmentProvider>,
        policy: JudgmentPolicy,
    ) -> Self {
        Self::assemble(
            event_bus,
            provider,
            policy,
            JudgmentMode::Live,
            StateLimits::default(),
        )
    }

    pub fn with_limits(
        event_bus: EventBus,
        provider: Arc<dyn JudgmentProvider>,
        policy: JudgmentPolicy,
        limits: StateLimits,
    ) -> Self {
        Self::assemble(event_bus, provider, policy, JudgmentMode::Live, limits)
    }

    pub fn replay_recorded(
        event_bus: EventBus,
        recorded: Vec<JudgmentEnvelope>,
        policy: JudgmentPolicy,
    ) -> (Self, Arc<RecordedJudgmentProvider>) {
        let provider = Arc::new(RecordedJudgmentProvider::new(recorded));
        let kernel = Self::assemble(
            event_bus,
            provider.clone(),
            policy,
            JudgmentMode::Replay,
            StateLimits::default(),
        );
        (kernel, provider)
    }

    fn assemble(
        event_bus: EventBus,
        provider: Arc<dyn JudgmentProvider>,
        policy: JudgmentPolicy,
        mode: JudgmentMode,
        limits: StateLimits,
    ) -> Self {
        Self {
            event_bus,
            validator: EpistemicValidator::new(),
            agent_states: Arc::new(RwLock::new(HashMap::new())),
            spatial_index: Arc::new(RwLock::new(SpatialIndex::new())),
            policy_config: Arc::new(RwLock::new(PolicyConfiguration::default())),
            judgment_provider: provider,
            judgment_policy: policy,
            judgment_mode: mode,
            state_limits: limits,
            operator_raw: Arc::new(RwLock::new(Value::Null)),
            adaptive: Arc::new(RwLock::new(None)),
            simulation_status: Arc::new(RwLock::new("SIMULATED".to_string())),
            pending_reviews: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn register_agent(&self, state: AuthoritativeAgentState) {
        let mut map = self.agent_states.write().await;
        map.insert(state.agent_id.clone(), state);
    }

    pub async fn get_agent(&self, agent_id: &str) -> Option<AuthoritativeAgentState> {
        let map = self.agent_states.read().await;
        map.get(agent_id).cloned()
    }

    pub async fn set_operator_telemetry(&self, raw: Value) {
        *self.operator_raw.write().await = raw;
    }

    pub async fn set_adaptive_snapshot(&self, snapshot: AdaptiveSnapshot) {
        *self.adaptive.write().await = Some(snapshot);
    }

    pub async fn set_simulation_status(&self, status: impl Into<String>) {
        *self.simulation_status.write().await = status.into();
    }

    pub async fn update_policy_for_cognitive_load(
        &self,
        cognitive_load: f64,
    ) -> PolicyConfiguration {
        let level = if cognitive_load < 0.4 {
            CognitiveLoadPolicyLevel::Normal
        } else if cognitive_load < 0.65 {
            CognitiveLoadPolicyLevel::Elevated
        } else if cognitive_load < 0.85 {
            CognitiveLoadPolicyLevel::High
        } else {
            CognitiveLoadPolicyLevel::Critical
        };
        let config = policy_for_level(level);
        *self.policy_config.write().await = config.clone();
        info!(
            policy_level = ?config.level,
            cognitive_load = cognitive_load,
            "Updated Kernel Policy Level"
        );
        config
    }

    pub async fn process_action_proposal(&self, proposal: ActionProposal) -> ValidationResult {
        self.process_action_proposal_detailed(proposal)
            .await
            .validation
    }

    pub async fn process_action_proposal_detailed(
        &self,
        proposal: ActionProposal,
    ) -> PipelineOutcome {
        let current_time = chrono::Utc::now().timestamp_millis();
        self.ensure_agent(&proposal, current_time).await;
        let validation = self.validator.validate(&proposal, current_time);
        let validation_event_id = self.publish(
            "validation",
            "1.0.0",
            &proposal.agent_id,
            &proposal.correlation_id,
            &proposal.proposal_id,
            &validation.provenance,
            &validation,
        );

        if !validation.accepted {
            error!(
                proposal_id = %proposal.proposal_id,
                agent_id = %proposal.agent_id,
                reasons = ?validation.reasons,
                contradictions = ?validation.contradictions,
                "Action proposal rejected by deterministic epistemic validation"
            );
            self.publish_commitment(
                &proposal,
                "rejected_deterministic",
                None,
                &["DETERMINISTIC_VALIDATION_FAILED".to_string()],
                None,
                &validation_event_id,
            );
            return PipelineOutcome {
                validation,
                judgment: None,
                committed: false,
            };
        }

        let case = self.build_case(&proposal, &validation).await;
        let prepared = match prepare_case(&case, &self.state_limits) {
            Ok(prepared) => prepared,
            Err(error) => {
                return self.finish_prepare_error(
                    proposal,
                    validation,
                    &validation_event_id,
                    error,
                );
            }
        };
        let request = JudgmentRequest {
            prepared,
            proposal_id: proposal.proposal_id.clone(),
            correlation_id: proposal.correlation_id.clone(),
            causation_id: validation_event_id.clone(),
        };
        let mut envelope = self.judgment_provider.evaluate(&request).await;
        self.finalize_envelope(&mut envelope, &request);
        let judgment_event_id = self.publish(
            "judgment",
            EVENT_SCHEMA_V1_1,
            &proposal.agent_id,
            &proposal.correlation_id,
            &validation_event_id,
            &format!(
                "Judgment:{}:{}",
                envelope.provider, envelope.question_set_version
            ),
            &envelope,
        );

        if envelope.disposition == Disposition::HumanReview {
            self.pending_reviews.write().await.insert(
                proposal.proposal_id.clone(),
                PendingReview {
                    proposal: proposal.clone(),
                    validation: validation.clone(),
                    judgment: envelope.clone(),
                    validation_event_id: validation_event_id.clone(),
                    judgment_event_id: judgment_event_id.clone(),
                },
            );
        }

        let committed = if commit_allowed(envelope.disposition) {
            self.commit_proposal(&proposal, &validation, current_time)
                .await;
            true
        } else {
            false
        };
        let outcome = if committed { "committed" } else { "withheld" };
        self.publish_commitment(
            &proposal,
            outcome,
            Some(envelope.disposition),
            &envelope.reason_codes,
            Some(&envelope.judgment_id),
            &judgment_event_id,
        );
        if committed {
            info!(
                proposal_id = %proposal.proposal_id,
                disposition = ?envelope.disposition,
                "Proposal committed after deterministic validation and judgment policy"
            );
        } else {
            warn!(
                proposal_id = %proposal.proposal_id,
                disposition = ?envelope.disposition,
                reasons = ?envelope.reason_codes,
                "Proposal withheld after judgment policy"
            );
        }
        PipelineOutcome {
            validation,
            judgment: Some(envelope),
            committed,
        }
    }

    pub async fn resolve_human_review(
        &self,
        decision: HumanReviewDecision,
    ) -> Result<PipelineOutcome, String> {
        let pending = self
            .pending_reviews
            .write()
            .await
            .remove(&decision.proposal_id)
            .ok_or_else(|| format!("no human review pending for {}", decision.proposal_id))?;
        let operator_ref = if decision.operator_ref.trim().is_empty() {
            "operator_console".to_string()
        } else {
            decision.operator_ref.clone()
        };
        let current_time = chrono::Utc::now().timestamp_millis();
        let revalidation = self.validator.validate(&pending.proposal, current_time);
        let approve = decision.approve && revalidation.accepted;
        let payload = HumanResolutionRecord {
            proposal_id: pending.proposal.proposal_id.clone(),
            decision: if decision.approve {
                "approve".to_string()
            } else {
                "reject".to_string()
            },
            operator_ref,
            note: decision.note,
            judgment_id: pending.judgment.judgment_id.clone(),
            validation_event_id: pending.validation_event_id.clone(),
            committed: approve,
            deterministic_block: decision.approve && !revalidation.accepted,
        };
        let human_event_id = self.publish(
            "human_resolution",
            EVENT_SCHEMA_V1_1,
            &pending.proposal.agent_id,
            &pending.proposal.correlation_id,
            &pending.judgment_event_id,
            "HumanReview:operator_decision",
            &payload,
        );
        if approve {
            self.commit_proposal(&pending.proposal, &revalidation, current_time)
                .await;
            self.publish_commitment(
                &pending.proposal,
                "committed",
                Some(Disposition::Pass),
                &["HUMAN_DECISION_APPROVE".to_string()],
                Some(&pending.judgment.judgment_id),
                &human_event_id,
            );
        } else {
            let reason = if payload.deterministic_block {
                "DETERMINISTIC_VALIDATION_FAILED"
            } else {
                "HUMAN_DECISION_REJECT"
            };
            self.publish_commitment(
                &pending.proposal,
                "withheld",
                Some(Disposition::HumanReview),
                &[reason.to_string()],
                Some(&pending.judgment.judgment_id),
                &human_event_id,
            );
        }
        Ok(PipelineOutcome {
            validation: if decision.approve {
                revalidation
            } else {
                pending.validation
            },
            judgment: Some(pending.judgment),
            committed: approve,
        })
    }

    fn finalize_envelope(&self, envelope: &mut JudgmentEnvelope, request: &JudgmentRequest) {
        match self.judgment_mode {
            JudgmentMode::Disabled => {
                envelope.provider_status = ProviderStatus::Disabled;
                envelope.answers.clear();
                envelope.provider = "disabled".to_string();
                envelope.model = "none".to_string();
                envelope.provider_model_version = "none".to_string();
                envelope.evaluation_mode = EvaluationMode::Disabled;
                align_live_identity(envelope, request);
                self.judgment_policy.apply(envelope);
            }
            JudgmentMode::Live => {
                if envelope.provider_status == ProviderStatus::Disabled {
                    envelope.provider_status = ProviderStatus::InvalidRequest;
                    envelope.answers.clear();
                }
                envelope.evaluation_mode = EvaluationMode::Live;
                align_live_identity(envelope, request);
                self.judgment_policy.apply(envelope);
            }
            JudgmentMode::Replay => {
                envelope.evaluation_mode = EvaluationMode::RecordedJudgment;
                envelope.correlation_id = request.correlation_id.clone();
                envelope.causation_id = request.causation_id.clone();
            }
        }
    }

    fn finish_prepare_error(
        &self,
        proposal: ActionProposal,
        validation: ValidationResult,
        validation_event_id: &str,
        error: PrepareError,
    ) -> PipelineOutcome {
        let status = match error {
            PrepareError::StateTooLarge { .. } => ProviderStatus::StateTooLarge,
            PrepareError::NonFinite | PrepareError::Serialization => {
                ProviderStatus::SerializationFailure
            }
        };
        warn!(
            proposal_id = %proposal.proposal_id,
            provider_status = status.reason_code(),
            "Judgment state was not sent to a provider"
        );
        let mut envelope = local_failure_envelope(
            &proposal.proposal_id,
            &proposal.correlation_id,
            validation_event_id,
            status,
        );
        self.judgment_policy.apply(&mut envelope);
        let judgment_event_id = self.publish(
            "judgment",
            EVENT_SCHEMA_V1_1,
            &proposal.agent_id,
            &proposal.correlation_id,
            validation_event_id,
            "Judgment:kernel:state_error",
            &envelope,
        );
        self.publish_commitment(
            &proposal,
            "withheld",
            Some(envelope.disposition),
            &envelope.reason_codes,
            Some(&envelope.judgment_id),
            &judgment_event_id,
        );
        PipelineOutcome {
            validation,
            judgment: Some(envelope),
            committed: false,
        }
    }

    async fn build_case(
        &self,
        proposal: &ActionProposal,
        validation: &ValidationResult,
    ) -> JudgmentCase {
        let simulation_status = self.simulation_status.read().await.clone();
        JudgmentCase {
            proposal_id: proposal.proposal_id.clone(),
            agent_id: proposal.agent_id.clone(),
            action_type: proposal.action_type.clone(),
            target: [
                proposal.target_position.x,
                proposal.target_position.y,
                proposal.target_position.z,
            ],
            priority: proposal.priority,
            source_observation: proposal.source_observation.clone(),
            observation_simulated: simulation_status.eq_ignore_ascii_case("SIMULATED"),
            validation_feasibility: validation.feasibility,
            validation_accepted: validation.accepted,
            constraints_checked: self.validator.checked_constraints(),
            contradictions: validation.contradictions.clone(),
            deterministic_confidence: validation.confidence,
            validation_provenance: validation.provenance.clone(),
            adaptive: self.adaptive.read().await.clone(),
            operator_raw: self.operator_raw.read().await.clone(),
            spatial_coordinate_system: "local_sim".to_string(),
            within_declared_bounds: validation.accepted,
            correlation_id: proposal.correlation_id.clone(),
            causation_id: proposal.proposal_id.clone(),
            simulation_status,
            source_event_ids: vec![proposal.proposal_id.clone()],
        }
    }

    async fn ensure_agent(&self, proposal: &ActionProposal, current_time: i64) {
        if self.get_agent(&proposal.agent_id).await.is_some() {
            return;
        }
        warn!(agent_id = %proposal.agent_id, "Proposal for unknown agent; auto-registering stub");
        self.register_agent(AuthoritativeAgentState {
            agent_id: proposal.agent_id.clone(),
            status: AgentStatus::Idle,
            capabilities: vec!["generic".to_string()],
            current_task: None,
            priority: proposal.priority,
            position: zero_vector(),
            velocity: zero_vector(),
            confidence: 0.9,
            last_updated: current_time,
        })
        .await;
    }

    async fn commit_proposal(
        &self,
        proposal: &ActionProposal,
        validation: &ValidationResult,
        current_time: i64,
    ) {
        let mut map = self.agent_states.write().await;
        if let Some(agent) = map.get_mut(&proposal.agent_id) {
            agent.status = AgentStatus::Executing;
            agent.current_task = Some(proposal.action_type.clone());
            agent.position = SpatialVector3 {
                x: proposal.target_position.x,
                y: proposal.target_position.y,
                z: proposal.target_position.z,
            };
            agent.last_updated = current_time;
        }
        drop(map);
        self.spatial_index
            .write()
            .await
            .upsert_entity(SpatialEntity {
                entity_id: proposal.agent_id.clone(),
                position: SpatialVector3 {
                    x: proposal.target_position.x,
                    y: proposal.target_position.y,
                    z: proposal.target_position.z,
                },
                orientation: zero_vector(),
                velocity: zero_vector(),
                terrain_reference: "default".into(),
                coordinate_system: "ECEF".into(),
                timestamp: current_time,
                confidence: validation.confidence,
            });
    }

    fn publish_commitment(
        &self,
        proposal: &ActionProposal,
        outcome: &str,
        disposition: Option<Disposition>,
        reason_codes: &[String],
        judgment_id: Option<&str>,
        causation_id: &str,
    ) {
        let payload = CommitmentRecord {
            proposal_id: proposal.proposal_id.clone(),
            outcome: outcome.to_string(),
            disposition,
            reason_codes: reason_codes.to_vec(),
            judgment_id: judgment_id.map(str::to_string),
        };
        self.publish(
            "commitment",
            EVENT_SCHEMA_V1_1,
            &proposal.agent_id,
            &proposal.correlation_id,
            causation_id,
            "KernelEngine:commitment",
            &payload,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn publish(
        &self,
        event_type: &str,
        schema_version: &str,
        subject_id: &str,
        correlation_id: &str,
        causation_id: &str,
        provenance: &str,
        payload: &impl Serialize,
    ) -> String {
        let event = EventEnvelope::new(
            event_type,
            "kernel_core",
            subject_id,
            correlation_id,
            causation_id,
            provenance,
            serde_json::to_string(payload)
                .unwrap_or_else(|_| r#"{"error":"payload_serialization_failed"}"#.to_string()),
        )
        .with_schema_version(schema_version);
        let event_id = event.event_id.clone();
        let _ = self.event_bus.publish(event);
        event_id
    }
}

fn align_live_identity(envelope: &mut JudgmentEnvelope, request: &JudgmentRequest) {
    envelope.proposal_id = request.proposal_id.clone();
    envelope.correlation_id = request.correlation_id.clone();
    envelope.causation_id = request.causation_id.clone();
    envelope.state_hash = request.prepared.state_hash.clone();
    envelope.truncated = request.prepared.truncated;
    envelope.simulation_label = request.prepared.simulation_label.clone();
    envelope.question_set_version = QUESTION_SET_VERSION.to_string();
    envelope.state_schema_version = typed_judgment::STATE_SCHEMA_VERSION.to_string();
}

fn policy_for_level(level: CognitiveLoadPolicyLevel) -> PolicyConfiguration {
    match level {
        CognitiveLoadPolicyLevel::Normal => PolicyConfiguration {
            level,
            update_frequency_ms: 100,
            visual_density_scale: 1.0,
            aggregate_alerts: false,
            defer_background_tasks: false,
        },
        CognitiveLoadPolicyLevel::Elevated => PolicyConfiguration {
            level,
            update_frequency_ms: 250,
            visual_density_scale: 0.8,
            aggregate_alerts: false,
            defer_background_tasks: false,
        },
        CognitiveLoadPolicyLevel::High => PolicyConfiguration {
            level,
            update_frequency_ms: 500,
            visual_density_scale: 0.5,
            aggregate_alerts: true,
            defer_background_tasks: true,
        },
        CognitiveLoadPolicyLevel::Critical => PolicyConfiguration {
            level,
            update_frequency_ms: 1000,
            visual_density_scale: 0.2,
            aggregate_alerts: true,
            defer_background_tasks: true,
        },
    }
}

fn zero_vector() -> SpatialVector3 {
    SpatialVector3 {
        x: 0.0,
        y: 0.0,
        z: 0.0,
    }
}

#[derive(Serialize)]
struct CommitmentRecord {
    proposal_id: String,
    outcome: String,
    disposition: Option<Disposition>,
    reason_codes: Vec<String>,
    judgment_id: Option<String>,
}

#[derive(Serialize)]
struct HumanResolutionRecord {
    proposal_id: String,
    decision: String,
    operator_ref: String,
    note: String,
    judgment_id: String,
    validation_event_id: String,
    committed: bool,
    deterministic_block: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use epistemic_validator::Vector3;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;
    use typed_judgment::{DeterministicMockJudgmentProvider, MockScenario};

    fn proposal(id: &str, action: &str, x: f64, correlation: &str) -> ActionProposal {
        ActionProposal {
            proposal_id: id.into(),
            agent_id: "agent_001".into(),
            action_type: action.into(),
            parameters_json: "{\"note\":\"do not send raw parameters\"}".into(),
            target_position: Vector3 {
                x,
                y: 200.0,
                z: 0.0,
            },
            priority: 1,
            timestamp: chrono::Utc::now().timestamp_millis(),
            correlation_id: correlation.into(),
            source_observation: "obs_001".into(),
        }
    }

    struct CountingProvider {
        calls: Arc<AtomicUsize>,
        seen: Arc<Mutex<String>>,
        inner: DeterministicMockJudgmentProvider,
    }

    #[async_trait]
    impl JudgmentProvider for CountingProvider {
        async fn evaluate(&self, request: &JudgmentRequest) -> JudgmentEnvelope {
            self.calls.fetch_add(1, Ordering::SeqCst);
            *self.seen.lock().expect("seen") = request.prepared.canonical_json.clone();
            self.inner.evaluate(request).await
        }
    }

    fn counting(scenario: MockScenario) -> (Arc<CountingProvider>, Arc<dyn JudgmentProvider>) {
        let provider = Arc::new(CountingProvider {
            calls: Arc::new(AtomicUsize::new(0)),
            seen: Arc::new(Mutex::new(String::new())),
            inner: DeterministicMockJudgmentProvider { scenario },
        });
        let trait_object: Arc<dyn JudgmentProvider> = provider.clone();
        (provider, trait_object)
    }

    fn drain(rx: &mut tokio::sync::broadcast::Receiver<EventEnvelope>) -> Vec<EventEnvelope> {
        let mut events = Vec::new();
        while let Ok(event) = rx.try_recv() {
            events.push(event);
        }
        events
    }

    #[tokio::test]
    async fn test_kernel_lifecycle_commit_and_rejection() {
        let bus = EventBus::new(10);
        let mut rx = bus.subscribe();
        let kernel = KernelEngine::new(bus);
        let res = kernel
            .process_action_proposal(proposal("prop_valid", "MOVE", 100.0, "corr_001"))
            .await;
        assert!(res.accepted);
        let agent = kernel.get_agent("agent_001").await.unwrap();
        assert_eq!(agent.status, AgentStatus::Executing);
        assert_eq!(agent.position.x, 100.0);
        let evt = rx.recv().await.unwrap();
        assert_eq!(evt.event_type, "validation");
        assert_eq!(evt.correlation_id, "corr_001");

        let res2 = kernel
            .process_action_proposal(proposal("prop_invalid", "MOVE", 999999.0, "corr_002"))
            .await;
        assert!(!res2.accepted);
        let agent_after = kernel.get_agent("agent_001").await.unwrap();
        assert_eq!(agent_after.position.x, 100.0);
    }

    #[tokio::test]
    async fn deterministic_rejection_does_not_call_judgment_or_move_state() {
        let bus = EventBus::new(16);
        let mut rx = bus.subscribe();
        let (counter, provider) = counting(MockScenario::Supported);
        let kernel = KernelEngine::with_judgment(bus, provider, JudgmentPolicy::default());
        kernel
            .register_agent(AuthoritativeAgentState {
                agent_id: "agent_001".into(),
                status: AgentStatus::Idle,
                capabilities: vec!["generic".into()],
                current_task: None,
                priority: 1,
                position: SpatialVector3 {
                    x: 5.0,
                    y: 0.0,
                    z: 0.0,
                },
                velocity: zero_vector(),
                confidence: 0.9,
                last_updated: 1,
            })
            .await;
        let outcome = kernel
            .process_action_proposal_detailed(proposal("prop_bad", "MOVE", 999999.0, "corr_bad"))
            .await;
        assert!(!outcome.validation.accepted);
        assert!(outcome.judgment.is_none());
        assert!(!outcome.committed);
        assert_eq!(counter.calls.load(Ordering::SeqCst), 0);
        assert_eq!(kernel.get_agent("agent_001").await.unwrap().position.x, 5.0);
        let events = drain(&mut rx);
        assert!(events.iter().any(|event| event.event_type == "validation"));
        assert!(events.iter().all(|event| event.event_type != "judgment"));
    }

    #[tokio::test]
    async fn valid_proposal_reaches_provider_and_commits_on_pass() {
        let bus = EventBus::new(16);
        let mut rx = bus.subscribe();
        let (counter, provider) = counting(MockScenario::Supported);
        let kernel = KernelEngine::with_judgment(bus, provider, JudgmentPolicy::default());
        let outcome = kernel
            .process_action_proposal_detailed(proposal("prop_ok", "PATROL", 12.0, "corr_ok"))
            .await;
        assert!(outcome.validation.accepted);
        assert_eq!(counter.calls.load(Ordering::SeqCst), 1);
        assert_eq!(outcome.judgment.unwrap().disposition, Disposition::Pass);
        assert!(outcome.committed);
        assert_eq!(
            kernel.get_agent("agent_001").await.unwrap().position.x,
            12.0
        );
        let events = drain(&mut rx);
        let validation = events
            .iter()
            .find(|event| event.event_type == "validation")
            .unwrap();
        let judgment = events
            .iter()
            .find(|event| event.event_type == "judgment")
            .unwrap();
        let commitment = events
            .iter()
            .find(|event| event.event_type == "commitment")
            .unwrap();
        assert_eq!(validation.causation_id, "prop_ok");
        assert_eq!(judgment.causation_id, validation.event_id);
        assert_eq!(commitment.causation_id, judgment.event_id);
        assert_eq!(judgment.correlation_id, "corr_ok");
        assert_eq!(judgment.schema_version, EVENT_SCHEMA_V1_1);
        assert_eq!(validation.schema_version, "1.0.0");
    }

    #[tokio::test]
    async fn revise_human_review_and_unavailable_do_not_commit() {
        for scenario in [
            MockScenario::Unsupported,
            MockScenario::HumanReview,
            MockScenario::LowConfidence,
            MockScenario::Timeout,
        ] {
            let bus = EventBus::new(16);
            let (_, provider) = counting(scenario);
            let kernel = KernelEngine::with_judgment(bus, provider, JudgmentPolicy::default());
            let outcome = kernel
                .process_action_proposal_detailed(proposal(
                    "prop_hold",
                    "INSPECT",
                    4.0,
                    "corr_hold",
                ))
                .await;
            assert!(outcome.validation.accepted);
            assert!(!outcome.committed);
            assert_eq!(kernel.get_agent("agent_001").await.unwrap().position.x, 0.0);
        }
    }

    #[tokio::test]
    async fn human_review_requires_an_explicit_decision() {
        let bus = EventBus::new(32);
        let mut rx = bus.subscribe();
        let (_, provider) = counting(MockScenario::HumanReview);
        let kernel = KernelEngine::with_judgment(bus, provider, JudgmentPolicy::default());
        let outcome = kernel
            .process_action_proposal_detailed(proposal("prop_human", "HOLD", 6.0, "corr_human"))
            .await;
        assert_eq!(
            outcome.judgment.unwrap().disposition,
            Disposition::HumanReview
        );
        assert!(!outcome.committed);
        assert!(kernel
            .resolve_human_review(HumanReviewDecision {
                proposal_id: "missing".into(),
                approve: true,
                operator_ref: "operator_console".into(),
                note: "no".into(),
            })
            .await
            .is_err());
        let rejected = kernel
            .resolve_human_review(HumanReviewDecision {
                proposal_id: "prop_human".into(),
                approve: false,
                operator_ref: "operator_console".into(),
                note: "withhold".into(),
            })
            .await
            .unwrap();
        assert!(!rejected.committed);
        assert_eq!(kernel.get_agent("agent_001").await.unwrap().position.x, 0.0);

        let (_, provider) = counting(MockScenario::HumanReview);
        let kernel =
            KernelEngine::with_judgment(EventBus::new(32), provider, JudgmentPolicy::default());
        kernel
            .process_action_proposal_detailed(proposal("prop_human", "HOLD", 6.0, "corr_human"))
            .await;
        let approved = kernel
            .resolve_human_review(HumanReviewDecision {
                proposal_id: "prop_human".into(),
                approve: true,
                operator_ref: "operator_console".into(),
                note: "reviewed".into(),
            })
            .await
            .unwrap();
        assert!(approved.committed);
        assert_eq!(kernel.get_agent("agent_001").await.unwrap().position.x, 6.0);
        let _ = drain(&mut rx);
    }

    #[tokio::test]
    async fn disabled_judgment_keeps_the_deterministic_commit_path() {
        let bus = EventBus::new(16);
        let mut rx = bus.subscribe();
        let kernel = KernelEngine::new(bus);
        let outcome = kernel
            .process_action_proposal_detailed(proposal("prop_off", "MOVE", 3.0, "corr_off"))
            .await;
        assert!(outcome.committed);
        let judgment = outcome.judgment.unwrap();
        assert_eq!(judgment.disposition, Disposition::Skipped);
        assert_eq!(judgment.provider_status, ProviderStatus::Disabled);
        assert!(judgment.answers.is_empty());
        let events = drain(&mut rx);
        assert!(events.iter().any(|event| event.event_type == "judgment"));
    }

    #[tokio::test]
    async fn provider_cannot_mutate_authoritative_state_or_export_raw_biosignals() {
        let (counter, provider) = counting(MockScenario::Unsupported);
        let kernel =
            KernelEngine::with_judgment(EventBus::new(16), provider, JudgmentPolicy::default());
        kernel
            .set_operator_telemetry(serde_json::json!({
                "cognitive_load": 0.25,
                "is_simulated": true,
                "eeg": "RAW_EEG_BUFFER_9f3a",
                "api_key": "super-secret-key"
            }))
            .await;
        kernel
            .register_agent(AuthoritativeAgentState {
                agent_id: "agent_001".into(),
                status: AgentStatus::Idle,
                capabilities: vec!["generic".into()],
                current_task: None,
                priority: 1,
                position: SpatialVector3 {
                    x: 8.0,
                    y: 1.0,
                    z: 0.0,
                },
                velocity: zero_vector(),
                confidence: 0.9,
                last_updated: 1,
            })
            .await;
        let outcome = kernel
            .process_action_proposal_detailed(proposal("prop_private", "MOVE", 9.0, "corr_private"))
            .await;
        assert!(!outcome.committed);
        assert_eq!(kernel.get_agent("agent_001").await.unwrap().position.x, 8.0);
        assert_eq!(counter.calls.load(Ordering::SeqCst), 1);
        let seen = counter.seen.lock().expect("seen").clone();
        assert!(!seen.contains("RAW_EEG_BUFFER_9f3a"));
        assert!(!seen.contains("super-secret-key"));
        assert!(!seen.contains("do not send raw parameters"));
        assert!(seen.contains("SIMULATED"));
        assert!(seen.contains("0.25"));
    }

    #[tokio::test]
    async fn state_too_large_does_not_call_the_provider() {
        let (counter, provider) = counting(MockScenario::Supported);
        let kernel = KernelEngine::with_limits(
            EventBus::new(16),
            provider,
            JudgmentPolicy::default(),
            StateLimits {
                max_bytes: 8,
                max_text_chars: 8,
            },
        );
        let outcome = kernel
            .process_action_proposal_detailed(proposal("prop_big", "MOVE", 1.0, "corr_big"))
            .await;
        assert_eq!(counter.calls.load(Ordering::SeqCst), 0);
        assert!(!outcome.committed);
        let judgment = outcome.judgment.unwrap();
        assert_eq!(judgment.provider_status, ProviderStatus::StateTooLarge);
        assert_eq!(judgment.disposition, Disposition::Unavailable);
    }

    #[tokio::test]
    async fn replay_uses_the_recorded_envelope_and_makes_no_provider_network_call() {
        let seed_bus = EventBus::new(8);
        let (_, provider) = counting(MockScenario::Supported);
        let seed_kernel =
            KernelEngine::with_judgment(seed_bus, provider, JudgmentPolicy::default());
        let seeded = seed_kernel
            .process_action_proposal_detailed(proposal("prop_replay", "MOVE", 15.0, "corr_replay"))
            .await;
        let recorded = seeded.judgment.unwrap();
        assert_eq!(recorded.disposition, Disposition::Pass);
        let original_id = recorded.judgment_id.clone();

        let (kernel, replay_provider) = KernelEngine::replay_recorded(
            EventBus::new(16),
            vec![recorded],
            JudgmentPolicy::default(),
        );
        let outcome = kernel
            .process_action_proposal_detailed(proposal("prop_replay", "MOVE", 15.0, "corr_replay"))
            .await;
        assert!(outcome.committed);
        let replayed = outcome.judgment.unwrap();
        assert_eq!(replayed.evaluation_mode, EvaluationMode::RecordedJudgment);
        assert_eq!(replayed.judgment_id, original_id);
        assert_eq!(replayed.disposition, Disposition::Pass);
        assert_eq!(replay_provider.network_call_count(), 0);
        assert_eq!(
            replay_provider
                .stored("prop_replay")
                .unwrap()
                .evaluation_mode,
            EvaluationMode::Live
        );
    }
}
