//! Capability discovery: what this installation can do right now.
//!
//! Every statement is backed by a probe of the actual environment: files that
//! exist, configuration that is set, runs that were recorded, processes that
//! are connected. Nothing is advertised that is not available, and the reason
//! is given for what is not.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Capability {
    pub id: String,
    pub statement: String,
    pub available: bool,
    pub reason: Option<String>,
    pub command: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SystemStatus {
    pub human_state_input: String,
    pub remote_judgment: String,
    pub physical_control: String,
    pub data_mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CapabilityReport {
    pub orientation: Vec<String>,
    pub status: SystemStatus,
    pub capabilities: Vec<Capability>,
    pub scenarios: Vec<String>,
    pub experiments: Vec<String>,
    pub latest_run: Option<String>,
}

/// Facts a running server knows that a one-shot CLI does not.
#[derive(Debug, Clone, Default)]
pub struct LiveFacts {
    pub server: bool,
    pub live_sources_connected: Vec<String>,
    pub live_session_running: bool,
}

pub struct Probe {
    pub root: PathBuf,
    pub artifacts: PathBuf,
}

fn yaml_files(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.extension().and_then(|x| x.to_str()) == Some("yaml"))
                .collect()
        })
        .unwrap_or_default();
    files.sort();
    files
}

pub fn scenario_files(root: &Path) -> Vec<PathBuf> {
    yaml_files(&root.join("scenarios"))
        .into_iter()
        .filter(|p| crate::config::load_scenario(p).is_ok())
        .collect()
}

pub fn manifest_files(root: &Path) -> Vec<PathBuf> {
    let mut manifests: Vec<PathBuf> = std::fs::read_dir(root.join("experiments"))
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .map(|e| e.path().join("manifest.yaml"))
                .filter(|p| p.is_file())
                .collect()
        })
        .unwrap_or_default();
    manifests.sort();
    manifests
        .into_iter()
        .filter(|p| crate::config::load_manifest(p).is_ok())
        .collect()
}

/// Recorded run bundles, newest first.
pub fn recorded_runs(artifacts: &Path) -> Vec<(PathBuf, std::time::SystemTime)> {
    let mut found = Vec::new();
    let mut scan = |dir: PathBuf| {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                let manifest = entry.path().join("manifest.json");
                if let Ok(meta) = std::fs::metadata(&manifest) {
                    found.push((
                        entry.path(),
                        meta.modified().unwrap_or(std::time::UNIX_EPOCH),
                    ));
                }
            }
        }
    };
    scan(artifacts.join("runs"));
    if let Ok(entries) = std::fs::read_dir(artifacts.join("experiments")) {
        for entry in entries.filter_map(|e| e.ok()) {
            scan(entry.path().join("runs"));
        }
    }
    found.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    found
}

