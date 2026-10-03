//! The run loop.
//!
//! Each tick follows the execution law of the mesh:
//!
//! ```text
//! OBSERVE (environment, sensors, simulated human state)
//!   -> PROPOSE (agents, through MeshAgentAdapter)
//!   -> DETERMINISTIC VALIDATION -> OPTIONAL JUDGMENT -> POLICY -> COMMIT / WITHHOLD   (Pordenone kernel)
//!   -> OBSERVE RESULT (agents receive outcomes)
//!   -> RECORD (hash-chained event log, decision records)
//! ```
//!
//! The runner owns time (a logical clock), identifiers (a sequential source),
//! and randomness (named seeded streams). It never mutates authoritative state
//! itself: every change goes through the kernel.

use crate::agents::{
    AgentIntent, AgentObservation, BuiltinAgent, Constraints, ExternalProcessAgent,
    MeshAgentAdapter, ProposalContext, ProposalOutcome, RecordedAgent,
};
use crate::config::{
    point, Behavior, FaultKind, FaultSpec, JudgeProfile, JudgeProvider, ReviewMode, RunConfig,
};
use crate::invariants;
use crate::metrics::{self, MetricsDoc, RunData};
use crate::record::{
    coords, DecisionRecord, JudgmentDigest, PolicySummary, RecordedIntent, TransitionSummary,
    ValidationSummary,
};
use crate::sim::{
    self, FaultableJudge, JudgeFaults, LoadInputs, OperatorLoadModel, SensorReading, Sensors,
};
use epistemic_validator::{ActionProposal, Vector3};
use event_bus::{
    quantize, ChainedEvent, DataMode, EventBus, EventChain, EventDraft, EventEnvelope, EventStamp,
    HumanStateDatum, SignalQuality, SOFTWARE_VERSION,
};
use kernel_core::clock::{Clock, IdSource};
use kernel_core::{
    AuthoritativeAgentState, HumanReviewDecision, KernelEngine, ManualClock, PipelineOutcome,
    PolicyOutcome, SequentialIds, StateSnapshot,
};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::time::Instant;
use thiserror::Error;
use typed_judgment::{
    build_provider, ContrarianJudge, DeterministicMockJudgmentProvider, EvidenceHeuristicJudge,
    JudgmentEnvelope, JudgmentProvider, JudgmentSettings, ProviderDescriptor, ProviderSelection,
    RecordedJudgmentProvider,
};

pub const LAB_SOURCE: &str = "mesh.lab";

#[derive(Debug, Error)]
pub enum RunError {
    #[error("configuration: {0}")]
    Config(String),
    #[error("kernel: {0}")]
    Kernel(String),
    #[error("{0}")]
    Unavailable(String),
}

/// Recorded inputs that replay substitutes for components it must not re-run.
#[derive(Debug, Clone, Default)]
pub struct Substitutions {
    /// Judgments to answer from (used when the original judge was networked or recorded).
    pub judgments: Option<Vec<JudgmentEnvelope>>,
    /// Intents of non-deterministic agents, keyed by agent then tick.
    pub intents: BTreeMap<String, BTreeMap<u64, Option<AgentIntent>>>,
    /// Interactive commands recorded in a live session, keyed by tick.
    pub commands: BTreeMap<u64, Vec<ControlCommand>>,
}

/// Commands an interactive session may issue between ticks. Each one is
/// recorded as an event, so live sessions replay like any other run.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum ControlCommand {
    InjectFault {
        fault: FaultSpec,
    },
    OperatorProposal {
        agent_id: String,
        intent: AgentIntent,
    },
    HumanDecision {
        proposal_id: String,
        approve: bool,
        note: String,
    },
}

/// Hook for live sessions: called once per tick before agents act.
pub trait RunControl: Send {
    fn commands(&mut self, tick: u64) -> Vec<ControlCommand>;
    /// Called after each tick with the events it produced.
    fn on_tick(&mut self, _tick: u64, _events: &[ChainedEvent]) {}
    fn should_stop(&self) -> bool {
        false
    }
}

#[derive(Default)]
pub struct RunOptions {
    pub substitutions: Substitutions,
    /// Permit a networked judge. Never set during replay.
    pub allow_network: bool,
    /// Live fan-out of recorded events (dashboard).
    pub bus: Option<EventBus>,
    pub control: Option<Box<dyn RunControl>>,
    /// Real-time pacing between ticks for live sessions.
    pub tick_delay: Option<std::time::Duration>,
    /// Data mode stamped on lab events. SIMULATED unless a live session says otherwise.
    pub mode: Option<DataMode>,
}

pub struct RunResult {
    pub config: RunConfig,
    pub genesis_hash: String,
    pub events: Vec<ChainedEvent>,
    pub decisions: Vec<DecisionRecord>,
    pub judgments: Vec<JudgmentEnvelope>,
    pub intents: Vec<RecordedIntent>,
    pub final_state: StateSnapshot,
    pub head_hash: String,
    pub metrics: MetricsDoc,
    pub timing: crate::record::Timing,
    pub network_calls: u64,
    pub judgment_calls: u64,
    pub judge: Option<ProviderDescriptor>,
    pub agent_descriptors: Vec<crate::agents::AgentDescriptor>,
    pub ticks_run: u64,
    pub termination: String,
    pub substituted: Vec<String>,
    pub validator_config_hash: String,
}

