//! Static export for the research cockpit.
//!
//! The cockpit can run against `mesh serve`, or, with no backend, against a
//! static export of real recorded runs. The export contains complete run
//! bundles (so the browser can verify the hash chain itself), a counterfactual
//! branch with its divergence analysis, experiment summaries, a capability
//! snapshot, and claim evaluations. Nothing in it is synthetic display data.

use crate::bundle::{save_run, SaveOptions};
use crate::record::write_json;
use serde_json::json;
use std::path::Path;

pub const EXPORT_FORMAT: &str = "resonate-ai-mesh.web-export.v1";
pub const CANONICAL_SCENARIO: &str = "scenarios/perturbed-mesh.yaml";
pub const EXPORT_EXPERIMENTS: &[&str] =
    &["judgment-ablation", "validator-stress", "fault-injection"];

pub async fn export(root: &Path, out: &Path, repetitions: Option<u32>) -> Result<(), String> {
    let runs_dir = out.join("runs");
    let experiments_dir = out.join("experiments");
    if out.exists() {
        std::fs::remove_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
    }
    std::fs::create_dir_all(&runs_dir).map_err(|e| e.to_string())?;

    let scenario_path = root.join(CANONICAL_SCENARIO);
    let target = crate::cli::load_target(&scenario_path)?;
    let config = crate::cli::resolve_target(&target, None, 0, Some(42), &Default::default())?;
    let result = crate::runner::run(&config, crate::runner::RunOptions::default())
        .await
        .map_err(|e| e.to_string())?;
    let canonical_dir = runs_dir.join(&config.run_id);
    save_run(
        &result,
        &canonical_dir,
        SaveOptions {
            force: true,
            command: vec!["mesh".into(), "export-web".into()],
            reproduce: vec![format!("mesh run {CANONICAL_SCENARIO} --seed 42")],
            parent: None,
            started_at_wall: chrono::Utc::now().to_rfc3339(),
            limitations: &[],
        },
    )
    .map_err(|e| e.to_string())?;

    let bundle = crate::record::Bundle::load(&canonical_dir).map_err(|e| e.to_string())?;
    let mut overrides = std::collections::BTreeMap::new();
    overrides.insert("kernel.judgment.provider".to_string(), json!("disabled"));
    let branch =
        crate::counterfactual::run_branch(&bundle, overrides, "no-judgment", false).await?;
    let branch_dir = runs_dir.join(&branch.result.config.run_id);
    save_run(
        &branch.result,
        &branch_dir,
        SaveOptions {
            force: true,
            command: vec!["mesh".into(), "export-web".into()],
            reproduce: vec![format!(
                "mesh counterfactual {} --set kernel.judgment.provider=disabled --label no-judgment",
                config.run_id
            )],
            parent: Some(branch.parent.clone()),
            started_at_wall: chrono::Utc::now().to_rfc3339(),
            limitations: &[],
        },
    )
    .map_err(|e| e.to_string())?;
    write_json(&branch_dir.join("divergence.json"), &branch.comparison)
        .map_err(|e| e.to_string())?;

    // Experiments are run into a scratch artifacts root, then only their
    // summaries and reports are exported.
    let scratch = out.join(".scratch");
    let mut experiments = Vec::new();
    for id in EXPORT_EXPERIMENTS {
        let manifest_path = root.join("experiments").join(id).join("manifest.yaml");
        let (manifest, scenario) =
            crate::config::load_manifest(&manifest_path).map_err(|e| e.to_string())?;
        let exp_out = scratch.join("experiments").join(id);
        let outcome = crate::experiment::run_experiment(
            &manifest,
            &scenario,
            &crate::experiment::ExperimentOptions {
                out_dir: exp_out.clone(),
                repetitions,
                keep: crate::experiment::Keep::None,
                allow_network: false,
                manifest_path: Path::new("experiments").join(id).join("manifest.yaml"),
                force: true,
                progress: false,
            },
        )
        .await?;
        let target_dir = experiments_dir.join(id);
        std::fs::create_dir_all(&target_dir).map_err(|e| e.to_string())?;
        for file in ["summary.json", "report.md", "runs.csv"] {
            std::fs::copy(exp_out.join(file), target_dir.join(file)).map_err(|e| e.to_string())?;
        }
        experiments.push(json!({
            "id": id,
            "title": outcome.summary.title,
            "summary": format!("experiments/{id}/summary.json"),
            "report": format!("experiments/{id}/report.md"),
            "raw": format!("experiments/{id}/runs.csv"),
        }));
    }

    let claims = match crate::claims::check_claims(&root.join("claims"), &scratch) {
        Ok(results) => serde_json::to_value(results).unwrap_or_default(),
        Err(error) => json!({ "error": error }),
    };
    write_json(&out.join("claims.json"), &claims).map_err(|e| e.to_string())?;
    std::fs::remove_dir_all(&scratch).map_err(|e| e.to_string())?;

    let capabilities = crate::capabilities::discover(
        &crate::capabilities::Probe {
            root: root.to_path_buf(),
            artifacts: out.to_path_buf(),
        },
        &crate::capabilities::LiveFacts::default(),
    );
    write_json(&out.join("capabilities.json"), &capabilities).map_err(|e| e.to_string())?;

    let scenarios: Vec<serde_json::Value> = crate::capabilities::scenario_files(root)
        .iter()
        .filter_map(|path| crate::config::load_scenario(path).ok().map(|s| (path, s)))
        .map(|(path, s)| {
            json!({
                "id": s.id,
                "description": s.description,
                "path": path.strip_prefix(root).unwrap_or(path).display().to_string(),
                "agents": s.agents.len(),
                "ticks": s.ticks,
                "faults": s.faults.iter().map(|f| f.label()).collect::<Vec<_>>(),
                "judgment": s.kernel.judgment.provider,
            })
        })
        .collect();
    let manifests: Vec<serde_json::Value> = crate::capabilities::manifest_files(root)
        .iter()
        .filter_map(|path| crate::config::load_manifest(path).ok().map(|m| (path, m.0)))
        .map(|(path, m)| {
            json!({
                "id": m.id,
                "title": m.title,
                "question": m.question.trim(),
                "hypothesis": m.hypothesis.statement.trim(),
                "prediction": m.hypothesis.prediction,
                "conditions": m.conditions.iter().map(|c| json!({"id": c.id, "description": c.description, "set": c.set, "requires": c.requires})).collect::<Vec<_>>(),
                "repetitions": m.repetitions,
                "path": path.strip_prefix(root).unwrap_or(path).display().to_string(),
            })
        })
        .collect();
    write_json(
        &out.join("index.json"),
        &json!({
            "format": EXPORT_FORMAT,
            "software_version": event_bus::SOFTWARE_VERSION,
            "note": "Recorded SIMULATED runs exported by `mesh export-web`. The cockpit verifies each event chain in the browser; `mesh replay` re-executes them.",
            "runs": [
                {
                    "id": config.run_id,
                    "label": "The Perturbed Mesh (seed 42)",
                    "kind": "recorded",
                    "path": format!("runs/{}", config.run_id),
                },
                {
                    "id": branch.result.config.run_id,
                    "label": "Counterfactual: judgment disabled",
                    "kind": "counterfactual",
                    "parent": config.run_id,
                    "path": format!("runs/{}", branch.result.config.run_id),
                    "divergence": format!("runs/{}/divergence.json", branch.result.config.run_id),
                }
            ],
            "experiments": experiments,
            "scenarios": scenarios,
            "manifests": manifests,
            "capabilities": "capabilities.json",
            "claims": "claims.json",
        }),
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
