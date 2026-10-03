//! Experiments: conditions x repetitions, summary statistics, paired
//! comparisons, prediction checks, and the experiment report.

use crate::bundle::{fmt_value, save_run, short, SaveOptions};
use crate::config::{self, Direction, ExperimentManifest, Scenario};
use crate::metrics::{definition, DEFINITIONS};
use crate::record::{write_json, write_jsonl, BundleError};
use crate::runner::{self, RunOptions, Substitutions};
use crate::stats::{self, Descriptive, PairedComparison};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use typed_judgment::JudgmentEnvelope;

pub const SUMMARY_VERSION: &str = "resonate-ai-mesh.experiment-summary.v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Keep {
    /// Full bundle for repetition 0 of each condition.
    First,
    All,
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RunRow {
    pub condition: String,
    pub repetition: u32,
    pub seed: u64,
    pub run_id: String,
    pub head_hash: String,
    pub final_state_hash: String,
    pub ticks_run: u64,
    pub termination: String,
    pub metrics: BTreeMap<String, Option<f64>>,
    pub invariants_failed: Vec<String>,
    pub bundle: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FailureRate {
    pub runs_with_event: usize,
    pub runs: usize,
    pub rate: f64,
    pub wilson95: Option<[f64; 2]>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConditionSummary {
    pub id: String,
    pub description: String,
    pub status: String,
    pub skip_reason: Option<String>,
    pub overrides: BTreeMap<String, serde_json::Value>,
    pub runs: usize,
    pub metrics: BTreeMap<String, Descriptive>,
    /// Runs in which a count metric was above zero (e.g. unsafe commits).
    pub failure_rates: BTreeMap<String, FailureRate>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InvariantSummary {
    pub name: String,
    pub expected: bool,
    pub runs_checked: usize,
    pub runs_violated: usize,
    pub violating_runs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PredictionResult {
    pub metric: String,
    pub direction: Direction,
    pub baseline: String,
    pub treatment: String,
    /// supported | contradicted | inconclusive | not_evaluable
    pub status: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExperimentSummary {
    pub version: String,
    pub experiment_id: String,
    pub title: String,
    pub question: String,
    pub hypothesis: config::Hypothesis,
    pub manifest_hash: String,
    pub scenario_id: String,
    pub base_seed: u64,
    pub repetitions: u32,
    pub dependent_variables: Vec<String>,
    pub conditions: Vec<ConditionSummary>,
    pub comparisons: Vec<PairedComparison>,
    pub prediction: Option<PredictionResult>,
    pub invariants: Vec<InvariantSummary>,
    pub unexpected: Vec<String>,
    pub limitations: Vec<String>,
    pub software_version: String,
}

const FAILURE_METRICS: &[&str] = &[
    "unsafe_commits",
    "rejected_proposals",
    "judgment_unavailable",
];

pub const DEFAULT_DEPENDENT: &[&str] = &[
    "committed",
    "rejected_proposals",
    "withheld",
    "unsafe_commits",
    "task_success",
    "judgment_disagreement_rate",
    "resonance.stability",
    "resonance.recovery",
    "resonance.uncertainty",
];

pub fn known_metric(id: &str) -> bool {
    DEFINITIONS.iter().any(|def| def.id == id)
}

fn flat_metrics(doc: &crate::metrics::MetricsDoc) -> BTreeMap<String, Option<f64>> {
    let mut out = doc.values.clone();
    let r = &doc.resonance;
    for (name, dim) in [
        ("coherence", &r.coherence),
        ("stability", &r.stability),
        ("convergence", &r.convergence),
        ("disagreement", &r.disagreement),
        ("recovery", &r.recovery),
        ("uncertainty", &r.uncertainty),
    ] {
        out.insert(format!("resonance.{name}"), dim.value);
    }
    out
}

pub struct ExperimentOptions {
    pub out_dir: PathBuf,
    pub repetitions: Option<u32>,
    pub keep: Keep,
    pub allow_network: bool,
    pub manifest_path: PathBuf,
    pub force: bool,
    pub progress: bool,
}

pub struct ExperimentOutcome {
    pub summary: ExperimentSummary,
    pub rows: Vec<RunRow>,
    pub dir: PathBuf,
}

fn requirement_met(requirement: &str, allow_network: bool) -> Result<(), String> {
    match requirement {
        "typesafe" => {
            let key = std::env::var(typed_judgment::API_KEY_ENV)
                .map(|k| !k.trim().is_empty())
                .unwrap_or(false);
            if !key {
                Err(format!("{} is not set", typed_judgment::API_KEY_ENV))
            } else if !allow_network {
                Err("network access not permitted (pass --allow-network)".to_string())
            } else {
                Ok(())
            }
        }
        "python3" => std::process::Command::new("python3")
            .arg("--version")
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|_| ())
            .ok_or_else(|| "python3 is not available".to_string()),
        other => Err(format!("unknown requirement `{other}`")),
    }
}

#[allow(clippy::too_many_arguments)]
async fn run_one(
    manifest: &ExperimentManifest,
    scenario: &Scenario,
    condition: &config::Condition,
    rep: u32,
    recorded_judgments: Option<Vec<JudgmentEnvelope>>,
    options: &ExperimentOptions,
    dir: &Path,
    keep_judgments: bool,
) -> Result<(RunRow, Option<Vec<JudgmentEnvelope>>), String> {
    let run_config =
        config::resolve_run(manifest, scenario, condition, rep).map_err(|e| e.to_string())?;
    let mut substitutions = Substitutions::default();
    if run_config
        .scenario
        .kernel
        .judgment
        .recorded_from_condition
        .is_some()
    {
        substitutions.judgments = Some(recorded_judgments.ok_or_else(|| {
            format!(
                "{}: no recorded judgments for repetition {rep}",
                run_config.run_id
            )
        })?);
    }
    let result = runner::run(
        &run_config,
        RunOptions {
            substitutions,
            allow_network: options.allow_network,
            ..RunOptions::default()
        },
    )
    .await
    .map_err(|error| format!("{}: {error}", run_config.run_id))?;
    let keep = match options.keep {
        Keep::All => true,
        Keep::First => rep == 0,
        Keep::None => false,
    };
    let bundle = if keep {
        let run_dir = dir.join("runs").join(&run_config.run_id);
        save_run(
            &result,
            &run_dir,
            SaveOptions {
                force: true,
                command: crate::cli::command_line(),
                reproduce: vec![format!(
                    "mesh run {} --condition {} --rep {rep}",
                    options.manifest_path.display(),
                    condition.id
                )],
                parent: None,
                started_at_wall: chrono::Utc::now().to_rfc3339(),
                limitations: &manifest.limitations,
            },
        )
        .map_err(|error| error.to_string())?;
        Some(format!("runs/{}", run_config.run_id))
    } else {
        None
    };
    let row = RunRow {
        condition: condition.id.clone(),
        repetition: rep,
        seed: run_config.seed,
        run_id: run_config.run_id.clone(),
        head_hash: result.head_hash.clone(),
        final_state_hash: result.final_state.hash(),
        ticks_run: result.ticks_run,
        termination: result.termination.clone(),
        metrics: flat_metrics(&result.metrics),
        invariants_failed: result
            .metrics
            .invariants
            .iter()
            .filter(|i| !i.holds)
            .map(|i| i.name.clone())
            .collect(),
        bundle,
    };
    Ok((row, keep_judgments.then(|| result.judgments.clone())))
}

pub async fn run_experiment(
    manifest: &ExperimentManifest,
    scenario: &Scenario,
    options: &ExperimentOptions,
) -> Result<ExperimentOutcome, String> {
    let repetitions = options.repetitions.unwrap_or(manifest.repetitions).max(1);
    let dir = options.out_dir.clone();
    if dir.join("summary.json").exists() && !options.force {
        return Err(BundleError::Exists(dir).to_string());
    }
    std::fs::create_dir_all(dir.join("runs")).map_err(|error| error.to_string())?;
    let dependent: Vec<String> = if manifest.dependent_variables.is_empty() {
        DEFAULT_DEPENDENT.iter().map(|s| s.to_string()).collect()
    } else {
        manifest.dependent_variables.clone()
    };
    for metric in &dependent {
        if !known_metric(metric) {
            return Err(format!("unknown dependent variable `{metric}`"));
        }
    }

    let mut rows: Vec<RunRow> = Vec::new();
    let mut statuses: BTreeMap<String, Option<String>> = BTreeMap::new();
    let mut recorded: BTreeMap<(String, u32), Vec<JudgmentEnvelope>> = BTreeMap::new();
    let needs_recording: Vec<String> = manifest
        .conditions
        .iter()
        .filter_map(|c| {
            c.set
                .get("kernel.judgment.recorded_from_condition")
                .and_then(|v| v.as_str().map(str::to_string))
        })
        .collect();
    for condition in &manifest.conditions {
        let unmet = condition
            .requires
            .iter()
            .find_map(|requirement| requirement_met(requirement, options.allow_network).err());
        if let Some(reason) = unmet {
            statuses.insert(condition.id.clone(), Some(reason));
            continue;
        }
        statuses.insert(condition.id.clone(), None);
        let source = condition
            .set
            .get("kernel.judgment.recorded_from_condition")
            .and_then(|v| v.as_str().map(str::to_string));
        if let Some(source) = &source {
            if !recorded.keys().any(|(c, _)| c == source) {
                return Err(format!(
                    "condition `{}` replays judgments from `{source}`, which must run earlier in the manifest",
                    condition.id
                ));
            }
        }
        // Repetitions are independent and deterministic, so they run in
        // parallel; results are ordered by repetition afterwards.
        let workers = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1)
            .clamp(1, 8)
            .min(repetitions as usize);
        let next = std::sync::atomic::AtomicU32::new(0);
        let done = std::sync::atomic::AtomicU32::new(0);
        let keep_judgments = needs_recording.contains(&condition.id);
        let recorded_ref = &recorded;
        let results: Vec<Result<(RunRow, Option<Vec<JudgmentEnvelope>>), String>> =
            std::thread::scope(|scope| {
                let handles: Vec<_> = (0..workers)
                    .map(|_| {
                        let next = &next;
                        let done = &done;
                        let source = source.clone();
                        let dir = dir.clone();
                        scope.spawn(move || {
                            let runtime = tokio::runtime::Builder::new_current_thread()
                                .enable_time()
                                .build()
                                .expect("tokio runtime");
                            let mut out = Vec::new();
                            loop {
                                let rep = next.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                                if rep >= repetitions {
                                    break;
                                }
                                let outcome = runtime.block_on(run_one(
                                    manifest,
                                    scenario,
                                    condition,
                                    rep,
                                    source.as_deref().and_then(|s| {
                                        recorded_ref.get(&(s.to_string(), rep)).cloned()
                                    }),
                                    options,
                                    &dir,
                                    keep_judgments,
                                ));
                                let finished =
                                    done.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
                                if options.progress
                                    && (finished % 10 == 0 || finished == repetitions)
                                {
                                    eprintln!(
                                        "  {:<28} {:>4}/{}",
                                        condition.id, finished, repetitions
                                    );
                                }
                                out.push(outcome);
                            }
                            out
                        })
                    })
                    .collect();
                handles
                    .into_iter()
                    .flat_map(|handle| handle.join().expect("experiment worker panicked"))
                    .collect()
            });
        let mut condition_rows = Vec::new();
        for result in results {
            let (row, judgments) = result?;
            if let Some(judgments) = judgments {
                recorded.insert((condition.id.clone(), row.repetition), judgments);
            }
            condition_rows.push(row);
        }
        condition_rows.sort_by_key(|row| row.repetition);
        rows.extend(condition_rows);
    }

    let summary = summarize(
        manifest,
        scenario,
        &dependent,
        &rows,
        &statuses,
        repetitions,
    );
    write_json(&dir.join("summary.json"), &summary).map_err(|e| e.to_string())?;
    write_jsonl(&dir.join("runs.jsonl"), &rows).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("runs.csv"), runs_csv(&rows)).map_err(|e| e.to_string())?;
    write_json(
        &dir.join("experiment-provenance.json"),
        &serde_json::json!({
            "software_version": event_bus::SOFTWARE_VERSION,
            "git_commit": crate::record::git_metadata().0,
            "command": crate::cli::command_line(),
            "finished_at_wall": chrono::Utc::now().to_rfc3339(),
            "os": std::env::consts::OS,
            "arch": std::env::consts::ARCH,
        }),
    )
    .map_err(|e| e.to_string())?;
    let report = experiment_report(&summary, &rows, &dir, &options.manifest_path);
    std::fs::write(dir.join("report.md"), report).map_err(|e| e.to_string())?;
    Ok(ExperimentOutcome { summary, rows, dir })
}

fn runs_csv(rows: &[RunRow]) -> String {
    let mut keys: Vec<String> = rows
        .iter()
        .flat_map(|r| r.metrics.keys().cloned())
        .collect();
    keys.sort();
    keys.dedup();
    let mut out = String::from("condition,repetition,seed,run_id,head_hash,final_state_hash,ticks_run,termination,invariants_failed");
    for key in &keys {
        out.push(',');
        out.push_str(key);
    }
    out.push('\n');
    for row in rows {
        let _ = write!(
            out,
            "{},{},{},{},{},{},{},{},{}",
            row.condition,
            row.repetition,
            row.seed,
            row.run_id,
            row.head_hash,
            row.final_state_hash,
            row.ticks_run,
            row.termination,
            row.invariants_failed.join(";")
        );
        for key in &keys {
            out.push(',');
            if let Some(Some(value)) = row.metrics.get(key) {
                let _ = write!(out, "{value}");
            }
        }
        out.push('\n');
    }
    out
}

fn values_of(rows: &[RunRow], condition: &str, metric: &str) -> Vec<(u32, f64)> {
    rows.iter()
        .filter(|row| row.condition == condition)
        .filter_map(|row| {
            row.metrics
                .get(metric)
                .copied()
                .flatten()
                .map(|v| (row.repetition, v))
        })
        .collect()
}

pub fn summarize(
    manifest: &ExperimentManifest,
    scenario: &Scenario,
    dependent: &[String],
    rows: &[RunRow],
    statuses: &BTreeMap<String, Option<String>>,
    repetitions: u32,
) -> ExperimentSummary {
    let mut conditions = Vec::new();
    for condition in &manifest.conditions {
        let skip = statuses.get(&condition.id).cloned().flatten();
        let runs = rows.iter().filter(|r| r.condition == condition.id).count();
        let mut metrics = BTreeMap::new();
        let mut keys: Vec<String> = rows
            .iter()
            .filter(|r| r.condition == condition.id)
            .flat_map(|r| r.metrics.keys().cloned())
            .collect();
        keys.sort();
        keys.dedup();
        for key in keys {
            let values: Vec<f64> = values_of(rows, &condition.id, &key)
                .into_iter()
                .map(|v| v.1)
                .collect();
            if let Some(d) = stats::describe(&values) {
                metrics.insert(key, d);
            }
        }
        let mut failure_rates = BTreeMap::new();
        for metric in FAILURE_METRICS {
            let values = values_of(rows, &condition.id, metric);
            if values.is_empty() {
                continue;
            }
            let k = values.iter().filter(|v| v.1 > 0.0).count();
            failure_rates.insert(
                metric.to_string(),
                FailureRate {
                    runs_with_event: k,
                    runs: values.len(),
                    rate: event_bus::quantize(k as f64 / values.len() as f64, 6),
                    wilson95: stats::wilson(k, values.len()),
                },
            );
        }
        conditions.push(ConditionSummary {
            id: condition.id.clone(),
            description: condition.description.clone(),
            status: if skip.is_some() {
                "skipped".into()
            } else {
                "ran".into()
            },
            skip_reason: skip,
            overrides: condition.set.clone(),
            runs,
            metrics,
            failure_rates,
        });
    }

    let ran: Vec<&str> = conditions
        .iter()
        .filter(|c| c.status == "ran")
        .map(|c| c.id.as_str())
        .collect();
    let mut comparisons = Vec::new();
    if let Some(baseline) = ran.first() {
        for treatment in ran.iter().skip(1) {
            for metric in dependent {
                let base: BTreeMap<u32, f64> =
                    values_of(rows, baseline, metric).into_iter().collect();
                let pairs: Vec<(f64, f64)> = values_of(rows, treatment, metric)
                    .into_iter()
                    .filter_map(|(rep, t)| base.get(&rep).map(|b| (*b, t)))
                    .collect();
                if let Some(comparison) = stats::paired(metric, baseline, treatment, &pairs) {
                    comparisons.push(comparison);
                }
            }
        }
    }

    let prediction = manifest.hypothesis.prediction.as_ref().map(|p| {
        let not_evaluable = |detail: String| PredictionResult {
            metric: p.metric.clone(),
            direction: p.direction,
            baseline: p.baseline.clone(),
            treatment: p.treatment.clone(),
            status: "not_evaluable".into(),
            detail,
        };
        if !ran.contains(&p.baseline.as_str()) || !ran.contains(&p.treatment.as_str()) {
            return not_evaluable("a condition named in the prediction did not run".into());
        }
        let base: BTreeMap<u32, f64> = values_of(rows, &p.baseline, &p.metric).into_iter().collect();
        let pairs: Vec<(f64, f64)> = values_of(rows, &p.treatment, &p.metric)
            .into_iter()
            .filter_map(|(rep, t)| base.get(&rep).map(|b| (*b, t)))
            .collect();
        let Some(c) = stats::paired(&p.metric, &p.baseline, &p.treatment, &pairs) else {
            return not_evaluable("no paired values".into());
        };
        let interval = c.ci95_t.or(c.ci95_bootstrap);
        let (status, detail) = match (interval, p.direction) {
            (None, _) if c.sd_difference == Some(0.0) || c.n_pairs < 2 => {
                let observed = c.mean_difference;
                let status = match p.direction {
                    Direction::Decrease if observed < 0.0 => "supported",
                    Direction::Increase if observed > 0.0 => "supported",
                    Direction::NoChange if observed == 0.0 => "supported",
                    _ if observed == 0.0 => "inconclusive",
                    _ => "contradicted",
                };
                (status, format!("every pair differs by {observed}; no interval needed"))
            }
            (None, _) => ("not_evaluable", "no interval could be computed".to_string()),
            (Some([low, high]), Direction::Decrease) => {
                if high < 0.0 {
                    ("supported", format!("95% CI of the paired difference [{}, {}] lies below 0", fmt_value(Some(low)), fmt_value(Some(high))))
                } else if low > 0.0 {
                    ("contradicted", format!("95% CI [{}, {}] lies above 0", fmt_value(Some(low)), fmt_value(Some(high))))
                } else {
                    ("inconclusive", format!("95% CI [{}, {}] includes 0", fmt_value(Some(low)), fmt_value(Some(high))))
                }
            }
            (Some([low, high]), Direction::Increase) => {
                if low > 0.0 {
                    ("supported", format!("95% CI of the paired difference [{}, {}] lies above 0", fmt_value(Some(low)), fmt_value(Some(high))))
                } else if high < 0.0 {
                    ("contradicted", format!("95% CI [{}, {}] lies below 0", fmt_value(Some(low)), fmt_value(Some(high))))
                } else {
                    ("inconclusive", format!("95% CI [{}, {}] includes 0", fmt_value(Some(low)), fmt_value(Some(high))))
                }
            }
            (Some([low, high]), Direction::NoChange) => {
                if low <= 0.0 && high >= 0.0 {
                    (
                        "inconclusive",
                        format!("95% CI [{}, {}] includes 0: no difference detected, which is not proof of equivalence", fmt_value(Some(low)), fmt_value(Some(high))),
                    )
                } else {
                    ("contradicted", format!("95% CI [{}, {}] excludes 0", fmt_value(Some(low)), fmt_value(Some(high))))
                }
            }
        };
        PredictionResult {
            metric: p.metric.clone(),
            direction: p.direction,
            baseline: p.baseline.clone(),
            treatment: p.treatment.clone(),
            status: status.to_string(),
            detail: format!(
                "mean paired difference {} ({} pairs); {detail}",
                fmt_value(Some(c.mean_difference)),
                c.n_pairs
            ),
        }
    });

    let mut invariants = Vec::new();
    for name in crate::invariants::KNOWN {
        let violating: Vec<String> = rows
            .iter()
            .filter(|r| r.invariants_failed.iter().any(|f| f == name))
            .map(|r| r.run_id.clone())
            .collect();
        invariants.push(InvariantSummary {
            name: name.to_string(),
            expected: manifest.invariants.iter().any(|i| i == name),
            runs_checked: rows.len(),
            runs_violated: violating.len(),
            violating_runs: violating.into_iter().take(10).collect(),
        });
    }

    let mut unexpected = Vec::new();
    for invariant in &invariants {
        if invariant.runs_violated > 0 {
            unexpected.push(if invariant.expected {
                format!(
                    "Expected invariant `{}` was violated in {} of {} runs (first: {}).",
                    invariant.name,
                    invariant.runs_violated,
                    invariant.runs_checked,
                    invariant.violating_runs.first().cloned().unwrap_or_default()
                )
            } else {
                format!(
                    "Invariant `{}` failed in {} of {} runs; the manifest does not require it (first: {}).",
                    invariant.name,
                    invariant.runs_violated,
                    invariant.runs_checked,
                    invariant.violating_runs.first().cloned().unwrap_or_default()
                )
            });
        }
    }
    for condition in &manifest.conditions {
        if let Some(source) = condition
            .set
            .get("kernel.judgment.recorded_from_condition")
            .and_then(|v| v.as_str())
        {
            for rep in 0..repetitions {
                let find = |c: &str| {
                    rows.iter()
                        .find(|r| r.condition == c && r.repetition == rep)
                };
                if let (Some(a), Some(b)) = (find(source), find(&condition.id)) {
                    let differing: Vec<&String> = a
                        .metrics
                        .iter()
                        .filter(|(k, v)| {
                            b.metrics.get(*k) != Some(*v) && k.as_str() != "network_calls"
                        })
                        .map(|(k, _)| k)
                        .collect();
                    if !differing.is_empty() {
                        unexpected.push(format!(
                            "Replayed judgments in `{}` did not reproduce `{source}` at repetition {rep}: {} differ.",
                            condition.id,
                            differing.iter().take(5).map(|s| s.as_str()).collect::<Vec<_>>().join(", ")
                        ));
                        break;
                    }
                }
            }
        }
    }
    for condition in &conditions {
        if let Some(reason) = &condition.skip_reason {
            unexpected.push(format!(
                "Condition `{}` did not run: {reason}.",
                condition.id
            ));
        }
    }
    if let Some(prediction) = &prediction {
        if prediction.status == "contradicted" {
            unexpected.push(format!(
                "The prediction for `{}` was contradicted: {}",
                prediction.metric, prediction.detail
            ));
        }
    }

    ExperimentSummary {
        version: SUMMARY_VERSION.to_string(),
        experiment_id: manifest.id.clone(),
        title: manifest.title.clone(),
        question: manifest.question.clone(),
        hypothesis: manifest.hypothesis.clone(),
        manifest_hash: event_bus::hash_canonical(manifest).unwrap_or_default(),
        scenario_id: scenario.id.clone(),
        base_seed: manifest.seed,
        repetitions,
        dependent_variables: dependent.to_vec(),
        conditions,
        comparisons,
        prediction,
        invariants,
        unexpected,
        limitations: manifest.limitations.clone(),
        software_version: event_bus::SOFTWARE_VERSION.to_string(),
    }
}

fn ci(interval: &Option<[f64; 2]>) -> String {
    match interval {
        Some([low, high]) => format!("[{}, {}]", fmt_value(Some(*low)), fmt_value(Some(*high))),
        None => "—".to_string(),
    }
}

pub fn experiment_report(
    summary: &ExperimentSummary,
    rows: &[RunRow],
    dir: &Path,
    manifest_path: &Path,
) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "# {}\n", summary.title);
    let _ = writeln!(
        out,
        "Experiment `{}`. Generated by `mesh` from `summary.json` and `runs.jsonl`. Every number is measured from recorded simulated runs; no text below was written by a model.\n",
        summary.experiment_id
    );
    let _ = writeln!(out, "## Research question\n\n{}\n", summary.question.trim());
    let _ = writeln!(
        out,
        "## Hypothesis\n\n{}\n",
        summary.hypothesis.statement.trim()
    );
    if let Some(p) = &summary.hypothesis.prediction {
        let _ = writeln!(
            out,
            "Prediction: `{}` will {} from `{}` to `{}`.\n",
            p.metric,
            match p.direction {
                Direction::Decrease => "decrease",
                Direction::Increase => "increase",
                Direction::NoChange => "not change",
            },
            p.baseline,
            p.treatment
        );
    }
    let _ = writeln!(out, "## Configuration\n");
    let _ = writeln!(out, "- Scenario: `{}`", summary.scenario_id);
    let _ = writeln!(out, "- Repetitions per condition: {} (base seed {}; repetition r uses the same derived seed in every condition)", summary.repetitions, summary.base_seed);
    let _ = writeln!(out, "- Manifest hash: `{}`", summary.manifest_hash);
    let _ = writeln!(out, "- Software: `{}`", summary.software_version);
    let _ = writeln!(
        out,
        "\n## Conditions\n\n| Condition | Status | Runs | Overrides |\n| --- | --- | ---: | --- |"
    );
    for c in &summary.conditions {
        let _ = writeln!(
            out,
            "| `{}` | {} | {} | `{}` |",
            c.id,
            match &c.skip_reason {
                Some(reason) => format!("skipped: {reason}"),
                None => "ran".into(),
            },
            c.runs,
            serde_json::to_string(&c.overrides).unwrap_or_default()
        );
    }
    let _ = writeln!(out, "\n## Results\n\nMean, 95% t-interval, median, and standard deviation across repetitions.\n");
    for metric in &summary.dependent_variables {
        let unit = definition(metric).map(|d| d.unit).unwrap_or("");
        let _ = writeln!(out, "### `{metric}` ({unit})\n");
        if let Some(def) = definition(metric) {
            let _ = writeln!(out, "{}\n", def.definition);
        }
        let _ = writeln!(out, "| Condition | n | Mean | 95% CI | Median | SD | Min | Max |\n| --- | ---: | ---: | --- | ---: | ---: | ---: | ---: |");
        for c in summary.conditions.iter().filter(|c| c.status == "ran") {
            match c.metrics.get(metric) {
                Some(d) => {
                    let _ = writeln!(
                        out,
                        "| `{}` | {} | {} | {} | {} | {} | {} | {} |",
                        c.id,
                        d.n,
                        fmt_value(Some(d.mean)),
                        ci(&d.ci95),
                        fmt_value(Some(d.median)),
                        fmt_value(d.sd),
                        fmt_value(Some(d.min)),
                        fmt_value(Some(d.max))
                    );
                }
                None => {
                    let _ = writeln!(out, "| `{}` | 0 | undefined in every run | | | | | |", c.id);
                }
            }
        }
        out.push('\n');
    }
    let _ = writeln!(out, "## Paired comparisons\n\nTreatment minus baseline, paired by repetition (same seed). d_z = mean difference / SD of differences.\n");
    let _ = writeln!(out, "| Metric | Baseline → Treatment | Mean diff | 95% CI (t) | 95% CI (bootstrap) | d_z | Hedges g | + / − / = |\n| --- | --- | ---: | --- | --- | ---: | ---: | --- |");
    for c in &summary.comparisons {
        let _ = writeln!(
            out,
            "| `{}` | `{}` → `{}` | {} | {} | {} | {} | {} | {} / {} / {} |",
            c.metric,
            c.baseline,
            c.treatment,
            fmt_value(Some(c.mean_difference)),
            ci(&c.ci95_t),
            ci(&c.ci95_bootstrap),
            fmt_value(c.cohens_dz),
            fmt_value(c.hedges_g),
            c.pairs_increased,
            c.pairs_decreased,
            c.pairs_equal
        );
    }
    if let Some(p) = &summary.prediction {
        let _ = writeln!(
            out,
            "\n## Prediction\n\n**{}**: {}\n",
            p.status.to_uppercase(),
            p.detail
        );
        let _ = writeln!(out, "A supported prediction is evidence about this simulation under this configuration only.");
    }
    let _ = writeln!(out, "\n## Failures and invariants\n\n| Invariant | Required | Runs violated |\n| --- | --- | --- |");
    for i in &summary.invariants {
        let _ = writeln!(
            out,
            "| `{}` | {} | {} / {} |",
            i.name,
            if i.expected { "yes" } else { "no" },
            i.runs_violated,
            i.runs_checked
        );
    }
    let _ = writeln!(out, "\nRuns in which a count was above zero (Wilson 95% interval):\n\n| Condition | Metric | Runs | Rate | 95% CI |\n| --- | --- | --- | ---: | --- |");
    for c in summary.conditions.iter().filter(|c| c.status == "ran") {
        for (metric, rate) in &c.failure_rates {
            let _ = writeln!(
                out,
                "| `{}` | `{metric}` | {} / {} | {} | {} |",
                c.id,
                rate.runs_with_event,
                rate.runs,
                fmt_value(Some(rate.rate)),
                ci(&rate.wilson95)
            );
        }
    }
    let _ = writeln!(out, "\n## Unexpected behavior\n");
    if summary.unexpected.is_empty() {
        let _ = writeln!(out, "None detected by the automatic checks (required and other invariant violations, replay fidelity of replayed-judgment conditions, skipped conditions, contradicted predictions).");
    } else {
        for item in &summary.unexpected {
            let _ = writeln!(out, "- {item}");
        }
    }
    let _ = writeln!(out, "\n## Limitations\n");
    let _ = writeln!(out, "- All inputs are simulated. Results describe this simulator and kernel configuration, not people or physical systems.");
    let _ = writeln!(out, "- Repetitions differ only by seed; they are not independent samples of real-world conditions.");
    let _ = writeln!(
        out,
        "- Mock judges are deterministic rules defined in this repository, not language models."
    );
    for limitation in &summary.limitations {
        let _ = writeln!(out, "- {limitation}");
    }
    let _ = writeln!(
        out,
        "\n## Reproduce\n\n```bash\nmesh experiment run {}\n```\n",
        manifest_path.display()
    );
    let _ = writeln!(out, "Any single run can be regenerated with `mesh run {} --condition <id> --rep <n>`; its event-log head hash must equal the `head_hash` column of `runs.csv`.\n", manifest_path.display());
    let _ = writeln!(
        out,
        "## Artifact hashes\n\n| Artifact | SHA-256 |\n| --- | --- |"
    );
    for name in ["summary.json", "runs.jsonl", "runs.csv"] {
        if let Ok(hash) = crate::record::file_hash(&dir.join(name)) {
            let _ = writeln!(out, "| `{name}` | `{}` |", short(&hash));
        }
    }
    for row in rows.iter().filter(|r| r.bundle.is_some()) {
        let _ = writeln!(
            out,
            "| `{}` (event log head) | `{}` |",
            row.bundle.as_deref().unwrap_or_default(),
            short(&row.head_hash)
        );
    }
    out
}

/// Render a comparison table for the CLI.
pub fn render_comparisons(
    summary: &ExperimentSummary,
    baseline: Option<&str>,
    treatment: Option<&str>,
    metric: Option<&str>,
) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "{} ({})\n", summary.title, summary.experiment_id);
    for c in summary.comparisons.iter().filter(|c| {
        baseline.is_none_or(|b| b == c.baseline)
            && treatment.is_none_or(|t| t == c.treatment)
            && metric.is_none_or(|m| m == c.metric)
    }) {
        let _ = writeln!(
            out,
            "{:<30} {:>18} -> {:<24} mean {:>10} -> {:<10} diff {:>9}  CI95 {:<24} d_z {}",
            c.metric,
            c.baseline,
            c.treatment,
            fmt_value(Some(c.baseline_mean)),
            fmt_value(Some(c.treatment_mean)),
            fmt_value(Some(c.mean_difference)),
            ci(&c.ci95_t),
            fmt_value(c.cohens_dz)
        );
    }
    if let Some(p) = &summary.prediction {
        let _ = writeln!(
            out,
            "\nPrediction: {} — {}",
            p.status.to_uppercase(),
            p.detail
        );
    }
    out
}