impl RunResult {
    pub fn counts(&self) -> BTreeMap<String, u64> {
        let mut counts = BTreeMap::new();
        counts.insert("events".to_string(), self.events.len() as u64);
        for chained in &self.events {
            *counts
                .entry(format!("events.{}", chained.event.event_type))
                .or_insert(0) += 1;
        }
        counts.insert("decisions".to_string(), self.decisions.len() as u64);
        counts
    }
}

struct Recorder {
    chain: EventChain,
    events: Vec<ChainedEvent>,
    bus: Option<EventBus>,
    tick_start: usize,
}

impl Recorder {
    fn record(&mut self, event: EventEnvelope) -> String {
        let id = event.event_id.clone();
        let chained = self.chain.append(event);
        if let Some(bus) = &self.bus {
            bus.publish(chained.event.clone());
        }
        self.events.push(chained);
        id
    }

    fn record_all(&mut self, events: Vec<EventEnvelope>) {
        for event in events {
            self.record(event);
        }
    }
}

struct Lab {
    stamp: EventStamp,
    clock: Arc<ManualClock>,
    ids: Arc<SequentialIds>,
    tick: u64,
}

impl Lab {
    #[allow(clippy::too_many_arguments)]
    fn event(
        &self,
        event_type: &str,
        source: &str,
        subject: &str,
        correlation: &str,
        causation: &str,
        provenance: &str,
        ai_involved: bool,
        payload: Value,
    ) -> EventEnvelope {
        self.stamp.envelope(
            self.ids.next_id("evt"),
            self.clock.now_ms(),
            Some(self.tick),
            EventDraft {
                event_type,
                source,
                subject_id: subject,
                correlation_id: correlation,
                causation_id: causation,
                provenance,
                policy_version: "",
                ai_involved,
                payload,
            },
        )
    }
}

/// Build the judgment provider for a run. Returns the wrapped provider and
/// whether the kernel must treat its answers as recordings. When recorded
/// judgments are substituted (replay), no networked provider is constructed.
fn build_judge(
    config: &RunConfig,
    substitute: Option<&Vec<JudgmentEnvelope>>,
    allow_network: bool,
) -> Result<(Option<Arc<FaultableJudge>>, bool), RunError> {
    let spec = &config.scenario.kernel.judgment;
    if spec.provider == JudgeProvider::Disabled {
        return Ok((None, false));
    }
    if let Some(recorded) = substitute {
        let provider = Arc::new(RecordedJudgmentProvider::new(recorded.clone()));
        return Ok((
            Some(Arc::new(FaultableJudge::new(provider, spec.timeout_ms))),
            true,
        ));
    }
    let (inner, recorded): (Arc<dyn JudgmentProvider>, bool) = match spec.provider {
        JudgeProvider::Disabled => unreachable!(),
        JudgeProvider::Mock => (
            match spec.profile {
                JudgeProfile::Supported => Arc::new(DeterministicMockJudgmentProvider::supported()),
                JudgeProfile::EvidenceHeuristic => Arc::new(EvidenceHeuristicJudge),
                JudgeProfile::Contrarian => Arc::new(ContrarianJudge),
            },
            false,
        ),
        JudgeProvider::Recorded => {
            let path = spec.recorded_from.as_ref().ok_or_else(|| {
                RunError::Config(
                    "a recorded judge needs judgment.recorded_from (a run bundle) or, in an experiment, judgment.recorded_from_condition".into(),
                )
            })?;
            let judgments: Vec<JudgmentEnvelope> =
                crate::record::read_jsonl(&path.join("judgments.jsonl"))
                    .map_err(|error| RunError::Config(error.to_string()))?;
            (Arc::new(RecordedJudgmentProvider::new(judgments)), true)
        }
        JudgeProvider::Typesafe => {
            if !allow_network {
                return Err(RunError::Unavailable(
                    "the TypeSafe judge needs network access; pass --allow-network (never used in replay)".into(),
                ));
            }
            if std::env::var(typed_judgment::API_KEY_ENV)
                .map(|k| k.trim().is_empty())
                .unwrap_or(true)
            {
                return Err(RunError::Unavailable(format!(
                    "{} is not set; the TypeSafe condition cannot run",
                    typed_judgment::API_KEY_ENV
                )));
            }
            let mut settings = JudgmentSettings::from_env();
            settings.enabled = true;
            settings.provider = ProviderSelection::TypeSafe;
            settings.timeout = std::time::Duration::from_millis(spec.timeout_ms);
            (build_provider(&settings), false)
        }
    };
    Ok((
        Some(Arc::new(FaultableJudge::new(inner, spec.timeout_ms))),
        recorded,
    ))
}

fn adapter_for(
    config: &RunConfig,
    spec: &crate::config::AgentSpec,
    substitutions: &Substitutions,
) -> Box<dyn MeshAgentAdapter> {
    let scenario = &config.scenario;
    let goal = spec
        .goal
        .as_ref()
        .and_then(|g| scenario.poi(g))
        .map(|poi| point(poi.position));
    if spec.behavior == Behavior::External {
        if let Some(intents) = substitutions.intents.get(&spec.id) {
            return Box::new(RecordedAgent::new(
                crate::agents::AgentDescriptor {
                    agent_id: spec.id.clone(),
                    adapter: "recorded:external".into(),
                    deterministic: true,
                    networked: false,
                },
                intents.clone(),
            ));
        }
        return Box::new(ExternalProcessAgent::start(
            &spec.id,
            &spec.command,
            spec.timeout_ms,
        ));
    }
    Box::new(BuiltinAgent::new(
        spec,
        goal,
        scenario.termination.goal_radius,
    ))
}

