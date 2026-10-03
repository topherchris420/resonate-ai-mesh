//! Metrics and the resonance vector.
//!
//! Every value is computed from recorded decisions and events, and every
//! definition is listed in [`DEFINITIONS`] and `docs/resonance-metrics.md`.
//! "Resonance" here is a set of operational measurements of the run. It says
//! nothing about cognition, emotion, wellness, or physiology. A dimension that
//! the run gives no evidence for is `null` with a stated reason, never a guess.

use crate::config::RunConfig;
use crate::record::DecisionRecord;
use event_bus::{quantize, ChainedEvent};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// (tick, committed displacement) of one agent.
type TickMove = (u64, (f64, f64));
/// (agent id, finite target) of one proposal.
type AgentTarget<'a> = (&'a str, (f64, f64, f64));

pub const METRICS_VERSION: &str = "resonate-ai-mesh.metrics.v1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Dimension {
    pub value: Option<f64>,
    /// Number of observations the value is computed from.
    pub basis: u64,
    /// Why the value is null, when it is.
    pub note: Option<String>,
}

impl Dimension {
    fn some(value: f64, basis: u64) -> Self {
        Self {
            value: Some(round(value)),
            basis,
            note: None,
        }
    }

    fn none(note: &str) -> Self {
        Self {
            value: None,
            basis: 0,
            note: Some(note.to_string()),
        }
    }
}

/// Six operational dimensions of one run. Not a single score.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResonanceVector {
    pub coherence: Dimension,
    pub stability: Dimension,
    pub convergence: Dimension,
    pub disagreement: Dimension,
    pub recovery: Dimension,
    pub uncertainty: Dimension,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MetricsDoc {
    pub version: String,
    pub run_id: String,
    /// Scalar metrics. `null` means the metric is undefined for this run.
    pub values: BTreeMap<String, Option<f64>>,
    pub resonance: ResonanceVector,
    pub rejection_reasons: BTreeMap<String, u64>,
    pub withhold_reasons: BTreeMap<String, u64>,
    pub invariants: Vec<crate::invariants::InvariantResult>,
}

impl MetricsDoc {
    pub fn get(&self, metric: &str) -> Option<f64> {
        if let Some(value) = self.values.get(metric) {
            return *value;
        }
        let resonance = &self.resonance;
        match metric {
            "resonance.coherence" => resonance.coherence.value,
            "resonance.stability" => resonance.stability.value,
            "resonance.convergence" => resonance.convergence.value,
            "resonance.disagreement" => resonance.disagreement.value,
            "resonance.recovery" => resonance.recovery.value,
            "resonance.uncertainty" => resonance.uncertainty.value,
            _ => None,
        }
    }
}

pub struct MetricDef {
    pub id: &'static str,
    pub unit: &'static str,
    pub definition: &'static str,
}

