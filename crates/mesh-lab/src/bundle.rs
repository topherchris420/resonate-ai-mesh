//! Writing a run to an artifact bundle, and the per-run report.

use crate::metrics::{definition, MetricsDoc};
use crate::record::{
    file_hash, git_metadata, rustc_version, write_json, write_jsonl, BundleError, ParentRun,
    Provenance, ReplayInfo, BUNDLE_FORMAT, HASHED_FILES,
};
use crate::runner::RunResult;
use crate::topology;
use serde_json::json;
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

pub struct SaveOptions<'a> {
    pub force: bool,
    pub command: Vec<String>,
    /// Shell commands that reproduce this run.
    pub reproduce: Vec<String>,
    pub parent: Option<ParentRun>,
    pub started_at_wall: String,
    pub limitations: &'a [String],
}

pub fn replay_info(result: &RunResult) -> ReplayInfo {
    let mut policy_versions: Vec<String> = result
        .events
        .iter()
        .map(|c| c.event.policy_version.clone())
        .filter(|v| !v.is_empty())
        .collect();
    policy_versions.sort();
    policy_versions.dedup();
    ReplayInfo {
        run_id: result.config.run_id.clone(),
        seed: result.config.seed,
        genesis_hash: result.genesis_hash.clone(),
        head_hash: result.head_hash.clone(),
        event_count: result.events.len() as u64,
        final_state_hash: result.final_state.hash(),
        ticks_run: result.ticks_run,
        termination: result.termination.clone(),
        substituted: result.substituted.clone(),
        counts: result.counts(),
        policy_versions,
    }
}

pub fn save_run(
    result: &RunResult,
    dir: &Path,
    options: SaveOptions<'_>,
) -> Result<(), BundleError> {
    if dir.join("manifest.json").exists() && !options.force {
        return Err(BundleError::Exists(dir.to_path_buf()));
    }
    std::fs::create_dir_all(dir).map_err(|error| BundleError::Io {
        path: dir.to_path_buf(),
        message: error.to_string(),
    })?;
    let config = &result.config;
    write_json(&dir.join("manifest.json"), config)?;
    write_jsonl(&dir.join("events.jsonl"), &result.events)?;
    write_jsonl(&dir.join("decisions.jsonl"), &result.decisions)?;
    write_jsonl(&dir.join("judgments.jsonl"), &result.judgments)?;
    write_jsonl(&dir.join("intents.jsonl"), &result.intents)?;
    write_json(&dir.join("metrics.json"), &result.metrics)?;
    write_json(
        &dir.join("environment.json"),
        &json!({
            "scenario": config.scenario.id,
            "description": config.scenario.description,
            "ticks": config.scenario.ticks,
            "dt_ms": config.scenario.dt_ms,
            "arena_half_extent": config.scenario.arena_half_extent,
            "hazards": config.scenario.hazards,
            "points_of_interest": config.scenario.points_of_interest,
            "agents": config.scenario.agents,
            "faults": config.scenario.faults,
            "human_state": config.scenario.human_state,
            "human_review": config.scenario.human_review,
            "kernel": config.scenario.kernel,
            "final_state": result.final_state,
        }),
    )?;
    let topology = topology::from_config(
        config,
        Some(&result.agent_descriptors),
        Some(result.timing.pipeline_latency_ms_p50),
        false,
    );
    write_json(&dir.join("topology.json"), &topology)?;
    let replay = replay_info(result);
    write_json(&dir.join("replay.json"), &replay)?;
    write_json(&dir.join("timing.json"), &result.timing)?;

    let mut file_hashes = BTreeMap::new();
    for name in HASHED_FILES {
        let path = dir.join(name);
        if path.exists() {
            file_hashes.insert(name.to_string(), file_hash(&path)?);
        }
    }
    let (git_commit, git_dirty) = git_metadata();
    let provenance = Provenance {
        bundle_format: BUNDLE_FORMAT.to_string(),
        software_version: event_bus::SOFTWARE_VERSION.to_string(),
        git_commit,
        git_dirty,
        rustc: rustc_version(),
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        command: options.command.clone(),
        started_at_wall: options.started_at_wall.clone(),
        finished_at_wall: chrono::Utc::now().to_rfc3339(),
        run_config_hash: config.hash(),
        validator_version: epistemic_validator::VALIDATOR_VERSION.to_string(),
        validator_config_hash: result.validator_config_hash.clone(),
        kernel_policy_version: kernel_core::KERNEL_POLICY_VERSION.to_string(),
        judgment_policy_version: config.scenario.kernel.judgment_policy.version.clone(),
        question_set_version: typed_judgment::QUESTION_SET_VERSION.to_string(),
        judgment_state_schema: typed_judgment::STATE_SCHEMA_VERSION.to_string(),
        event_schema_version: event_bus::EVENT_SCHEMA_VERSION.to_string(),
        judge: result.judge.clone(),
        agents: result.agent_descriptors.clone(),
        parent: options.parent.clone(),
        file_hashes: file_hashes.clone(),
    };
    write_json(&dir.join("provenance.json"), &provenance)?;
    let report = run_report(
        result,
        &replay,
        &provenance,
        &options.reproduce,
        options.limitations,
    );
    std::fs::write(dir.join("report.md"), report).map_err(|error| BundleError::Io {
        path: dir.join("report.md"),
        message: error.to_string(),
    })?;
    Ok(())
}