fn judgment_digest(
    event_id: &str,
    envelope: &JudgmentEnvelope,
    model_involved: bool,
) -> JudgmentDigest {
    let min_confidence = envelope
        .answers
        .iter()
        .filter_map(|answer| answer.confidence)
        .reduce(f64::min);
    JudgmentDigest {
        event_id: event_id.to_string(),
        judgment_id: envelope.judgment_id.clone(),
        provider: envelope.provider.clone(),
        model: envelope.provider_model_version.clone(),
        disposition: serde_json::to_value(envelope.disposition)
            .ok()
            .and_then(|v| v.as_str().map(str::to_string))
            .unwrap_or_default(),
        reason_codes: envelope.reason_codes.clone(),
        provider_status: envelope
            .provider_status
            .reason_code()
            .to_string()
            .to_ascii_lowercase()
            .replace("provider_", "")
            .replace("judgment_", ""),
        latency_ms: envelope.latency_ms,
        model_involved,
        min_confidence,
    }
}

fn find_event<'a>(events: &'a [EventEnvelope], event_type: &str) -> Option<&'a EventEnvelope> {
    events.iter().find(|event| event.event_type == event_type)
}

fn enum_str<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default()
}

struct ObservationInfo {
    id: String,
    observed_at: i64,
    quality: SignalQuality,
}

#[allow(clippy::too_many_arguments)]
fn decision_record(
    tick: u64,
    kind: &str,
    proposal: &ActionProposal,
    from: Vector3,
    observation: Option<&ObservationInfo>,
    trigger_event_id: &str,
    outcome: &PipelineOutcome,
    oracle: Option<&'static str>,
    rationale: Option<String>,
    duplicate: bool,
) -> DecisionRecord {
    let validation_event = find_event(&outcome.events, "validation");
    let judgment_event = find_event(&outcome.events, "judgment");
    let commitment_event = outcome
        .events
        .iter()
        .rev()
        .find(|event| event.event_type == "commitment");
    let transition_event = find_event(&outcome.events, "state_transition");
    let failed_checks = outcome
        .validation
        .failed_checks()
        .map(|check| check.check.as_str().to_string())
        .collect();
    DecisionRecord {
        tick,
        kind: kind.to_string(),
        proposal_id: proposal.proposal_id.clone(),
        agent_id: proposal.agent_id.clone(),
        correlation_id: proposal.correlation_id.clone(),
        action_type: proposal.action_type.clone(),
        target: coords(&proposal.target_position),
        from: coords(&from),
        priority: proposal.priority,
        observation_id: observation.map(|o| o.id.clone()),
        observed_at: observation.map(|o| o.observed_at),
        observation_age_ms: outcome.validation.observation_age_ms.max(0),
        observation_quality: observation
            .map(|o| enum_str(&o.quality))
            .unwrap_or_else(|| "MISSING".to_string()),
        trigger_event_id: trigger_event_id.to_string(),
        validation: ValidationSummary {
            event_id: validation_event
                .map(|e| e.event_id.clone())
                .unwrap_or_default(),
            accepted: outcome.validation.accepted,
            reasons: outcome.validation.reasons.clone(),
            failed_checks,
            confidence: outcome.validation.confidence,
        },
        judgment: match (judgment_event, outcome.judgment.as_ref()) {
            (Some(event), Some(envelope)) => Some(judgment_digest(
                &event.event_id,
                envelope,
                event.ai_involved,
            )),
            _ => None,
        },
        judgment_skip_reason: outcome
            .decision
            .judgment
            .as_ref()
            .and_then(|summary| summary.skip_reason.clone()),
        policy: PolicySummary {
            event_id: commitment_event
                .map(|e| e.event_id.clone())
                .unwrap_or_default(),
            outcome: outcome.decision.outcome.as_str().to_string(),
            reason_codes: outcome.decision.reason_codes.clone(),
            basis: outcome.decision.basis.as_ref().map(enum_str),
            adaptive_level: outcome.decision.adaptive_level.as_str().to_string(),
        },
        transition: match (transition_event, outcome.transition.as_ref()) {
            (Some(event), Some(transition)) => Some(TransitionSummary {
                event_id: event.event_id.clone(),
                revision: transition.revision,
                before_hash: transition.before_hash.clone(),
                after_hash: transition.after_hash.clone(),
            }),
            _ => None,
        },
        committed: outcome.committed,
        oracle_unsafe: oracle.map(str::to_string),
        rationale,
        duplicate_submission: duplicate,
    }
}

fn positions(snapshot: &StateSnapshot) -> BTreeMap<String, Vector3> {
    snapshot
        .agents
        .iter()
        .map(|(id, agent)| (id.clone(), agent.position))
        .collect()
}