pub const DEFINITIONS: &[MetricDef] = &[
    MetricDef { id: "proposals", unit: "count", definition: "Proposals submitted to the kernel (duplicates included)." },
    MetricDef { id: "accepted_proposals", unit: "count", definition: "Proposals that passed every deterministic check on first submission." },
    MetricDef { id: "rejected_proposals", unit: "count", definition: "Proposals rejected by deterministic validation." },
    MetricDef { id: "committed", unit: "count", definition: "Proposals whose commit changed authoritative state (including after human approval)." },
    MetricDef { id: "withheld", unit: "count", definition: "Proposals that passed validation but were not committed (judgment, operator-load gating, or human rejection)." },
    MetricDef { id: "human_reviews", unit: "count", definition: "Proposals routed to human review." },
    MetricDef { id: "human_approved", unit: "count", definition: "Human reviews resolved by approval that then committed." },
    MetricDef { id: "unsafe_proposals", unit: "count", definition: "Proposals the independent oracle classifies unsafe (non-finite, out of bounds, or path through an active hazard)." },
    MetricDef { id: "unsafe_proposals_blocked", unit: "count", definition: "Unsafe proposals that were not committed." },
    MetricDef { id: "unsafe_commits", unit: "count", definition: "Unsafe proposals that were committed. Expected 0 whenever hazard checks are enabled." },
    MetricDef { id: "safe_rejections", unit: "count", definition: "Oracle-safe proposals rejected by deterministic validation (staleness, separation, step size, duplicates, registration)." },
    MetricDef { id: "unstable_commits", unit: "count", definition: "Commits whose displacement reverses the same agent's previous committed displacement (dot product < 0)." },
    MetricDef { id: "degraded_evidence_commits", unit: "count", definition: "Commits of proposals whose observation was older than max_stale_ms/2 or not of GOOD quality." },
    MetricDef { id: "min_pairwise_distance", unit: "distance", definition: "Smallest distance between any two agents' authoritative positions at the end of any tick." },
    MetricDef { id: "separation_breaches", unit: "count", definition: "Agent pairs closer than 1.0 at the end of a tick, summed over ticks." },
    MetricDef { id: "judgment_calls", unit: "count", definition: "Judgment requests issued after validation passed." },
    MetricDef { id: "judgment_disagreements", unit: "count", definition: "Judgments of deterministic-valid proposals whose disposition was not PASS." },
    MetricDef { id: "judgment_disagreement_rate", unit: "ratio", definition: "judgment_disagreements / judgment_calls; null without judgment calls." },
    MetricDef { id: "judgment_unavailable", unit: "count", definition: "Judgments with disposition UNAVAILABLE (timeouts, network, disabled provider)." },
    MetricDef { id: "mean_judgment_latency_ms", unit: "ms", definition: "Mean latency_ms reported in judgment envelopes (0 for local deterministic judges)." },
    MetricDef { id: "network_calls", unit: "count", definition: "Judgment requests that reached a networked provider." },
    MetricDef { id: "deterministic_rejection_rate", unit: "ratio", definition: "rejected_proposals / proposals." },
    MetricDef { id: "commit_rate", unit: "ratio", definition: "committed / proposals." },
    MetricDef { id: "goals_total", unit: "count", definition: "Agents with a terminal goal (patrol agents have none)." },
    MetricDef { id: "goals_reached", unit: "count", definition: "Agents whose authoritative position ended within goal_radius of their goal." },
    MetricDef { id: "task_success", unit: "ratio", definition: "goals_reached / goals_total; null without goals." },
    MetricDef { id: "convergence_tick", unit: "tick", definition: "First tick after which every goal was reached; null if never." },
    MetricDef { id: "ticks_run", unit: "ticks", definition: "Ticks executed before termination." },
    MetricDef { id: "mean_observation_age_ms", unit: "ms", definition: "Mean age of the observation each proposal relied on." },
    MetricDef { id: "faults_injected", unit: "count", definition: "Faults activated during the run." },
    MetricDef { id: "recovery_ticks", unit: "ticks", definition: "Mean ticks after a fault ends until 3-tick commit throughput reaches 80% of the pre-fault level; null if never or no faults." },
    MetricDef { id: "mean_operator_load", unit: "index[0,1]", definition: "Mean of usable simulated operator-load samples." },
    MetricDef { id: "max_operator_load", unit: "index[0,1]", definition: "Maximum usable simulated operator-load sample." },
    MetricDef { id: "ticks_at_high_load", unit: "ticks", definition: "Ticks with adaptive level HIGH or CRITICAL." },
    MetricDef { id: "human_state_coupling", unit: "pearson r", definition: "Correlation between per-tick operator load and the per-tick fraction of proposals not committed; null with fewer than 5 ticks or zero variance." },
    MetricDef { id: "resonance.coherence", unit: "[0,1]", definition: "Mean over ticks with >= 2 committed moves of |sum of unit displacement vectors| / n (Vicsek order parameter). 1 = all committed motion aligned." },
    MetricDef { id: "resonance.stability", unit: "[0,1]", definition: "1 - (direction reversals / consecutive committed-move pairs), pooled over agents." },
    MetricDef { id: "resonance.convergence", unit: "[0,1]", definition: "task_success." },
    MetricDef { id: "resonance.disagreement", unit: "[0,1]", definition: "Fraction of same-tick proposal pairs from different agents whose finite targets are closer than max(min_separation, 1)." },
    MetricDef { id: "resonance.recovery", unit: "[0,1]", definition: "Mean over faults of min(1, commit throughput in the W=8 ticks after the fault / in the W ticks before); null without faults or pre-fault commits." },
    MetricDef { id: "resonance.uncertainty", unit: "[0,1]", definition: "Fraction of proposals decided on degraded evidence: observation older than max_stale_ms/2, observation quality not GOOD, or judgment not OK." },
];

