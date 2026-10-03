//! Deterministic validation: the hard gate of the Pordenone kernel.
//!
//! Validation is a pure function of the proposal, a context snapshot of
//! authoritative state, and a versioned configuration. It runs every check in
//! a fixed order and reports each one. A proposal passes only when no check
//! fails. Unknown actions, unknown agents, non-finite numbers, stale
//! observations, and future timestamps fail closed.
//!
//! [`EpistemicValidator::evaluate`] returns a [`Verdict`]. The only way to
//! obtain a [`ValidatedProposal`] is a passing verdict; the kernel requires one
//! to authorize a commit, so a failed check cannot reach state mutation.

use event_bus::{hash_canonical, quantize};
use serde::{Deserialize, Serialize};
pub use spatial_state::Vector3;
use thiserror::Error;

pub const VALIDATOR_VERSION: &str = "pordenone.validator.v2";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ActionProposal {
    pub proposal_id: String,
    pub agent_id: String,
    pub action_type: String,
    pub parameters_json: String,
    pub target_position: Vector3,
    pub priority: i32,
    /// When the proposal was created (ms).
    pub timestamp: i64,
    pub correlation_id: String,
    pub source_observation: String,
    /// When the observation the proposal relies on was taken (ms). Falls back
    /// to `timestamp` when absent.
    #[serde(default)]
    pub observed_at: Option<i64>,
}

impl ActionProposal {
    pub fn observation_time(&self) -> i64 {
        self.observed_at.unwrap_or(self.timestamp)
    }
}

/// Circular (spherical in 3D) region that committed positions and paths may not enter.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HazardZone {
    pub id: String,
    pub center: Vector3,
    pub radius: f64,
}

/// Every deterministic check, in evaluation order.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "snake_case")]
pub enum CheckId {
    UniqueProposal,
    FiniteValues,
    ActionAllowlist,
    AgentRegistered,
    PriorityRange,
    ObservationFreshness,
    ClockSkew,
    CoordinateBounds,
    MaxStep,
    HazardClearance,
    Separation,
}

impl CheckId {
    pub const ALL: [CheckId; 11] = [
        CheckId::UniqueProposal,
        CheckId::FiniteValues,
        CheckId::ActionAllowlist,
        CheckId::AgentRegistered,
        CheckId::PriorityRange,
        CheckId::ObservationFreshness,
        CheckId::ClockSkew,
        CheckId::CoordinateBounds,
        CheckId::MaxStep,
        CheckId::HazardClearance,
        CheckId::Separation,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::UniqueProposal => "unique_proposal",
            Self::FiniteValues => "finite_values",
            Self::ActionAllowlist => "action_allowlist",
            Self::AgentRegistered => "agent_registered",
            Self::PriorityRange => "priority_range",
            Self::ObservationFreshness => "observation_freshness",
            Self::ClockSkew => "clock_skew",
            Self::CoordinateBounds => "coordinate_bounds",
            Self::MaxStep => "max_step",
            Self::HazardClearance => "hazard_clearance",
            Self::Separation => "separation",
        }
    }

    pub fn reason_code(self) -> &'static str {
        match self {
            Self::UniqueProposal => "DUPLICATE_PROPOSAL",
            Self::FiniteValues => "NON_FINITE_VALUE",
            Self::ActionAllowlist => "ACTION_NOT_ALLOWED",
            Self::AgentRegistered => "AGENT_NOT_REGISTERED",
            Self::PriorityRange => "PRIORITY_OUT_OF_RANGE",
            Self::ObservationFreshness => "OBSERVATION_STALE",
            Self::ClockSkew => "TIMESTAMP_IN_FUTURE",
            Self::CoordinateBounds => "OUT_OF_BOUNDS",
            Self::MaxStep => "STEP_TOO_LARGE",
            Self::HazardClearance => "HAZARD_INTERSECTION",
            Self::Separation => "SEPARATION_VIOLATION",
        }
    }

    /// Core checks protect the kernel's integrity and cannot be disabled.
    /// Domain checks encode environment rules and may be ablated in experiments.
    pub fn is_core(self) -> bool {
        !matches!(
            self,
            Self::MaxStep | Self::HazardClearance | Self::Separation
        )
    }

    pub fn parse(name: &str) -> Option<CheckId> {
        CheckId::ALL
            .into_iter()
            .find(|check| check.as_str() == name)
    }

    /// Checks whose failure means the proposed motion is physically unsafe or impossible.
    fn is_physical(self) -> bool {
        matches!(
            self,
            Self::FiniteValues
                | Self::CoordinateBounds
                | Self::MaxStep
                | Self::HazardClearance
                | Self::Separation
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields, default)]
pub struct ValidatorConfig {
    pub max_spatial_bound: f64,
    pub max_stale_ms: i64,
    pub max_future_skew_ms: i64,
    pub min_priority: i32,
    pub max_priority: i32,
    pub allowed_actions: Vec<String>,
    /// Largest displacement a single committed proposal may make.
    pub max_step: f64,
    /// Smallest allowed distance between a target and another agent's committed position.
    pub min_separation: f64,
    /// Domain checks switched off for an ablation. Core checks are rejected here.
    pub disabled_checks: Vec<CheckId>,
}

