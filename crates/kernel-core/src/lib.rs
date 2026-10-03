//! Pordenone Kernel.
//!
//! The deterministic execution kernel of Resonate AI Mesh. For each proposal:
//!
//! ```text
//! PROPOSE -> DETERMINISTIC VALIDATION -> OPTIONAL BOUNDED JUDGMENT -> POLICY -> COMMIT / WITHHOLD
//! ```
//!
//! * Validation failure ends the pipeline. Judgment is not consulted and state
//!   is not touched.
//! * Judgment receives an owned, privacy-minimized evidence package and has no
//!   handle to state. Its answers are interpreted by deterministic policy.
//! * Authoritative state changes only through [`state`]'s single mutation
//!   interface, reached with a registration, a constraint declaration, or a
//!   commit authorization that only the policy module can create.
//! * Every step emits an event. The kernel returns its events, in order, to the
//!   caller (the lossless record) and also fans them out on the event bus.
//! * Time and identifiers come from injected sources, so a seeded run is
//!   reproducible byte for byte.

pub mod clock;
pub mod policy;
pub mod state;

use clock::{Clock, IdSource, RandomIds, SystemClock};
use epistemic_validator::{
    ActionProposal, ConfigError, EpistemicValidator, HazardZone, ValidationContext,
    ValidationResult, ValidatorConfig, Verdict, VALIDATOR_VERSION,
};
use event_bus::{
    DataMode, EventBus, EventDraft, EventEnvelope, EventStamp, HumanStateDatum, SignalQuality,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::Mutex;
use tracing::{info, warn};
use typed_judgment::{
    local_failure_envelope, prepare_case, AdaptiveSnapshot, DisabledJudgmentProvider,
    EvaluationMode, JudgmentCase, JudgmentEnvelope, JudgmentPolicy, JudgmentProvider,
    JudgmentRequest, PrepareError, ProviderDescriptor, ProviderKind, ProviderStatus,
    RecordedJudgmentProvider, StateLimits, QUESTION_SET_VERSION,
};

pub use clock::{ManualClock, SequentialIds};
pub use policy::{
    policy_for_level, AdaptiveLevel, AdaptivePolicyConfig, CognitiveLoadPolicyLevel, CommitBasis,
    JudgmentRouting, JudgmentSummary, KernelPolicyConfig, PolicyConfiguration, PolicyDecision,
    PolicyOutcome, KERNEL_POLICY_VERSION,
};
pub use state::{
    AgentStatus, AuthoritativeAgentState, CommitAuthorization, MutationError, StateSnapshot,
    StateTransition,
};

use policy::AgentHistory;
use state::{AuthoritativeState, StateMutation};

pub const KERNEL_SOURCE: &str = "pordenone.kernel";

/// Everything the kernel did with one proposal (or one human decision).
#[derive(Debug, Clone)]
pub struct PipelineOutcome {
    pub validation: ValidationResult,
    pub judgment: Option<JudgmentEnvelope>,
    pub decision: PolicyDecision,
    pub committed: bool,
    pub transition: Option<StateTransition>,
    /// Events emitted for this proposal, in order.
    pub events: Vec<EventEnvelope>,
}

/// Result of a setup mutation such as registering an agent.
#[derive(Debug, Clone)]
pub struct KernelReceipt {
    pub transition: StateTransition,
    pub events: Vec<EventEnvelope>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HumanReviewDecision {
    pub proposal_id: String,
    pub approve: bool,
    pub operator_ref: String,
    pub note: String,
    /// True when the decision came from a simulated operator rather than a person.
    #[serde(default)]
    pub simulated_operator: bool,
}

#[derive(Debug, Error)]
pub enum KernelError {
    #[error("no human review is pending for proposal `{0}`")]
    NoPendingReview(String),
    #[error(transparent)]
    Mutation(#[from] MutationError),
    #[error(transparent)]
    Config(#[from] ConfigError),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum JudgmentMode {
    Disabled,
    Live,
    Replay,
}

struct PendingReview {
    proposal: ActionProposal,
    judgment: JudgmentEnvelope,
    commitment_event_id: String,
}

struct KernelInner {
    state: AuthoritativeState,
    operator_evidence: Value,
    operator_signal: Option<SignalQuality>,
    adaptive_snapshot: Option<AdaptiveSnapshot>,
    adaptive_level: AdaptiveLevel,
    pending_reviews: std::collections::BTreeMap<String, PendingReview>,
    history: AgentHistory,
    processed: std::collections::BTreeSet<String>,
    tick: Option<u64>,
}

pub struct KernelBuilder {
    bus: EventBus,
    validator: ValidatorConfig,
    provider: Arc<dyn JudgmentProvider>,
    mode: JudgmentMode,
    judgment_policy: JudgmentPolicy,
    policy: KernelPolicyConfig,
    limits: StateLimits,
    clock: Arc<dyn Clock>,
    ids: Arc<dyn IdSource>,
    stamp: EventStamp,
}

impl KernelBuilder {
    pub fn new(bus: EventBus) -> Self {
        Self {
            bus,
            validator: ValidatorConfig::default(),
            provider: Arc::new(DisabledJudgmentProvider),
            mode: JudgmentMode::Disabled,
            judgment_policy: JudgmentPolicy::default(),
            policy: KernelPolicyConfig::default(),
            limits: StateLimits::default(),
            clock: Arc::new(SystemClock),
            ids: Arc::new(RandomIds),
            stamp: EventStamp::default(),
        }
    }

    pub fn validator(mut self, config: ValidatorConfig) -> Self {
        self.validator = config;
        self
    }

    /// Consult `provider` after validation passes. A provider that describes
    /// itself as disabled keeps judgment off.
    pub fn judgment(mut self, provider: Arc<dyn JudgmentProvider>, policy: JudgmentPolicy) -> Self {
        self.mode = if provider.descriptor().kind == ProviderKind::Disabled {
            JudgmentMode::Disabled
        } else {
            JudgmentMode::Live
        };
        self.provider = provider;
        self.judgment_policy = policy;
        self
    }

    /// Answer judgment from recordings (a `RecordedJudgmentProvider`, possibly
    /// wrapped). Envelopes are marked RECORDED_JUDGMENT.
    pub fn replay_judgments(
        mut self,
        recorded: Arc<dyn JudgmentProvider>,
        policy: JudgmentPolicy,
    ) -> Self {
        self.provider = recorded;
        self.mode = JudgmentMode::Replay;
        self.judgment_policy = policy;
        self
    }

    pub fn policy(mut self, policy: KernelPolicyConfig) -> Self {
        self.policy = policy;
        self
    }

    pub fn limits(mut self, limits: StateLimits) -> Self {
        self.limits = limits;
        self
    }

    pub fn clock(mut self, clock: Arc<dyn Clock>) -> Self {
        self.clock = clock;
        self
    }

    pub fn ids(mut self, ids: Arc<dyn IdSource>) -> Self {
        self.ids = ids;
        self
    }

    pub fn stamp(mut self, stamp: EventStamp) -> Self {
        self.stamp = stamp;
        self
    }

    pub fn build(self) -> Result<KernelEngine, KernelError> {
        let validator = EpistemicValidator::with_config(self.validator)?;
        let descriptor = self.provider.descriptor();
        Ok(KernelEngine {
            bus: self.bus,
            validator,
            provider: self.provider,
            descriptor,
            judgment_policy: self.judgment_policy,
            mode: self.mode,
            policy: self.policy,
            limits: self.limits,
            clock: self.clock,
            ids: self.ids,
            stamp: self.stamp,
            inner: Mutex::new(KernelInner {
                state: AuthoritativeState::default(),
                operator_evidence: Value::Null,
                operator_signal: None,
                adaptive_snapshot: None,
                adaptive_level: AdaptiveLevel::Normal,
                pending_reviews: Default::default(),
                history: AgentHistory::default(),
                processed: Default::default(),
                tick: None,
            }),
        })
    }
}

pub struct KernelEngine {
    bus: EventBus,
    validator: EpistemicValidator,
    provider: Arc<dyn JudgmentProvider>,
    descriptor: ProviderDescriptor,
    judgment_policy: JudgmentPolicy,
    mode: JudgmentMode,
    policy: KernelPolicyConfig,
    limits: StateLimits,
    clock: Arc<dyn Clock>,
    ids: Arc<dyn IdSource>,
    stamp: EventStamp,
    /// One lock for the whole pipeline: proposals are validated and committed
    /// one at a time, so a commit can never be based on a stale validation.
    inner: Mutex<KernelInner>,
}

impl KernelEngine {
    pub fn builder(bus: EventBus) -> KernelBuilder {
        KernelBuilder::new(bus)
    }

    /// Deterministic validation only; judgment disabled.
    pub fn new(bus: EventBus) -> Self {
        KernelBuilder::new(bus)
            .build()
            .expect("default kernel configuration is valid")
    }

    pub fn with_judgment(
        bus: EventBus,
        provider: Arc<dyn JudgmentProvider>,
        policy: JudgmentPolicy,
    ) -> Self {
        KernelBuilder::new(bus)
            .judgment(provider, policy)
            .build()
            .expect("default kernel configuration is valid")
    }

    pub fn with_limits(
        bus: EventBus,
        provider: Arc<dyn JudgmentProvider>,
        policy: JudgmentPolicy,
        limits: StateLimits,
    ) -> Self {
        KernelBuilder::new(bus)
            .judgment(provider, policy)
            .limits(limits)
            .build()
            .expect("default kernel configuration is valid")
    }

    pub fn replay_recorded(
        bus: EventBus,
        recorded: Vec<JudgmentEnvelope>,
        policy: JudgmentPolicy,
    ) -> (Self, Arc<RecordedJudgmentProvider>) {
        let provider = Arc::new(RecordedJudgmentProvider::new(recorded));
        let kernel = KernelBuilder::new(bus)
            .replay_judgments(provider.clone() as Arc<dyn JudgmentProvider>, policy)
            .build()
            .expect("default kernel configuration is valid");
        (kernel, provider)
    }

    pub fn validator(&self) -> &EpistemicValidator {
        &self.validator
    }

    pub fn judgment_descriptor(&self) -> Option<&ProviderDescriptor> {
        (self.mode != JudgmentMode::Disabled).then_some(&self.descriptor)
    }

    pub fn judgment_policy(&self) -> &JudgmentPolicy {
        &self.judgment_policy
    }

    pub fn kernel_policy(&self) -> &KernelPolicyConfig {
        &self.policy
    }

    pub fn stamp(&self) -> &EventStamp {
        &self.stamp
    }

    pub fn bus(&self) -> &EventBus {
        &self.bus
    }

    /// Attach a simulation tick to subsequent events.
    pub async fn set_tick(&self, tick: Option<u64>) {
        self.inner.lock().await.tick = tick;
    }

    pub async fn get_agent(&self, agent_id: &str) -> Option<AuthoritativeAgentState> {
        self.inner
            .lock()
            .await
            .state
            .snapshot()
            .agents
            .get(agent_id)
            .cloned()
    }

    pub async fn snapshot(&self) -> StateSnapshot {
        self.inner.lock().await.state.snapshot().clone()
    }

    pub async fn state_hash(&self) -> String {
        self.inner.lock().await.state.snapshot().hash()
    }

    pub async fn adaptive_level(&self) -> AdaptiveLevel {
        self.inner.lock().await.adaptive_level
    }

    pub async fn pending_reviews(&self) -> Vec<String> {
        self.inner
            .lock()
            .await
            .pending_reviews
            .keys()
            .cloned()
            .collect()
    }

    /// Register an agent. This is a setup mutation by the experiment harness
    /// or operator, recorded as a state transition.
    pub async fn register_agent(
        &self,
        agent: AuthoritativeAgentState,
    ) -> Result<KernelReceipt, KernelError> {
        self.setup_mutation(StateMutation::RegisterAgent(agent), "setup")
            .await
    }

    /// Declare a hazard zone that validation must keep committed paths out of.
    pub async fn declare_hazard(
        &self,
        zone: HazardZone,
        causation_id: &str,
    ) -> Result<KernelReceipt, KernelError> {
        self.setup_mutation(StateMutation::DeclareHazard(zone), causation_id)
            .await
    }

    pub async fn retire_hazard(
        &self,
        zone_id: &str,
        causation_id: &str,
    ) -> Result<KernelReceipt, KernelError> {
        self.setup_mutation(
            StateMutation::RetireHazard(zone_id.to_string()),
            causation_id,
        )
        .await
    }

    async fn setup_mutation(
        &self,
        mutation: StateMutation,
        causation_id: &str,
    ) -> Result<KernelReceipt, KernelError> {
        let mut inner = self.inner.lock().await;
        let now = self.clock.now_ms();
        let transition = inner.state.apply(mutation, now)?;
        let mut events = Vec::new();
        self.emit(
            &mut events,
            inner.tick,
            "state_transition",
            &transition.subject_id.clone(),
            &format!("setup:{}", transition.subject_id),
            causation_id,
            KERNEL_POLICY_VERSION,
            false,
            json!(transition),
        );
        self.publish(&events);
        Ok(KernelReceipt { transition, events })
    }

    /// Feed a human-state datum. Only derived, privacy-minimized features are
    /// kept for judgment evidence. Returns an `adaptive_level` event when the
    /// level or the signal status changes.
    pub async fn observe_human_state(
        &self,
        datum: &HumanStateDatum,
        causation_id: &str,
    ) -> Option<EventEnvelope> {
        if datum.metric != "operator_load_index" {
            return None;
        }
        let mut inner = self.inner.lock().await;
        let usable = datum.validate().is_ok()
            && matches!(datum.quality, SignalQuality::Good | SignalQuality::Degraded);
        let previous_level = inner.adaptive_level;
        let previous_signal = inner.operator_signal;
        if usable {
            let quality = match datum.quality {
                SignalQuality::Good => 1.0,
                _ => 0.5,
            };
            inner.operator_evidence = json!({
                "operator_load_index": datum.value,
                "signal_quality": quality,
                "state_confidence": datum.confidence,
                "is_simulated": datum.mode != DataMode::Live,
            });
            inner.adaptive_level = self.policy.adaptive.level_for(datum.value);
        } else {
            // A missing or invalid signal is reported, never replaced by a guess.
            // The last derived level stays in force and the evidence is dropped.
            inner.operator_evidence = Value::Null;
        }
        inner.operator_signal = Some(datum.quality);
        if inner.adaptive_level == previous_level && previous_signal.is_some() {
            let usable_before = matches!(
                previous_signal,
                Some(SignalQuality::Good | SignalQuality::Degraded)
            );
            if usable_before == usable {
                return None;
            }
        }
        let mut events = Vec::new();
        let config = policy_for_level(inner.adaptive_level);
        self.emit(
            &mut events,
            inner.tick,
            "adaptive_level",
            "operator",
            causation_id,
            causation_id,
            KERNEL_POLICY_VERSION,
            false,
            json!({
                "level": inner.adaptive_level,
                "previous_level": previous_level,
                "operator_load_index": datum.value,
                "signal_quality": datum.quality,
                "signal_usable": usable,
                "data_mode": datum.mode,
                "defer_background_tasks": config.defer_background_tasks,
                "gating_enabled": self.policy.adaptive.enabled,
            }),
        );
        self.publish(&events);
        events.pop()
    }

    /// Former entry point: set the operator level directly from a load value.
    pub async fn update_policy_for_cognitive_load(&self, load: f64) -> PolicyConfiguration {
        let mut inner = self.inner.lock().await;
        inner.adaptive_level = self.policy.adaptive.level_for(load);
        let config = policy_for_level(inner.adaptive_level);
        info!(policy_level = ?config.level, load, "updated kernel policy level");
        config
    }

    /// Raw operator telemetry. Only allow-listed derived fields reach judgment.
    pub async fn set_operator_telemetry(&self, raw: Value) {
        self.inner.lock().await.operator_evidence = raw;
    }

    pub async fn set_adaptive_snapshot(&self, snapshot: AdaptiveSnapshot) {
        self.inner.lock().await.adaptive_snapshot = Some(snapshot);
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
        let causation = proposal.proposal_id.clone();
        self.process_proposal(proposal, &causation).await
    }

    /// Run one proposal through validation, optional judgment, and policy.
    /// `causation_id` is the event that carried the proposal.
    pub async fn process_proposal(
        &self,
        proposal: ActionProposal,
        causation_id: &str,
    ) -> PipelineOutcome {
        let mut guard = self.inner.lock().await;
        let inner = &mut *guard;
        let now = self.clock.now_ms();
        let mut events = Vec::new();
        let agent_id = proposal.agent_id.clone();
        let correlation_id = proposal.correlation_id.clone();

        let duplicate = !inner.processed.insert(proposal.proposal_id.clone());
        let verdict = {
            let me = inner.state.agent_view(&agent_id);
            let others = inner.state.other_views(&agent_id);
            let hazards = inner.state.hazards();
            self.validator.evaluate(
                proposal,
                &ValidationContext {
                    now_ms: now,
                    agent: me.as_ref(),
                    others: &others,
                    hazards: &hazards,
                    duplicate,
                },
            )
        };
        let validation = verdict.result().clone();
        let validation_event_id = self.emit(
            &mut events,
            inner.tick,
            "validation",
            &agent_id,
            &correlation_id,
            causation_id,
            VALIDATOR_VERSION,
            false,
            json!(validation),
        );

        let validated = match verdict {
            Verdict::Fail { result, .. } => {
                warn!(
                    proposal_id = %result.proposal_id,
                    reasons = ?result.reasons,
                    "proposal rejected by deterministic validation"
                );
                let decision = PolicyDecision::rejected(
                    &result.proposal_id,
                    result.reasons.clone(),
                    inner.adaptive_level,
                );
                self.emit_decision(
                    &mut events,
                    inner.tick,
                    &agent_id,
                    &correlation_id,
                    &validation_event_id,
                    &decision,
                    false,
                );
                inner.history.record(&agent_id, false);
                self.publish(&events);
                return PipelineOutcome {
                    validation,
                    judgment: None,
                    decision,
                    committed: false,
                    transition: None,
                    events,
                };
            }
            Verdict::Pass(validated) => validated,
        };

        let (skip_reason, routing_signal) = if self.mode == JudgmentMode::Disabled {
            (Some("JUDGMENT_DISABLED"), None)
        } else {
            policy::route(&self.policy.routing, &inner.history, &agent_id)
        };

        let mut decision_cause = validation_event_id.clone();
        let (judgment, summary) = match skip_reason {
            Some(reason) => (None, JudgmentSummary::not_consulted(reason, routing_signal)),
            None => {
                let envelope = self
                    .judge(inner, &validated, &validation_event_id, now)
                    .await;
                let model_involved = self.mode != JudgmentMode::Disabled
                    && envelope.provider != "kernel"
                    && self.descriptor_involves_model(&envelope);
                let judgment_event_id = self.emit(
                    &mut events,
                    inner.tick,
                    "judgment",
                    &agent_id,
                    &correlation_id,
                    &validation_event_id,
                    &envelope.policy_version.clone(),
                    model_involved,
                    json!(envelope),
                );
                decision_cause = judgment_event_id;
                let summary = JudgmentSummary {
                    consulted: true,
                    skip_reason: None,
                    disposition: Some(envelope.disposition),
                    judgment_id: Some(envelope.judgment_id.clone()),
                    provider: Some(envelope.provider.clone()),
                    model_involved,
                    routing_signal,
                };
                (Some(envelope), summary)
            }
        };
        let model_involved = summary.model_involved;

        let decision = policy::decide(
            &validated,
            judgment.as_ref(),
            summary,
            inner.adaptive_level,
            &self.policy,
        );
        let commitment_event_id = self.emit_decision(
            &mut events,
            inner.tick,
            &agent_id,
            &correlation_id,
            &decision_cause,
            &decision,
            model_involved,
        );

        let mut transition = None;
        match decision.outcome {
            PolicyOutcome::Committed => {
                let authorization = policy::authorize(
                    validated,
                    &decision,
                    vec![validation_event_id.clone(), commitment_event_id.clone()],
                )
                .expect("policy decided to commit");
                match inner
                    .state
                    .apply(StateMutation::Commit(Box::new(authorization)), now)
                {
                    Ok(applied) => {
                        self.emit(
                            &mut events,
                            inner.tick,
                            "state_transition",
                            &agent_id,
                            &correlation_id,
                            &commitment_event_id,
                            KERNEL_POLICY_VERSION,
                            model_involved,
                            json!(applied),
                        );
                        transition = Some(applied);
                    }
                    Err(error) => warn!(%error, "commit was authorized but not applied"),
                }
            }
            PolicyOutcome::AwaitingHumanReview => {
                if let Some(envelope) = &judgment {
                    let (proposal, _) = validated.into_parts();
                    inner.pending_reviews.insert(
                        proposal.proposal_id.clone(),
                        PendingReview {
                            proposal,
                            judgment: envelope.clone(),
                            commitment_event_id: commitment_event_id.clone(),
                        },
                    );
                }
            }
            PolicyOutcome::Withheld | PolicyOutcome::RejectedDeterministic => {}
        }
        let committed = transition.is_some();
        inner.history.record(&agent_id, committed);
        self.publish(&events);
        PipelineOutcome {
            validation,
            judgment,
            decision,
            committed,
            transition,
            events,
        }
    }

    /// Apply an explicit human decision to a proposal awaiting review. Approval
    /// re-runs deterministic validation against current state; a failed
    /// re-check still blocks the commit.
    pub async fn resolve_human_review(
        &self,
        decision: HumanReviewDecision,
    ) -> Result<PipelineOutcome, KernelError> {
        let mut guard = self.inner.lock().await;
        let inner = &mut *guard;
        let pending = inner
            .pending_reviews
            .remove(&decision.proposal_id)
            .ok_or_else(|| KernelError::NoPendingReview(decision.proposal_id.clone()))?;
        let now = self.clock.now_ms();
        let agent_id = pending.proposal.agent_id.clone();
        let correlation_id = pending.proposal.correlation_id.clone();
        let operator_ref = if decision.operator_ref.trim().is_empty() {
            "operator_console".to_string()
        } else {
            decision.operator_ref.chars().take(64).collect()
        };
        let mut events = Vec::new();
        let human_event_id = self.emit_with_mode(
            &mut events,
            inner.tick,
            "human_resolution",
            &agent_id,
            &correlation_id,
            &pending.commitment_event_id,
            KERNEL_POLICY_VERSION,
            false,
            if decision.simulated_operator {
                DataMode::Simulated
            } else {
                DataMode::Live
            },
            json!({
                "proposal_id": pending.proposal.proposal_id,
                "decision": if decision.approve { "approve" } else { "reject" },
                "operator_ref": operator_ref,
                "simulated_operator": decision.simulated_operator,
                "note": decision.note.chars().take(512).collect::<String>(),
                "judgment_id": pending.judgment.judgment_id,
            }),
        );

        let verdict = {
            let me = inner.state.agent_view(&agent_id);
            let others = inner.state.other_views(&agent_id);
            let hazards = inner.state.hazards();
            // The re-check after a human decision concerns the same proposal,
            // so it is not a duplicate submission.
            self.validator.evaluate(
                pending.proposal.clone(),
                &ValidationContext {
                    now_ms: now,
                    agent: me.as_ref(),
                    others: &others,
                    hazards: &hazards,
                    duplicate: false,
                },
            )
        };
        let validation = verdict.result().clone();
        let validation_event_id = self.emit(
            &mut events,
            inner.tick,
            "validation",
            &agent_id,
            &correlation_id,
            &human_event_id,
            VALIDATOR_VERSION,
            false,
            json!(validation),
        );

        let summary = JudgmentSummary {
            consulted: true,
            skip_reason: None,
            disposition: Some(pending.judgment.disposition),
            judgment_id: Some(pending.judgment.judgment_id.clone()),
            provider: Some(pending.judgment.provider.clone()),
            model_involved: self.descriptor_involves_model(&pending.judgment),
            routing_signal: None,
        };
        let (decision_record, validated) = match verdict {
            Verdict::Fail { result, .. } => {
                let mut record = PolicyDecision::rejected(
                    &result.proposal_id,
                    result.reasons.clone(),
                    inner.adaptive_level,
                );
                if decision.approve {
                    record
                        .reason_codes
                        .push("HUMAN_APPROVAL_BLOCKED".to_string());
                } else {
                    record
                        .reason_codes
                        .push("HUMAN_DECISION_REJECT".to_string());
                }
                record.judgment = Some(summary);
                (record, None)
            }
            Verdict::Pass(validated) => {
                let (outcome, basis, reason) = if decision.approve {
                    (
                        PolicyOutcome::Committed,
                        Some(CommitBasis::HumanApproved),
                        "HUMAN_DECISION_APPROVE",
                    )
                } else {
                    (PolicyOutcome::Withheld, None, "HUMAN_DECISION_REJECT")
                };
                (
                    PolicyDecision {
                        proposal_id: validated.proposal().proposal_id.clone(),
                        outcome,
                        reason_codes: vec![reason.to_string()],
                        policy_version: KERNEL_POLICY_VERSION.to_string(),
                        adaptive_level: inner.adaptive_level,
                        judgment: Some(summary),
                        basis,
                    },
                    Some(validated),
                )
            }
        };
        let commitment_event_id = self.emit_decision(
            &mut events,
            inner.tick,
            &agent_id,
            &correlation_id,
            &validation_event_id,
            &decision_record,
            false,
        );
        let mut transition = None;
        if let Some(validated) = validated {
            if let Some(authorization) = policy::authorize(
                validated,
                &decision_record,
                vec![
                    human_event_id.clone(),
                    validation_event_id.clone(),
                    commitment_event_id.clone(),
                ],
            ) {
                match inner
                    .state
                    .apply(StateMutation::Commit(Box::new(authorization)), now)
                {
                    Ok(applied) => {
                        self.emit(
                            &mut events,
                            inner.tick,
                            "state_transition",
                            &agent_id,
                            &correlation_id,
                            &commitment_event_id,
                            KERNEL_POLICY_VERSION,
                            false,
                            json!(applied),
                        );
                        transition = Some(applied);
                    }
                    Err(error) => warn!(%error, "human-approved commit was not applied"),
                }
            }
        }
        let committed = transition.is_some();
        self.publish(&events);
        Ok(PipelineOutcome {
            validation,
            judgment: Some(pending.judgment),
            decision: decision_record,
            committed,
            transition,
            events,
        })
    }

    fn descriptor_involves_model(&self, envelope: &JudgmentEnvelope) -> bool {
        match self.mode {
            JudgmentMode::Disabled => false,
            JudgmentMode::Live => self.descriptor.model_involved(),
            // A recorded envelope from a remote provider still carries model output.
            JudgmentMode::Replay => envelope.provider == "typesafe",
        }
    }

    async fn judge(
        &self,
        inner: &KernelInner,
        validated: &epistemic_validator::ValidatedProposal,
        validation_event_id: &str,
        now: i64,
    ) -> JudgmentEnvelope {
        let proposal = validated.proposal();
        let result = validated.result();
        let case = JudgmentCase {
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
            observation_simulated: self.stamp.mode != DataMode::Live,
            validation_feasibility: result.feasibility,
            validation_accepted: result.accepted,
            constraints_checked: self.validator.checked_constraints(),
            contradictions: result.contradictions.clone(),
            deterministic_confidence: result.confidence,
            validation_provenance: result.provenance.clone(),
            adaptive: inner.adaptive_snapshot.clone(),
            operator_raw: inner.operator_evidence.clone(),
            spatial_coordinate_system: "local_sim".to_string(),
            within_declared_bounds: result.accepted,
            correlation_id: proposal.correlation_id.clone(),
            causation_id: proposal.proposal_id.clone(),
            simulation_status: self.stamp.mode.as_str().to_string(),
            source_event_ids: vec![proposal.proposal_id.clone()],
        };
        let judgment_id = self.ids.next_id("jdg");
        let mut envelope = match prepare_case(&case, &self.limits) {
            Ok(prepared) => {
                let request = JudgmentRequest {
                    prepared,
                    proposal_id: proposal.proposal_id.clone(),
                    correlation_id: proposal.correlation_id.clone(),
                    causation_id: validation_event_id.to_string(),
                };
                let mut envelope = self.provider.evaluate(&request).await;
                self.finalize_envelope(&mut envelope, &request);
                envelope
            }
            Err(error) => {
                let status = match error {
                    PrepareError::StateTooLarge { .. } => ProviderStatus::StateTooLarge,
                    PrepareError::NonFinite | PrepareError::Serialization => {
                        ProviderStatus::SerializationFailure
                    }
                };
                warn!(proposal_id = %proposal.proposal_id, provider_status = status.reason_code(),
                    "judgment state was not sent to a provider");
                let mut envelope = local_failure_envelope(
                    &proposal.proposal_id,
                    &proposal.correlation_id,
                    validation_event_id,
                    status,
                );
                self.judgment_policy.apply(&mut envelope);
                envelope
            }
        };
        envelope.judgment_id = judgment_id;
        envelope.requested_at = now;
        envelope.completed_at = now.saturating_add(i64::try_from(envelope.latency_ms).unwrap_or(0));
        envelope
    }

    fn finalize_envelope(&self, envelope: &mut JudgmentEnvelope, request: &JudgmentRequest) {
        match self.mode {
            JudgmentMode::Disabled | JudgmentMode::Live => {
                if envelope.provider_status == ProviderStatus::Disabled {
                    // A provider that reports itself disabled during a live
                    // evaluation is not permission to commit.
                    envelope.provider_status = ProviderStatus::InvalidRequest;
                    envelope.answers.clear();
                }
                envelope.evaluation_mode = EvaluationMode::Live;
                envelope.proposal_id = request.proposal_id.clone();
                envelope.correlation_id = request.correlation_id.clone();
                envelope.causation_id = request.causation_id.clone();
                envelope.state_hash = request.prepared.state_hash.clone();
                envelope.truncated = request.prepared.truncated;
                envelope.simulation_label = request.prepared.simulation_label.clone();
                envelope.question_set_version = QUESTION_SET_VERSION.to_string();
                envelope.state_schema_version = typed_judgment::STATE_SCHEMA_VERSION.to_string();
                self.judgment_policy.apply(envelope);
            }
            JudgmentMode::Replay => {
                envelope.evaluation_mode = EvaluationMode::RecordedJudgment;
                envelope.correlation_id = request.correlation_id.clone();
                envelope.causation_id = request.causation_id.clone();
                if envelope
                    .reason_codes
                    .iter()
                    .any(|code| code.starts_with("RECORDED_JUDGMENT_"))
                {
                    envelope.policy_version = self.judgment_policy.version.clone();
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn emit_decision(
        &self,
        events: &mut Vec<EventEnvelope>,
        tick: Option<u64>,
        agent_id: &str,
        correlation_id: &str,
        causation_id: &str,
        decision: &PolicyDecision,
        model_involved: bool,
    ) -> String {
        self.emit(
            events,
            tick,
            "commitment",
            agent_id,
            correlation_id,
            causation_id,
            KERNEL_POLICY_VERSION,
            model_involved,
            json!(decision),
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn emit(
        &self,
        events: &mut Vec<EventEnvelope>,
        tick: Option<u64>,
        event_type: &str,
        subject_id: &str,
        correlation_id: &str,
        causation_id: &str,
        policy_version: &str,
        ai_involved: bool,
        payload: Value,
    ) -> String {
        self.emit_with_mode(
            events,
            tick,
            event_type,
            subject_id,
            correlation_id,
            causation_id,
            policy_version,
            ai_involved,
            self.stamp.mode,
            payload,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn emit_with_mode(
        &self,
        events: &mut Vec<EventEnvelope>,
        tick: Option<u64>,
        event_type: &str,
        subject_id: &str,
        correlation_id: &str,
        causation_id: &str,
        policy_version: &str,
        ai_involved: bool,
        mode: DataMode,
        payload: Value,
    ) -> String {
        let event_id = self.ids.next_id("evt");
        let provenance = match event_type {
            "validation" => format!("{VALIDATOR_VERSION}:{}", self.validator.config_hash()),
            "judgment" => format!(
                "{}:{}:{}",
                self.descriptor.name, self.descriptor.model, QUESTION_SET_VERSION
            ),
            _ => KERNEL_POLICY_VERSION.to_string(),
        };
        let mut envelope = self.stamp.envelope(
            event_id.clone(),
            self.clock.now_ms(),
            tick,
            EventDraft {
                event_type,
                source: KERNEL_SOURCE,
                subject_id,
                correlation_id,
                causation_id,
                provenance: &provenance,
                policy_version,
                ai_involved,
                payload,
            },
        );
        envelope.mode = mode;
        events.push(envelope);
        event_id
    }

    fn publish(&self, events: &[EventEnvelope]) {
        for event in events {
            self.bus.publish(event.clone());
        }
    }
}

#[cfg(test)]
mod tests;
