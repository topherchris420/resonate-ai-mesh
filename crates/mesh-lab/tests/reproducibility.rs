//! Reproducibility, replay, and authority-boundary tests over real runs.

use mesh_lab::config::{self, load_scenario, resolve_with_overrides, RunConfig};
use mesh_lab::record::Bundle;
use mesh_lab::runner::{run, RunOptions};
use proptest::prelude::*;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("runtime")
}

fn scenario_config(name: &str, overrides: BTreeMap<String, Value>, seed: u64) -> RunConfig {
    let scenario = load_scenario(&root().join("scenarios").join(format!("{name}.yaml")))
        .expect("scenario loads");
    resolve_with_overrides(&scenario, &scenario.id, "test", 0, seed, overrides)
        .expect("config resolves")
}

fn python_available() -> bool {
    std::process::Command::new("python3")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("mesh-lab-test-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn save(result: &mesh_lab::runner::RunResult, dir: &Path) {
    mesh_lab::bundle::save_run(
        result,
        dir,
        mesh_lab::bundle::SaveOptions {
            force: true,
            command: vec!["test".into()],
            reproduce: vec![],
            parent: None,
            started_at_wall: "test".into(),
            limitations: &[],
        },
    )
    .expect("bundle saves");
}

#[test]
fn every_scenario_is_deterministic_and_keeps_its_invariants() {
    let rt = runtime();
    let mut checked = 0;
    for path in mesh_lab::capabilities::scenario_files(&root()) {
        let scenario = load_scenario(&path).unwrap();
        if scenario
            .agents
            .iter()
            .any(|a| a.behavior == config::Behavior::External)
            && !python_available()
        {
            continue;
        }
        // Keep the test quick: at most 30 ticks per scenario.
        let mut overrides = BTreeMap::new();
        overrides.insert("ticks".to_string(), json!(scenario.ticks.min(30)));
        let config =
            resolve_with_overrides(&scenario, &scenario.id, "test", 0, 42, overrides).unwrap();
        let first = rt.block_on(run(&config, RunOptions::default())).unwrap();
        let external = scenario
            .agents
            .iter()
            .any(|a| a.behavior == config::Behavior::External);
        if external {
            // A live process is declared non-deterministic (its timing depends on
            // the machine). What must be exact is the replay of its recording.
            let dir = scratch(&format!("external-{}", scenario.id));
            save(&first, &dir);
            let report = rt.block_on(mesh_lab::replay::replay_bundle(
                &Bundle::load(&dir).unwrap(),
            ));
            assert!(report.verified, "{}", mesh_lab::replay::render(&report));
            std::fs::remove_dir_all(dir).ok();
        } else {
            let second = rt.block_on(run(&config, RunOptions::default())).unwrap();
            assert_eq!(
                first.head_hash, second.head_hash,
                "{} is not deterministic",
                scenario.id
            );
            assert_eq!(first.events, second.events);
            assert_eq!(first.final_state.hash(), second.final_state.hash());
        }
        for invariant in &first.metrics.invariants {
            assert!(
                invariant.holds,
                "{}: {} violated: {:?}",
                scenario.id, invariant.name, invariant.violations
            );
        }
        assert_eq!(first.network_calls, 0);
        checked += 1;
    }
    assert!(checked >= 12, "only {checked} scenarios were checked");
}

#[test]
fn recorded_runs_replay_exactly_with_zero_network_calls() {
    let rt = runtime();
    let config = scenario_config("perturbed-mesh", BTreeMap::new(), 42);
    let result = rt.block_on(run(&config, RunOptions::default())).unwrap();
    let dir = scratch("replay");
    save(&result, &dir);
    let bundle = Bundle::load(&dir).unwrap();
    let report = rt.block_on(mesh_lab::replay::replay_bundle(&bundle));
    assert!(report.verified, "{}", mesh_lab::replay::render(&report));
    assert!(report.head_exact_match);
    assert_eq!(report.network_calls, 0);
    assert_eq!(report.events_matching as usize, result.events.len());
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn tampering_is_detected_and_blocks_replay() {
    let rt = runtime();
    let config = scenario_config(
        "corrupted-event",
        [("ticks".to_string(), json!(15))].into(),
        42,
    );
    let result = rt.block_on(run(&config, RunOptions::default())).unwrap();
    let dir = scratch("tamper");
    save(&result, &dir);
    let path = dir.join("events.jsonl");
    let text = std::fs::read_to_string(&path).unwrap();
    let tampered = text.replacen("\"accepted\":false", "\"accepted\":true", 1);
    assert_ne!(
        text, tampered,
        "the run must contain a rejection to tamper with"
    );
    std::fs::write(&path, tampered).unwrap();
    let bundle = Bundle::load(&dir).unwrap();
    let integrity = mesh_lab::replay::verify_integrity(&bundle);
    assert!(!integrity.ok);
    assert!(!integrity.chain_ok);
    let report = rt.block_on(mesh_lab::replay::replay_bundle(&bundle));
    assert!(!report.verified);
    assert!(
        report.error.is_some(),
        "a tampered recording must not be re-executed"
    );
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn counterfactual_branches_leave_the_original_untouched_and_locate_divergence() {
    let rt = runtime();
    let config = scenario_config("perturbed-mesh", BTreeMap::new(), 42);
    let result = rt.block_on(run(&config, RunOptions::default())).unwrap();
    let dir = scratch("counterfactual");
    save(&result, &dir);
    let before = mesh_lab::record::file_hash(&dir.join("events.jsonl")).unwrap();
    let bundle = Bundle::load(&dir).unwrap();

    let same = rt
        .block_on(mesh_lab::counterfactual::run_branch(
            &bundle,
            BTreeMap::new(),
            "identity",
            false,
        ))
        .unwrap();
    assert!(
        same.comparison.first_divergent_event.is_none(),
        "an empty change must not diverge"
    );
    assert!(same.comparison.decision_changes.is_empty());

    let mut overrides = BTreeMap::new();
    overrides.insert("kernel.judgment.provider".to_string(), json!("disabled"));
    let branch = rt
        .block_on(mesh_lab::counterfactual::run_branch(
            &bundle,
            overrides,
            "no-judgment",
            false,
        ))
        .unwrap();
    assert!(branch.comparison.first_divergent_event.is_some());
    assert!(branch.comparison.first_state_divergence_tick.is_some());
    assert!(!branch.comparison.decision_changes.is_empty());
    assert_eq!(
        branch.result.config.run_id,
        format!("{}~no-judgment", config.run_id)
    );
    assert_eq!(branch.parent.head_hash, result.head_hash);
    assert_eq!(
        mesh_lab::record::file_hash(&dir.join("events.jsonl")).unwrap(),
        before
    );
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn a_networked_judge_is_never_built_without_permission() {
    let rt = runtime();
    let mut overrides = BTreeMap::new();
    overrides.insert("kernel.judgment.provider".to_string(), json!("typesafe"));
    overrides.insert("ticks".to_string(), json!(3));
    let config = scenario_config("baseline", overrides, 1);
    let outcome = rt.block_on(run(&config, RunOptions::default()));
    assert!(
        outcome.is_err(),
        "typesafe without --allow-network must not run"
    );
}

#[test]
fn replay_substitutes_a_networked_judge_with_recordings() {
    // Record with a deterministic judge, then relabel the recording as if it
    // had come from the remote provider. Replay must answer from the recording
    // and never construct the networked provider.
    let rt = runtime();
    let mut overrides = BTreeMap::new();
    overrides.insert("kernel.judgment.provider".to_string(), json!("mock"));
    overrides.insert("ticks".to_string(), json!(12));
    let config = scenario_config("sensor-dropout", overrides, 9);
    let recorded = rt.block_on(run(&config, RunOptions::default())).unwrap();
    let substitutions = mesh_lab::runner::Substitutions {
        judgments: Some(recorded.judgments.clone()),
        ..Default::default()
    };
    let mut remote = config.clone();
    remote.scenario.kernel.judgment.provider = config::JudgeProvider::Typesafe;
    let replayed = rt
        .block_on(run(
            &remote,
            RunOptions {
                substitutions,
                ..RunOptions::default()
            },
        ))
        .unwrap();
    assert_eq!(replayed.network_calls, 0);
    assert_eq!(replayed.judgment_calls, recorded.judgment_calls);
    assert_eq!(replayed.final_state.hash(), recorded.final_state.hash());
    assert_eq!(
        replayed.judge.unwrap().kind,
        typed_judgment::ProviderKind::Recorded
    );
}

#[test]
fn golden_fixtures_still_replay() {
    let rt = runtime();
    let results = rt.block_on(mesh_lab::golden::verify(&root().join("fixtures/golden")));
    assert!(!results.is_empty());
    for result in results {
        let report = result.report.expect("fixture loads");
        assert!(
            report.verified,
            "golden {} diverged:\n{}",
            result.name,
            mesh_lab::replay::render(&report)
        );
    }
}

fn arbitrary_overrides() -> impl Strategy<Value = BTreeMap<String, Value>> {
    (
        prop_oneof![Just("disabled"), Just("mock")],
        prop_oneof![Just("evidence_heuristic"), Just("contrarian"), Just("supported")],
        proptest::option::of((0usize..14, 2u64..12, 1u64..6, 0.0f64..40.0)),
        any::<bool>(),
    )
        .prop_map(|(provider, profile, fault, gating)| {
            let mut overrides = BTreeMap::new();
            overrides.insert("ticks".to_string(), json!(16));
            overrides.insert("kernel.judgment.provider".to_string(), json!(provider));
            overrides.insert("kernel.judgment.profile".to_string(), json!(profile));
            overrides.insert("kernel.policy.adaptive.enabled".to_string(), json!(gating));
            if let Some((kind, at, duration, magnitude)) = fault {
                let kind = config::FaultKind::ALL[kind];
                let mut spec = json!({
                    "kind": kind.as_str(), "at_tick": at, "duration_ticks": duration, "magnitude": magnitude
                });
                if kind.targets_agent() {
                    spec["target"] = json!("all");
                }
                overrides.insert("faults".to_string(), json!([spec]));
            }
            overrides
        })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(12))]

    /// Identical seed, inputs, and versions produce identical outputs; replay
    /// makes no network calls; failed validation never leads to judgment or a
    /// commit; every simulated human-state datum stays SIMULATED.
    #[test]
    fn runs_are_reproducible_and_keep_authority_invariants(
        seed in 0u64..10_000,
        overrides in arbitrary_overrides(),
    ) {
        let rt = runtime();
        let config = scenario_config("perturbed-mesh", overrides, seed);
        let first = rt.block_on(run(&config, RunOptions::default())).unwrap();
        let second = rt.block_on(run(&config, RunOptions::default())).unwrap();
        prop_assert_eq!(&first.head_hash, &second.head_hash);
        for invariant in &first.metrics.invariants {
            prop_assert!(invariant.holds, "{} violated: {:?}", invariant.name, invariant.violations);
        }
        for decision in &first.decisions {
            if !decision.validation.accepted {
                prop_assert!(decision.judgment.is_none());
                prop_assert!(!decision.committed);
            }
        }
        for chained in first.events.iter().filter(|c| c.event.event_type == "human_state") {
            prop_assert_eq!(chained.event.mode, event_bus::DataMode::Simulated);
            prop_assert_eq!(&chained.event.payload["datum"]["mode"], &json!("SIMULATED"));
        }
        let dir = scratch(&format!("prop-{seed}"));
        save(&first, &dir);
        let report = rt.block_on(mesh_lab::replay::replay_bundle(&Bundle::load(&dir).unwrap()));
        prop_assert!(report.verified, "{}", mesh_lab::replay::render(&report));
        prop_assert_eq!(report.network_calls, 0);
        std::fs::remove_dir_all(dir).ok();
    }
}

/// A scripted stand-in for a person at the live console: commands at fixed
/// ticks, then a stop before the scenario ends.
struct ScriptedConsole {
    commands: BTreeMap<u64, Vec<mesh_lab::runner::ControlCommand>>,
    last_tick: Option<u64>,
    stop_before: u64,
}

impl mesh_lab::runner::RunControl for ScriptedConsole {
    fn commands(&mut self, tick: u64) -> Vec<mesh_lab::runner::ControlCommand> {
        self.last_tick = Some(tick);
        self.commands.remove(&tick).unwrap_or_default()
    }

    fn should_stop(&self) -> bool {
        self.last_tick.is_some_and(|t| t + 1 >= self.stop_before)
    }
}

#[test]
fn a_live_session_stopped_early_replays_exactly() {
    let command = |value: Value| serde_json::from_value(value).expect("command parses");
    let mut commands = BTreeMap::new();
    commands.insert(
        5,
        vec![command(json!({"command": "inject_fault", "fault": {"kind": "sensor_dropout", "target": "all", "at_tick": 6, "duration_ticks": 4, "magnitude": 0.0}}))],
    );
    commands.insert(
        8,
        vec![command(json!({"command": "operator_proposal", "agent_id": "medic_05", "intent": {"action_type": "MOVE", "target": {"x": 0.0, "y": 0.0, "z": 0.0}, "priority": 5, "rationale": "operator console"}}))],
    );
    let mut overrides = BTreeMap::new();
    overrides.insert("human_review.mode".to_string(), json!("manual"));
    let config = scenario_config("perturbed-mesh", overrides, 42);
    let console = ScriptedConsole {
        commands,
        last_tick: None,
        stop_before: 21,
    };
    let result = runtime()
        .block_on(run(
            &config,
            RunOptions {
                control: Some(Box::new(console)),
                ..RunOptions::default()
            },
        ))
        .expect("live run");
    assert_eq!(result.ticks_run, 21);
    let operator_proposal = result
        .events
        .iter()
        .find(|c| c.event.event_type == "proposal" && c.event.source.contains("operator"))
        .expect("operator proposal recorded");
    let validation = result
        .events
        .iter()
        .find(|c| {
            c.event.event_type == "validation"
                && c.event.correlation_id == operator_proposal.event.correlation_id
        })
        .expect("operator proposal validated");
    assert_eq!(
        validation.event.payload["accepted"], false,
        "a too-long operator step is rejected like any other"
    );

    let dir = scratch("live-stop");
    save(&result, &dir);
    let bundle = Bundle::load(&dir).expect("bundle loads");
    let report = runtime().block_on(mesh_lab::replay::replay_bundle(&bundle));
    assert!(report.verified, "{}", mesh_lab::replay::render(&report));
    assert!(report.head_exact_match);
    assert!(report
        .substituted
        .iter()
        .any(|s| s.contains("operator stop before tick 21")));
    assert!(report
        .substituted
        .iter()
        .any(|s| s.contains("operator commands")));
    let _ = std::fs::remove_dir_all(dir);
}
