//! Records produced by a run and the artifact bundle on disk.
//!
//! ```text
//! <run-dir>/
//!   manifest.json     resolved RunConfig (the genesis of the event chain)
//!   events.jsonl      hash-chained event log
//!   decisions.jsonl   one causal summary per proposal or human resolution
//!   judgments.jsonl   every judgment envelope, for replay substitution
//!   intents.jsonl     intents from non-deterministic agents, for replay substitution
//!   metrics.json      metric values, resonance vector, invariants
//!   environment.json  world definition and kernel configuration
//!   topology.json     the mesh graph this run used
//!   provenance.json   software, versions, config hashes, file hashes, wall time
//!   replay.json       what replay needs and what it must reproduce
//!   timing.json       wall-clock measurements (not deterministic, never compared)
//!   report.md         human-readable report generated from the files above
//! ```

use crate::agents::AgentIntent;
use crate::config::RunConfig;
use event_bus::{sha256_hex, ChainedEvent};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use thiserror::Error;
use typed_judgment::JudgmentEnvelope;

pub const BUNDLE_FORMAT: &str = "resonate-ai-mesh.run-bundle.v1";

/// Files whose SHA-256 is recorded in provenance and checked by `mesh verify`.
pub const HASHED_FILES: &[&str] = &[
    "manifest.json",
    "events.jsonl",
    "decisions.jsonl",
    "judgments.jsonl",
    "intents.jsonl",
    "metrics.json",
    "environment.json",
    "topology.json",
    "replay.json",
];

/// Largest file `mesh` will read from a bundle.
pub const MAX_BUNDLE_FILE_BYTES: u64 = 256 * 1024 * 1024;

#[derive(Debug, Error)]
pub enum BundleError {
    #[error("{path}: {message}")]
    Io { path: PathBuf, message: String },
    #[error("{path} line {line}: {message}")]
    Parse {
        path: PathBuf,
        line: usize,
        message: String,
    },
    #[error("{0} already exists; pass --force to overwrite or choose another --out")]
    Exists(PathBuf),
    #[error("{0}")]
    Invalid(String),
}

fn io_error(path: &Path, error: impl std::fmt::Display) -> BundleError {
    BundleError::Io {
        path: path.to_path_buf(),
        message: error.to_string(),
    }
}

/// Finite coordinates as numbers, non-finite as null (JSON has no NaN).
pub type Coords = [Option<f64>; 3];