fn typesafe_status() -> (bool, String) {
    let enabled = std::env::var("JUDGMENT_ENABLED")
        .map(|v| {
            matches!(
                v.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false);
    let key = std::env::var(typed_judgment::API_KEY_ENV)
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false);
    match (key, enabled) {
        (false, _) => (false, format!("OFF ({} is not set)", typed_judgment::API_KEY_ENV)),
        (true, false) => (true, "AVAILABLE (credentials present; JUDGMENT_ENABLED is not true; runs need --allow-network)".into()),
        (true, true) => (true, "CONFIGURED (credentials present, not probed; runs need --allow-network)".into()),
    }
}

fn python_available() -> bool {
    std::process::Command::new("python3")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn cap(
    id: &str,
    statement: String,
    available: bool,
    reason: Option<String>,
    command: Option<String>,
) -> Capability {
    Capability {
        id: id.to_string(),
        statement,
        available,
        reason,
        command,
    }
}

pub fn discover(probe: &Probe, live: &LiveFacts) -> CapabilityReport {
    let scenarios = scenario_files(&probe.root);
    let manifests = manifest_files(&probe.root);
    let runs = recorded_runs(&probe.artifacts);
    let latest = runs.first().map(|(dir, _)| dir.clone());
    let latest_id = latest
        .as_ref()
        .and_then(|dir| dir.file_name())
        .map(|name| name.to_string_lossy().to_string());
    let (typesafe_ok, typesafe_text) = typesafe_status();
    let comparable: Vec<String> = manifests
        .iter()
        .filter_map(|p| crate::config::load_manifest(p).ok())
        .filter(|(m, _)| m.conditions.len() >= 2)
        .map(|(m, _)| m.id)
        .collect();
    let rel = |p: &Path| {
        p.strip_prefix(&probe.root)
            .unwrap_or(p)
            .display()
            .to_string()
    };
    let canonical = scenarios
        .iter()
        .find(|p| p.ends_with("perturbed-mesh.yaml"))
        .or(scenarios.first())
        .map(|p| rel(p));

    let mut capabilities = Vec::new();
    capabilities.push(cap(
        "simulate",
        format!(
            "run a deterministic multi-agent simulation ({} scenarios)",
            scenarios.len()
        ),
        !scenarios.is_empty(),
        scenarios
            .is_empty()
            .then(|| "no valid scenario files under scenarios/".to_string()),
        canonical.as_ref().map(|s| format!("mesh run {s}")),
    ));
    capabilities.push(cap(
        "experiment",
        format!(
            "run a controlled experiment with repetitions and paired statistics ({} manifests)",
            manifests.len()
        ),
        !manifests.is_empty(),
        manifests
            .is_empty()
            .then(|| "no valid manifests under experiments/".to_string()),
        manifests
            .first()
            .map(|m| format!("mesh experiment run {}", rel(m))),
    ));
    capabilities.push(cap(
        "compare_policies",
        "compare validation, judgment, or gating policies under identical seeds".to_string(),
        !comparable.is_empty(),
        comparable
            .is_empty()
            .then(|| "no manifest has two or more conditions".to_string()),
        comparable
            .first()
            .map(|id| format!("mesh experiment compare {id}")),
    ));
    capabilities.push(cap(
        "replay",
        match &latest_id {
            Some(id) => format!("replay the most recent run ({id}) with zero network calls"),
            None => "replay a recorded run with zero network calls".to_string(),
        },
        latest.is_some(),
        latest
            .is_none()
            .then(|| "no run has been recorded yet".to_string()),
        latest_id.as_ref().map(|id| format!("mesh replay {id}")),
    ));
    capabilities.push(cap(
        "counterfactual",
        "re-run a recorded run with judgment disabled and show where the timelines diverge".to_string(),
        latest.is_some(),
        latest.is_none().then(|| "no run has been recorded yet".to_string()),
        latest_id
            .as_ref()
            .map(|id| format!("mesh counterfactual {id} --set kernel.judgment.provider=disabled --label no-judgment")),
    ));
    capabilities.push(cap(
        "explain",
        "explain why a decision happened by walking its causal chain".to_string(),
        latest.is_some(),
        latest
            .is_none()
            .then(|| "no run has been recorded yet".to_string()),
        latest_id
            .as_ref()
            .map(|id| format!("mesh explain {id} <proposal-id>")),
    ));
    capabilities.push(cap(
        "inject_faults",
        format!(
            "inject faults: {}",
            crate::config::FaultKind::ALL
                .iter()
                .map(|k| k.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ),
        true,
        None,
        canonical
            .as_ref()
            .map(|s| format!("mesh run {s} --set 'faults=[{{kind: judge_timeout, at_tick: 5, duration_ticks: 10}}]'")),
    ));
    capabilities.push(cap(
        "mock_judgment",
        "ablate judgment with deterministic offline judges (evidence-heuristic, contrarian)"
            .to_string(),
        true,
        None,
        Some("mesh experiment run experiments/judgment-ablation/manifest.yaml".to_string()),
    ));
    capabilities.push(cap(
        "jev",
        "test TypeSafe Jev against deterministic-only execution".to_string(),
        typesafe_ok,
        (!typesafe_ok).then(|| format!("{} is not set", typed_judgment::API_KEY_ENV)),
        typesafe_ok.then(|| {
            "mesh experiment run experiments/judgment-ablation/manifest.yaml --allow-network"
                .to_string()
        }),
    ));
    capabilities.push(cap(
        "export_evidence",
        "export an evidence bundle (events, decisions, metrics, provenance, report)".to_string(),
        latest.is_some(),
        latest
            .is_none()
            .then(|| "no run has been recorded yet".to_string()),
        latest
            .as_ref()
            .map(|dir| format!("mesh verify {}", dir.display())),
    ));
    let rain_example = probe.root.join("examples/rain_agent_stub.py");
    let python = python_available();
    capabilities.push(cap(
        "external_agent",
        "attach an external deliberative agent (R.A.I.N. adapter protocol mesh-agent/1)"
            .to_string(),
        python && rain_example.is_file(),
        if !rain_example.is_file() {
            Some("examples/rain_agent_stub.py not found".to_string())
        } else if !python {
            Some("python3 is not available".to_string())
        } else {
            None
        },
        Some("mesh run scenarios/external-agent.yaml".to_string()),
    ));
    let golden = probe.root.join("fixtures/golden");
    capabilities.push(cap(
        "golden",
        "verify the committed golden recordings still replay exactly".to_string(),
        golden.is_dir(),
        (!golden.is_dir()).then(|| "fixtures/golden not found".to_string()),
        Some("mesh golden verify".to_string()),
    ));
    let claims = probe.root.join("claims");
    capabilities.push(cap(
        "claims",
        "check research claims against recorded experiment evidence".to_string(),
        claims.is_dir(),
        (!claims.is_dir()).then(|| "claims/ not found".to_string()),
        Some("mesh claims check".to_string()),
    ));
    capabilities.push(cap(
        "cockpit",
        if live.server {
            "stream live sessions to the research cockpit".to_string()
        } else {
            "serve the research cockpit API and live sessions".to_string()
        },
        true,
        None,
        Some(if live.server {
            "open http://localhost:3000".to_string()
        } else {
            "mesh serve".to_string()
        }),
    ));
    capabilities.push(cap(
        "live_human_state",
        "accept LIVE human-state data from a registered source".to_string(),
        !live.live_sources_connected.is_empty(),
        Some(if live.live_sources_connected.is_empty() {
            "no live source is connected; human-state input is SIMULATED".to_string()
        } else {
            format!("connected: {}", live.live_sources_connected.join(", "))
        }),
        None,
    ));

    let human = if live.live_sources_connected.is_empty() {
        "SIMULATED".to_string()
    } else {
        format!(
            "LIVE from {} (simulated sources remain labeled SIMULATED)",
            live.live_sources_connected.join(", ")
        )
    };
    CapabilityReport {
        orientation: vec![
            "This is a deterministic multi-agent research environment.".into(),
            "You can run an experiment, inspect why a decision occurred, inject a failure, compare policies, or replay a previous run.".into(),
            "Nothing here controls physical hardware.".into(),
            format!("Human-state data is currently {human}."),
            format!("Remote judgment is currently {}.", if typesafe_ok { "available but not used unless a run asks for it" } else { "OFF" }),
        ],
        status: SystemStatus {
            human_state_input: human,
            remote_judgment: typesafe_text,
            physical_control: "NONE (not implemented)".into(),
            data_mode: "SIMULATED".into(),
        },
        capabilities,
        scenarios: scenarios.iter().map(|p| rel(p)).collect(),
        experiments: manifests.iter().map(|p| rel(p)).collect(),
        latest_run: latest_id,
    }
}

pub fn render(report: &CapabilityReport) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    let _ = writeln!(out, "Resonate AI Mesh\n");
    for line in &report.orientation {
        let _ = writeln!(out, "  {line}");
    }
    let _ = writeln!(
        out,
        "\n  Human-state input:  {}",
        report.status.human_state_input
    );
    let _ = writeln!(
        out,
        "  Remote judgment:    {}",
        report.status.remote_judgment
    );
    let _ = writeln!(
        out,
        "  Physical control:   {}",
        report.status.physical_control
    );
    let _ = writeln!(out, "\nRight now I can:\n");
    for capability in report.capabilities.iter().filter(|c| c.available) {
        let _ = writeln!(out, "  • {}", capability.statement);
        if let Some(command) = &capability.command {
            let _ = writeln!(out, "      {command}");
        }
        if let Some(reason) = &capability.reason {
            let _ = writeln!(out, "      ({reason})");
        }
    }
    let unavailable: Vec<&Capability> = report
        .capabilities
        .iter()
        .filter(|c| !c.available)
        .collect();
    if !unavailable.is_empty() {
        let _ = writeln!(out, "\nNot available right now:\n");
        for capability in unavailable {
            let _ = writeln!(
                out,
                "  • {} — {}",
                capability.statement,
                capability.reason.as_deref().unwrap_or("unavailable")
            );
        }
    }
    out
}
