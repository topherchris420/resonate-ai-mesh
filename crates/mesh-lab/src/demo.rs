//! `mesh demo`: the Perturbed Mesh, end to end, narrated from recorded data.
//!
//! Every sentence the demo prints about the run is filled in from the events
//! and decisions it just recorded. If the scenario changes and an episode does
//! not happen, the demo says so instead of describing it.

use crate::bundle::{save_run, SaveOptions};
use crate::record::{Bundle, DecisionRecord};
use std::fmt::Write as _;
use std::path::Path;

fn section(out: &mut String, n: usize, title: &str) {
    let _ = writeln!(out, "\n{n}. {title}\n{}", "-".repeat(title.len() + 3));
}

fn find(
    decisions: &[DecisionRecord],
    predicate: impl Fn(&DecisionRecord) -> bool,
) -> Option<&DecisionRecord> {
    decisions.iter().find(|d| predicate(d))
}

pub async fn run(root: &Path, artifacts: &Path) -> Result<String, String> {
    let mut out = String::new();
    let report = crate::capabilities::discover(
        &crate::capabilities::Probe {
            root: root.to_path_buf(),
            artifacts: artifacts.to_path_buf(),
        },
        &crate::capabilities::LiveFacts::default(),
    );
    let _ = writeln!(out, "RESONATE AI MESH — The Perturbed Mesh\n");
    for line in &report.orientation {
        let _ = writeln!(out, "  {line}");
    }

    let scenario_path = root.join("scenarios/perturbed-mesh.yaml");
    let target = crate::cli::load_target(&scenario_path)?;
    let config = crate::cli::resolve_target(&target, None, 0, Some(42), &Default::default())?;
    let result = crate::runner::run(&config, crate::runner::RunOptions::default())
        .await
        .map_err(|e| e.to_string())?;
    let dir = artifacts.join("runs").join(&config.run_id);
    save_run(
        &result,
        &dir,
        SaveOptions {
            force: true,
            command: crate::cli::command_line(),
            reproduce: vec!["mesh run scenarios/perturbed-mesh.yaml --seed 42".into()],
            parent: None,
            started_at_wall: chrono::Utc::now().to_rfc3339(),
            limitations: &[],
        },
    )
    .map_err(|e| e.to_string())?;
    let d = &result.decisions;
    let v = |key: &str| crate::bundle::fmt_value(result.metrics.values.get(key).copied().flatten());

    section(&mut out, 1, "Recorded a run");
    let _ = writeln!(
        out,
        "  {} agents, {} ticks, seed {}. {} events in a hash chain ending {}.",
        config.scenario.agents.len(),
        result.ticks_run,
        config.seed,
        result.events.len(),
        crate::bundle::short(&result.head_hash)
    );
    let _ = writeln!(out, "  Bundle: {}", dir.display());

    section(&mut out, 2, "Simulated human state");
    let samples: Vec<f64> = result
        .events
        .iter()
        .filter(|c| c.event.event_type == "human_state")
        .filter_map(|c| c.event.payload["datum"]["value"].as_f64())
        .collect();
    if let (Some(first), Some(max)) = (samples.first(), samples.iter().copied().reduce(f64::max)) {
        let _ = writeln!(out, "  The operator-load index started at {first} and peaked at {max} (label: SIMULATED, source sim.operator_load.v1).");
        let _ = writeln!(
            out,
            "  {} ticks were at HIGH or CRITICAL; background work was deferred {} times.",
            v("ticks_at_high_load"),
            result
                .metrics
                .withhold_reasons
                .get("OPERATOR_LOAD_DEFERRAL")
                .copied()
                .unwrap_or(0)
        );
    }

    section(
        &mut out,
        3,
        "A proposal that failed deterministic validation",
    );
    match find(d, |r| !r.validation.accepted && r.kind == "proposal") {
        Some(r) => {
            let _ = writeln!(
                out,
                "  t{} {} asked for {} and was rejected: {}.",
                r.tick,
                r.agent_id,
                r.action_type,
                r.validation.reasons.join(", ")
            );
            let _ = writeln!(
                out,
                "  Judgment was {}consulted. State did not change.",
                if r.judgment.is_some() { "" } else { "not " }
            );
        }
        None => {
            let _ = writeln!(out, "  No proposal was rejected in this run.");
        }
    }
    let _ = writeln!(
        out,
        "  The independent oracle counted {} unsafe proposals; {} were committed.",
        v("unsafe_proposals"),
        v("unsafe_commits")
    );

    section(
        &mut out,
        4,
        "A proposal that passed, was judged, and was committed",
    );
    if let Some(r) = find(d, |r| {
        r.committed && r.judgment.is_some() && r.kind == "proposal"
    }) {
        let j = r.judgment.as_ref().expect("checked");
        let _ = writeln!(
            out,
            "  t{} {}: validation passed, judge {} ({}) returned {}, policy {} on basis {}.",
            r.tick,
            r.agent_id,
            j.provider,
            j.model,
            j.disposition,
            r.policy.outcome,
            r.policy.basis.as_deref().unwrap_or("-")
        );
        let _ = writeln!(
            out,
            "  The judge is a deterministic stand-in, not a model; remote judgment was OFF."
        );
    }

    section(&mut out, 5, "Bounded judgment versus degraded evidence");
    match find(d, |r| {
        r.judgment.as_ref().is_some_and(|j| j.disposition != "PASS") && r.observation_age_ms > 0
    }) {
        Some(r) => {
            let j = r.judgment.as_ref().expect("checked");
            let _ = writeln!(
                out,
                "  t{} {} proposed from an observation {} ms old. Validation passed (limit {} ms).",
                r.tick,
                r.agent_id,
                r.observation_age_ms,
                config.scenario.kernel.validator.max_stale_ms
            );
            let _ = writeln!(
                out,
                "  The judge returned {} [{}]; the policy withheld the commit ({}).",
                j.disposition,
                j.reason_codes.join(", "),
                r.policy.outcome
            );
            if let Some(later) = find(d, |x| {
                x.kind == "human_resolution" && x.proposal_id == r.proposal_id
            }) {
                let _ = writeln!(out, "  At t{} the simulated operator decided; deterministic re-validation then said: {} -> {}.",
                    later.tick, later.validation.reasons.join(", "), later.policy.outcome);
            }
        }
        None => {
            let _ = writeln!(
                out,
                "  No judgment disagreed with a valid proposal in this run."
            );
        }
    }

    section(&mut out, 6, "Human authority");
    for r in d.iter().filter(|r| r.kind == "human_resolution").take(3) {
        let _ = writeln!(
            out,
            "  t{} review of {} -> {} [{}]",
            r.tick,
            r.proposal_id,
            r.policy.outcome,
            r.policy.reason_codes.join(", ")
        );
    }

    section(&mut out, 7, "Injected faults");
    for chained in result
        .events
        .iter()
        .filter(|c| c.event.event_type == "fault_injected")
    {
        let _ = writeln!(
            out,
            "  t{} {}",
            chained.event.tick.unwrap_or(0),
            chained.event.payload["label"].as_str().unwrap_or("")
        );
    }
    if let Some(r) = find(d, |r| {
        r.validation.reasons.iter().any(|x| x == "STEP_TOO_LARGE") && r.agent_id == "tuner_03"
    }) {
        let _ = writeln!(out, "  After the corrupted reading, t{} {} proposed a move judged against its true position: {}.",
            r.tick, r.agent_id, r.validation.reasons.join(", "));
    }
    let _ = writeln!(
        out,
        "  Resonance recovery dimension: {}.",
        crate::bundle::fmt_value(result.metrics.resonance.recovery.value)
    );

    section(&mut out, 8, "Why did this happen?");
    let interesting = find(d, |r| r.kind == "human_resolution" && !r.committed)
        .or_else(|| find(d, |r| !r.validation.accepted))
        .map(|r| r.proposal_id.clone());
    if let Some(id) = interesting {
        if let Some(explanation) = crate::explain::explain(&result.events, &id) {
            out.push_str(&crate::explain::render(&explanation));
        }
        let _ = writeln!(out, "  (mesh explain {} {id})", config.run_id);
    }

    section(&mut out, 9, "Replay");
    let bundle = Bundle::load(&dir).map_err(|e| e.to_string())?;
    let replay = crate::replay::replay_bundle(&bundle).await;
    out.push_str(&crate::replay::render(&replay));

    section(
        &mut out,
        10,
        "Counterfactual: the same run with judgment disabled",
    );
    let mut overrides = std::collections::BTreeMap::new();
    overrides.insert(
        "kernel.judgment.provider".to_string(),
        serde_json::json!("disabled"),
    );
    let branch =
        crate::counterfactual::run_branch(&bundle, overrides, "no-judgment", false).await?;
    let branch_dir = artifacts.join("runs").join(&branch.result.config.run_id);
    save_run(
        &branch.result,
        &branch_dir,
        SaveOptions {
            force: true,
            command: crate::cli::command_line(),
            reproduce: vec![format!("mesh counterfactual {} --set kernel.judgment.provider=disabled --label no-judgment", config.run_id)],
            parent: Some(branch.parent.clone()),
            started_at_wall: chrono::Utc::now().to_rfc3339(),
            limitations: &[],
        },
    )
    .map_err(|e| e.to_string())?;
    crate::record::write_json(&branch_dir.join("divergence.json"), &branch.comparison)
        .map_err(|e| e.to_string())?;
    out.push_str(&crate::counterfactual::render(&branch.comparison));
    let _ = writeln!(
        out,
        "\n  The original recording was not modified. The branch is {}",
        branch_dir.display()
    );

    let _ = writeln!(out, "\nNext:");
    let _ = writeln!(out, "  mesh experiment run experiments/judgment-ablation/manifest.yaml   # 100 paired repetitions per condition");
    let _ = writeln!(out, "  mesh capabilities                                                # what this installation can do now");
    let _ = writeln!(out, "  make cockpit                                                     # the research cockpit (needs Node.js)");
    Ok(out)
}
