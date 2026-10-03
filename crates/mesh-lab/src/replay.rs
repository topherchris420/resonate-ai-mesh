//! Verifying, replaying, and comparing recorded runs.
//!
//! *Integrity* checks that a bundle is what it claims to be: the event chain
//! links from the hash of the recorded configuration to the recorded head, and
//! every file matches the hash in provenance.
//!
//! *Replay* re-executes the run from its configuration and seed, with no
//! network access, and compares every event. Deterministic components (the
//! kernel, built-in agents, simulators, mock judges) are re-executed.
//! Components that cannot be re-executed reproducibly (a networked judge, an
//! external agent process, a person's live commands) are substituted from the
//! recording. Substitutions are listed in the report.

use crate::agents::AgentIntent;
use crate::config::RunConfig;
use crate::record::{file_hash, Bundle, BundleError};
use crate::runner::{self, ControlCommand, RunOptions, RunResult, Substitutions};
use event_bus::{canonical_json, chain::verify_chain, ChainedEvent, SOFTWARE_VERSION};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use typed_judgment::ProviderKind;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IntegrityReport {
    pub ok: bool,
    pub chain_ok: bool,
    pub genesis_matches_config: bool,
    pub head_matches_replay_info: bool,
    pub files_checked: u64,
    pub files_matching: u64,
    pub problems: Vec<String>,
}

pub fn verify_integrity(bundle: &Bundle) -> IntegrityReport {
    let mut problems = Vec::new();
    let genesis = bundle.config.hash();
    let genesis_matches_config = genesis == bundle.replay.genesis_hash;
    if !genesis_matches_config {
        problems.push("manifest.json does not hash to the recorded genesis".to_string());
    }
    let chain = verify_chain(&genesis, &bundle.events);
    let chain_ok = chain.is_ok();
    let head_matches_replay_info = match &chain {
        Ok(head) => {
            if head != &bundle.replay.head_hash {
                problems.push("event log head differs from replay.json".to_string());
            }
            head == &bundle.replay.head_hash
        }
        Err(error) => {
            problems.push(format!("event chain broken: {error}"));
            false
        }
    };
    let mut files_checked = 0;
    let mut files_matching = 0;
    for (name, recorded) in &bundle.provenance.file_hashes {
        files_checked += 1;
        match file_hash(&bundle.dir.join(name)) {
            Ok(actual) if &actual == recorded => files_matching += 1,
            Ok(_) => problems.push(format!("{name} does not match its recorded hash")),
            Err(error) => problems.push(error.to_string()),
        }
    }
    IntegrityReport {
        ok: problems.is_empty(),
        chain_ok,
        genesis_matches_config,
        head_matches_replay_info,
        files_checked,
        files_matching,
        problems,
    }
}