/// Execute one run.
pub async fn run(config: &RunConfig, mut options: RunOptions) -> Result<RunResult, RunError> {
    config
        .validate()
        .map_err(|error| RunError::Config(error.to_string()))?;
    let wall_start = Instant::now();
    let scenario = &config.scenario;
    let genesis_hash = config.hash();
    let clock = Arc::new(ManualClock::new(scenario.start_time_ms));
    let ids = Arc::new(SequentialIds::new());
    let stamp = EventStamp {
        run_id: config.run_id.clone(),
        experiment_id: config.experiment_id.clone(),
        mode: options.mode.unwrap_or(DataMode::Simulated),
        software_version: SOFTWARE_VERSION.to_string(),
    };

    let replaying_judgment = options.substitutions.judgments.as_ref();
    let (judge, judgment_is_recorded) =
        build_judge(config, replaying_judgment, options.allow_network)?;
    let mut substituted = Vec::new();
    if replaying_judgment.is_some() && judge.is_some() {
        substituted.push("judgment".to_string());
    }
    let judgment_policy = scenario.kernel.judgment_policy.to_policy();
    // The kernel gets a private bus: the recorder republishes every event, kernel
    // events included, once they are chained.
    let mut builder = KernelEngine::builder(EventBus::new(16))
        .validator(scenario.kernel.validator.clone())
        .policy(scenario.kernel.policy.clone())
        .clock(clock.clone())
        .ids(ids.clone())
        .stamp(stamp.clone());
    if let Some(judge) = &judge {
        builder = if judgment_is_recorded {
            builder.replay_judgments(judge.clone(), judgment_policy.clone())
        } else {
            builder.judgment(judge.clone(), judgment_policy.clone())
        };
    }
    let kernel = builder
        .build()
        .map_err(|error| RunError::Kernel(error.to_string()))?;
    let validator_config_hash = kernel.validator().config_hash().to_string();

    let lab = |tick: u64| Lab {
        stamp: stamp.clone(),
        clock: clock.clone(),
        ids: ids.clone(),
        tick,
    };
    let mut recorder = Recorder {
        chain: EventChain::new(genesis_hash.clone()),
        events: Vec::new(),
        bus: None,
        tick_start: 0,
    };
    if let Some(bus) = options.bus.take() {
        // The kernel publishes its own events; the recorder publishes lab events.
        recorder.bus = Some(bus);
    }

    // Setup: registration and initial hazards (tick 0, before agents act).
    let setup = lab(0);
    kernel.set_tick(Some(0)).await;
    let start_event = setup.event(
        "run_started",
        LAB_SOURCE,
        &config.run_id,
        &config.run_id,
        &genesis_hash,
        "mesh.lab.runner",
        false,
        json!({
            "experiment_id": config.experiment_id,
            "condition_id": config.condition_id,
            "repetition": config.repetition,
            "seed": config.seed,
            "scenario": scenario.id,
            "run_config_hash": genesis_hash,
            "agents": scenario.agents.iter().map(|a| a.id.clone()).collect::<Vec<_>>(),
            "judge": judge.as_ref().map(|j| j.descriptor()),
            "substituted": substituted,
        }),
    );
    let start_id = recorder.record(start_event);
    for spec in &scenario.agents {
        let mut agent =
            AuthoritativeAgentState::idle(&spec.id, point(spec.start), scenario.start_time_ms);
        agent.priority = spec.priority;
        agent.capabilities = vec![crate::agents::behavior_name(spec.behavior).to_string()];
        let receipt = kernel
            .register_agent(agent)
            .await
            .map_err(|error| RunError::Kernel(error.to_string()))?;
        let mut events = receipt.events;
        for event in &mut events {
            event.causation_id = start_id.clone();
        }
        recorder.record_all(events);
    }

    let mut adapters: BTreeMap<String, Box<dyn MeshAgentAdapter>> = scenario
        .agents
        .iter()
        .map(|spec| {
            (
                spec.id.clone(),
                adapter_for(config, spec, &options.substitutions),
            )
        })
        .collect();
    for (id, intents) in &options.substitutions.intents {
        if adapters.contains_key(id) && !intents.is_empty() {
            substituted.push(format!("agent:{id}"));
        }
    }
    let agent_descriptors: Vec<_> = adapters.values().map(|a| a.descriptor()).collect();
    let record_intents_for: BTreeSet<String> = agent_descriptors
        .iter()
        .filter(|d| !d.deterministic || d.adapter.starts_with("recorded:"))
        .map(|d| d.agent_id.clone())
        .collect();
    let mut recorded_intents = Vec::new();

    let mut sensors = Sensors::new(
        config.seed,
        scenario.sensors.position_noise,
        scenario.sensors.latency_ticks,
        scenario.dt_ms,
    );
    let mut load_model = OperatorLoadModel::new(&scenario.human_state, config.seed);
    let trace = match &scenario.human_state.trace {
        Some(path) => Some(sim::load_trace(path).map_err(RunError::Config)?),
        None => None,
    };
    let constraints = Constraints {
        max_step: scenario.kernel.validator.max_step,
        min_separation: scenario.kernel.validator.min_separation,
        allowed_actions: scenario.kernel.validator.allowed_actions.clone(),
    };

    let mut faults: Vec<FaultSpec> = scenario.faults.clone();
    let mut decisions: Vec<DecisionRecord> = Vec::new();
    let mut last_observation: BTreeMap<String, ObservationInfo> = BTreeMap::new();
    let mut pending_reviews: BTreeMap<String, (u64, String)> = BTreeMap::new();
    let mut rejection_ticks: Vec<u64> = Vec::new();
    let mut latencies: Vec<f64> = Vec::new();
    let mut termination = "max_ticks".to_string();
    let mut convergence_tick = None;
    let mut ticks_run = 0;
    let declared_total = scenario.hazards.len();
    let mut last_level = None;

    for tick in 0..scenario.ticks {
        if options
            .control
            .as_ref()
            .is_some_and(|control| control.should_stop())
        {
            termination = "stopped_by_operator".to_string();
            break;
        }
        ticks_run = tick + 1;
        let now = scenario.start_time_ms + tick as i64 * scenario.dt_ms;
        clock.set(now);
        kernel.set_tick(Some(tick)).await;
        recorder.tick_start = recorder.events.len();
        let lab = lab(tick);

        // Interactive commands (live sessions) or their recordings (replay).
        let mut commands = options
            .substitutions
            .commands
            .get(&tick)
            .cloned()
            .unwrap_or_default();
        if let Some(control) = options.control.as_mut() {
            commands.extend(control.commands(tick));
        }
        let mut operator_proposals = Vec::new();
        let mut human_decisions = Vec::new();
        for command in commands {
            let mut command_event = lab.event(
                "operator_command",
                "operator_console",
                "operator",
                &format!("cmd-{tick}"),
                &start_id,
                "mesh.session.control",
                false,
                json!(command),
            );
            // Commands come from a person at the console.
            command_event.mode = DataMode::Live;
            recorder.record(command_event);
            match command {
                ControlCommand::InjectFault { mut fault } => {
                    fault.at_tick = tick;
                    faults.push(fault);
                }
                ControlCommand::OperatorProposal { agent_id, intent } => {
                    operator_proposals.push((agent_id, intent))
                }
                ControlCommand::HumanDecision {
                    proposal_id,
                    approve,
                    note,
                } => human_decisions.push((proposal_id, approve, note)),
            }
        }

        // Faults that start or end at this tick.
        for fault in faults.iter().filter(|f| f.at_tick == tick) {
            recorder.record(lab.event(
                "fault_injected",
                "mesh.lab.faults",
                fault.target.as_deref().unwrap_or("mesh"),
                &format!("fault-{}", fault.label()),
                &start_id,
                "mesh.lab.fault_schedule",
                false,
                json!({ "fault": fault, "label": fault.label() }),
            ));
        }
        for fault in faults
            .iter()
            .filter(|f| f.at_tick + f.duration_ticks == tick && f.duration_ticks > 0)
        {
            recorder.record(lab.event(
                "fault_cleared",
                "mesh.lab.faults",
                fault.target.as_deref().unwrap_or("mesh"),
                &format!("fault-{}", fault.label()),
                &start_id,
                "mesh.lab.fault_schedule",
                false,
                json!({ "fault": fault, "label": fault.label() }),
            ));
        }
        let active: Vec<&FaultSpec> = faults.iter().filter(|f| f.active_at(tick)).collect();
        if let Some(judge) = &judge {
            let mut state = JudgeFaults::default();
            for fault in &active {
                match fault.kind {
                    FaultKind::DisableJudge => state.disabled = true,
                    FaultKind::JudgeTimeout => state.timeout = true,
                    FaultKind::NetworkUnavailable => state.network_down = true,
                    FaultKind::JudgeLatency => {
                        state.latency_ms = Some(fault.magnitude.max(0.0) as u64)
                    }
                    _ => {}
                }
            }
            judge.set_faults(state);
        }

        // Environment: hazards appear or retire and are declared to the kernel.
        for hazard in &scenario.hazards {
            let appears = hazard.appears_at_tick == tick;
            let retires = hazard.retires_at_tick == Some(tick);
            if !(appears || retires) {
                continue;
            }
            let change = lab.event(
                "environment_change",
                "environment",
                &hazard.id,
                &format!("env-{}", hazard.id),
                &start_id,
                &format!("scenario:{}", scenario.id),
                false,
                json!({ "change": if appears { "hazard_appeared" } else { "hazard_retired" }, "hazard": hazard }),
            );
            let change_id = recorder.record(change);
            let receipt = if appears {
                kernel.declare_hazard(hazard.zone(), &change_id).await
            } else {
                kernel.retire_hazard(&hazard.id, &change_id).await
            };
            match receipt {
                Ok(receipt) => recorder.record_all(receipt.events),
                Err(error) => return Err(RunError::Kernel(error.to_string())),
            }
        }

        // Simulated human state.
        let snapshot = kernel.snapshot().await;
        let window = load_model.window();
        let recent_rejections = rejection_ticks
            .iter()
            .filter(|t| **t + window > tick)
            .count() as u64;
        let load = load_model.step(
            tick,
            LoadInputs {
                recent_rejections,
                agents: scenario.agents.len(),
                pending_reviews: pending_reviews.len(),
                active_hazards: snapshot.hazards.len(),
                declared_hazards_total: declared_total,
            },
        );
        if scenario.human_state.enabled {
            let human_faults: Vec<&FaultSpec> = active
                .iter()
                .copied()
                .filter(|f| {
                    matches!(
                        f.kind,
                        FaultKind::HumanStateDropout | FaultKind::HumanStateDegraded
                    )
                })
                .collect();
            let datum: HumanStateDatum = match &trace {
                Some(trace) => trace.get(tick as usize).cloned().unwrap_or_else(|| {
                    let mut missing = load_model.datum(0.0, now, &[]).as_replay();
                    missing.quality = SignalQuality::Missing;
                    missing.confidence = 0.0;
                    missing
                }),
                None => load_model.datum(load, now, &human_faults),
            };
            let mut event = lab.event(
                "human_state",
                &datum.source.clone(),
                "operator",
                &format!("hs-{tick}"),
                &start_id,
                &format!("{}:{}", datum.source, datum.metric),
                false,
                json!({ "datum": datum }),
            );
            event.mode = datum.mode;
            let hs_id = recorder.record(event);
            if let Some(level_event) = kernel.observe_human_state(&datum, &hs_id).await {
                last_level = level_event.payload["level"].as_str().map(str::to_string);
                recorder.record(level_event);
            }
        }
        let current_load = load;

        // Human review: interactive decisions, then the simulated operator or expiry.
        let mut due: Vec<(String, bool, String, bool)> = human_decisions
            .into_iter()
            .map(|(id, approve, note)| (id, approve, note, false))
            .collect();
        match scenario.human_review.mode {
            ReviewMode::SimulatedOperator => {
                for (proposal_id, (requested, _)) in &pending_reviews {
                    if tick >= requested + scenario.human_review.latency_ticks
                        && !due.iter().any(|d| &d.0 == proposal_id)
                    {
                        let approve = current_load < scenario.human_review.approve_below_load;
                        due.push((
                            proposal_id.clone(),
                            approve,
                            format!("simulated operator, load index {current_load:.2}"),
                            true,
                        ));
                    }
                }
            }
            ReviewMode::Expire => {
                for (proposal_id, (requested, _)) in &pending_reviews {
                    if tick >= requested + scenario.human_review.latency_ticks {
                        due.push((proposal_id.clone(), false, "review expired".into(), true));
                    }
                }
            }
            ReviewMode::Manual => {}
        }
        for (proposal_id, approve, note, simulated) in due {
            let Some((_, agent_id)) = pending_reviews.remove(&proposal_id) else {
                continue;
            };
            let from = snapshot
                .agents
                .get(&agent_id)
                .map(|a| a.position)
                .unwrap_or(Vector3::ZERO);
            let started = Instant::now();
            let resolved = kernel
                .resolve_human_review(HumanReviewDecision {
                    proposal_id: proposal_id.clone(),
                    approve,
                    operator_ref: if simulated {
                        "simulated_operator".into()
                    } else {
                        "operator_console".into()
                    },
                    note,
                    simulated_operator: simulated,
                })
                .await;
            latencies.push(started.elapsed().as_secs_f64() * 1000.0);
            let Ok(outcome) = resolved else { continue };
            let trigger = outcome
                .events
                .first()
                .map(|e| e.event_id.clone())
                .unwrap_or_default();
            recorder.record_all(outcome.events.clone());
            let proposal = rebuild_proposal(&decisions, &proposal_id);
            if let Some(proposal) = proposal {
                let hazards: Vec<_> = scenario
                    .hazards
                    .iter()
                    .filter(|h| h.active_at(tick))
                    .collect();
                let oracle = sim::oracle_unsafe(
                    &from,
                    &proposal.target_position,
                    &hazards,
                    scenario.kernel.validator.max_spatial_bound,
                );
                decisions.push(decision_record(
                    tick,
                    "human_resolution",
                    &proposal,
                    from,
                    last_observation.get(&agent_id),
                    &trigger,
                    &outcome,
                    oracle,
                    None,
                    false,
                ));
                if let Some(adapter) = adapters.get_mut(&agent_id) {
                    adapter.receive_outcome(&ProposalOutcome {
                        proposal_id,
                        outcome: outcome.decision.outcome,
                        committed: outcome.committed,
                        reasons: outcome.decision.reason_codes.clone(),
                        committed_position: outcome.committed.then_some(proposal.target_position),
                    });
                }
            }
        }

        // OBSERVE: sensors deliver observations to agents.
        let snapshot = kernel.snapshot().await;
        let truth = positions(&snapshot);
        sensors.record_truth(&truth);
        let hazards_known: Vec<_> = snapshot.hazards.values().cloned().collect();
        let agent_ids: Vec<String> = adapters.keys().cloned().collect();
        for agent_id in &agent_ids {
            let agent_faults: Vec<&FaultSpec> = active
                .iter()
                .copied()
                .filter(|f| f.kind.targets_agent() && f.applies_to(agent_id))
                .collect();
            let goal = scenario
                .agents
                .iter()
                .find(|a| &a.id == agent_id)
                .and_then(|a| a.goal.as_ref())
                .and_then(|g| scenario.poi(g))
                .map(|poi| point(poi.position));
            let observation_id = ids.next_id("obs");
            match sensors.read(
                agent_id,
                observation_id.clone(),
                tick,
                now,
                &truth,
                goal,
                &hazards_known,
                &agent_faults,
            ) {
                SensorReading::Dropped(reason) => {
                    recorder.record(lab.event(
                        "fault_effect",
                        "mesh.lab.faults",
                        agent_id,
                        &format!("obs-{tick}-{agent_id}"),
                        &start_id,
                        "mesh.lab.sensors",
                        false,
                        json!({ "effect": reason, "agent_id": agent_id }),
                    ));
                }
                SensorReading::Delivered(observation, effect) => {
                    let mut event = lab.event(
                        "observation",
                        &format!("sensor.{agent_id}"),
                        agent_id,
                        &format!("obs-{tick}-{agent_id}"),
                        &start_id,
                        "mesh.lab.sensors.v1",
                        false,
                        json!({ "observation": observation_payload(&observation), "fault_effect": effect }),
                    );
                    event.event_id = observation_id.clone();
                    recorder.record(event);
                    last_observation.insert(
                        agent_id.clone(),
                        ObservationInfo {
                            id: observation_id,
                            observed_at: observation.observed_at,
                            quality: observation.quality,
                        },
                    );
                    if let Some(adapter) = adapters.get_mut(agent_id) {
                        adapter.observe(&observation);
                    }
                }
            }
        }

        // PROPOSE -> kernel.
        let context = ProposalContext {
            tick,
            now_ms: now,
            constraints: constraints.clone(),
        };
        for agent_id in &agent_ids {
            let agent_faults: Vec<&FaultSpec> = active
                .iter()
                .copied()
                .filter(|f| f.kind.targets_agent() && f.applies_to(agent_id))
                .collect();
            let frozen = agent_faults
                .iter()
                .any(|f| f.kind == FaultKind::FreezeAgent);
            let adapter = adapters.get_mut(agent_id).expect("adapter exists");
            let mut intent = if frozen {
                None
            } else {
                adapter.propose(&context)
            };
            for problem in adapter.take_faults() {
                recorder.record(lab.event(
                    "fault_effect",
                    agent_id,
                    agent_id,
                    &format!("agent-{tick}-{agent_id}"),
                    &start_id,
                    "mesh.lab.agents",
                    false,
                    json!({ "effect": "agent_protocol", "detail": problem }),
                ));
            }
            if record_intents_for.contains(agent_id) {
                recorded_intents.push(RecordedIntent {
                    tick,
                    agent_id: agent_id.clone(),
                    intent: intent.clone(),
                });
            }
            let mut source = agent_id.clone();
            if let Some(position) = operator_proposals.iter().position(|(id, _)| id == agent_id) {
                intent = Some(operator_proposals.remove(position).1);
                source = "operator_console".to_string();
            }
            let Some(intent) = intent else { continue };
            let explanation = adapters.get(agent_id).and_then(|a| a.explain());
            let observation = last_observation.get(agent_id);
            let skew = agent_faults
                .iter()
                .find(|f| f.kind == FaultKind::ClockSkew)
                .map(|f| f.magnitude as i64)
                .unwrap_or(0);
            let proposal = ActionProposal {
                proposal_id: format!("prop-{tick:05}-{agent_id}"),
                agent_id: agent_id.clone(),
                action_type: intent.action_type.clone(),
                parameters_json: "{}".to_string(),
                target_position: intent.target,
                priority: intent.priority,
                timestamp: now + skew,
                correlation_id: format!("corr-{tick:05}-{agent_id}"),
                source_observation: observation
                    .map(|o| o.id.clone())
                    .unwrap_or_else(|| "none".into()),
                observed_at: observation.map(|o| o.observed_at + skew),
            };
            let duplicates = if agent_faults
                .iter()
                .any(|f| f.kind == FaultKind::DuplicateProposal)
            {
                2
            } else {
                1
            };
            let from = snapshot
                .agents
                .get(agent_id)
                .map(|a| a.position)
                .unwrap_or(Vector3::ZERO);
            let hazards: Vec<_> = scenario
                .hazards
                .iter()
                .filter(|h| h.active_at(tick))
                .collect();
            let oracle = sim::oracle_unsafe(
                &from,
                &proposal.target_position,
                &hazards,
                scenario.kernel.validator.max_spatial_bound,
            );
            for copy in 0..duplicates {
                let proposal_event = lab.event(
                    "proposal",
                    &source,
                    agent_id,
                    &proposal.correlation_id,
                    &proposal.source_observation,
                    &format!("agent:{}", adapters[agent_id].descriptor().adapter),
                    false,
                    json!({
                        "proposal": proposal_payload(&proposal),
                        "rationale": intent.rationale,
                        "explanation": explanation,
                        "duplicate_submission": copy > 0,
                    }),
                );
                let proposal_event_id = recorder.record(proposal_event);
                let started = Instant::now();
                let outcome = kernel
                    .process_proposal(proposal.clone(), &proposal_event_id)
                    .await;
                latencies.push(started.elapsed().as_secs_f64() * 1000.0);
                if !outcome.validation.accepted {
                    rejection_ticks.push(tick);
                }
                if outcome.decision.outcome == PolicyOutcome::AwaitingHumanReview {
                    pending_reviews.insert(proposal.proposal_id.clone(), (tick, agent_id.clone()));
                }
                recorder.record_all(outcome.events.clone());
                decisions.push(decision_record(
                    tick,
                    "proposal",
                    &proposal,
                    from,
                    observation,
                    &proposal_event_id,
                    &outcome,
                    oracle,
                    Some(intent.rationale.clone()),
                    copy > 0,
                ));
                if copy == 0 {
                    if let Some(adapter) = adapters.get_mut(agent_id) {
                        adapter.receive_outcome(&ProposalOutcome {
                            proposal_id: proposal.proposal_id.clone(),
                            outcome: outcome.decision.outcome,
                            committed: outcome.committed,
                            reasons: if outcome.validation.accepted {
                                outcome.decision.reason_codes.clone()
                            } else {
                                outcome.validation.reasons.clone()
                            },
                            committed_position: outcome
                                .committed
                                .then_some(proposal.target_position),
                        });
                    }
                }
            }
        }

        // Rolling resonance sample for the timeline.
        let sample = metrics::rolling_sample(&decisions, tick, 10);
        recorder.record(lab.event(
            "resonance_sample",
            "mesh.metrics",
            "mesh",
            &format!("res-{tick}"),
            &start_id,
            metrics::METRICS_VERSION,
            false,
            json!({ "operator_load_index": load, "adaptive_level": last_level, "sample": sample }),
        ));

        // Termination.
        let snapshot = kernel.snapshot().await;
        let goals: Vec<(String, Vector3)> = scenario
            .agents
            .iter()
            .filter(|a| a.behavior != Behavior::Patrol)
            .filter_map(|a| {
                let goal = a.goal.as_ref().and_then(|g| scenario.poi(g))?;
                Some((a.id.clone(), point(goal.position)))
            })
            .collect();
        let all_reached = !goals.is_empty()
            && goals.iter().all(|(id, goal)| {
                snapshot
                    .agents
                    .get(id)
                    .is_some_and(|a| a.position.distance(goal) <= scenario.termination.goal_radius)
            });
        if all_reached && convergence_tick.is_none() {
            convergence_tick = Some(tick);
        }
        if let Some(control) = options.control.as_mut() {
            control.on_tick(tick, &recorder.events[recorder.tick_start..]);
        }
        if all_reached && scenario.termination.stop_when_all_goals_reached {
            termination = "all_goals_reached".to_string();
            break;
        }
        if let Some(delay) = options.tick_delay {
            tokio::time::sleep(delay).await;
        }
    }

    let final_state = kernel.snapshot().await;
    let final_hash = final_state.hash();
    let finish = lab(ticks_run.saturating_sub(1));
    recorder.record(finish.event(
        "run_completed",
        LAB_SOURCE,
        &config.run_id,
        &config.run_id,
        &start_id,
        "mesh.lab.runner",
        false,
        json!({
            "termination": termination,
            "ticks_run": ticks_run,
            "final_state_hash": final_hash,
            "pending_reviews": pending_reviews.keys().collect::<Vec<_>>(),
        }),
    ));

    let network_calls = judge.as_ref().map(|j| j.network_calls()).unwrap_or(0);
    let judgment_calls = judge.as_ref().map(|j| j.calls()).unwrap_or(0);
    let judgments: Vec<JudgmentEnvelope> = recorder
        .events
        .iter()
        .filter(|c| c.event.event_type == "judgment")
        .filter_map(|c| serde_json::from_value(c.event.payload.clone()).ok())
        .collect();
    let final_positions = positions(&final_state);
    let invariants =
        invariants::check_all(&genesis_hash, &recorder.events, &decisions, network_calls);
    let metrics = metrics::compute(
        &RunData {
            config,
            events: &recorder.events,
            decisions: &decisions,
            final_positions: &final_positions,
            convergence_tick,
            ticks_run,
            network_calls,
        },
        invariants,
    );
    let wall_ms = wall_start.elapsed().as_secs_f64() * 1000.0;
    latencies.sort_by(f64::total_cmp);
    let pick = |q: f64| -> f64 {
        if latencies.is_empty() {
            0.0
        } else {
            latencies[((latencies.len() - 1) as f64 * q).round() as usize]
        }
    };
    let timing = crate::record::Timing {
        wall_ms_total: quantize(wall_ms, 3),
        ticks_per_second: quantize(ticks_run as f64 / (wall_ms / 1000.0).max(1e-9), 1),
        pipeline_latency_ms_p50: quantize(pick(0.5), 4),
        pipeline_latency_ms_p95: quantize(pick(0.95), 4),
        pipeline_latency_ms_max: quantize(latencies.last().copied().unwrap_or(0.0), 4),
        note: "Wall-clock measurements on the recording machine. Not deterministic; never compared by replay.".into(),
    };
    let head_hash = recorder.chain.head().to_string();
    Ok(RunResult {
        config: config.clone(),
        genesis_hash,
        events: recorder.events,
        decisions,
        judgments,
        intents: recorded_intents,
        final_state,
        head_hash,
        metrics,
        timing,
        network_calls,
        judgment_calls,
        judge: judge.as_ref().map(|j| j.descriptor()),
        agent_descriptors,
        ticks_run,
        termination,
        substituted,
        validator_config_hash,
    })
}