fn round(value: f64) -> f64 {
    quantize(value, 6)
}

fn ratio(numerator: u64, denominator: u64) -> Option<f64> {
    (denominator > 0).then(|| round(numerator as f64 / denominator as f64))
}

fn vec_of(coords: &crate::record::Coords) -> Option<(f64, f64, f64)> {
    Some((coords[0]?, coords[1]?, coords[2]?))
}

/// Inputs for metric computation.
pub struct RunData<'a> {
    pub config: &'a RunConfig,
    pub events: &'a [ChainedEvent],
    pub decisions: &'a [DecisionRecord],
    pub final_positions: &'a BTreeMap<String, epistemic_validator::Vector3>,
    pub convergence_tick: Option<u64>,
    pub ticks_run: u64,
    pub network_calls: u64,
}

pub fn compute(
    data: &RunData<'_>,
    invariants: Vec<crate::invariants::InvariantResult>,
) -> MetricsDoc {
    let scenario = &data.config.scenario;
    let proposals: Vec<&DecisionRecord> = data
        .decisions
        .iter()
        .filter(|d| d.kind == "proposal")
        .collect();
    let resolutions: Vec<&DecisionRecord> = data
        .decisions
        .iter()
        .filter(|d| d.kind == "human_resolution")
        .collect();
    let committed_records: Vec<&DecisionRecord> =
        data.decisions.iter().filter(|d| d.committed).collect();
    let mut values: BTreeMap<String, Option<f64>> = BTreeMap::new();
    let mut set = |key: &str, value: Option<f64>| {
        values.insert(key.to_string(), value);
    };
    let count = |n: usize| Some(n as f64);

    let n_proposals = proposals.len() as u64;
    let accepted = proposals.iter().filter(|d| d.validation.accepted).count() as u64;
    let rejected = n_proposals - accepted;
    let committed = committed_records.len() as u64;
    let human_reviews = proposals
        .iter()
        .filter(|d| d.policy.outcome == "awaiting_human_review")
        .count() as u64;
    let human_approved = resolutions.iter().filter(|d| d.committed).count() as u64;
    let withheld = proposals
        .iter()
        .filter(|d| d.validation.accepted && !d.committed)
        .count() as u64
        - human_approved.min(human_reviews);
    set("proposals", count(proposals.len()));
    set("accepted_proposals", Some(accepted as f64));
    set("rejected_proposals", Some(rejected as f64));
    set("committed", Some(committed as f64));
    set("withheld", Some(withheld as f64));
    set("human_reviews", Some(human_reviews as f64));
    set("human_approved", Some(human_approved as f64));

    let unsafe_proposals = proposals
        .iter()
        .filter(|d| d.oracle_unsafe.is_some())
        .count() as u64;
    let unsafe_commits = committed_records
        .iter()
        .filter(|d| d.oracle_unsafe.is_some())
        .count() as u64;
    set("unsafe_proposals", Some(unsafe_proposals as f64));
    set("unsafe_commits", Some(unsafe_commits as f64));
    set(
        "unsafe_proposals_blocked",
        Some(unsafe_proposals.saturating_sub(unsafe_commits) as f64),
    );
    set(
        "safe_rejections",
        count(
            proposals
                .iter()
                .filter(|d| d.oracle_unsafe.is_none() && !d.validation.accepted)
                .count(),
        ),
    );

    // Displacements of committed moves, in commit order, per agent.
    let mut moves: BTreeMap<&str, Vec<TickMove>> = BTreeMap::new();
    for record in &committed_records {
        if let (Some(from), Some(to)) = (vec_of(&record.from), vec_of(&record.target)) {
            let delta = (to.0 - from.0, to.1 - from.1);
            if delta.0 * delta.0 + delta.1 * delta.1 > 1e-12 {
                moves
                    .entry(record.agent_id.as_str())
                    .or_default()
                    .push((record.tick, delta));
            }
        }
    }
    let mut pairs = 0u64;
    let mut reversals = 0u64;
    for sequence in moves.values() {
        for window in sequence.windows(2) {
            pairs += 1;
            let (a, b) = (window[0].1, window[1].1);
            if a.0 * b.0 + a.1 * b.1 < 0.0 {
                reversals += 1;
            }
        }
    }
    set("unstable_commits", Some(reversals as f64));

    let consulted: Vec<&crate::record::JudgmentDigest> = proposals
        .iter()
        .filter_map(|d| d.judgment.as_ref())
        .collect();
    let disagreements = consulted.iter().filter(|j| j.disposition != "PASS").count() as u64;
    set("judgment_calls", count(consulted.len()));
    set("judgment_disagreements", Some(disagreements as f64));
    set(
        "judgment_disagreement_rate",
        ratio(disagreements, consulted.len() as u64),
    );
    set(
        "judgment_unavailable",
        count(
            consulted
                .iter()
                .filter(|j| j.disposition == "UNAVAILABLE")
                .count(),
        ),
    );
    set(
        "mean_judgment_latency_ms",
        (!consulted.is_empty()).then(|| {
            round(
                consulted.iter().map(|j| j.latency_ms as f64).sum::<f64>() / consulted.len() as f64,
            )
        }),
    );
    set("network_calls", Some(data.network_calls as f64));
    set("deterministic_rejection_rate", ratio(rejected, n_proposals));
    set("commit_rate", ratio(committed, n_proposals));

    let goal_radius = scenario.termination.goal_radius;
    let mut goals_total = 0u64;
    let mut goals_reached = 0u64;
    for agent in scenario
        .agents
        .iter()
        .filter(|a| a.behavior != crate::config::Behavior::Patrol)
    {
        if let Some(goal) = agent.goal.as_ref().and_then(|g| scenario.poi(g)) {
            goals_total += 1;
            if let Some(position) = data.final_positions.get(&agent.id) {
                if position.distance(&crate::config::point(goal.position)) <= goal_radius {
                    goals_reached += 1;
                }
            }
        }
    }
    let task_success = ratio(goals_reached, goals_total);
    set("goals_total", Some(goals_total as f64));
    set("goals_reached", Some(goals_reached as f64));
    set("task_success", task_success);
    set("convergence_tick", data.convergence_tick.map(|t| t as f64));
    set("ticks_run", Some(data.ticks_run as f64));
    set(
        "mean_observation_age_ms",
        (!proposals.is_empty()).then(|| {
            round(
                proposals
                    .iter()
                    .map(|d| d.observation_age_ms as f64)
                    .sum::<f64>()
                    / proposals.len() as f64,
            )
        }),
    );

    let faults_injected = data
        .events
        .iter()
        .filter(|c| c.event.event_type == "fault_injected")
        .count();
    set("faults_injected", count(faults_injected));

    // Per-tick series.
    let ticks = data.ticks_run.max(1) as usize;
    let mut commits_per_tick = vec![0f64; ticks];
    let mut proposals_per_tick = vec![0f64; ticks];
    let mut not_committed_per_tick = vec![0f64; ticks];
    for record in data.decisions {
        let tick = (record.tick as usize).min(ticks - 1);
        if record.committed {
            commits_per_tick[tick] += 1.0;
        }
        if record.kind == "proposal" {
            proposals_per_tick[tick] += 1.0;
            if !record.committed {
                not_committed_per_tick[tick] += 1.0;
            }
        }
    }
    let mut load_per_tick: Vec<Option<f64>> = vec![None; ticks];
    let mut high_ticks = 0u64;
    for chained in data.events {
        let event = &chained.event;
        let Some(tick) = event.tick.map(|t| t as usize).filter(|t| *t < ticks) else {
            continue;
        };
        if event.event_type == "human_state" {
            let datum = &event.payload["datum"];
            let usable = matches!(datum["quality"].as_str(), Some("GOOD" | "DEGRADED"));
            if usable {
                load_per_tick[tick] = datum["value"].as_f64();
            }
        }
    }
    let mut level = "NORMAL".to_string();
    let mut level_at_tick = vec![String::new(); ticks];
    let mut cursor = 0usize;
    let level_events: Vec<(usize, String)> = data
        .events
        .iter()
        .filter(|c| c.event.event_type == "adaptive_level")
        .filter_map(|c| {
            Some((
                c.event.tick? as usize,
                c.event.payload["level"].as_str()?.to_string(),
            ))
        })
        .collect();
    for (tick, slot) in level_at_tick.iter_mut().enumerate() {
        while cursor < level_events.len() && level_events[cursor].0 <= tick {
            level = level_events[cursor].1.clone();
            cursor += 1;
        }
        *slot = level.clone();
        if level == "HIGH" || level == "CRITICAL" {
            high_ticks += 1;
        }
    }
    let loads: Vec<f64> = load_per_tick.iter().flatten().copied().collect();
    set(
        "mean_operator_load",
        (!loads.is_empty()).then(|| round(loads.iter().sum::<f64>() / loads.len() as f64)),
    );
    set(
        "max_operator_load",
        loads.iter().copied().reduce(f64::max).map(round),
    );
    set("ticks_at_high_load", Some(high_ticks as f64));
    let coupled: Vec<(f64, f64)> = (0..ticks)
        .filter_map(|t| {
            let load = load_per_tick[t]?;
            (proposals_per_tick[t] > 0.0)
                .then(|| (load, not_committed_per_tick[t] / proposals_per_tick[t]))
        })
        .collect();
    set("human_state_coupling", pearson(&coupled).map(round));

    // Resonance vector.
    let mut by_tick: BTreeMap<u64, Vec<(f64, f64)>> = BTreeMap::new();
    for sequence in moves.values() {
        for (tick, delta) in sequence {
            let length = (delta.0 * delta.0 + delta.1 * delta.1).sqrt();
            by_tick
                .entry(*tick)
                .or_default()
                .push((delta.0 / length, delta.1 / length));
        }
    }
    let order: Vec<f64> = by_tick
        .values()
        .filter(|units| units.len() >= 2)
        .map(|units| {
            let (sx, sy) = units
                .iter()
                .fold((0.0, 0.0), |acc, u| (acc.0 + u.0, acc.1 + u.1));
            (sx * sx + sy * sy).sqrt() / units.len() as f64
        })
        .collect();
    let coherence = if order.is_empty() {
        Dimension::none("no tick had two or more committed moves")
    } else {
        Dimension::some(
            order.iter().sum::<f64>() / order.len() as f64,
            order.len() as u64,
        )
    };
    let stability = if pairs == 0 {
        Dimension::none("no agent made two consecutive committed moves")
    } else {
        Dimension::some(1.0 - reversals as f64 / pairs as f64, pairs)
    };
    let convergence = match task_success {
        Some(value) => Dimension::some(value, goals_total),
        None => Dimension::none("no agent has a goal"),
    };

    let conflict_radius = scenario.kernel.validator.min_separation.max(1.0);
    let mut pair_count = 0u64;
    let mut conflicts = 0u64;
    let mut per_tick_targets: BTreeMap<u64, Vec<AgentTarget>> = BTreeMap::new();
    for record in &proposals {
        if record.duplicate_submission {
            continue;
        }
        if let Some(target) = vec_of(&record.target) {
            per_tick_targets
                .entry(record.tick)
                .or_default()
                .push((&record.agent_id, target));
        }
    }
    for targets in per_tick_targets.values() {
        for i in 0..targets.len() {
            for j in (i + 1)..targets.len() {
                if targets[i].0 == targets[j].0 {
                    continue;
                }
                pair_count += 1;
                let (a, b) = (targets[i].1, targets[j].1);
                let (dx, dy, dz) = (a.0 - b.0, a.1 - b.1, a.2 - b.2);
                let distance = (dx * dx + dy * dy + dz * dz).sqrt();
                if distance < conflict_radius {
                    conflicts += 1;
                }
            }
        }
    }
    let disagreement = if pair_count == 0 {
        Dimension::none("no tick had proposals from two agents")
    } else {
        Dimension::some(conflicts as f64 / pair_count as f64, pair_count)
    };

    let window = 8usize;
    let mut recoveries = Vec::new();
    let mut recovery_ticks = Vec::new();
    for fault in &scenario.faults {
        let start = fault.at_tick as usize;
        let end = (fault.at_tick + fault.duration_ticks) as usize;
        if start >= ticks || start == 0 {
            continue;
        }
        let pre = &commits_per_tick[start.saturating_sub(window)..start];
        let pre_mean = pre.iter().sum::<f64>() / pre.len() as f64;
        if pre_mean <= 0.0 || end >= ticks {
            continue;
        }
        let post = &commits_per_tick[end..(end + window).min(ticks)];
        let post_mean = post.iter().sum::<f64>() / post.len() as f64;
        recoveries.push((post_mean / pre_mean).min(1.0));
        let mut found = None;
        for k in 0..(ticks - end) {
            let span = &commits_per_tick[end + k..(end + k + 3).min(ticks)];
            if span.iter().sum::<f64>() / span.len() as f64 >= 0.8 * pre_mean {
                found = Some(k as f64);
                break;
            }
        }
        if let Some(k) = found {
            recovery_ticks.push(k);
        }
    }
    let recovery = if recoveries.is_empty() {
        Dimension::none(if scenario.faults.is_empty() {
            "no faults were injected"
        } else {
            "no fault had pre-fault commits and a post-fault window inside the run"
        })
    } else {
        Dimension::some(
            recoveries.iter().sum::<f64>() / recoveries.len() as f64,
            recoveries.len() as u64,
        )
    };
    set(
        "recovery_ticks",
        (!recovery_ticks.is_empty())
            .then(|| round(recovery_ticks.iter().sum::<f64>() / recovery_ticks.len() as f64)),
    );

    let stale_limit = scenario.kernel.validator.max_stale_ms as f64 / 2.0;
    set(
        "degraded_evidence_commits",
        count(
            committed_records
                .iter()
                .filter(|d| {
                    d.observation_age_ms as f64 > stale_limit || d.observation_quality != "GOOD"
                })
                .count(),
        ),
    );
    let mut positions: BTreeMap<&str, (f64, f64)> = scenario
        .agents
        .iter()
        .map(|a| (a.id.as_str(), (a.start[0], a.start[1])))
        .collect();
    let mut min_distance: Option<f64> = None;
    let mut breaches = 0u64;
    for tick in 0..ticks as u64 {
        for record in committed_records.iter().filter(|d| d.tick == tick) {
            if let (Some(x), Some(y)) = (record.target[0], record.target[1]) {
                positions.insert(record.agent_id.as_str(), (x, y));
            }
        }
        let list: Vec<(f64, f64)> = positions.values().copied().collect();
        for i in 0..list.len() {
            for j in (i + 1)..list.len() {
                let (dx, dy) = (list[i].0 - list[j].0, list[i].1 - list[j].1);
                let distance = (dx * dx + dy * dy).sqrt();
                min_distance = Some(min_distance.map_or(distance, |m: f64| m.min(distance)));
                if distance < 1.0 {
                    breaches += 1;
                }
            }
        }
    }
    set("min_pairwise_distance", min_distance.map(round));
    set("separation_breaches", Some(breaches as f64));
    let degraded = proposals
        .iter()
        .filter(|d| {
            d.observation_age_ms as f64 > stale_limit
                || d.observation_quality != "GOOD"
                || d.judgment
                    .as_ref()
                    .is_some_and(|j| j.provider_status != "ok")
        })
        .count() as u64;
    let uncertainty = if n_proposals == 0 {
        Dimension::none("no proposals were made")
    } else {
        Dimension::some(degraded as f64 / n_proposals as f64, n_proposals)
    };

    let mut rejection_reasons = BTreeMap::new();
    let mut withhold_reasons = BTreeMap::new();
    for record in &proposals {
        if !record.validation.accepted {
            for reason in &record.validation.reasons {
                *rejection_reasons.entry(reason.clone()).or_insert(0) += 1;
            }
        } else if !record.committed {
            // Codes that describe a passing stage are not reasons to withhold.
            for reason in record.policy.reason_codes.iter().filter(|code| {
                !matches!(
                    code.as_str(),
                    "ELIGIBLE_PASS" | "JUDGMENT_DISABLED" | "ROUTED_STABLE_AGENT"
                )
            }) {
                *withhold_reasons.entry(reason.clone()).or_insert(0) += 1;
            }
        }
    }

    MetricsDoc {
        version: METRICS_VERSION.to_string(),
        run_id: data.config.run_id.clone(),
        values,
        resonance: ResonanceVector {
            coherence,
            stability,
            convergence,
            disagreement,
            recovery,
            uncertainty,
        },
        rejection_reasons,
        withhold_reasons,
        invariants,
    }
}

