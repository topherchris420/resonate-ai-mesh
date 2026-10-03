//! Counterfactual branches and timeline comparison.
//!
//! A counterfactual re-runs a recorded run with changed configuration (judgment
//! disabled, a different policy threshold, an agent removed, a delayed
//! observation) and the same seed. The original bundle is never modified; the
//! branch is a new bundle whose provenance names its parent and overrides.
//! [`compare`] reports where the two timelines first diverge, which decisions
//! changed, and how the metrics moved.

use crate::config::{self, ConfigError, JudgeProvider, RunConfig};
use crate::record::{Bundle, DecisionRecord, ParentRun};
use crate::replay::{first_divergence, Divergence, Normalization};
use crate::runner::{self, RunOptions, RunResult, Substitutions};
use event_bus::ChainedEvent;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DecisionChange {
    pub proposal_id: String,
    pub agent_id: String,
    /// Tick the proposal was first made.
    pub tick: u64,
    /// Final outcome on each side, `None` when the proposal was never made.
    pub left: Option<String>,
    pub right: Option<String>,
    /// Tick at which the final outcome was decided on each side.
    pub left_decided_at: Option<u64>,
    pub right_decided_at: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StateAtTick {
    pub tick: u64,
    pub left: String,
    pub right: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MetricDelta {
    pub metric: String,
    pub left: Option<f64>,
    pub right: Option<f64>,
    pub delta: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TimelineComparison {
    pub left_run: String,
    pub right_run: String,
    pub overrides: BTreeMap<String, Value>,
    pub left_events: u64,
    pub right_events: u64,
    pub first_divergent_event: Option<Divergence>,
    /// First tick at which the authoritative state hashes differ.
    pub first_state_divergence_tick: Option<u64>,
    /// First tick with a proposal whose outcome or decision time differs.
    pub first_decision_divergence_tick: Option<u64>,
    pub state_by_tick: Vec<StateAtTick>,
    pub decision_changes: Vec<DecisionChange>,
    pub metric_deltas: Vec<MetricDelta>,
}

/// Authoritative state hash at the end of each tick, carried forward.
pub fn state_by_tick(events: &[ChainedEvent], ticks: u64) -> Vec<String> {
    let mut hashes = Vec::with_capacity(ticks as usize);
    let mut current = String::new();
    let mut cursor = 0;
    for tick in 0..ticks {
        while cursor < events.len() && events[cursor].event.tick.unwrap_or(0) <= tick {
            let event = &events[cursor].event;
            if event.event_type == "state_transition" {
                if let Some(hash) = event.payload["after_hash"].as_str() {
                    current = hash.to_string();
                }
            }
            cursor += 1;
        }
        hashes.push(current.clone());
    }
    hashes
}

/// Final outcome of each proposal: (agent, tick first proposed, outcome, tick decided).
fn final_outcomes(decisions: &[DecisionRecord]) -> BTreeMap<String, (String, u64, String, u64)> {
    let mut outcomes: BTreeMap<String, (String, u64, String, u64)> = BTreeMap::new();
    for record in decisions.iter().filter(|d| !d.duplicate_submission) {
        // A later human resolution supersedes the first-pass outcome.
        let first_tick = outcomes
            .get(&record.proposal_id)
            .map(|o| o.1)
            .unwrap_or(record.tick);
        outcomes.insert(
            record.proposal_id.clone(),
            (
                record.agent_id.clone(),
                first_tick,
                record.policy.outcome.clone(),
                record.tick,
            ),
        );
    }
    outcomes
}

/// One side of a timeline comparison.
pub struct TimelineSide<'a> {
    pub run_id: &'a str,
    pub events: &'a [ChainedEvent],
    pub decisions: &'a [DecisionRecord],
    pub metrics: &'a Value,
}

pub fn compare(
    left: TimelineSide<'_>,
    right: TimelineSide<'_>,
    overrides: BTreeMap<String, Value>,
) -> TimelineComparison {
    let (left_run, left_events, left_decisions, left_metrics) =
        (left.run_id, left.events, left.decisions, left.metrics);
    let (right_run, right_events, right_decisions, right_metrics) =
        (right.run_id, right.events, right.decisions, right.metrics);
    let rules = Normalization {
        ignore_run_identity: true,
        ignore_software_version: true,
        ignore_judgment_mode: false,
    };
    let first_divergent_event = first_divergence(left_events, right_events, &rules);
    let ticks = left_events
        .iter()
        .chain(right_events)
        .filter_map(|c| c.event.tick)
        .max()
        .map(|t| t + 1)
        .unwrap_or(0);
    let left_states = state_by_tick(left_events, ticks);
    let right_states = state_by_tick(right_events, ticks);
    let state_by_tick: Vec<StateAtTick> = (0..ticks as usize)
        .map(|t| StateAtTick {
            tick: t as u64,
            left: left_states[t].clone(),
            right: right_states[t].clone(),
        })
        .collect();
    let first_state_divergence_tick = state_by_tick
        .iter()
        .find(|row| row.left != row.right)
        .map(|row| row.tick);

    let left_outcomes = final_outcomes(left_decisions);
    let right_outcomes = final_outcomes(right_decisions);
    let mut ids: Vec<&String> = left_outcomes.keys().chain(right_outcomes.keys()).collect();
    ids.sort();
    ids.dedup();
    let mut decision_changes: Vec<DecisionChange> = ids
        .into_iter()
        .filter_map(|id| {
            let left = left_outcomes.get(id);
            let right = right_outcomes.get(id);
            if left.map(|l| (&l.2, l.3)) == right.map(|r| (&r.2, r.3)) {
                return None;
            }
            let agent_id = left.or(right).map(|o| o.0.clone()).unwrap_or_default();
            let tick = match (left, right) {
                (Some(l), Some(r)) => l.1.min(r.1),
                (Some(o), None) | (None, Some(o)) => o.1,
                (None, None) => 0,
            };
            Some(DecisionChange {
                proposal_id: id.clone(),
                agent_id,
                tick,
                left: left.map(|o| o.2.clone()),
                right: right.map(|o| o.2.clone()),
                left_decided_at: left.map(|o| o.3),
                right_decided_at: right.map(|o| o.3),
            })
        })
        .collect();
    decision_changes.sort_by(|a, b| a.tick.cmp(&b.tick).then(a.proposal_id.cmp(&b.proposal_id)));
    let first_decision_divergence_tick = decision_changes.first().map(|change| change.tick);

    let mut metric_deltas = Vec::new();
    if let (Some(Value::Object(left)), Some(Value::Object(right))) =
        (left_metrics.get("values"), right_metrics.get("values"))
    {
        let mut keys: Vec<&String> = left.keys().chain(right.keys()).collect();
        keys.sort();
        keys.dedup();
        for key in keys {
            let l = left.get(key).and_then(Value::as_f64);
            let r = right.get(key).and_then(Value::as_f64);
            if l != r {
                metric_deltas.push(MetricDelta {
                    metric: key.clone(),
                    left: l,
                    right: r,
                    delta: match (l, r) {
                        (Some(a), Some(b)) => Some(event_bus::quantize(b - a, 6)),
                        _ => None,
                    },
                });
            }
        }
    }
    for dimension in [
        "coherence",
        "stability",
        "convergence",
        "disagreement",
        "recovery",
        "uncertainty",
    ] {
        let l = left_metrics["resonance"][dimension]["value"].as_f64();
        let r = right_metrics["resonance"][dimension]["value"].as_f64();
        if l != r {
            metric_deltas.push(MetricDelta {
                metric: format!("resonance.{dimension}"),
                left: l,
                right: r,
                delta: match (l, r) {
                    (Some(a), Some(b)) => Some(event_bus::quantize(b - a, 6)),
                    _ => None,
                },
            });
        }
    }

    TimelineComparison {
        left_run: left_run.to_string(),
        right_run: right_run.to_string(),
        overrides,
        left_events: left_events.len() as u64,
        right_events: right_events.len() as u64,
        first_divergent_event,
        first_state_divergence_tick,
        first_decision_divergence_tick,
        state_by_tick,
        decision_changes,
        metric_deltas,
    }
}

pub fn label_ok(label: &str) -> bool {
    !label.is_empty()
        && label.len() <= 48
        && label
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-'))
}

/// Configuration of a counterfactual branch of `parent`.
pub fn branch_config(
    parent: &RunConfig,
    overrides: &BTreeMap<String, Value>,
    label: &str,
) -> Result<RunConfig, ConfigError> {
    if !label_ok(label) {
        return Err(ConfigError::Invalid(format!(
            "counterfactual label `{label}` must match [A-Za-z0-9_-]{{1,48}}"
        )));
    }
    let mut all = parent.overrides.clone();
    all.extend(overrides.clone());
    let mut config = config::resolve_with_overrides(
        // Resolve against the parent's *resolved* scenario, so the branch
        // differs from the parent only by the new overrides.
        &parent.scenario,
        &parent.experiment_id,
        &parent.condition_id,
        parent.repetition,
        parent.seed,
        overrides.clone(),
    )?;
    config.run_id = format!("{}~{label}", parent.run_id);
    config.overrides = all;
    config.validate()?;
    Ok(config)
}

pub struct Branch {
    pub result: RunResult,
    pub comparison: TimelineComparison,
    pub parent: ParentRun,
}

/// Run a counterfactual of a recorded bundle. Recorded judgments are reused
/// only when the branch keeps the same networked or recorded judge; a recorded
/// answer is only valid for identical evidence, which the state-hash check
/// enforces.
pub async fn run_branch(
    bundle: &Bundle,
    overrides: BTreeMap<String, Value>,
    label: &str,
    allow_network: bool,
) -> Result<Branch, String> {
    let config =
        branch_config(&bundle.config, &overrides, label).map_err(|error| error.to_string())?;
    let mut substitutions: Substitutions = crate::replay::substitutions_for(bundle);
    let original_provider = bundle.config.scenario.kernel.judgment.provider;
    let branch_provider = config.scenario.kernel.judgment.provider;
    if branch_provider != original_provider
        || !matches!(
            branch_provider,
            JudgeProvider::Typesafe | JudgeProvider::Recorded
        )
    {
        substitutions.judgments = None;
    }
    let result = runner::run(
        &config,
        RunOptions {
            substitutions,
            allow_network,
            ..RunOptions::default()
        },
    )
    .await
    .map_err(|error| error.to_string())?;
    let right_metrics = serde_json::to_value(&result.metrics).unwrap_or(Value::Null);
    let left_decisions: Vec<DecisionRecord> =
        crate::record::read_jsonl(&bundle.dir.join("decisions.jsonl"))
            .map_err(|error| error.to_string())?;
    let comparison = compare(
        TimelineSide {
            run_id: &bundle.config.run_id,
            events: &bundle.events,
            decisions: &left_decisions,
            metrics: &bundle.metrics,
        },
        TimelineSide {
            run_id: &config.run_id,
            events: &result.events,
            decisions: &result.decisions,
            metrics: &right_metrics,
        },
        overrides.clone(),
    );
    Ok(Branch {
        parent: ParentRun {
            run_id: bundle.config.run_id.clone(),
            head_hash: bundle.replay.head_hash.clone(),
            overrides,
            label: label.to_string(),
        },
        result,
        comparison,
    })
}

pub fn render(comparison: &TimelineComparison) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    let _ = writeln!(out, "TIMELINE COMPARISON");
    let _ = writeln!(
        out,
        "  left:   {} ({} events)",
        comparison.left_run, comparison.left_events
    );
    let _ = writeln!(
        out,
        "  right:  {} ({} events)",
        comparison.right_run, comparison.right_events
    );
    if !comparison.overrides.is_empty() {
        let _ = writeln!(
            out,
            "  change: {}",
            serde_json::to_string(&comparison.overrides).unwrap_or_default()
        );
    }
    match &comparison.first_divergent_event {
        None => {
            let _ = writeln!(out, "\n  The timelines are identical (after ignoring run identity and software version).");
        }
        Some(divergence) => {
            let at = divergence
                .left
                .as_ref()
                .or(divergence.right.as_ref())
                .map(|e| {
                    format!(
                        "tick {}, {} {}",
                        e.tick.unwrap_or(0),
                        e.event_type,
                        e.subject_id
                    )
                })
                .unwrap_or_default();
            let _ = writeln!(
                out,
                "\n  First divergent event: #{} ({at})",
                divergence.index
            );
            for difference in divergence.differences.iter().take(8) {
                let _ = writeln!(
                    out,
                    "    {}: {} -> {}",
                    difference.path, difference.left, difference.right
                );
            }
        }
    }
    match comparison.first_decision_divergence_tick {
        Some(tick) => {
            let _ = writeln!(out, "  First decision divergence: tick {tick}");
        }
        None => {
            let _ = writeln!(out, "  Decisions: identical");
        }
    }
    match comparison.first_state_divergence_tick {
        Some(tick) => {
            let _ = writeln!(out, "  First authoritative-state divergence: tick {tick}");
        }
        None => {
            let _ = writeln!(out, "  Authoritative state: identical at every tick");
        }
    }
    let _ = writeln!(
        out,
        "\n  Decisions that changed: {}",
        comparison.decision_changes.len()
    );
    for change in comparison.decision_changes.iter().take(15) {
        let side = |outcome: &Option<String>, at: Option<u64>| match (outcome, at) {
            (Some(outcome), Some(at)) => format!("{outcome} (t{at})"),
            _ => "(not proposed)".to_string(),
        };
        let _ = writeln!(
            out,
            "    t{:<3} {:<26} {:<34} -> {}",
            change.tick,
            change.proposal_id,
            side(&change.left, change.left_decided_at),
            side(&change.right, change.right_decided_at)
        );
    }
    if comparison.decision_changes.len() > 15 {
        let _ = writeln!(
            out,
            "    ... {} more",
            comparison.decision_changes.len() - 15
        );
    }
    let _ = writeln!(out, "\n  Metric changes:");
    for delta in &comparison.metric_deltas {
        let _ = writeln!(
            out,
            "    {:<32} {:>10} -> {:<10} ({})",
            delta.metric,
            crate::bundle::fmt_value(delta.left),
            crate::bundle::fmt_value(delta.right),
            delta
                .delta
                .map(crate::bundle::fmt_delta)
                .unwrap_or_else(|| "n/a".into())
        );
    }
    out
}
