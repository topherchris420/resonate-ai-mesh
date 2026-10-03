//! The POLICY stage: deterministic rules that turn a validated proposal and an
//! optional judgment into commit, withhold, or await-human.
//!
//! This module is the only place a [`CommitAuthorization`] can be created, and
//! it requires a `ValidatedProposal`. No judgment disposition can produce an
//! authorization for a proposal that failed validation, because such a
//! proposal never becomes a `ValidatedProposal`.

use crate::state::CommitAuthorization;
use epistemic_validator::ValidatedProposal;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};
use typed_judgment::{Disposition, JudgmentEnvelope};

pub const KERNEL_POLICY_VERSION: &str = "pordenone.kernel.policy.v2";

/// Operator-load level derived from the human-state input.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AdaptiveLevel {
    Normal,
    Elevated,
    High,
    Critical,
}

/// Former name, kept for existing callers.
pub type CognitiveLoadPolicyLevel = AdaptiveLevel;

impl AdaptiveLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Normal => "NORMAL",
            Self::Elevated => "ELEVATED",
            Self::High => "HIGH",
            Self::Critical => "CRITICAL",
        }
    }
}

/// Presentation and scheduling hints for a level.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PolicyConfiguration {
    pub level: AdaptiveLevel,
    pub update_frequency_ms: u64,
    pub visual_density_scale: f64,
    pub aggregate_alerts: bool,
    /// When adaptive gating is enabled, background proposals are withheld.
    pub defer_background_tasks: bool,
}

impl Default for PolicyConfiguration {
    fn default() -> Self {
        policy_for_level(AdaptiveLevel::Normal)
    }
}

pub fn policy_for_level(level: AdaptiveLevel) -> PolicyConfiguration {
    let (update_frequency_ms, visual_density_scale, aggregate_alerts, defer_background_tasks) =
        match level {
            AdaptiveLevel::Normal => (100, 1.0, false, false),
            AdaptiveLevel::Elevated => (250, 0.8, false, false),
            AdaptiveLevel::High => (500, 0.5, true, true),
            AdaptiveLevel::Critical => (1000, 0.2, true, true),
        };
    PolicyConfiguration {
        level,
        update_frequency_ms,
        visual_density_scale,
        aggregate_alerts,
        defer_background_tasks,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields, default)]
pub struct AdaptivePolicyConfig {
    /// Withhold background proposals while the operator level defers them.
    pub enabled: bool,
    /// Load thresholds for ELEVATED, HIGH, CRITICAL (load below the first is NORMAL).
    pub thresholds: [f64; 3],
    /// Proposals at or below this priority count as background work.
    pub background_priority_max: i32,
}

impl Default for AdaptivePolicyConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            thresholds: [0.4, 0.65, 0.85],
            background_priority_max: 0,
        }
    }
}

impl AdaptivePolicyConfig {
    pub fn level_for(&self, load: f64) -> AdaptiveLevel {
        let [elevated, high, critical] = self.thresholds;
        if load < elevated {
            AdaptiveLevel::Normal
        } else if load < high {
            AdaptiveLevel::Elevated
        } else if load < critical {
            AdaptiveLevel::High
        } else {
            AdaptiveLevel::Critical
        }
    }
}

/// When the kernel consults the judgment provider.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum JudgmentRouting {
    /// Every proposal that passes validation is judged.
    #[default]
    Always,
    /// Only proposals from agents whose recent proposals were often not
    /// committed are judged. `instability` is the fraction of the agent's last
    /// `window` proposals that were rejected or withheld; judgment is consulted
    /// when it is strictly above `threshold`.
    OnAgentInstability { window: usize, threshold: f64 },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(deny_unknown_fields, default)]
pub struct KernelPolicyConfig {
    pub adaptive: AdaptivePolicyConfig,
    pub routing: JudgmentRouting,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PolicyOutcome {
    Committed,
    Withheld,
    AwaitingHumanReview,
    RejectedDeterministic,
}

impl PolicyOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Committed => "committed",
            Self::Withheld => "withheld",
            Self::AwaitingHumanReview => "awaiting_human_review",
            Self::RejectedDeterministic => "rejected_deterministic",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CommitBasis {
    /// Validation passed and judgment returned PASS.
    JudgmentPass,
    /// Validation passed and judgment was not consulted (disabled or routed).
    DeterministicOnly,
    /// Validation passed again after an explicit human approval.
    HumanApproved,
}

/// Recorded on every policy decision so the reader can see what judgment did.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JudgmentSummary {
    pub consulted: bool,
    /// Why judgment was not consulted: JUDGMENT_DISABLED or ROUTED_STABLE_AGENT.
    pub skip_reason: Option<String>,
    pub disposition: Option<Disposition>,
    pub judgment_id: Option<String>,
    pub provider: Option<String>,
    pub model_involved: bool,
    /// Agent instability that drove routing, when routing is active.
    pub routing_signal: Option<f64>,
}