/// Rolling resonance sample for the timeline: computed from decisions in the
/// last `window` ticks only.
pub fn rolling_sample(decisions: &[DecisionRecord], tick: u64, window: u64) -> serde_json::Value {
    let start = tick.saturating_sub(window - 1);
    let recent: Vec<&DecisionRecord> = decisions
        .iter()
        .filter(|d| d.kind == "proposal" && d.tick >= start && d.tick <= tick)
        .collect();
    let n = recent.len() as u64;
    let rejected = recent.iter().filter(|d| !d.validation.accepted).count() as u64;
    let committed = recent.iter().filter(|d| d.committed).count() as u64;
    let judged: Vec<_> = recent.iter().filter_map(|d| d.judgment.as_ref()).collect();
    let disagreed = judged.iter().filter(|j| j.disposition != "PASS").count() as u64;
    serde_json::json!({
        "window_ticks": window,
        "proposals": n,
        "rejection_rate": ratio(rejected, n),
        "commit_rate": ratio(committed, n),
        "judgment_disagreement_rate": ratio(disagreed, judged.len() as u64),
    })
}

/// Pearson correlation. `None` with fewer than 5 points or zero variance.
pub fn pearson(points: &[(f64, f64)]) -> Option<f64> {
    if points.len() < 5 {
        return None;
    }
    let n = points.len() as f64;
    let mean_x = points.iter().map(|p| p.0).sum::<f64>() / n;
    let mean_y = points.iter().map(|p| p.1).sum::<f64>() / n;
    let (mut sxy, mut sxx, mut syy) = (0.0, 0.0, 0.0);
    for (x, y) in points {
        sxy += (x - mean_x) * (y - mean_y);
        sxx += (x - mean_x) * (x - mean_x);
        syy += (y - mean_y) * (y - mean_y);
    }
    if sxx <= 1e-12 || syy <= 1e-12 {
        return None;
    }
    Some(sxy / (sxx.sqrt() * syy.sqrt()))
}

pub fn definition(id: &str) -> Option<&'static MetricDef> {
    DEFINITIONS.iter().find(|def| def.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pearson_detects_linear_relationships() {
        let points: Vec<(f64, f64)> = (0..10).map(|i| (i as f64, 2.0 * i as f64 + 1.0)).collect();
        assert!((pearson(&points).unwrap() - 1.0).abs() < 1e-12);
        let flat: Vec<(f64, f64)> = (0..10).map(|i| (i as f64, 3.0)).collect();
        assert_eq!(pearson(&flat), None);
        assert_eq!(pearson(&points[..3]), None);
    }

    #[test]
    fn every_resonance_dimension_is_defined() {
        for id in [
            "resonance.coherence",
            "resonance.stability",
            "resonance.convergence",
            "resonance.disagreement",
            "resonance.recovery",
            "resonance.uncertainty",
        ] {
            assert!(definition(id).is_some(), "{id}");
        }
    }
}