pub fn coords(v: &epistemic_validator::Vector3) -> Coords {
    let f = |x: f64| x.is_finite().then_some(x);
    [f(v.x), f(v.y), f(v.z)]
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ValidationSummary {
    pub event_id: String,
    pub accepted: bool,
    pub reasons: Vec<String>,
    pub failed_checks: Vec<String>,
    pub confidence: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JudgmentDigest {
    pub event_id: String,
    pub judgment_id: String,
    pub provider: String,
    pub model: String,
    pub disposition: String,
    pub reason_codes: Vec<String>,
    pub provider_status: String,
    pub latency_ms: u64,
    pub model_involved: bool,
    /// Lowest Choice/Score confidence returned, if any.
    pub min_confidence: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PolicySummary {
    pub event_id: String,
    pub outcome: String,
    pub reason_codes: Vec<String>,
    pub basis: Option<String>,
    pub adaptive_level: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TransitionSummary {
    pub event_id: String,
    pub revision: u64,
    pub before_hash: String,
    pub after_hash: String,
}

/// The causal chain of one decision, flattened for analysis.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DecisionRecord {
    pub tick: u64,
    /// `proposal` or `human_resolution`.
    pub kind: String,
    pub proposal_id: String,
    pub agent_id: String,
    pub correlation_id: String,
    pub action_type: String,
    pub target: Coords,
    pub from: Coords,
    pub priority: i32,
    pub observation_id: Option<String>,
    pub observed_at: Option<i64>,
    pub observation_age_ms: i64,
    pub observation_quality: String,
    pub trigger_event_id: String,
    pub validation: ValidationSummary,
    pub judgment: Option<JudgmentDigest>,
    pub judgment_skip_reason: Option<String>,
    pub policy: PolicySummary,
    pub transition: Option<TransitionSummary>,
    pub committed: bool,
    /// Ground truth from the independent oracle (`None` = safe).
    pub oracle_unsafe: Option<String>,
    pub rationale: Option<String>,
    pub duplicate_submission: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RecordedIntent {
    pub tick: u64,
    pub agent_id: String,
    pub intent: Option<AgentIntent>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Provenance {
    pub bundle_format: String,
    pub software_version: String,
    pub git_commit: String,
    pub git_dirty: Option<bool>,
    pub rustc: Option<String>,
    pub os: String,
    pub arch: String,
    pub command: Vec<String>,
    pub started_at_wall: String,
    pub finished_at_wall: String,
    pub run_config_hash: String,
    pub validator_version: String,
    pub validator_config_hash: String,
    pub kernel_policy_version: String,
    pub judgment_policy_version: String,
    pub question_set_version: String,
    pub judgment_state_schema: String,
    pub event_schema_version: String,
    pub judge: Option<typed_judgment::ProviderDescriptor>,
    pub agents: Vec<crate::agents::AgentDescriptor>,
    /// Set for counterfactual branches.
    pub parent: Option<ParentRun>,
    pub file_hashes: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ParentRun {
    pub run_id: String,
    pub head_hash: String,
    pub overrides: BTreeMap<String, serde_json::Value>,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReplayInfo {
    pub run_id: String,
    pub seed: u64,
    pub genesis_hash: String,
    pub head_hash: String,
    pub event_count: u64,
    pub final_state_hash: String,
    pub ticks_run: u64,
    pub termination: String,
    /// Components replay substitutes from recordings instead of re-executing.
    pub substituted: Vec<String>,
    pub counts: BTreeMap<String, u64>,
    pub policy_versions: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Timing {
    pub wall_ms_total: f64,
    pub ticks_per_second: f64,
    pub pipeline_latency_ms_p50: f64,
    pub pipeline_latency_ms_p95: f64,
    pub pipeline_latency_ms_max: f64,
    pub note: String,
}

pub fn write_json(path: &Path, value: &impl Serialize) -> Result<(), BundleError> {
    let text = serde_json::to_string_pretty(value).map_err(|error| io_error(path, error))?;
    std::fs::write(path, text + "\n").map_err(|error| io_error(path, error))
}

pub fn write_jsonl<T: Serialize>(path: &Path, items: &[T]) -> Result<(), BundleError> {
    let file = std::fs::File::create(path).map_err(|error| io_error(path, error))?;
    let mut writer = std::io::BufWriter::new(file);
    for item in items {
        serde_json::to_writer(&mut writer, item).map_err(|error| io_error(path, error))?;
        writer
            .write_all(b"\n")
            .map_err(|error| io_error(path, error))?;
    }
    writer.flush().map_err(|error| io_error(path, error))
}

pub fn read_text(path: &Path) -> Result<String, BundleError> {
    let metadata = std::fs::metadata(path).map_err(|error| io_error(path, error))?;
    if metadata.len() > MAX_BUNDLE_FILE_BYTES {
        return Err(BundleError::Invalid(format!(
            "{} is {} bytes; the limit is {MAX_BUNDLE_FILE_BYTES}",
            path.display(),
            metadata.len()
        )));
    }
    std::fs::read_to_string(path).map_err(|error| io_error(path, error))
}

pub fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, BundleError> {
    let text = read_text(path)?;
    serde_json::from_str(&text).map_err(|error| BundleError::Parse {
        path: path.to_path_buf(),
        line: error.line(),
        message: error.to_string(),
    })
}

pub fn read_jsonl<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<Vec<T>, BundleError> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let text = read_text(path)?;
    text.lines()
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty())
        .map(|(index, line)| {
            serde_json::from_str(line).map_err(|error| BundleError::Parse {
                path: path.to_path_buf(),
                line: index + 1,
                message: error.to_string(),
            })
        })
        .collect()
}

pub fn file_hash(path: &Path) -> Result<String, BundleError> {
    let bytes = std::fs::read(path).map_err(|error| io_error(path, error))?;
    Ok(sha256_hex(&bytes))
}

/// A run bundle loaded from disk.
pub struct Bundle {
    pub dir: PathBuf,
    pub config: RunConfig,
    pub events: Vec<ChainedEvent>,
    pub judgments: Vec<JudgmentEnvelope>,
    pub intents: Vec<RecordedIntent>,
    pub replay: ReplayInfo,
    pub provenance: Provenance,
    pub metrics: serde_json::Value,
}

impl Bundle {
    pub fn load(dir: &Path) -> Result<Self, BundleError> {
        if !dir.join("manifest.json").is_file() {
            return Err(BundleError::Invalid(format!(
                "{} is not a run bundle (manifest.json missing)",
                dir.display()
            )));
        }
        Ok(Self {
            dir: dir.to_path_buf(),
            config: read_json(&dir.join("manifest.json"))?,
            events: read_jsonl(&dir.join("events.jsonl"))?,
            judgments: read_jsonl(&dir.join("judgments.jsonl"))?,
            intents: read_jsonl(&dir.join("intents.jsonl"))?,
            replay: read_json(&dir.join("replay.json"))?,
            provenance: read_json(&dir.join("provenance.json"))?,
            metrics: read_json(&dir.join("metrics.json"))?,
        })
    }
}

/// Locate a run by directory path or by run id under the artifacts root.
pub fn resolve_run_dir(reference: &str, artifacts: &Path) -> Result<PathBuf, BundleError> {
    let direct = PathBuf::from(reference);
    if direct.join("manifest.json").is_file() {
        return Ok(direct);
    }
    if !crate::config::valid_run_id(reference) {
        return Err(BundleError::Invalid(format!(
            "`{reference}` is neither a run bundle directory nor a valid run id"
        )));
    }
    let mut candidates = vec![artifacts.join("runs").join(reference)];
    if let Ok(entries) = std::fs::read_dir(artifacts.join("experiments")) {
        let mut experiment_dirs: Vec<PathBuf> =
            entries.filter_map(|e| e.ok()).map(|e| e.path()).collect();
        experiment_dirs.sort();
        for experiment in experiment_dirs {
            candidates.push(experiment.join("runs").join(reference));
        }
    }
    candidates
        .into_iter()
        .find(|candidate| candidate.join("manifest.json").is_file())
        .ok_or_else(|| {
            BundleError::Invalid(format!(
                "no run `{reference}` under {}",
                artifacts.display()
            ))
        })
}

/// Best-effort git metadata for provenance. Never part of hashed content.
pub fn git_metadata() -> (String, Option<bool>) {
    let run = |args: &[&str]| {
        std::process::Command::new("git")
            .args(args)
            .output()
            .ok()
            .filter(|output| output.status.success())
            .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
    };
    let commit = run(&["rev-parse", "HEAD"]).unwrap_or_else(|| "unknown".to_string());
    let dirty =
        run(&["status", "--porcelain", "--untracked-files=no"]).map(|text| !text.is_empty());
    (commit, dirty)
}

pub fn rustc_version() -> Option<String> {
    std::process::Command::new("rustc")
        .arg("--version")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
}