impl JudgmentSummary {
    pub fn not_consulted(reason: &str, routing_signal: Option<f64>) -> Self {
        Self {
            consulted: false,
            skip_reason: Some(reason.to_string()),
            disposition: None,
            judgment_id: None,
            provider: None,
            model_involved: false,
            routing_signal,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PolicyDecision {
    pub proposal_id: String,
    pub outcome: PolicyOutcome,
    pub reason_codes: Vec<String>,
    pub policy_version: String,
    pub adaptive_level: AdaptiveLevel,
    pub judgment: Option<JudgmentSummary>,
    pub basis: Option<CommitBasis>,
}

impl PolicyDecision {
    pub fn permits_commit(&self) -> bool {
        self.outcome == PolicyOutcome::Committed
    }

    pub(crate) fn rejected(proposal_id: &str, reasons: Vec<String>, level: AdaptiveLevel) -> Self {
        let mut reason_codes = vec!["DETERMINISTIC_VALIDATION_FAILED".to_string()];
        reason_codes.extend(reasons);
        Self {
            proposal_id: proposal_id.to_string(),
            outcome: PolicyOutcome::RejectedDeterministic,
            reason_codes,
            policy_version: KERNEL_POLICY_VERSION.to_string(),
            adaptive_level: level,
            judgment: None,
            basis: None,
        }
    }
}

/// Combine judgment and operator-load rules for a validated proposal.
pub(crate) fn decide(
    validated: &ValidatedProposal,
    judgment: Option<&JudgmentEnvelope>,
    summary: JudgmentSummary,
    level: AdaptiveLevel,
    config: &KernelPolicyConfig,
) -> PolicyDecision {
    let proposal = validated.proposal();
    let mut reasons = Vec::new();
    let (mut outcome, mut basis) = match judgment.map(|envelope| envelope.disposition) {
        None => {
            reasons.push(
                summary
                    .skip_reason
                    .clone()
                    .unwrap_or_else(|| "JUDGMENT_NOT_CONSULTED".to_string()),
            );
            (
                PolicyOutcome::Committed,
                Some(CommitBasis::DeterministicOnly),
            )
        }
        Some(Disposition::Pass) => {
            reasons.push("ELIGIBLE_PASS".to_string());
            (PolicyOutcome::Committed, Some(CommitBasis::JudgmentPass))
        }
        Some(Disposition::Skipped) => {
            reasons.push("JUDGMENT_DISABLED".to_string());
            (
                PolicyOutcome::Committed,
                Some(CommitBasis::DeterministicOnly),
            )
        }
        Some(Disposition::HumanReview) => {
            reasons.extend(judgment_reasons(judgment));
            (PolicyOutcome::AwaitingHumanReview, None)
        }
        Some(Disposition::Revise | Disposition::Unavailable | Disposition::Pending) => {
            reasons.extend(judgment_reasons(judgment));
            (PolicyOutcome::Withheld, None)
        }
    };
    if outcome == PolicyOutcome::Committed
        && config.adaptive.enabled
        && policy_for_level(level).defer_background_tasks
        && proposal.priority <= config.adaptive.background_priority_max
    {
        outcome = PolicyOutcome::Withheld;
        basis = None;
        reasons.push("OPERATOR_LOAD_DEFERRAL".to_string());
    }
    PolicyDecision {
        proposal_id: proposal.proposal_id.clone(),
        outcome,
        reason_codes: reasons,
        policy_version: KERNEL_POLICY_VERSION.to_string(),
        adaptive_level: level,
        judgment: Some(summary),
        basis,
    }
}

fn judgment_reasons(judgment: Option<&JudgmentEnvelope>) -> Vec<String> {
    judgment
        .map(|envelope| {
            if envelope.reason_codes.is_empty() {
                vec![format!("JUDGMENT_{:?}", envelope.disposition).to_ascii_uppercase()]
            } else {
                envelope.reason_codes.clone()
            }
        })
        .unwrap_or_default()
}

/// The only constructor of a commit authorization.
pub(crate) fn authorize(
    validated: ValidatedProposal,
    decision: &PolicyDecision,
    authorized_by: Vec<String>,
) -> Option<CommitAuthorization> {
    if !decision.permits_commit() || decision.proposal_id != validated.proposal().proposal_id {
        return None;
    }
    Some(CommitAuthorization {
        validated,
        basis: decision.basis?,
        authorized_by,
    })
}

/// Rolling record of whether each agent's recent proposals were committed.
#[derive(Debug, Clone, Default)]
pub(crate) struct AgentHistory {
    unstable: BTreeMap<String, VecDeque<bool>>,
}

impl AgentHistory {
    const CAPACITY: usize = 64;

    pub(crate) fn record(&mut self, agent_id: &str, committed: bool) {
        let entries = self.unstable.entry(agent_id.to_string()).or_default();
        entries.push_back(!committed);
        while entries.len() > Self::CAPACITY {
            entries.pop_front();
        }
    }

    /// Fraction of the last `window` proposals that were not committed. An
    /// agent with no history is treated as fully unstable, so new agents are
    /// judged until they establish a record.
    pub(crate) fn instability(&self, agent_id: &str, window: usize) -> f64 {
        let Some(entries) = self.unstable.get(agent_id) else {
            return 1.0;
        };
        let window = window.clamp(1, Self::CAPACITY);
        let recent: Vec<bool> = entries.iter().rev().take(window).copied().collect();
        if recent.is_empty() {
            return 1.0;
        }
        recent.iter().filter(|unstable| **unstable).count() as f64 / recent.len() as f64
    }
}

/// Decide whether to consult judgment. Returns the skip reason when not.
pub(crate) fn route(
    routing: &JudgmentRouting,
    history: &AgentHistory,
    agent_id: &str,
) -> (Option<&'static str>, Option<f64>) {
    match routing {
        JudgmentRouting::Always => (None, None),
        JudgmentRouting::OnAgentInstability { window, threshold } => {
            let signal = history.instability(agent_id, *window);
            if signal > *threshold {
                (None, Some(signal))
            } else {
                (Some("ROUTED_STABLE_AGENT"), Some(signal))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_use_lower_inclusive_thresholds() {
        let config = AdaptivePolicyConfig::default();
        assert_eq!(config.level_for(0.0), AdaptiveLevel::Normal);
        assert_eq!(config.level_for(0.399), AdaptiveLevel::Normal);
        assert_eq!(config.level_for(0.4), AdaptiveLevel::Elevated);
        assert_eq!(config.level_for(0.65), AdaptiveLevel::High);
        assert_eq!(config.level_for(0.85), AdaptiveLevel::Critical);
        assert!(policy_for_level(AdaptiveLevel::High).defer_background_tasks);
        assert!(!policy_for_level(AdaptiveLevel::Elevated).defer_background_tasks);
    }

    #[test]
    fn new_agents_are_unstable_until_they_build_a_record() {
        let mut history = AgentHistory::default();
        assert_eq!(history.instability("a", 4), 1.0);
        for committed in [true, true, false, true] {
            history.record("a", committed);
        }
        assert_eq!(history.instability("a", 4), 0.25);
        assert_eq!(history.instability("a", 2), 0.5);
        assert_eq!(history.instability("a", 1), 0.0);
        let routing = JudgmentRouting::OnAgentInstability {
            window: 4,
            threshold: 0.2,
        };
        assert_eq!(route(&routing, &history, "a"), (None, Some(0.25)));
        let routing = JudgmentRouting::OnAgentInstability {
            window: 4,
            threshold: 0.25,
        };
        assert_eq!(
            route(&routing, &history, "a"),
            (Some("ROUTED_STABLE_AGENT"), Some(0.25))
        );
    }

    #[test]
    fn routing_serializes_with_a_mode_tag() {
        let routing: JudgmentRouting = serde_json::from_value(serde_json::json!({
            "mode": "on_agent_instability", "window": 8, "threshold": 0.3
        }))
        .unwrap();
        assert_eq!(
            routing,
            JudgmentRouting::OnAgentInstability {
                window: 8,
                threshold: 0.3
            }
        );
    }
}