fn rebuild_proposal(decisions: &[DecisionRecord], proposal_id: &str) -> Option<ActionProposal> {
    let record = decisions
        .iter()
        .find(|d| d.proposal_id == proposal_id && d.kind == "proposal")?;
    let coordinate = |value: Option<f64>| value.unwrap_or(f64::NAN);
    Some(ActionProposal {
        proposal_id: record.proposal_id.clone(),
        agent_id: record.agent_id.clone(),
        action_type: record.action_type.clone(),
        parameters_json: "{}".into(),
        target_position: Vector3::new(
            coordinate(record.target[0]),
            coordinate(record.target[1]),
            coordinate(record.target[2]),
        ),
        priority: record.priority,
        timestamp: 0,
        correlation_id: record.correlation_id.clone(),
        source_observation: record.observation_id.clone().unwrap_or_default(),
        observed_at: record.observed_at,
    })
}

/// JSON form of a proposal that keeps non-finite coordinates visible.
pub fn proposal_payload(proposal: &ActionProposal) -> Value {
    json!({
        "proposal_id": proposal.proposal_id,
        "agent_id": proposal.agent_id,
        "action_type": proposal.action_type,
        "target": coords(&proposal.target_position),
        "target_finite": proposal.target_position.is_finite(),
        "priority": proposal.priority,
        "timestamp": proposal.timestamp,
        "observed_at": proposal.observed_at,
        "correlation_id": proposal.correlation_id,
        "source_observation": proposal.source_observation,
    })
}

fn observation_payload(observation: &AgentObservation) -> Value {
    json!({
        "observation_id": observation.observation_id,
        "observed_at": observation.observed_at,
        "own_position": coords(&observation.own_position),
        "goal": observation.goal.as_ref().map(coords),
        "known_hazards": observation.known_hazards.iter().map(|h| h.id.clone()).collect::<Vec<_>>(),
        "neighbors": observation.neighbors.iter().map(|n| json!({"agent_id": n.agent_id, "position": coords(&n.position)})).collect::<Vec<_>>(),
        "quality": observation.quality,
    })
}