pub fn fmt_value(value: Option<f64>) -> String {
    match value {
        None => "—".to_string(),
        Some(v) if v.fract() == 0.0 && v.abs() < 1e15 => format!("{}", v as i64),
        Some(v) => {
            // Precision follows magnitude: no more digits than the data supports.
            let text = if v.abs() >= 100.0 {
                format!("{v:.1}")
            } else if v.abs() >= 1.0 {
                format!("{v:.2}")
            } else {
                format!("{v:.3}")
            };
            if text.contains('.') {
                text.trim_end_matches('0').trim_end_matches('.').to_string()
            } else {
                text
            }
        }
    }
}

fn metric_rows(metrics: &MetricsDoc) -> String {
    let mut out = String::from("| Metric | Value | Unit |\n| --- | ---: | --- |\n");
    for (id, value) in &metrics.values {
        let unit = definition(id).map(|d| d.unit).unwrap_or("");
        let _ = writeln!(out, "| `{id}` | {} | {unit} |", fmt_value(*value));
    }
    out
}

fn resonance_rows(metrics: &MetricsDoc) -> String {
    let r = &metrics.resonance;
    let mut out =
        String::from("| Dimension | Value | Basis | Note |\n| --- | ---: | ---: | --- |\n");
    for (name, dim) in [
        ("coherence", &r.coherence),
        ("stability", &r.stability),
        ("convergence", &r.convergence),
        ("disagreement", &r.disagreement),
        ("recovery", &r.recovery),
        ("uncertainty", &r.uncertainty),
    ] {
        let _ = writeln!(
            out,
            "| {name} | {} | {} | {} |",
            fmt_value(dim.value),
            dim.basis,
            dim.note.as_deref().unwrap_or("")
        );
    }
    out
}