impl Default for ValidatorConfig {
    fn default() -> Self {
        Self {
            max_spatial_bound: 10_000.0,
            max_stale_ms: 30_000,
            max_future_skew_ms: 1_000,
            min_priority: 0,
            max_priority: 10,
            allowed_actions: ["MOVE", "PATROL", "INSPECT", "STANDBY", "HOLD"]
                .iter()
                .map(|action| action.to_string())
                .collect(),
            max_step: 10_000.0,
            min_separation: 0.0,
            disabled_checks: Vec::new(),
        }
    }
}

#[derive(Debug, Error, PartialEq)]
pub enum ConfigError {
    #[error("core check `{0}` cannot be disabled")]
    CoreCheckDisabled(&'static str),
    #[error("`{0}` must be a finite, non-negative number")]
    Threshold(&'static str),
    #[error("min_priority is greater than max_priority")]
    PriorityRange,
    #[error("allowed_actions is empty")]
    NoActions,
}

impl ValidatorConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        for check in &self.disabled_checks {
            if check.is_core() {
                return Err(ConfigError::CoreCheckDisabled(check.as_str()));
            }
        }
        for (name, value) in [
            ("max_spatial_bound", self.max_spatial_bound),
            ("max_step", self.max_step),
            ("min_separation", self.min_separation),
        ] {
            if !value.is_finite() || value < 0.0 {
                return Err(ConfigError::Threshold(name));
            }
        }
        if self.max_stale_ms < 0 {
            return Err(ConfigError::Threshold("max_stale_ms"));
        }
        if self.max_future_skew_ms < 0 {
            return Err(ConfigError::Threshold("max_future_skew_ms"));
        }
        if self.min_priority > self.max_priority {
            return Err(ConfigError::PriorityRange);
        }
        if self.allowed_actions.is_empty() {
            return Err(ConfigError::NoActions);
        }
        Ok(())
    }

    pub fn hash(&self) -> String {
        hash_canonical(self).expect("validator config serializes")
    }

    fn enabled(&self, check: CheckId) -> bool {
        check.is_core() || !self.disabled_checks.contains(&check)
    }
}

/// Position of a registered agent as recorded in authoritative state.
#[derive(Debug, Clone, PartialEq)]
pub struct AgentView {
    pub agent_id: String,
    pub position: Vector3,
}