/// Build the substitutions replay needs from a recorded bundle.
pub fn substitutions_for(bundle: &Bundle) -> Substitutions {
    let mut substitutions = Substitutions::default();
    let judge_needs_recording = bundle.provenance.judge.as_ref().is_some_and(|judge| {
        judge.networked
            || judge.kind == ProviderKind::RemoteModel
            || judge.kind == ProviderKind::Recorded
    });
    if judge_needs_recording {
        substitutions.judgments = Some(bundle.judgments.clone());
    }
    let nondeterministic: Vec<&str> = bundle
        .provenance
        .agents
        .iter()
        .filter(|agent| !agent.deterministic || agent.adapter.starts_with("recorded:"))
        .map(|agent| agent.agent_id.as_str())
        .collect();
    for recorded in &bundle.intents {
        if nondeterministic.contains(&recorded.agent_id.as_str()) {
            substitutions
                .intents
                .entry(recorded.agent_id.clone())
                .or_default()
                .insert(recorded.tick, recorded.intent.clone());
        }
    }
    for chained in &bundle.events {
        if chained.event.event_type == "operator_command" {
            if let (Some(tick), Ok(command)) = (
                chained.event.tick,
                serde_json::from_value::<ControlCommand>(chained.event.payload.clone()),
            ) {
                substitutions
                    .commands
                    .entry(tick)
                    .or_default()
                    .push(command);
            }
        }
    }
    substitutions
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EventSummary {
    pub seq: u64,
    pub event_id: String,
    pub event_type: String,
    pub tick: Option<u64>,
    pub subject_id: String,
    pub source: String,
}

impl EventSummary {
    pub fn of(chained: &ChainedEvent) -> Self {
        Self {
            seq: chained.event.seq,
            event_id: chained.event.event_id.clone(),
            event_type: chained.event.event_type.clone(),
            tick: chained.event.tick,
            subject_id: chained.event.subject_id.clone(),
            source: chained.event.source.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FieldDifference {
    pub path: String,
    pub left: Value,
    pub right: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Divergence {
    pub index: usize,
    pub left: Option<EventSummary>,
    pub right: Option<EventSummary>,
    pub differences: Vec<FieldDifference>,
}

/// Fields ignored when comparing events, and why.
#[derive(Debug, Clone, Default)]
pub struct Normalization {
    /// Run and experiment ids (differ between a run and its counterfactual).
    pub ignore_run_identity: bool,
    /// Semantic software version (a release bump alone is not a behavior change).
    pub ignore_software_version: bool,
    /// Judgment evaluation mode (LIVE vs RECORDED_JUDGMENT under substitution).
    pub ignore_judgment_mode: bool,
}

pub fn normalized(event: &ChainedEvent, rules: &Normalization) -> Value {
    let mut value = serde_json::to_value(&event.event).expect("events serialize");
    if let Value::Object(map) = &mut value {
        if rules.ignore_run_identity {
            for key in ["subject_id", "correlation_id"] {
                if map.get(key).and_then(Value::as_str) == Some(event.event.run_id.as_str()) {
                    map.insert(key.to_string(), Value::String("<run>".into()));
                }
            }
            map.remove("run_id");
            map.remove("experiment_id");
        }
        if rules.ignore_software_version {
            map.remove("software_version");
        }
        if rules.ignore_judgment_mode && event.event.event_type == "judgment" {
            if let Some(Value::Object(payload)) = map.get_mut("payload") {
                payload.remove("evaluation_mode");
            }
        }
    }
    value
}

pub fn diff_values(left: &Value, right: &Value, path: &str, out: &mut Vec<FieldDifference>) {
    if out.len() >= 20 || left == right {
        return;
    }
    match (left, right) {
        (Value::Object(a), Value::Object(b)) => {
            let mut keys: Vec<&String> = a.keys().chain(b.keys()).collect();
            keys.sort();
            keys.dedup();
            for key in keys {
                let child = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{path}.{key}")
                };
                diff_values(
                    a.get(key).unwrap_or(&Value::Null),
                    b.get(key).unwrap_or(&Value::Null),
                    &child,
                    out,
                );
            }
        }
        (Value::Array(a), Value::Array(b)) if a.len() == b.len() => {
            for (index, (x, y)) in a.iter().zip(b).enumerate() {
                diff_values(x, y, &format!("{path}[{index}]"), out);
            }
        }
        _ => out.push(FieldDifference {
            path: path.to_string(),
            left: left.clone(),
            right: right.clone(),
        }),
    }
}

/// First position where two event logs differ after normalization. With
/// `ignore_run_identity`, the `run_started` event (which restates the
/// configuration, and so always differs between branches) is skipped.
pub fn first_divergence(
    left: &[ChainedEvent],
    right: &[ChainedEvent],
    rules: &Normalization,
) -> Option<Divergence> {
    let keep =
        |c: &&ChainedEvent| !(rules.ignore_run_identity && c.event.event_type == "run_started");
    let left: Vec<ChainedEvent> = left.iter().filter(keep).cloned().collect();
    let right: Vec<ChainedEvent> = right.iter().filter(keep).cloned().collect();
    let length = left.len().max(right.len());
    for index in 0..length {
        match (left.get(index), right.get(index)) {
            (Some(a), Some(b)) => {
                let (na, nb) = (normalized(a, rules), normalized(b, rules));
                if canonical_json(&na) != canonical_json(&nb) {
                    let mut differences = Vec::new();
                    if a.event.event_type != b.event.event_type {
                        // Different kinds of event: the type is the difference.
                        differences.push(FieldDifference {
                            path: "event_type".into(),
                            left: Value::String(a.event.event_type.clone()),
                            right: Value::String(b.event.event_type.clone()),
                        });
                    } else {
                        diff_values(&na, &nb, "", &mut differences);
                    }
                    return Some(Divergence {
                        index: a.event.seq as usize,
                        left: Some(EventSummary::of(a)),
                        right: Some(EventSummary::of(b)),
                        differences,
                    });
                }
            }
            (a, b) => {
                return Some(Divergence {
                    index: a.or(b).map(|c| c.event.seq as usize).unwrap_or(index),
                    left: a.map(EventSummary::of),
                    right: b.map(EventSummary::of),
                    differences: Vec::new(),
                })
            }
        }
    }
    None
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CountRow {
    pub label: String,
    pub recorded: u64,
    pub replayed: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReplayReport {
    pub run_id: String,
    pub verified: bool,
    pub integrity: IntegrityReport,
    pub counts: Vec<CountRow>,
    pub events_matching: u64,
    pub final_state_recorded: String,
    pub final_state_replayed: String,
    pub head_recorded: String,
    pub head_replayed: String,
    pub head_exact_match: bool,
    pub metrics_compared: u64,
    pub metric_differences: Vec<String>,
    pub network_calls: u64,
    pub substituted: Vec<String>,
    pub normalized_fields: Vec<String>,
    pub software_recorded: String,
    pub software_current: String,
    pub seed: u64,
    pub first_divergence: Option<Divergence>,
    pub error: Option<String>,
}

const COUNTED: &[(&str, &str)] = &[
    ("Proposals", "proposal"),
    ("Validator outputs", "validation"),
    ("Judgments", "judgment"),
    ("Policy decisions", "commitment"),
    ("State transitions", "state_transition"),
    ("Human-state samples", "human_state"),
    ("Observations", "observation"),
    ("Faults injected", "fault_injected"),
    ("Human resolutions", "human_resolution"),
];

fn count(events: &[ChainedEvent], event_type: &str) -> u64 {
    events
        .iter()
        .filter(|c| c.event.event_type == event_type)
        .count() as u64
}

/// Verify integrity, re-execute, and compare.
pub async fn replay_bundle(bundle: &Bundle) -> ReplayReport {
    let integrity = verify_integrity(bundle);
    let substitutions = substitutions_for(bundle);
    let mut substituted: Vec<String> = Vec::new();
    if substitutions.judgments.is_some() {
        substituted.push("judgment (recorded envelopes)".to_string());
    }
    for agent in substitutions.intents.keys() {
        substituted.push(format!("agent {agent} (recorded intents)"));
    }
    if !substitutions.commands.is_empty() {
        substituted.push("operator commands (recorded)".to_string());
    }
    let rules = Normalization {
        ignore_run_identity: false,
        ignore_software_version: true,
        ignore_judgment_mode: substitutions.judgments.is_some(),
    };
    let mut normalized_fields = vec!["software_version".to_string()];
    if rules.ignore_judgment_mode {
        normalized_fields.push("judgment.payload.evaluation_mode".to_string());
    }
    let software_recorded = bundle
        .events
        .first()
        .map(|c| c.event.software_version.clone())
        .unwrap_or_default();
    let mut report = ReplayReport {
        run_id: bundle.config.run_id.clone(),
        verified: false,
        integrity: integrity.clone(),
        counts: Vec::new(),
        events_matching: 0,
        final_state_recorded: bundle.replay.final_state_hash.clone(),
        final_state_replayed: String::new(),
        head_recorded: bundle.replay.head_hash.clone(),
        head_replayed: String::new(),
        head_exact_match: false,
        metrics_compared: 0,
        metric_differences: Vec::new(),
        network_calls: 0,
        substituted,
        normalized_fields,
        software_recorded,
        software_current: SOFTWARE_VERSION.to_string(),
        seed: bundle.config.seed,
        first_divergence: None,
        error: None,
    };
    if !integrity.ok {
        report.error =
            Some("integrity check failed; the recording was not re-executed".to_string());
        return report;
    }
    let replayed = match runner::run(
        &bundle.config,
        RunOptions {
            substitutions,
            allow_network: false,
            ..RunOptions::default()
        },
    )
    .await
    {
        Ok(result) => result,
        Err(error) => {
            report.error = Some(format!("re-execution failed: {error}"));
            return report;
        }
    };
    fill_comparison(&mut report, bundle, &replayed, &rules);
    report
}

fn fill_comparison(
    report: &mut ReplayReport,
    bundle: &Bundle,
    replayed: &RunResult,
    rules: &Normalization,
) {
    report.network_calls = replayed.network_calls;
    report.final_state_replayed = replayed.final_state.hash();
    report.head_replayed = replayed.head_hash.clone();
    report.head_exact_match = report.head_recorded == report.head_replayed;
    report.counts.push(CountRow {
        label: "Events".into(),
        recorded: bundle.events.len() as u64,
        replayed: replayed.events.len() as u64,
    });
    for (label, event_type) in COUNTED {
        report.counts.push(CountRow {
            label: label.to_string(),
            recorded: count(&bundle.events, event_type),
            replayed: count(&replayed.events, event_type),
        });
    }
    report.first_divergence = first_divergence(&bundle.events, &replayed.events, rules);
    report.events_matching = match &report.first_divergence {
        Some(divergence) => divergence.index as u64,
        None => bundle.events.len() as u64,
    };
    let recorded_metrics = &bundle.metrics;
    let replayed_metrics = serde_json::to_value(&replayed.metrics).unwrap_or(Value::Null);
    if let (Some(a), Some(b)) = (
        recorded_metrics.get("values"),
        replayed_metrics.get("values"),
    ) {
        if let (Value::Object(a), Value::Object(b)) = (a, b) {
            report.metrics_compared = a.len().max(b.len()) as u64;
        }
    }
    let mut differences = Vec::new();
    diff_values(
        recorded_metrics,
        &replayed_metrics,
        "metrics",
        &mut differences,
    );
    report.metric_differences = differences
        .iter()
        .map(|d| format!("{}: recorded {} replayed {}", d.path, d.left, d.right))
        .collect();
    report.verified = report.integrity.ok
        && report.first_divergence.is_none()
        && report.final_state_recorded == report.final_state_replayed
        && report.metric_differences.is_empty()
        && report.network_calls == 0;
}

pub fn render(report: &ReplayReport) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    let verdict = if report.verified {
        "REPLAY VERIFIED"
    } else if report.error.is_some() {
        "REPLAY FAILED"
    } else {
        "REPLAY DIVERGED"
    };
    let _ = writeln!(out, "{verdict}  {}\n", report.run_id);
    let i = &report.integrity;
    let _ = writeln!(
        out,
        "  Integrity:            {} (chain {}, {}/{} file hashes match)",
        if i.ok { "OK" } else { "FAILED" },
        if i.chain_ok { "intact" } else { "broken" },
        i.files_matching,
        i.files_checked
    );
    for problem in &i.problems {
        let _ = writeln!(out, "    ! {problem}");
    }
    if let Some(error) = &report.error {
        let _ = writeln!(out, "  Error:                {error}");
        return out;
    }
    for row in &report.counts {
        let mark = if row.recorded == row.replayed {
            ""
        } else {
            "   <- differs"
        };
        let _ = writeln!(
            out,
            "  {:<21} {:>7} / {:<7}{mark}",
            format!("{}:", row.label),
            row.recorded,
            row.replayed
        );
    }
    let _ = writeln!(
        out,
        "  {:<21} {:>7} / {:<7}",
        "Identical events:",
        report.events_matching,
        report.counts.first().map(|c| c.recorded).unwrap_or(0)
    );
    let _ = writeln!(
        out,
        "  Metrics:              {}",
        if report.metric_differences.is_empty() {
            format!(
                "{} / {} match",
                report.metrics_compared, report.metrics_compared
            )
        } else {
            format!("{} differ", report.metric_differences.len())
        }
    );
    let _ = writeln!(
        out,
        "  Final state hash:     {} {}",
        if report.final_state_recorded == report.final_state_replayed {
            "MATCH"
        } else {
            "MISMATCH"
        },
        report.final_state_replayed
    );
    let _ = writeln!(
        out,
        "  Event log head:       {} {}",
        if report.head_exact_match {
            "MATCH"
        } else if report.first_divergence.is_none() {
            "EQUIVALENT (normalized fields differ)"
        } else {
            "MISMATCH"
        },
        report.head_replayed
    );
    let _ = writeln!(out, "  Network calls:        {}", report.network_calls);
    let _ = writeln!(out, "  Seed:                 {}", report.seed);
    let _ = writeln!(
        out,
        "  Software:             recorded {} / current {}",
        report.software_recorded, report.software_current
    );
    if !report.substituted.is_empty() {
        let _ = writeln!(
            out,
            "  Substituted:          {}",
            report.substituted.join("; ")
        );
    }
    if let Some(divergence) = &report.first_divergence {
        let _ = writeln!(out, "\n  First divergence at event #{}", divergence.index);
        let describe = |side: &Option<EventSummary>| match side {
            Some(e) => format!(
                "{} {} (tick {}, subject {}, source {})",
                e.event_id,
                e.event_type,
                e.tick.map(|t| t.to_string()).unwrap_or_else(|| "-".into()),
                e.subject_id,
                e.source
            ),
            None => "(no event)".to_string(),
        };
        let _ = writeln!(out, "    recorded: {}", describe(&divergence.left));
        let _ = writeln!(out, "    replayed: {}", describe(&divergence.right));
        for difference in &divergence.differences {
            let _ = writeln!(
                out,
                "    {}: recorded {} -> replayed {}",
                difference.path, difference.left, difference.right
            );
        }
    }
    for difference in report.metric_differences.iter().take(10) {
        let _ = writeln!(out, "    metric {difference}");
    }
    out
}

pub fn load(dir: &std::path::Path) -> Result<Bundle, BundleError> {
    Bundle::load(dir)
}

/// Intents keyed by tick, for tests and external tools.
pub fn intents_by_tick(bundle: &Bundle, agent_id: &str) -> BTreeMap<u64, Option<AgentIntent>> {
    bundle
        .intents
        .iter()
        .filter(|i| i.agent_id == agent_id)
        .map(|i| (i.tick, i.intent.clone()))
        .collect()
}

/// Run config of a bundle, for counterfactuals.
pub fn config_of(bundle: &Bundle) -> &RunConfig {
    &bundle.config
}