fn run_report(
    result: &RunResult,
    replay: &ReplayInfo,
    provenance: &Provenance,
    reproduce: &[String],
    limitations: &[String],
) -> String {
    let config = &result.config;
    let scenario = &config.scenario;
    let m = &result.metrics;
    let v = |key: &str| fmt_value(m.values.get(key).copied().flatten());
    let mut out = String::new();
    let _ = writeln!(out, "# Run report: `{}`\n", config.run_id);
    let _ = writeln!(
        out,
        "Generated by `mesh` from the files in this bundle. Every number below is computed from the recorded events. No model wrote any part of this report.\n"
    );
    let _ = writeln!(out, "## Configuration\n");
    let _ = writeln!(out, "| Field | Value |\n| --- | --- |");
    let _ = writeln!(out, "| Experiment | `{}` |", config.experiment_id);
    let _ = writeln!(out, "| Condition | `{}` |", config.condition_id);
    let _ = writeln!(out, "| Repetition | {} |", config.repetition);
    let _ = writeln!(out, "| Seed | {} |", config.seed);
    let _ = writeln!(
        out,
        "| Scenario | `{}`: {} |",
        scenario.id,
        scenario.description.trim()
    );
    let _ = writeln!(out, "| Agents | {} |", scenario.agents.len());
    let _ = writeln!(
        out,
        "| Judgment | {} |",
        match &result.judge {
            Some(judge) => format!(
                "{} / `{}` ({:?}, networked: {})",
                judge.name, judge.model, judge.kind, judge.networked
            ),
            None => "off (deterministic validation only)".to_string(),
        }
    );
    let _ = writeln!(
        out,
        "| Human state | {} |",
        if scenario.human_state.enabled {
            "SIMULATED operator-load index"
        } else {
            "off"
        }
    );
    let _ = writeln!(
        out,
        "| Validator | `{}` config `{}` |",
        provenance.validator_version,
        short(&provenance.validator_config_hash)
    );
    let _ = writeln!(
        out,
        "| Kernel policy | `{}` |",
        provenance.kernel_policy_version
    );
    let _ = writeln!(
        out,
        "| Ticks run | {} of {} ({}) |",
        result.ticks_run, scenario.ticks, result.termination
    );
    if !config.overrides.is_empty() {
        let _ = writeln!(
            out,
            "| Overrides | `{}` |",
            serde_json::to_string(&config.overrides).unwrap_or_default()
        );
    }
    if let Some(parent) = &provenance.parent {
        let _ = writeln!(
            out,
            "| Counterfactual of | `{}` (`{}`) |",
            parent.run_id, parent.label
        );
    }

    let _ = writeln!(out, "\n## What happened\n");
    let _ = writeln!(
        out,
        "- {} events recorded; {} proposals submitted.",
        replay.event_count,
        v("proposals")
    );
    let _ = writeln!(
        out,
        "- Deterministic validation accepted {} and rejected {}. {} commits changed authoritative state; {} validated proposals were withheld.",
        v("accepted_proposals"),
        v("rejected_proposals"),
        v("committed"),
        v("withheld")
    );
    if !m.rejection_reasons.is_empty() {
        let reasons: Vec<String> = m
            .rejection_reasons
            .iter()
            .map(|(k, n)| format!("`{k}` ×{n}"))
            .collect();
        let _ = writeln!(out, "- Rejection reasons: {}.", reasons.join(", "));
    }
    if !m.withhold_reasons.is_empty() {
        let reasons: Vec<String> = m
            .withhold_reasons
            .iter()
            .map(|(k, n)| format!("`{k}` ×{n}"))
            .collect();
        let _ = writeln!(out, "- Withhold reasons: {}.", reasons.join(", "));
    }
    let _ = writeln!(
        out,
        "- The independent oracle classified {} proposals as unsafe; {} were blocked and {} were committed.",
        v("unsafe_proposals"),
        v("unsafe_proposals_blocked"),
        v("unsafe_commits")
    );
    if result.judge.is_some() {
        let _ = writeln!(
            out,
            "- Judgment was consulted {} times and disagreed with deterministically valid proposals {} times. Network calls: {}.",
            v("judgment_calls"),
            v("judgment_disagreements"),
            v("network_calls")
        );
    }
    if let Some(first) = result.decisions.iter().find(|d| !d.validation.accepted) {
        let _ = writeln!(
            out,
            "- First deterministic rejection: tick {}, `{}` (`{}`): {}.",
            first.tick,
            first.proposal_id,
            first.agent_id,
            first.validation.reasons.join(", ")
        );
    }
    for chained in result
        .events
        .iter()
        .filter(|c| c.event.event_type == "fault_injected")
    {
        let _ = writeln!(
            out,
            "- Fault injected at tick {}: `{}`.",
            chained.event.tick.unwrap_or_default(),
            chained.event.payload["label"].as_str().unwrap_or_default()
        );
    }
    let _ = writeln!(
        out,
        "- Goals reached: {} of {}.",
        v("goals_reached"),
        v("goals_total")
    );

    let _ = writeln!(out, "\n## Resonance vector\n");
    let _ = writeln!(out, "Six separate operational measurements; see `docs/resonance-metrics.md`. They describe this simulated run only.\n");
    out.push_str(&resonance_rows(m));
    let _ = writeln!(out, "\n## Metrics\n");
    out.push_str(&metric_rows(m));

    let _ = writeln!(out, "\n## Invariants\n");
    let _ = writeln!(
        out,
        "| Invariant | Holds | Checked | First violation |\n| --- | --- | ---: | --- |"
    );
    for invariant in &m.invariants {
        let _ = writeln!(
            out,
            "| `{}` | {} | {} | {} |",
            invariant.name,
            if invariant.holds { "yes" } else { "**no**" },
            invariant.checked,
            invariant
                .violations
                .first()
                .map(String::as_str)
                .unwrap_or("")
        );
    }

    let _ = writeln!(out, "\n## Reproduce\n\n```bash");
    for line in reproduce {
        let _ = writeln!(out, "{line}");
    }
    let _ = writeln!(out, "mesh replay {}\n```", config.run_id);
    let _ = writeln!(out, "\n## Provenance\n");
    let _ = writeln!(
        out,
        "- Software: `{}` (git `{}`{})",
        provenance.software_version,
        short(&provenance.git_commit),
        if provenance.git_dirty == Some(true) {
            ", uncommitted changes"
        } else {
            ""
        }
    );
    let _ = writeln!(
        out,
        "- Run config hash (chain genesis): `{}`",
        provenance.run_config_hash
    );
    let _ = writeln!(out, "- Event log head: `{}`", replay.head_hash);
    let _ = writeln!(out, "- Final state hash: `{}`", replay.final_state_hash);
    let _ = writeln!(out, "\n| File | SHA-256 |\n| --- | --- |");
    for (name, hash) in &provenance.file_hashes {
        let _ = writeln!(out, "| `{name}` | `{hash}` |");
    }
    let _ = writeln!(out, "\n## Limitations\n");
    let _ = writeln!(out, "- Every input is simulated: agents, sensors, environment, and the operator-load index. Nothing here is a measurement of a person or a physical system.");
    let _ = writeln!(out, "- The operator-load index is a modeling device (`operator_load_v1`), not a physiological or cognitive measurement.");
    if result.judge.as_ref().is_some_and(|j| !j.networked) {
        let _ = writeln!(out, "- The judge in this run is a deterministic stand-in defined in this repository, not a language model.");
    }
    for limitation in limitations {
        let _ = writeln!(out, "- {limitation}");
    }
    out
}

pub fn short(hash: &str) -> String {
    let trimmed = hash.trim_start_matches("sha256:");
    trimmed.chars().take(12).collect()
}
