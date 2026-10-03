//! Scenario and experiment manifest schema.
//!
//! A *scenario* describes the world (arena, hazards, goals, agents, simulated
//! human state, faults) and a default kernel configuration. An *experiment
//! manifest* names a scenario, a research question, and conditions that
//! override parts of the resolved configuration. Every file is parsed with
//! `deny_unknown_fields`, so a typo is an error rather than a silently ignored
//! setting.

use epistemic_validator::{HazardZone, ValidatorConfig, Vector3};
use event_bus::{canonical_json_of, sha256_hex};
use kernel_core::KernelPolicyConfig;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use thiserror::Error;
use typed_judgment::JudgmentPolicy;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("cannot read {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("cannot parse {path}: {message}")]
    Parse { path: PathBuf, message: String },
    #[error("invalid configuration: {0}")]
    Invalid(String),
    #[error("override `{path}`: {message}")]
    Override { path: String, message: String },
}

pub type Point = [f64; 2];

pub fn point(p: Point) -> Vector3 {
    Vector3::new(p[0], p[1], 0.0)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct HazardSpec {
    pub id: String,
    pub center: Point,
    pub radius: f64,
    /// Tick at which the hazard appears and is declared to the kernel.
    #[serde(default)]
    pub appears_at_tick: u64,
    /// Tick at which the hazard is retired, if ever.
    #[serde(default)]
    pub retires_at_tick: Option<u64>,
}

impl HazardSpec {
    pub fn zone(&self) -> HazardZone {
        HazardZone {
            id: self.id.clone(),
            center: point(self.center),
            radius: self.radius,
        }
    }

    pub fn active_at(&self, tick: u64) -> bool {
        tick >= self.appears_at_tick && self.retires_at_tick.is_none_or(|end| tick < end)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PoiSpec {
    pub id: String,
    pub position: Point,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Behavior {
    /// Steps toward its goal, detours around known hazards and neighbours.
    Cautious,
    /// Straight line at full speed; ignores hazards until rejections force a detour.
    Greedy,
    /// Overshoots its goal by `gain`, producing reversals.
    Oscillating,
    /// Patrols between its start and goal at background priority.
    Patrol,
    /// Cautious, but every `malformed_every` proposals is deliberately invalid.
    Malformed,
    /// Replays `script` exactly.
    Scripted,
    /// An external process speaking the JSON-lines agent protocol.
    External,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ScriptedIntent {
    pub tick: u64,
    pub action_type: String,
    pub target: Point,
    #[serde(default = "default_priority")]
    pub priority: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct AgentSpec {
    pub id: String,
    pub behavior: Behavior,
    pub start: Point,
    /// Point of interest the agent tries to reach.
    #[serde(default)]
    pub goal: Option<String>,
    #[serde(default = "default_speed")]
    pub speed: f64,
    #[serde(default = "default_priority")]
    pub priority: i32,
    /// Overshoot factor for `oscillating` agents.
    #[serde(default = "default_gain")]
    pub gain: f64,
    /// For `malformed` agents: every n-th proposal is invalid.
    #[serde(default = "default_malformed_every")]
    pub malformed_every: u64,
    #[serde(default)]
    pub script: Vec<ScriptedIntent>,
    /// For `external` agents: program and arguments.
    #[serde(default)]
    pub command: Vec<String>,
    /// For `external` agents: per-call response timeout.
    #[serde(default = "default_external_timeout")]
    pub timeout_ms: u64,
}

fn default_speed() -> f64 {
    5.0
}
fn default_priority() -> i32 {
    1
}
fn default_gain() -> f64 {
    1.8
}
fn default_malformed_every() -> u64 {
    4
}
fn default_external_timeout() -> u64 {
    2_000
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields, default)]
pub struct SensorSpec {
    /// Uniform position noise amplitude.
    pub position_noise: f64,
    /// Observation latency in ticks.
    pub latency_ticks: u64,
}

impl Default for SensorSpec {
    fn default() -> Self {
        Self {
            position_noise: 0.25,
            latency_ticks: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Perturbation {
    pub at_tick: u64,
    pub duration_ticks: u64,
    /// Added to the target load while active.
    pub delta: f64,
}

/// Simulated operator-load model `operator_load_v1`.
///
/// ```text
/// target(t) = baseline
///           + w_rejections * (deterministic rejections in the last `window` ticks / (window * agents))
///           + w_reviews    * (pending human reviews / agents)
///           + w_complexity * (active hazards / declared hazards in the scenario)
///           + sum of active perturbation deltas
///           + uniform noise in [-noise, noise)
/// load(t)   = clamp(load(t-1) + alpha * (target(t) - load(t-1)), 0, 1)
/// ```
///
/// This is a load *index* for experiments, not a physiological model.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields, default)]
pub struct HumanStateSpec {
    pub enabled: bool,
    pub baseline: f64,
    pub alpha: f64,
    pub w_rejections: f64,
    pub w_reviews: f64,
    pub w_complexity: f64,
    pub window: u64,
    pub noise: f64,
    pub perturbations: Vec<Perturbation>,
    /// Replay a recorded human-state trace instead of simulating (JSONL of datums).
    pub trace: Option<PathBuf>,
}

impl Default for HumanStateSpec {
    fn default() -> Self {
        Self {
            enabled: true,
            baseline: 0.25,
            alpha: 0.3,
            w_rejections: 0.6,
            w_reviews: 0.3,
            w_complexity: 0.15,
            window: 6,
            noise: 0.01,
            perturbations: Vec::new(),
            trace: None,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReviewMode {
    /// A simulated operator decides after `latency_ticks`. Decisions are labeled SIMULATED.
    SimulatedOperator,
    /// Pending reviews expire unapproved after `latency_ticks`.
    Expire,
    /// Reviews stay pending (interactive sessions resolve them by hand).
    Manual,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields, default)]
pub struct HumanReviewSpec {
    pub mode: ReviewMode,
    pub latency_ticks: u64,
    /// The simulated operator approves only while its load index is below this.
    pub approve_below_load: f64,
}

impl Default for HumanReviewSpec {
    fn default() -> Self {
        Self {
            mode: ReviewMode::SimulatedOperator,
            latency_ticks: 3,
            approve_below_load: 0.75,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum FaultKind {
    /// Observations for the target are not delivered.
    SensorDropout,
    /// Observations arrive `magnitude` ms late.
    ObservationDelay,
    /// Observations carry a timestamp `magnitude` ms older than real (stale telemetry).
    StaleTelemetry,
    /// Observed position is offset by `magnitude` (or NaN when magnitude is 0).
    CorruptObservation,
    /// Observations are flagged INVALID by the sensor.
    InvalidateObservation,
    /// Every proposal from the target is submitted twice.
    DuplicateProposal,
    /// The target's clock runs `magnitude` ms ahead.
    ClockSkew,
    /// The target agent produces no proposals.
    FreezeAgent,
    /// Judgment provider is disabled (returns disabled status).
    DisableJudge,
    /// Judgment provider times out.
    JudgeTimeout,
    /// Judgment provider cannot be reached.
    NetworkUnavailable,
    /// Judgment latency is raised to `magnitude` ms (timeout above the judgment timeout).
    JudgeLatency,
    /// The human-state signal is missing.
    HumanStateDropout,
    /// The human-state signal is degraded (lower confidence, extra noise).
    HumanStateDegraded,
}

impl FaultKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SensorDropout => "sensor_dropout",
            Self::ObservationDelay => "observation_delay",
            Self::StaleTelemetry => "stale_telemetry",
            Self::CorruptObservation => "corrupt_observation",
            Self::InvalidateObservation => "invalidate_observation",
            Self::DuplicateProposal => "duplicate_proposal",
            Self::ClockSkew => "clock_skew",
            Self::FreezeAgent => "freeze_agent",
            Self::DisableJudge => "disable_judge",
            Self::JudgeTimeout => "judge_timeout",
            Self::NetworkUnavailable => "network_unavailable",
            Self::JudgeLatency => "judge_latency",
            Self::HumanStateDropout => "human_state_dropout",
            Self::HumanStateDegraded => "human_state_degraded",
        }
    }

    pub const ALL: [FaultKind; 14] = [
        Self::SensorDropout,
        Self::ObservationDelay,
        Self::StaleTelemetry,
        Self::CorruptObservation,
        Self::InvalidateObservation,
        Self::DuplicateProposal,
        Self::ClockSkew,
        Self::FreezeAgent,
        Self::DisableJudge,
        Self::JudgeTimeout,
        Self::NetworkUnavailable,
        Self::JudgeLatency,
        Self::HumanStateDropout,
        Self::HumanStateDegraded,
    ];

    pub fn targets_agent(self) -> bool {
        matches!(
            self,
            Self::SensorDropout
                | Self::ObservationDelay
                | Self::StaleTelemetry
                | Self::CorruptObservation
                | Self::InvalidateObservation
                | Self::DuplicateProposal
                | Self::ClockSkew
                | Self::FreezeAgent
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FaultSpec {
    pub kind: FaultKind,
    /// Agent id for agent-level faults; `all` for every agent.
    #[serde(default)]
    pub target: Option<String>,
    pub at_tick: u64,
    pub duration_ticks: u64,
    #[serde(default)]
    pub magnitude: f64,
}

impl FaultSpec {
    pub fn active_at(&self, tick: u64) -> bool {
        tick >= self.at_tick && tick < self.at_tick + self.duration_ticks
    }

    pub fn applies_to(&self, agent_id: &str) -> bool {
        match self.target.as_deref() {
            None | Some("all") => true,
            Some(target) => target == agent_id,
        }
    }

    pub fn label(&self) -> String {
        match &self.target {
            Some(target) => format!("{}:{target}@{}", self.kind.as_str(), self.at_tick),
            None => format!("{}@{}", self.kind.as_str(), self.at_tick),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum JudgeProfile {
    /// Every proposal judged supported (scripted).
    Supported,
    /// Responds to evidence staleness and signal quality (`evidence-heuristic-v1`).
    EvidenceHeuristic,
    /// Disagrees with a stable third of proposals (`contrarian-v1`).
    Contrarian,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum JudgeProvider {
    /// No judgment stage.
    Disabled,
    /// Deterministic offline judge.
    Mock,
    /// Remote TypeSafe Jev. Requires credentials; never used in replay.
    Typesafe,
    /// Judgments recorded in another run (`recorded_from`).
    Recorded,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields, default)]
pub struct JudgmentSpec {
    pub provider: JudgeProvider,
    pub profile: JudgeProfile,
    /// Run bundle whose judgments a `recorded` provider replays.
    pub recorded_from: Option<PathBuf>,
    /// In an experiment: replay the judgments recorded by this condition in
    /// the same repetition (same seed).
    pub recorded_from_condition: Option<String>,
    /// Judgment timeout used to classify injected latency.
    pub timeout_ms: u64,
}

impl Default for JudgmentSpec {
    fn default() -> Self {
        Self {
            provider: JudgeProvider::Disabled,
            profile: JudgeProfile::EvidenceHeuristic,
            recorded_from: None,
            recorded_from_condition: None,
            timeout_ms: 10_000,
        }
    }
}

/// Serializable thresholds of [`JudgmentPolicy`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields, default)]
pub struct JudgmentPolicySpec {
    pub version: String,
    pub minimum_confidence: f64,
    pub minimum_evidence_score: f64,
    pub scope_violation_threshold: f64,
    pub contradiction_revise_threshold: f64,
    pub contradiction_review_threshold: f64,
    pub human_review_threshold: f64,
}

impl Default for JudgmentPolicySpec {
    fn default() -> Self {
        let policy = JudgmentPolicy::default();
        Self {
            version: policy.version,
            minimum_confidence: policy.minimum_confidence,
            minimum_evidence_score: policy.minimum_evidence_score,
            scope_violation_threshold: policy.scope_violation_threshold,
            contradiction_revise_threshold: policy.contradiction_revise_threshold,
            contradiction_review_threshold: policy.contradiction_review_threshold,
            human_review_threshold: policy.human_review_threshold,
        }
    }
}

impl JudgmentPolicySpec {
    pub fn to_policy(&self) -> JudgmentPolicy {
        JudgmentPolicy {
            version: self.version.clone(),
            minimum_confidence: self.minimum_confidence,
            minimum_evidence_score: self.minimum_evidence_score,
            scope_violation_threshold: self.scope_violation_threshold,
            contradiction_revise_threshold: self.contradiction_revise_threshold,
            contradiction_review_threshold: self.contradiction_review_threshold,
            human_review_threshold: self.human_review_threshold,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(deny_unknown_fields, default)]
pub struct KernelSpec {
    pub validator: ValidatorConfig,
    pub judgment: JudgmentSpec,
    pub judgment_policy: JudgmentPolicySpec,
    pub policy: KernelPolicyConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields, default)]
pub struct Termination {
    pub stop_when_all_goals_reached: bool,
    /// Distance at which a goal counts as reached.
    pub goal_radius: f64,
}

impl Default for Termination {
    fn default() -> Self {
        Self {
            stop_when_all_goals_reached: true,
            goal_radius: 1.5,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Scenario {
    pub id: String,
    pub description: String,
    pub ticks: u64,
    #[serde(default = "default_dt")]
    pub dt_ms: i64,
    #[serde(default = "default_start")]
    pub start_time_ms: i64,
    /// Half-width of the square arena the scenario is designed for.
    #[serde(default = "default_extent")]
    pub arena_half_extent: f64,
    #[serde(default)]
    pub hazards: Vec<HazardSpec>,
    #[serde(default)]
    pub points_of_interest: Vec<PoiSpec>,
    pub agents: Vec<AgentSpec>,
    #[serde(default)]
    pub sensors: SensorSpec,
    #[serde(default)]
    pub human_state: HumanStateSpec,
    #[serde(default)]
    pub human_review: HumanReviewSpec,
    #[serde(default)]
    pub faults: Vec<FaultSpec>,
    #[serde(default)]
    pub kernel: KernelSpec,
    #[serde(default)]
    pub termination: Termination,
}

fn default_dt() -> i64 {
    250
}
fn default_start() -> i64 {
    1_700_000_000_000
}
fn default_extent() -> f64 {
    100.0
}

impl Scenario {
    pub fn validate(&self) -> Result<(), ConfigError> {
        let invalid = |message: String| Err(ConfigError::Invalid(message));
        if self.ticks == 0 || self.ticks > 100_000 {
            return invalid(format!("ticks must be in 1..=100000, got {}", self.ticks));
        }
        if self.dt_ms <= 0 {
            return invalid("dt_ms must be positive".into());
        }
        if self.agents.is_empty() {
            return invalid("a scenario needs at least one agent".into());
        }
        let mut ids = std::collections::BTreeSet::new();
        for agent in &self.agents {
            if !ids.insert(agent.id.as_str()) {
                return invalid(format!("duplicate agent id `{}`", agent.id));
            }
            if !valid_id(&agent.id) {
                return invalid(format!(
                    "agent id `{}` must match [A-Za-z0-9_.-]{{1,64}}",
                    agent.id
                ));
            }
            if let Some(goal) = &agent.goal {
                if !self.points_of_interest.iter().any(|poi| &poi.id == goal) {
                    return invalid(format!("agent `{}` has unknown goal `{goal}`", agent.id));
                }
            }
            if !(agent.speed.is_finite() && agent.speed > 0.0) {
                return invalid(format!("agent `{}` speed must be positive", agent.id));
            }
            if agent.behavior == Behavior::External && agent.command.is_empty() {
                return invalid(format!("external agent `{}` needs a command", agent.id));
            }
            if agent.behavior == Behavior::Malformed && agent.malformed_every == 0 {
                return invalid(format!("agent `{}` malformed_every must be >= 1", agent.id));
            }
        }
        for hazard in &self.hazards {
            if !(hazard.radius.is_finite() && hazard.radius > 0.0) || !valid_id(&hazard.id) {
                return invalid(format!("hazard `{}` is invalid", hazard.id));
            }
        }
        for fault in &self.faults {
            if fault.kind.targets_agent() {
                if let Some(target) = &fault.target {
                    if target != "all" && !ids.contains(target.as_str()) {
                        return invalid(format!("fault targets unknown agent `{target}`"));
                    }
                }
            }
            if !fault.magnitude.is_finite() {
                return invalid(format!("fault {} magnitude must be finite", fault.label()));
            }
        }
        let h = &self.human_state;
        for (name, value) in [
            ("baseline", h.baseline),
            ("alpha", h.alpha),
            ("noise", h.noise),
        ] {
            if !(0.0..=1.0).contains(&value) {
                return invalid(format!("human_state.{name} must be in [0, 1]"));
            }
        }
        self.kernel
            .validator
            .validate()
            .map_err(|error| ConfigError::Invalid(error.to_string()))?;
        Ok(())
    }

    pub fn poi(&self, id: &str) -> Option<&PoiSpec> {
        self.points_of_interest.iter().find(|poi| poi.id == id)
    }
}

pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.'))
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Decrease,
    Increase,
    NoChange,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Prediction {
    pub metric: String,
    pub direction: Direction,
    pub baseline: String,
    pub treatment: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Hypothesis {
    pub statement: String,
    #[serde(default)]
    pub prediction: Option<Prediction>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Condition {
    pub id: String,
    #[serde(default)]
    pub description: String,
    /// Dotted-path overrides applied to the resolved run configuration.
    #[serde(default)]
    pub set: BTreeMap<String, Value>,
    /// Skip (and report as unavailable) when a capability is missing, e.g. `typesafe`.
    #[serde(default)]
    pub requires: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ExperimentManifest {
    pub id: String,
    pub title: String,
    pub question: String,
    pub hypothesis: Hypothesis,
    #[serde(default)]
    pub independent_variables: Vec<String>,
    #[serde(default)]
    pub dependent_variables: Vec<String>,
    pub seed: u64,
    pub repetitions: u32,
    /// Path to the scenario, relative to the manifest.
    pub scenario: PathBuf,
    /// Overrides applied to every condition before the condition's own.
    #[serde(default)]
    pub set: BTreeMap<String, Value>,
    pub conditions: Vec<Condition>,
    #[serde(default)]
    pub telemetry: Vec<String>,
    /// Invariants that must hold in every run. A violation fails the experiment.
    #[serde(default)]
    pub invariants: Vec<String>,
    #[serde(default)]
    pub metrics: Vec<String>,
    #[serde(default)]
    pub limitations: Vec<String>,
}

impl ExperimentManifest {
    pub fn validate(&self) -> Result<(), ConfigError> {
        if !valid_id(&self.id) {
            return Err(ConfigError::Invalid(format!(
                "experiment id `{}` must match [A-Za-z0-9_.-]{{1,64}}",
                self.id
            )));
        }
        if self.repetitions == 0 || self.repetitions > 10_000 {
            return Err(ConfigError::Invalid(
                "repetitions must be in 1..=10000".into(),
            ));
        }
        if self.conditions.is_empty() {
            return Err(ConfigError::Invalid(
                "at least one condition is required".into(),
            ));
        }
        let mut seen = std::collections::BTreeSet::new();
        for condition in &self.conditions {
            if !valid_id(&condition.id) || !seen.insert(&condition.id) {
                return Err(ConfigError::Invalid(format!(
                    "condition id `{}` is invalid or duplicated",
                    condition.id
                )));
            }
        }
        if let Some(prediction) = &self.hypothesis.prediction {
            for side in [&prediction.baseline, &prediction.treatment] {
                if !seen.contains(side) {
                    return Err(ConfigError::Invalid(format!(
                        "prediction names unknown condition `{side}`"
                    )));
                }
            }
        }
        for invariant in &self.invariants {
            if !crate::invariants::KNOWN.contains(&invariant.as_str()) {
                return Err(ConfigError::Invalid(format!(
                    "unknown invariant `{invariant}`"
                )));
            }
        }
        Ok(())
    }
}

/// Fully resolved configuration of one run. This is what is recorded in
/// `manifest.json` and what replay re-executes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RunConfig {
    pub run_id: String,
    pub experiment_id: String,
    pub condition_id: String,
    pub repetition: u32,
    pub seed: u64,
    pub scenario: Scenario,
    /// Overrides that produced this configuration, for provenance.
    #[serde(default)]
    pub overrides: BTreeMap<String, Value>,
}

impl RunConfig {
    pub fn hash(&self) -> String {
        sha256_hex(
            canonical_json_of(self)
                .expect("run config serializes")
                .as_bytes(),
        )
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        if !valid_run_id(&self.run_id) {
            return Err(ConfigError::Invalid(format!(
                "run id `{}` is invalid",
                self.run_id
            )));
        }
        self.scenario.validate()
    }
}

pub fn valid_run_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 160
        && !id.starts_with('.')
        && !id.contains("..")
        && id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.' | '~'))
}

pub fn run_id(experiment: &str, condition: &str, rep: u32, seed: u64) -> String {
    format!("{experiment}.{condition}.r{rep:03}.s{seed}")
}

/// Apply dotted-path overrides to a serializable configuration and re-check it.
pub fn apply_overrides<T>(value: &T, overrides: &BTreeMap<String, Value>) -> Result<T, ConfigError>
where
    T: Serialize + for<'de> Deserialize<'de>,
{
    let mut tree =
        serde_json::to_value(value).map_err(|error| ConfigError::Invalid(error.to_string()))?;
    for (path, replacement) in overrides {
        set_path(&mut tree, path, replacement.clone())?;
    }
    serde_json::from_value(tree).map_err(|error| ConfigError::Override {
        path: overrides.keys().cloned().collect::<Vec<_>>().join(", "),
        message: error.to_string(),
    })
}

/// Set `a.b.c` (array indices allowed: `agents.2.speed`). Only existing object
/// keys or fields declared by the schema can be set; unknown keys are rejected
/// when the result is deserialized.
pub fn set_path(tree: &mut Value, path: &str, replacement: Value) -> Result<(), ConfigError> {
    let parts: Vec<&str> = path.split('.').collect();
    if parts.iter().any(|part| part.is_empty()) {
        return Err(ConfigError::Override {
            path: path.to_string(),
            message: "empty path segment".to_string(),
        });
    }
    let mut cursor = tree;
    for (index, part) in parts.iter().enumerate() {
        let last = index + 1 == parts.len();
        cursor = match cursor {
            Value::Object(map) => {
                if last {
                    map.insert((*part).to_string(), replacement);
                    return Ok(());
                }
                map.entry((*part).to_string())
                    .or_insert_with(|| Value::Object(Default::default()))
            }
            Value::Array(items) => {
                let position = resolve_index(items, part).ok_or_else(|| ConfigError::Override {
                    path: path.to_string(),
                    message: format!("`{part}` is not an index or id in this list"),
                })?;
                if last {
                    items[position] = replacement;
                    return Ok(());
                }
                &mut items[position]
            }
            Value::Null if !last => {
                *cursor = Value::Object(Default::default());
                match cursor {
                    Value::Object(map) => map
                        .entry((*part).to_string())
                        .or_insert_with(|| Value::Object(Default::default())),
                    _ => unreachable!(),
                }
            }
            _ => {
                return Err(ConfigError::Override {
                    path: path.to_string(),
                    message: format!("cannot descend into `{part}`"),
                })
            }
        };
    }
    Ok(())
}

/// A list element is addressed by numeric index or by its `id` field.
fn resolve_index(items: &[Value], part: &str) -> Option<usize> {
    if let Ok(index) = part.parse::<usize>() {
        return (index < items.len()).then_some(index);
    }
    items
        .iter()
        .position(|item| item.get("id").and_then(Value::as_str) == Some(part))
}

/// Remove list elements by id: `agents.-agent_03` style removal is expressed
/// as the override `remove_agents: [agent_03]`, handled here.
pub fn remove_agents(scenario: &mut Scenario, ids: &[String]) {
    scenario.agents.retain(|agent| !ids.contains(&agent.id));
    scenario
        .faults
        .retain(|fault| !matches!(&fault.target, Some(target) if ids.contains(target)));
}

/// Parse `key=value` where value is YAML (so `true`, `0.8`, `disabled`, `[a, b]` all work).
pub fn parse_assignment(text: &str) -> Result<(String, Value), ConfigError> {
    let (key, raw) = text.split_once('=').ok_or_else(|| ConfigError::Override {
        path: text.to_string(),
        message: "expected key=value".to_string(),
    })?;
    let value: Value = serde_yaml::from_str(raw).map_err(|error| ConfigError::Override {
        path: key.to_string(),
        message: error.to_string(),
    })?;
    Ok((key.trim().to_string(), value))
}

pub fn read_yaml<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, ConfigError> {
    let text = std::fs::read_to_string(path).map_err(|source| ConfigError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    if text.len() > 4 * 1024 * 1024 {
        return Err(ConfigError::Invalid(format!(
            "{} is larger than 4 MiB",
            path.display()
        )));
    }
    let parsed = if path.extension().and_then(|ext| ext.to_str()) == Some("json") {
        serde_json::from_str(&text).map_err(|error| error.to_string())
    } else {
        serde_yaml::from_str(&text).map_err(|error| error.to_string())
    };
    parsed.map_err(|message| ConfigError::Parse {
        path: path.to_path_buf(),
        message,
    })
}

pub fn load_scenario(path: &Path) -> Result<Scenario, ConfigError> {
    let scenario: Scenario = read_yaml(path)?;
    scenario.validate()?;
    Ok(scenario)
}

pub fn load_manifest(path: &Path) -> Result<(ExperimentManifest, Scenario), ConfigError> {
    let manifest: ExperimentManifest = read_yaml(path)?;
    manifest.validate()?;
    let base = path.parent().unwrap_or_else(|| Path::new("."));
    let scenario = load_scenario(&base.join(&manifest.scenario))?;
    Ok((manifest, scenario))
}

/// Overrides that are not plain field paths.
pub const REMOVE_AGENTS_KEY: &str = "remove_agents";

/// Resolve one (condition, repetition) of an experiment.
pub fn resolve_run(
    manifest: &ExperimentManifest,
    scenario: &Scenario,
    condition: &Condition,
    repetition: u32,
) -> Result<RunConfig, ConfigError> {
    let seed = crate::rng::repetition_seed(manifest.seed, repetition);
    let mut overrides = manifest.set.clone();
    overrides.extend(condition.set.clone());
    resolve_with_overrides(
        scenario,
        &manifest.id,
        &condition.id,
        repetition,
        seed,
        overrides,
    )
}

pub fn resolve_with_overrides(
    scenario: &Scenario,
    experiment_id: &str,
    condition_id: &str,
    repetition: u32,
    seed: u64,
    overrides: BTreeMap<String, Value>,
) -> Result<RunConfig, ConfigError> {
    let mut field_overrides = overrides.clone();
    let removals = field_overrides.remove(REMOVE_AGENTS_KEY);
    let mut resolved: Scenario = apply_overrides(scenario, &field_overrides)?;
    if let Some(removals) = removals {
        let ids: Vec<String> =
            serde_json::from_value(removals).map_err(|error| ConfigError::Override {
                path: REMOVE_AGENTS_KEY.to_string(),
                message: error.to_string(),
            })?;
        remove_agents(&mut resolved, &ids);
    }
    resolved.validate()?;
    let config = RunConfig {
        run_id: run_id(experiment_id, condition_id, repetition, seed),
        experiment_id: experiment_id.to_string(),
        condition_id: condition_id.to_string(),
        repetition,
        seed,
        scenario: resolved,
        overrides,
    };
    config.validate()?;
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn scenario() -> Scenario {
        serde_yaml::from_str(
            r#"
id: tiny
description: test
ticks: 10
points_of_interest: [{id: goal, position: [10, 0]}]
agents:
  - {id: a1, behavior: cautious, start: [0, 0], goal: goal}
  - {id: a2, behavior: greedy, start: [0, 5], goal: goal}
"#,
        )
        .unwrap()
    }

    #[test]
    fn unknown_fields_are_rejected() {
        let error = serde_yaml::from_str::<Scenario>(
            "id: x\ndescription: y\nticks: 1\nagents: []\nbogus: 1\n",
        );
        assert!(error.is_err());
    }

    #[test]
    fn overrides_reach_nested_fields_and_list_items_by_id() {
        let mut overrides = BTreeMap::new();
        overrides.insert("kernel.judgment.provider".into(), json!("mock"));
        overrides.insert("agents.a2.speed".into(), json!(9.5));
        overrides.insert("kernel.validator.max_step".into(), json!(7.0));
        let resolved: Scenario = apply_overrides(&scenario(), &overrides).unwrap();
        assert_eq!(resolved.kernel.judgment.provider, JudgeProvider::Mock);
        assert_eq!(resolved.agents[1].speed, 9.5);
        assert_eq!(resolved.kernel.validator.max_step, 7.0);
    }

    #[test]
    fn misspelled_override_is_an_error() {
        let mut overrides = BTreeMap::new();
        overrides.insert("kernel.judgement.provider".into(), json!("mock"));
        assert!(apply_overrides(&scenario(), &overrides).is_err());
        let mut overrides = BTreeMap::new();
        overrides.insert("agents.nobody.speed".into(), json!(1.0));
        assert!(apply_overrides(&scenario(), &overrides).is_err());
    }

    #[test]
    fn agents_can_be_removed_for_counterfactuals() {
        let mut overrides = BTreeMap::new();
        overrides.insert(REMOVE_AGENTS_KEY.into(), json!(["a2"]));
        let run = resolve_with_overrides(&scenario(), "exp", "cond", 0, 1, overrides).unwrap();
        assert_eq!(run.scenario.agents.len(), 1);
        assert_eq!(run.run_id, "exp.cond.r000.s1");
    }

    #[test]
    fn assignments_parse_yaml_values() {
        assert_eq!(
            parse_assignment("a.b=true").unwrap(),
            ("a.b".into(), json!(true))
        );
        assert_eq!(parse_assignment("x=0.8").unwrap().1, json!(0.8));
        assert_eq!(parse_assignment("p=disabled").unwrap().1, json!("disabled"));
        assert!(parse_assignment("nothing").is_err());
    }

    #[test]
    fn run_ids_reject_traversal() {
        assert!(valid_run_id("exp.cond.r000.s42"));
        assert!(!valid_run_id("../etc"));
        assert!(!valid_run_id("a/b"));
        assert!(!valid_run_id(""));
    }

    #[test]
    fn run_config_hash_is_stable() {
        let run = resolve_with_overrides(&scenario(), "exp", "c", 0, 1, BTreeMap::new()).unwrap();
        assert_eq!(run.hash(), run.clone().hash());
        assert!(run.hash().starts_with("sha256:"));
    }
}