/// Snapshot of the authoritative state the proposal is judged against.
#[derive(Debug, Clone, Copy)]
pub struct ValidationContext<'a> {
    pub now_ms: i64,
    /// The proposing agent, if it is registered.
    pub agent: Option<&'a AgentView>,
    /// Every other registered agent.
    pub others: &'a [AgentView],
    pub hazards: &'a [HazardZone],
    /// True when this proposal id was already processed by the kernel.
    pub duplicate: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CheckStatus {
    Pass,
    Fail,
    /// A prerequisite failed, so this check could not run. The proposal is
    /// already rejected.
    NotEvaluated,
    /// Domain check switched off by configuration.
    Disabled,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CheckOutcome {
    pub check: CheckId,
    pub status: CheckStatus,
    pub detail: String,
    pub measured: Option<f64>,
    pub limit: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ValidationResult {
    pub proposal_id: String,
    pub agent_id: String,
    pub accepted: bool,
    /// 1.0 when every physical check passed, otherwise 0.0.
    pub feasibility: f64,
    /// Human-readable details of failed checks.
    pub contradictions: Vec<String>,
    /// Evidence freshness factor: 1.0 for a current observation, falling
    /// linearly to 0.5 at the staleness limit. Deterministic; not a model output.
    pub confidence: f64,
    /// Reason codes: `ALL_CHECKS_PASSED` or the code of each failed check.
    pub reasons: Vec<String>,
    pub provenance: String,
    pub timestamp: i64,
    pub validator_version: String,
    pub config_hash: String,
    pub observation_age_ms: i64,
    pub checks: Vec<CheckOutcome>,
}

impl ValidationResult {
    pub fn failed_checks(&self) -> impl Iterator<Item = &CheckOutcome> {
        self.checks
            .iter()
            .filter(|outcome| outcome.status == CheckStatus::Fail)
    }

    pub fn check(&self, id: CheckId) -> Option<&CheckOutcome> {
        self.checks.iter().find(|outcome| outcome.check == id)
    }
}

mod seal {
    /// Private token: only this crate can construct a `ValidatedProposal`.
    #[derive(Debug, Clone)]
    pub struct Seal(pub(super) ());
}

/// A proposal that passed every enabled deterministic check against a specific
/// state snapshot. It cannot be constructed outside this crate.
#[derive(Debug, Clone)]
pub struct ValidatedProposal {
    proposal: ActionProposal,
    result: ValidationResult,
    _seal: seal::Seal,
}

impl ValidatedProposal {
    pub fn proposal(&self) -> &ActionProposal {
        &self.proposal
    }

    pub fn result(&self) -> &ValidationResult {
        &self.result
    }

    pub fn into_parts(self) -> (ActionProposal, ValidationResult) {
        (self.proposal, self.result)
    }
}

#[derive(Debug, Clone)]
pub enum Verdict {
    Pass(ValidatedProposal),
    Fail {
        proposal: ActionProposal,
        result: ValidationResult,
    },
}

impl Verdict {
    pub fn result(&self) -> &ValidationResult {
        match self {
            Verdict::Pass(validated) => validated.result(),
            Verdict::Fail { result, .. } => result,
        }
    }

    pub fn accepted(&self) -> bool {
        matches!(self, Verdict::Pass(_))
    }
}

#[derive(Debug, Clone, Default)]
pub struct EpistemicValidator {
    config: ValidatorConfig,
    config_hash: String,
}

impl EpistemicValidator {
    pub fn new() -> Self {
        Self::with_config(ValidatorConfig::default()).expect("default validator config is valid")
    }

    pub fn with_config(config: ValidatorConfig) -> Result<Self, ConfigError> {
        config.validate()?;
        let config_hash = config.hash();
        Ok(Self {
            config,
            config_hash,
        })
    }

    pub fn config(&self) -> &ValidatorConfig {
        &self.config
    }

    pub fn config_hash(&self) -> &str {
        &self.config_hash
    }

    /// Names of the checks this validator runs (enabled checks only).
    pub fn checked_constraints(&self) -> Vec<String> {
        CheckId::ALL
            .into_iter()
            .filter(|check| self.config.enabled(*check))
            .map(|check| check.as_str().to_string())
            .collect()
    }

    pub fn evaluate(&self, proposal: ActionProposal, context: &ValidationContext<'_>) -> Verdict {
        let result = self.validate(&proposal, context);
        if result.accepted {
            Verdict::Pass(ValidatedProposal {
                proposal,
                result,
                _seal: seal::Seal(()),
            })
        } else {
            Verdict::Fail { proposal, result }
        }
    }

    pub fn validate(
        &self,
        proposal: &ActionProposal,
        context: &ValidationContext<'_>,
    ) -> ValidationResult {
        let cfg = &self.config;
        let mut checks: Vec<CheckOutcome> = Vec::with_capacity(CheckId::ALL.len());
        let target = proposal.target_position;
        let finite = target.is_finite();
        let observed_at = proposal.observation_time();
        let age_ms = context.now_ms.saturating_sub(observed_at);

        checks.push(if context.duplicate {
            fail(
                CheckId::UniqueProposal,
                format!(
                    "proposal `{}` was already processed",
                    clip(&proposal.proposal_id, 64)
                ),
                None,
                None,
            )
        } else {
            pass(CheckId::UniqueProposal, "proposal id not seen before")
        });

        checks.push(if finite {
            pass(CheckId::FiniteValues, "target coordinates are finite")
        } else {
            fail(
                CheckId::FiniteValues,
                format!(
                    "target ({}, {}, {}) contains a non-finite value",
                    target.x, target.y, target.z
                ),
                None,
                None,
            )
        });

        checks.push(if cfg.allowed_actions.contains(&proposal.action_type) {
            pass(CheckId::ActionAllowlist, "action is on the allow-list")
        } else {
            fail(
                CheckId::ActionAllowlist,
                format!(
                    "unknown or prohibited action `{}`",
                    clip(&proposal.action_type, 64)
                ),
                None,
                None,
            )
        });

        checks.push(match context.agent {
            Some(agent) if agent.agent_id == proposal.agent_id => {
                pass(CheckId::AgentRegistered, "agent is registered")
            }
            _ => fail(
                CheckId::AgentRegistered,
                format!(
                    "agent `{}` is not registered with the kernel",
                    clip(&proposal.agent_id, 64)
                ),
                None,
                None,
            ),
        });

        checks.push(
            if (cfg.min_priority..=cfg.max_priority).contains(&proposal.priority) {
                pass(CheckId::PriorityRange, "priority within range")
            } else {
                fail(
                    CheckId::PriorityRange,
                    format!(
                        "priority {} outside [{}, {}]",
                        proposal.priority, cfg.min_priority, cfg.max_priority
                    ),
                    Some(proposal.priority as f64),
                    Some(cfg.max_priority as f64),
                )
            },
        );

        checks.push(if age_ms > cfg.max_stale_ms {
            fail(
                CheckId::ObservationFreshness,
                format!(
                    "observation is {age_ms} ms old (limit {} ms)",
                    cfg.max_stale_ms
                ),
                Some(age_ms as f64),
                Some(cfg.max_stale_ms as f64),
            )
        } else {
            measured_pass(
                CheckId::ObservationFreshness,
                "observation is fresh enough",
                age_ms as f64,
                cfg.max_stale_ms as f64,
            )
        });

        let future_by = proposal
            .timestamp
            .max(observed_at)
            .saturating_sub(context.now_ms);
        checks.push(if future_by > cfg.max_future_skew_ms {
            fail(
                CheckId::ClockSkew,
                format!(
                    "timestamp is {future_by} ms in the future (tolerance {} ms)",
                    cfg.max_future_skew_ms
                ),
                Some(future_by as f64),
                Some(cfg.max_future_skew_ms as f64),
            )
        } else {
            pass(CheckId::ClockSkew, "timestamps are not in the future")
        });

        let max_axis = target.x.abs().max(target.y.abs()).max(target.z.abs());
        checks.push(if !finite {
            not_evaluated(CheckId::CoordinateBounds, "coordinates are not finite")
        } else if max_axis > cfg.max_spatial_bound {
            fail(
                CheckId::CoordinateBounds,
                format!(
                    "target ({}, {}, {}) exceeds the spatial bound {}",
                    q(target.x),
                    q(target.y),
                    q(target.z),
                    cfg.max_spatial_bound
                ),
                Some(max_axis),
                Some(cfg.max_spatial_bound),
            )
        } else {
            measured_pass(
                CheckId::CoordinateBounds,
                "target within bounds",
                max_axis,
                cfg.max_spatial_bound,
            )
        });

        let current = context.agent.map(|agent| agent.position);
        checks.push(self.step_check(finite, current, &target));
        checks.push(self.hazard_check(finite, current, &target, context.hazards));
        checks.push(self.separation_check(finite, &proposal.agent_id, &target, context.others));

        let accepted = checks
            .iter()
            .all(|outcome| matches!(outcome.status, CheckStatus::Pass | CheckStatus::Disabled));
        let physical_ok = checks
            .iter()
            .filter(|outcome| outcome.check.is_physical())
            .all(|outcome| matches!(outcome.status, CheckStatus::Pass | CheckStatus::Disabled));
        let contradictions: Vec<String> = checks
            .iter()
            .filter(|outcome| outcome.status == CheckStatus::Fail)
            .map(|outcome| outcome.detail.clone())
            .collect();
        let reasons = if accepted {
            vec!["ALL_CHECKS_PASSED".to_string()]
        } else {
            checks
                .iter()
                .filter(|outcome| outcome.status == CheckStatus::Fail)
                .map(|outcome| outcome.check.reason_code().to_string())
                .collect()
        };
        let freshness = if cfg.max_stale_ms == 0 {
            if age_ms <= 0 {
                1.0
            } else {
                0.5
            }
        } else {
            let ratio = (age_ms.max(0) as f64 / cfg.max_stale_ms as f64).min(1.0);
            1.0 - 0.5 * ratio
        };

        ValidationResult {
            proposal_id: proposal.proposal_id.clone(),
            agent_id: proposal.agent_id.clone(),
            accepted,
            feasibility: if physical_ok { 1.0 } else { 0.0 },
            contradictions,
            confidence: q(freshness),
            reasons,
            provenance: format!(
                "{VALIDATOR_VERSION}:{}",
                clip(&proposal.source_observation, 128)
            ),
            timestamp: context.now_ms,
            validator_version: VALIDATOR_VERSION.to_string(),
            config_hash: self.config_hash.clone(),
            observation_age_ms: age_ms,
            checks,
        }
    }

    fn step_check(&self, finite: bool, current: Option<Vector3>, target: &Vector3) -> CheckOutcome {
        let cfg = &self.config;
        if !cfg.enabled(CheckId::MaxStep) {
            return disabled(CheckId::MaxStep);
        }
        let Some(current) = current.filter(|_| finite) else {
            return not_evaluated(CheckId::MaxStep, "no finite target or registered position");
        };
        let step = current.distance(target);
        if step > cfg.max_step {
            fail(
                CheckId::MaxStep,
                format!("step of {} exceeds max_step {}", q(step), cfg.max_step),
                Some(step),
                Some(cfg.max_step),
            )
        } else {
            measured_pass(CheckId::MaxStep, "step within limit", step, cfg.max_step)
        }
    }

    fn hazard_check(
        &self,
        finite: bool,
        current: Option<Vector3>,
        target: &Vector3,
        hazards: &[HazardZone],
    ) -> CheckOutcome {
        if !self.config.enabled(CheckId::HazardClearance) {
            return disabled(CheckId::HazardClearance);
        }
        let Some(start) = current.filter(|_| finite) else {
            return not_evaluated(
                CheckId::HazardClearance,
                "no finite target or registered position",
            );
        };
        let mut closest_margin = f64::INFINITY;
        for zone in hazards {
            let target_distance = zone.center.distance(target);
            let starts_inside = zone.center.distance(&start) < zone.radius;
            // An agent caught inside a newly declared zone may only move out of it.
            let clearance = if starts_inside {
                target_distance
            } else {
                segment_point_distance(&start, target, &zone.center)
            };
            let margin = clearance - zone.radius;
            if margin < closest_margin {
                closest_margin = margin;
            }
            if margin < 0.0 {
                let what = if starts_inside {
                    "escape target is still inside"
                } else if target_distance < zone.radius {
                    "target is inside"
                } else {
                    "path crosses"
                };
                return fail(
                    CheckId::HazardClearance,
                    format!("{what} hazard zone `{}`", clip(&zone.id, 64)),
                    Some(margin),
                    Some(0.0),
                );
            }
        }
        if hazards.is_empty() {
            pass(CheckId::HazardClearance, "no hazard zones declared")
        } else {
            measured_pass(
                CheckId::HazardClearance,
                "path clears every hazard zone",
                closest_margin,
                0.0,
            )
        }
    }

    fn separation_check(
        &self,
        finite: bool,
        agent_id: &str,
        target: &Vector3,
        others: &[AgentView],
    ) -> CheckOutcome {
        let cfg = &self.config;
        if !cfg.enabled(CheckId::Separation) {
            return disabled(CheckId::Separation);
        }
        if !finite {
            return not_evaluated(CheckId::Separation, "target is not finite");
        }
        let nearest = others
            .iter()
            .filter(|other| other.agent_id != agent_id)
            .map(|other| (other, other.position.distance(target)))
            .min_by(|left, right| {
                left.1
                    .total_cmp(&right.1)
                    .then_with(|| left.0.agent_id.cmp(&right.0.agent_id))
            });
        match nearest {
            Some((other, distance)) if distance < cfg.min_separation => fail(
                CheckId::Separation,
                format!(
                    "target is {} from `{}` (minimum {})",
                    q(distance),
                    clip(&other.agent_id, 64),
                    cfg.min_separation
                ),
                Some(distance),
                Some(cfg.min_separation),
            ),
            Some((_, distance)) => measured_pass(
                CheckId::Separation,
                "target keeps separation",
                distance,
                cfg.min_separation,
            ),
            None => pass(CheckId::Separation, "no other agents registered"),
        }
    }
}

/// Distance from `point` to the segment `a`–`b`. Uses only IEEE-exact
/// operations and `sqrt`, so it is reproducible across platforms.
pub fn segment_point_distance(a: &Vector3, b: &Vector3, point: &Vector3) -> f64 {
    let (abx, aby, abz) = (b.x - a.x, b.y - a.y, b.z - a.z);
    let (apx, apy, apz) = (point.x - a.x, point.y - a.y, point.z - a.z);
    let length_sq = abx * abx + aby * aby + abz * abz;
    let t = if length_sq == 0.0 {
        0.0
    } else {
        ((apx * abx + apy * aby + apz * abz) / length_sq).clamp(0.0, 1.0)
    };
    let closest = Vector3::new(a.x + t * abx, a.y + t * aby, a.z + t * abz);
    closest.distance(point)
}

fn q(value: f64) -> f64 {
    quantize(value, 6)
}

fn clip(text: &str, max: usize) -> String {
    text.chars()
        .take(max)
        .map(|ch| if ch.is_control() { '_' } else { ch })
        .collect()
}

fn pass(check: CheckId, detail: &str) -> CheckOutcome {
    CheckOutcome {
        check,
        status: CheckStatus::Pass,
        detail: detail.to_string(),
        measured: None,
        limit: None,
    }
}

fn measured_pass(check: CheckId, detail: &str, measured: f64, limit: f64) -> CheckOutcome {
    CheckOutcome {
        check,
        status: CheckStatus::Pass,
        detail: detail.to_string(),
        measured: Some(q(measured)),
        limit: Some(q(limit)),
    }
}

fn fail(check: CheckId, detail: String, measured: Option<f64>, limit: Option<f64>) -> CheckOutcome {
    CheckOutcome {
        check,
        status: CheckStatus::Fail,
        detail,
        measured: measured.filter(|value| value.is_finite()).map(q),
        limit: limit.filter(|value| value.is_finite()).map(q),
    }
}

fn not_evaluated(check: CheckId, detail: &str) -> CheckOutcome {
    CheckOutcome {
        check,
        status: CheckStatus::NotEvaluated,
        detail: detail.to_string(),
        measured: None,
        limit: None,
    }
}

fn disabled(check: CheckId) -> CheckOutcome {
    CheckOutcome {
        check,
        status: CheckStatus::Disabled,
        detail: "disabled by configuration".to_string(),
        measured: None,
        limit: None,
    }
}

#[cfg(test)]
mod tests;
