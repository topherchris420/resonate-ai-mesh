//! Research claims and the evidence registry.
//!
//! A claim states a finding, a human-declared status, the experiment evidence
//! it rests on, and its limitations. `mesh claims check` evaluates every piece
//! of evidence against the latest experiment summaries and reports whether the
//! evidence is consistent with the claim. It never edits a claim: statuses
//! change only when a person changes them. A claim declared `supported` whose
//! evidence is missing or inconsistent is an error.

use crate::experiment::ExperimentSummary;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ClaimStatus {
    Hypothesis,
    Provisional,
    Supported,
    Contradicted,
    Retracted,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Supports,
    Contradicts,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Check {
    /// Paired 95% CI of treatment - baseline lies entirely on one side of 0.
    Comparison {
        metric: String,
        baseline: String,
        treatment: String,
        direction: crate::config::Direction,
    },
    /// The largest per-run value of a metric in a condition is at most `max`.
    ConditionMax {
        condition: String,
        metric: String,
        max: f64,
    },
    /// The smallest per-run value of a metric in a condition is at least `min`.
    ConditionMin {
        condition: String,
        metric: String,
        min: f64,
    },
    /// An invariant held in every run of the experiment.
    Invariant { name: String },
    /// The experiment's pre-registered prediction was supported.
    Prediction,
    /// A replayed-judgment condition reproduced its source condition in every repetition.
    ReplayFidelity { condition: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Evidence {
    pub experiment: String,
    pub role: Role,
    pub check: Check,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Claim {
    pub id: String,
    pub claim: String,
    pub status: ClaimStatus,
    pub evidence: Vec<Evidence>,
    #[serde(default)]
    pub limitations: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EvidenceResult {
    pub experiment: String,
    pub role: Role,
    pub description: String,
    /// holds | fails | missing
    pub outcome: String,
    pub detail: String,
    pub summary_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ClaimResult {
    pub id: String,
    pub claim: String,
    pub declared_status: ClaimStatus,
    /// consistent | inconsistent | incomplete
    pub evidence_status: String,
    pub evidence: Vec<EvidenceResult>,
    pub limitations: Vec<String>,
    /// error | warning | note | ok
    pub level: String,
    pub message: String,
    pub file: String,
}

fn describe(check: &Check) -> String {
    match check {
        Check::Comparison {
            metric,
            baseline,
            treatment,
            direction,
        } => {
            format!("{metric}: {treatment} vs {baseline} ({direction:?})")
        }
        Check::ConditionMax {
            condition,
            metric,
            max,
        } => format!("max {metric} in {condition} <= {max}"),
        Check::ConditionMin {
            condition,
            metric,
            min,
        } => format!("min {metric} in {condition} >= {min}"),
        Check::Invariant { name } => format!("invariant {name} holds in every run"),
        Check::Prediction => "pre-registered prediction supported".to_string(),
        Check::ReplayFidelity { condition } => {
            format!("{condition} reproduces its recorded source exactly")
        }
    }
}

fn evaluate(check: &Check, summary: &ExperimentSummary) -> (String, String) {
    let missing = |what: String| ("missing".to_string(), what);
    match check {
        Check::Comparison {
            metric,
            baseline,
            treatment,
            direction,
        } => {
            let Some(c) = summary.comparisons.iter().find(|c| {
                &c.metric == metric && &c.baseline == baseline && &c.treatment == treatment
            }) else {
                return missing(format!(
                    "no comparison {baseline} -> {treatment} for {metric}"
                ));
            };
            let interval = c.ci95_t.or(c.ci95_bootstrap);
            let holds = match (interval, direction) {
                (Some([_, high]), crate::config::Direction::Decrease) => high < 0.0,
                (Some([low, _]), crate::config::Direction::Increase) => low > 0.0,
                (Some([low, high]), crate::config::Direction::NoChange) => {
                    low <= 0.0 && high >= 0.0
                }
                (None, crate::config::Direction::Decrease) => {
                    c.mean_difference < 0.0 && c.sd_difference == Some(0.0)
                }
                (None, crate::config::Direction::Increase) => {
                    c.mean_difference > 0.0 && c.sd_difference == Some(0.0)
                }
                (None, crate::config::Direction::NoChange) => c.mean_difference == 0.0,
            };
            (
                if holds { "holds" } else { "fails" }.to_string(),
                format!(
                    "mean paired difference {} over {} pairs, 95% CI {}",
                    crate::bundle::fmt_value(Some(c.mean_difference)),
                    c.n_pairs,
                    match interval {
                        Some([low, high]) => format!(
                            "[{}, {}]",
                            crate::bundle::fmt_value(Some(low)),
                            crate::bundle::fmt_value(Some(high))
                        ),
                        None => "not computed (no variation between pairs)".to_string(),
                    }
                ),
            )
        }
        Check::ConditionMax {
            condition,
            metric,
            max,
        }
        | Check::ConditionMin {
            condition,
            metric,
            min: max,
        } => {
            let Some(found) = summary.conditions.iter().find(|c| &c.id == condition) else {
                return missing(format!("condition {condition} not in summary"));
            };
            let Some(d) = found.metrics.get(metric) else {
                return missing(format!("metric {metric} not recorded for {condition}"));
            };
            let holds = match check {
                Check::ConditionMax { .. } => d.max <= *max,
                _ => d.min >= *max,
            };
            (
                if holds { "holds" } else { "fails" }.to_string(),
                format!("observed min {} max {} over {} runs", d.min, d.max, d.n),
            )
        }
        Check::Invariant { name } => match summary.invariants.iter().find(|i| &i.name == name) {
            None => missing(format!("invariant {name} not recorded")),
            Some(i) => (
                if i.runs_violated == 0 {
                    "holds"
                } else {
                    "fails"
                }
                .to_string(),
                format!("violated in {} of {} runs", i.runs_violated, i.runs_checked),
            ),
        },
        Check::ReplayFidelity { condition } => {
            if !summary
                .conditions
                .iter()
                .any(|c| &c.id == condition && c.status == "ran")
            {
                return missing(format!("condition {condition} did not run"));
            }
            let marker = format!("Replayed judgments in `{condition}`");
            match summary
                .unexpected
                .iter()
                .find(|item| item.starts_with(&marker))
            {
                Some(item) => ("fails".to_string(), item.clone()),
                None => (
                    "holds".to_string(),
                    format!("no replay-fidelity difference recorded for {condition}"),
                ),
            }
        }
        Check::Prediction => match &summary.prediction {
            None => missing("the experiment has no prediction".to_string()),
            Some(p) => (
                if p.status == "supported" {
                    "holds"
                } else {
                    "fails"
                }
                .to_string(),
                format!("{}: {}", p.status, p.detail),
            ),
        },
    }
}

pub fn load_claims(dir: &Path) -> Result<Vec<(PathBuf, Claim)>, String> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|error| format!("{}: {error}", dir.display()))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|x| x.to_str()) == Some("yaml"))
        .collect();
    files.sort();
    files
        .into_iter()
        .map(|path| {
            let claim: Claim =
                crate::config::read_yaml(&path).map_err(|error| error.to_string())?;
            Ok((path, claim))
        })
        .collect()
}

pub fn check_claims(claims_dir: &Path, artifacts: &Path) -> Result<Vec<ClaimResult>, String> {
    let mut results = Vec::new();
    for (path, claim) in load_claims(claims_dir)? {
        let mut evidence = Vec::new();
        for item in &claim.evidence {
            let summary_path = artifacts
                .join("experiments")
                .join(&item.experiment)
                .join("summary.json");
            let (outcome, detail, hash) =
                match crate::record::read_json::<ExperimentSummary>(&summary_path) {
                    Err(_) => (
                        "missing".to_string(),
                        format!(
                        "no summary at {}; run `mesh experiment run experiments/{}/manifest.yaml`",
                        summary_path.display(),
                        item.experiment
                    ),
                        None,
                    ),
                    Ok(summary) => {
                        let (outcome, detail) = evaluate(&item.check, &summary);
                        (
                            outcome,
                            detail,
                            crate::record::file_hash(&summary_path).ok(),
                        )
                    }
                };
            evidence.push(EvidenceResult {
                experiment: item.experiment.clone(),
                role: item.role,
                description: describe(&item.check),
                outcome,
                detail,
                summary_hash: hash,
            });
        }
        let against = evidence.iter().any(|e| {
            (e.role == Role::Supports && e.outcome == "fails")
                || (e.role == Role::Contradicts && e.outcome == "holds")
        });
        let missing = evidence.iter().any(|e| e.outcome == "missing");
        let evidence_status = if against {
            "inconsistent"
        } else if missing || evidence.is_empty() {
            "incomplete"
        } else {
            "consistent"
        };
        let (level, message) = match (claim.status, evidence_status) {
            (ClaimStatus::Supported, "consistent") => ("ok", "declared supported; evidence is consistent".to_string()),
            (ClaimStatus::Supported, status) => (
                "error",
                format!("declared supported but the evidence is {status}; the claim overstates its evidence"),
            ),
            (ClaimStatus::Contradicted, "consistent") => (
                "warning",
                "declared contradicted but current evidence is consistent with the claim".to_string(),
            ),
            (ClaimStatus::Hypothesis | ClaimStatus::Provisional, "consistent") => (
                "note",
                "evidence is consistent; the status stays as declared until a person changes it".to_string(),
            ),
            (ClaimStatus::Hypothesis | ClaimStatus::Provisional, "inconsistent") => (
                "warning",
                "evidence is inconsistent with the claim".to_string(),
            ),
            (_, status) => ("ok", format!("evidence is {status}")),
        };
        results.push(ClaimResult {
            id: claim.id.clone(),
            claim: claim.claim.trim().to_string(),
            declared_status: claim.status,
            evidence_status: evidence_status.to_string(),
            evidence,
            limitations: claim.limitations.clone(),
            level: level.to_string(),
            message,
            file: path.display().to_string(),
        });
    }
    Ok(results)
}

pub fn render(results: &[ClaimResult]) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    for result in results {
        let _ = writeln!(
            out,
            "[{}] {}  (declared {:?}, evidence {})",
            result.level.to_uppercase(),
            result.id,
            result.declared_status,
            result.evidence_status
        );
        let _ = writeln!(out, "  {}", result.claim);
        for e in &result.evidence {
            let _ = writeln!(
                out,
                "    {:<8} {:?} {} / {} — {}",
                e.outcome, e.role, e.experiment, e.description, e.detail
            );
        }
        let _ = writeln!(out, "  {}\n", result.message);
    }
    out
}
