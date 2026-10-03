//! The `mesh` command-line interface.

use crate::bundle::{save_run, SaveOptions};
use crate::config::{self, ExperimentManifest, RunConfig, Scenario};
use crate::runner::{self, RunOptions, RunResult};
use clap::{Args, Parser, Subcommand};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(
    name = "mesh",
    version,
    about = "Resonate AI Mesh: run, record, replay, and compare experiments on the Pordenone kernel",
    long_about = "Resonate AI Mesh is a deterministic multi-agent research environment. Every run is \
recorded as a hash-chained event log that can be verified, replayed without network access, and \
branched into counterfactuals. Nothing here controls physical hardware."
)]
pub struct Cli {
    /// Root directory for run and experiment artifacts.
    #[arg(
        long,
        global = true,
        default_value = "artifacts",
        env = "MESH_ARTIFACTS"
    )]
    pub artifacts: PathBuf,
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Run one scenario, or one condition of an experiment manifest, and record it.
    Run(RunArgs),
    /// Re-execute a recorded run without network access and compare every event.
    Replay(ReplayArgs),
    /// Check a bundle's hash chain and file hashes without re-executing it.
    Verify(ReplayArgs),
    /// Re-run a recorded run with changed configuration and show where the timelines diverge.
    Counterfactual(CounterfactualArgs),
    /// Compare two recorded runs.
    Diff(DiffArgs),
    /// Run or compare experiments (conditions x repetitions).
    #[command(subcommand)]
    Experiment(ExperimentCommand),
}

#[derive(Subcommand)]
pub enum ExperimentCommand {
    /// Run every condition and repetition of a manifest and write a report.
    Run(ExperimentRunArgs),
    /// Show paired comparisons from a finished experiment.
    Compare(ExperimentCompareArgs),
}

#[derive(Args)]
pub struct ExperimentRunArgs {
    pub manifest: PathBuf,
    /// Override the manifest's repetition count.
    #[arg(long)]
    pub reps: Option<u32>,
    /// Which runs keep a full bundle: first (repetition 0 of each condition), all, none.
    #[arg(long, default_value = "first")]
    pub keep: String,
    #[arg(long)]
    pub out: Option<PathBuf>,
    #[arg(long)]
    pub force: bool,
    #[arg(long)]
    pub allow_network: bool,
    #[arg(long)]
    pub json: bool,
}

#[derive(Args)]
pub struct ExperimentCompareArgs {
    /// Experiment id, directory, or summary.json.
    pub experiment: String,
    /// Baseline condition (default: all comparisons).
    pub baseline: Option<String>,
    pub treatment: Option<String>,
    #[arg(long)]
    pub metric: Option<String>,
    #[arg(long)]
    pub json: bool,
}

#[derive(Args)]
pub struct ReplayArgs {
    /// Run bundle directory or run id.
    pub run: String,
    #[arg(long)]
    pub json: bool,
}

#[derive(Args)]
pub struct CounterfactualArgs {
    /// Run bundle directory or run id.
    pub run: String,
    /// Configuration change: --set kernel.judgment.provider=disabled
    #[arg(long = "set", value_name = "PATH=VALUE", required = true)]
    pub set: Vec<String>,
    /// Short name for the branch (used in its run id).
    #[arg(long, default_value = "cf")]
    pub label: String,
    #[arg(long)]
    pub out: Option<PathBuf>,
    #[arg(long)]
    pub force: bool,
    #[arg(long)]
    pub allow_network: bool,
    #[arg(long)]
    pub json: bool,
}

#[derive(Args)]
pub struct DiffArgs {
    pub left: String,
    pub right: String,
    #[arg(long)]
    pub json: bool,
}

#[derive(Args)]
pub struct RunArgs {
    /// Scenario file or experiment manifest.
    pub path: PathBuf,
    /// Condition to run when PATH is a manifest (default: the first condition).
    #[arg(long)]
    pub condition: Option<String>,
    /// Repetition index (selects the derived seed).
    #[arg(long, default_value_t = 0)]
    pub rep: u32,
    /// Seed for a scenario run (manifests derive seeds from their base seed).
    #[arg(long)]
    pub seed: Option<u64>,
    /// Override any configuration field: --set kernel.judgment.provider=disabled
    #[arg(long = "set", value_name = "PATH=VALUE")]
    pub set: Vec<String>,
    /// Output directory (default: <artifacts>/runs/<run-id>).
    #[arg(long)]
    pub out: Option<PathBuf>,
    /// Overwrite an existing bundle.
    #[arg(long)]
    pub force: bool,
    /// Permit a networked judge (TypeSafe). Off by default.
    #[arg(long)]
    pub allow_network: bool,
    /// Print the result as JSON.
    #[arg(long)]
    pub json: bool,
}

pub enum Target {
    Scenario(Scenario),
    Manifest(Box<ExperimentManifest>, Scenario),
}

/// A YAML file is a manifest when it has a `conditions` key.
pub fn load_target(path: &Path) -> Result<Target, String> {
    let raw: Value = config::read_yaml(path).map_err(|error| error.to_string())?;
    if raw.get("conditions").is_some() {
        let (manifest, scenario) =
            config::load_manifest(path).map_err(|error| error.to_string())?;
        Ok(Target::Manifest(Box::new(manifest), scenario))
    } else {
        config::load_scenario(path)
            .map(Target::Scenario)
            .map_err(|error| error.to_string())
    }
}

pub fn parse_sets(sets: &[String]) -> Result<BTreeMap<String, Value>, String> {
    let mut overrides = BTreeMap::new();
    for assignment in sets {
        let (key, value) =
            config::parse_assignment(assignment).map_err(|error| error.to_string())?;
        overrides.insert(key, value);
    }
    Ok(overrides)
}

pub fn resolve_target(
    target: &Target,
    condition: Option<&str>,
    rep: u32,
    seed: Option<u64>,
    extra: &BTreeMap<String, Value>,
) -> Result<RunConfig, String> {
    match target {
        Target::Scenario(scenario) => {
            let seed = seed.unwrap_or(42);
            config::resolve_with_overrides(
                scenario,
                &scenario.id,
                "default",
                rep,
                seed,
                extra.clone(),
            )
            .map_err(|error| error.to_string())
        }
        Target::Manifest(manifest, scenario) => {
            let chosen = match condition {
                Some(id) => manifest
                    .conditions
                    .iter()
                    .find(|c| c.id == id)
                    .ok_or_else(|| {
                        format!(
                            "unknown condition `{id}`; available: {}",
                            manifest
                                .conditions
                                .iter()
                                .map(|c| c.id.as_str())
                                .collect::<Vec<_>>()
                                .join(", ")
                        )
                    })?,
                None => &manifest.conditions[0],
            };
            let mut overrides = manifest.set.clone();
            overrides.extend(chosen.set.clone());
            overrides.extend(extra.clone());
            let seed = seed.unwrap_or_else(|| crate::rng::repetition_seed(manifest.seed, rep));
            config::resolve_with_overrides(scenario, &manifest.id, &chosen.id, rep, seed, overrides)
                .map_err(|error| error.to_string())
        }
    }
}

pub fn reproduce_command(source: &Path, config: &RunConfig, is_manifest: bool) -> String {
    let mut parts = vec![format!("mesh run {}", source.display())];
    if is_manifest {
        parts.push(format!("--condition {}", config.condition_id));
        parts.push(format!("--rep {}", config.repetition));
    } else {
        parts.push(format!("--seed {}", config.seed));
    }
    let condition_keys: Vec<&String> = config.overrides.keys().collect();
    if !is_manifest {
        for key in condition_keys {
            parts.push(format!("--set '{}={}'", key, config.overrides[key]));
        }
    }
    parts.join(" ")
}

pub fn command_line() -> Vec<String> {
    std::env::args().collect()
}

pub fn print_run_summary(result: &RunResult, dir: &Path) {
    let m = &result.metrics;
    let v = |key: &str| crate::bundle::fmt_value(m.values.get(key).copied().flatten());
    println!("RUN RECORDED  {}", result.config.run_id);
    println!();
    println!("  Bundle:             {}", dir.display());
    println!(
        "  Ticks:              {} ({})",
        result.ticks_run, result.termination
    );
    println!("  Events:             {}", result.events.len());
    println!(
        "  Proposals:          {} (accepted {}, rejected {}, committed {}, withheld {})",
        v("proposals"),
        v("accepted_proposals"),
        v("rejected_proposals"),
        v("committed"),
        v("withheld")
    );
    println!(
        "  Unsafe (oracle):    {} proposed, {} blocked, {} committed",
        v("unsafe_proposals"),
        v("unsafe_proposals_blocked"),
        v("unsafe_commits")
    );
    match &result.judge {
        Some(judge) => println!(
            "  Judgment:           {} / {} — {} calls, {} disagreements, {} network calls",
            judge.name,
            judge.model,
            v("judgment_calls"),
            v("judgment_disagreements"),
            v("network_calls")
        ),
        None => println!("  Judgment:           OFF (deterministic validation only)"),
    }
    println!(
        "  Goals reached:      {} / {}",
        v("goals_reached"),
        v("goals_total")
    );
    println!("  Human state:        SIMULATED operator-load index");
    println!("  Head hash:          {}", result.head_hash);
    let failed: Vec<&str> = m
        .invariants
        .iter()
        .filter(|i| !i.holds)
        .map(|i| i.name.as_str())
        .collect();
    if failed.is_empty() {
        println!("  Invariants:         all {} hold", m.invariants.len());
    } else {
        println!("  Invariants:         VIOLATED: {}", failed.join(", "));
    }
}

async fn cmd_run(cli: &Cli, args: &RunArgs) -> Result<i32, String> {
    let started = chrono::Utc::now().to_rfc3339();
    let target = load_target(&args.path)?;
    let overrides = parse_sets(&args.set)?;
    let config = resolve_target(
        &target,
        args.condition.as_deref(),
        args.rep,
        args.seed,
        &overrides,
    )?;
    let result = runner::run(
        &config,
        RunOptions {
            allow_network: args.allow_network,
            ..RunOptions::default()
        },
    )
    .await
    .map_err(|error| error.to_string())?;
    let dir = args
        .out
        .clone()
        .unwrap_or_else(|| cli.artifacts.join("runs").join(&config.run_id));
    let is_manifest = matches!(target, Target::Manifest(..));
    let limitations = match &target {
        Target::Manifest(manifest, _) => manifest.limitations.clone(),
        Target::Scenario(_) => Vec::new(),
    };
    save_run(
        &result,
        &dir,
        SaveOptions {
            force: args.force,
            command: command_line(),
            reproduce: vec![reproduce_command(&args.path, &config, is_manifest)],
            parent: None,
            started_at_wall: started,
            limitations: &limitations,
        },
    )
    .map_err(|error| error.to_string())?;
    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "run_id": config.run_id,
                "bundle": dir,
                "head_hash": result.head_hash,
                "metrics": result.metrics,
            }))
            .unwrap_or_default()
        );
    } else {
        print_run_summary(&result, &dir);
    }
    Ok(if result.metrics.invariants.iter().all(|i| i.holds) {
        0
    } else {
        3
    })
}

fn load_bundle(cli: &Cli, reference: &str) -> Result<crate::record::Bundle, String> {
    let dir = crate::record::resolve_run_dir(reference, &cli.artifacts)
        .map_err(|error| error.to_string())?;
    crate::record::Bundle::load(&dir).map_err(|error| error.to_string())
}

async fn cmd_replay(cli: &Cli, args: &ReplayArgs) -> Result<i32, String> {
    let bundle = load_bundle(cli, &args.run)?;
    let report = crate::replay::replay_bundle(&bundle).await;
    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report).unwrap_or_default()
        );
    } else {
        print!("{}", crate::replay::render(&report));
    }
    Ok(if report.verified { 0 } else { 1 })
}

async fn cmd_verify(cli: &Cli, args: &ReplayArgs) -> Result<i32, String> {
    let bundle = load_bundle(cli, &args.run)?;
    let report = crate::replay::verify_integrity(&bundle);
    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report).unwrap_or_default()
        );
    } else {
        println!(
            "{}  {}\n\n  Events:        {}\n  Hash chain:    {}\n  Genesis:       {}\n  Head:          {}\n  File hashes:   {}/{} match",
            if report.ok { "INTEGRITY OK" } else { "INTEGRITY FAILED" },
            bundle.config.run_id,
            bundle.events.len(),
            if report.chain_ok { "intact" } else { "broken" },
            if report.genesis_matches_config { "matches manifest.json" } else { "does not match manifest.json" },
            bundle.replay.head_hash,
            report.files_matching,
            report.files_checked
        );
        for problem in &report.problems {
            println!("  ! {problem}");
        }
    }
    Ok(if report.ok { 0 } else { 1 })
}

async fn cmd_counterfactual(cli: &Cli, args: &CounterfactualArgs) -> Result<i32, String> {
    let started = chrono::Utc::now().to_rfc3339();
    let bundle = load_bundle(cli, &args.run)?;
    let overrides = parse_sets(&args.set)?;
    let branch =
        crate::counterfactual::run_branch(&bundle, overrides, &args.label, args.allow_network)
            .await?;
    let dir = args.out.clone().unwrap_or_else(|| {
        bundle
            .dir
            .parent()
            .map(|parent| parent.join(&branch.result.config.run_id))
            .unwrap_or_else(|| {
                cli.artifacts
                    .join("runs")
                    .join(&branch.result.config.run_id)
            })
    });
    let sets: Vec<String> = args.set.iter().map(|s| format!("--set '{s}'")).collect();
    save_run(
        &branch.result,
        &dir,
        SaveOptions {
            force: args.force,
            command: command_line(),
            reproduce: vec![format!(
                "mesh counterfactual {} {} --label {}",
                bundle.config.run_id,
                sets.join(" "),
                args.label
            )],
            parent: Some(branch.parent.clone()),
            started_at_wall: started,
            limitations: &[],
        },
    )
    .map_err(|error| error.to_string())?;
    crate::record::write_json(&dir.join("divergence.json"), &branch.comparison)
        .map_err(|error| error.to_string())?;
    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&branch.comparison).unwrap_or_default()
        );
    } else {
        println!(
            "COUNTERFACTUAL RECORDED  {}\n  Bundle: {}\n  Parent: {} (unchanged)\n",
            branch.result.config.run_id,
            dir.display(),
            bundle.config.run_id
        );
        print!("{}", crate::counterfactual::render(&branch.comparison));
    }
    Ok(0)
}

async fn cmd_diff(cli: &Cli, args: &DiffArgs) -> Result<i32, String> {
    let left = load_bundle(cli, &args.left)?;
    let right = load_bundle(cli, &args.right)?;
    let read = |bundle: &crate::record::Bundle| {
        crate::record::read_jsonl::<crate::record::DecisionRecord>(
            &bundle.dir.join("decisions.jsonl"),
        )
        .map_err(|error| error.to_string())
    };
    let comparison = crate::counterfactual::compare(
        &left.config.run_id,
        &left.events,
        &read(&left)?,
        &left.metrics,
        &right.config.run_id,
        &right.events,
        &read(&right)?,
        &right.metrics,
        BTreeMap::new(),
    );
    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&comparison).unwrap_or_default()
        );
    } else {
        print!("{}", crate::counterfactual::render(&comparison));
    }
    Ok(if comparison.first_divergent_event.is_none() {
        0
    } else {
        1
    })
}

async fn cmd_experiment_run(cli: &Cli, args: &ExperimentRunArgs) -> Result<i32, String> {
    let (manifest, scenario) =
        config::load_manifest(&args.manifest).map_err(|error| error.to_string())?;
    let keep = match args.keep.as_str() {
        "first" => crate::experiment::Keep::First,
        "all" => crate::experiment::Keep::All,
        "none" => crate::experiment::Keep::None,
        other => {
            return Err(format!(
                "--keep must be first, all, or none (got `{other}`)"
            ))
        }
    };
    let out_dir = args
        .out
        .clone()
        .unwrap_or_else(|| cli.artifacts.join("experiments").join(&manifest.id));
    if !args.json {
        eprintln!(
            "EXPERIMENT {}: {} conditions x {} repetitions",
            manifest.id,
            manifest.conditions.len(),
            args.reps.unwrap_or(manifest.repetitions)
        );
    }
    let outcome = crate::experiment::run_experiment(
        &manifest,
        &scenario,
        &crate::experiment::ExperimentOptions {
            out_dir,
            repetitions: args.reps,
            keep,
            allow_network: args.allow_network,
            manifest_path: args.manifest.clone(),
            force: args.force,
            progress: !args.json,
        },
    )
    .await?;
    let required_failures: Vec<&crate::experiment::InvariantSummary> = outcome
        .summary
        .invariants
        .iter()
        .filter(|i| i.expected && i.runs_violated > 0)
        .collect();
    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&outcome.summary).unwrap_or_default()
        );
    } else {
        println!("\nEXPERIMENT COMPLETE  {}", outcome.summary.experiment_id);
        println!("  Report:   {}", outcome.dir.join("report.md").display());
        println!("  Summary:  {}", outcome.dir.join("summary.json").display());
        println!("  Raw data: {}", outcome.dir.join("runs.csv").display());
        for condition in &outcome.summary.conditions {
            match &condition.skip_reason {
                Some(reason) => println!("  {:<32} SKIPPED ({reason})", condition.id),
                None => println!("  {:<32} {} runs", condition.id, condition.runs),
            }
        }
        if let Some(p) = &outcome.summary.prediction {
            println!("\n  Prediction: {} — {}", p.status.to_uppercase(), p.detail);
        }
        if required_failures.is_empty() {
            println!("  Required invariants: hold in every run");
        } else {
            for failure in &required_failures {
                println!(
                    "  REQUIRED INVARIANT VIOLATED: {} in {} runs",
                    failure.name, failure.runs_violated
                );
            }
        }
        for item in &outcome.summary.unexpected {
            println!("  Unexpected: {item}");
        }
    }
    Ok(if required_failures.is_empty() { 0 } else { 3 })
}

fn find_summary(
    cli: &Cli,
    reference: &str,
) -> Result<crate::experiment::ExperimentSummary, String> {
    let path = PathBuf::from(reference);
    let candidates = [
        path.clone(),
        path.join("summary.json"),
        cli.artifacts
            .join("experiments")
            .join(reference)
            .join("summary.json"),
    ];
    let file = candidates
        .iter()
        .find(|candidate| candidate.is_file())
        .ok_or_else(|| format!("no experiment summary found for `{reference}`"))?;
    crate::record::read_json(file).map_err(|error| error.to_string())
}

async fn cmd_experiment_compare(cli: &Cli, args: &ExperimentCompareArgs) -> Result<i32, String> {
    let summary = find_summary(cli, &args.experiment)?;
    if args.json {
        let selected: Vec<&crate::stats::PairedComparison> = summary
            .comparisons
            .iter()
            .filter(|c| {
                args.baseline.as_deref().is_none_or(|b| b == c.baseline)
                    && args.treatment.as_deref().is_none_or(|t| t == c.treatment)
                    && args.metric.as_deref().is_none_or(|m| m == c.metric)
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&selected).unwrap_or_default()
        );
    } else {
        print!(
            "{}",
            crate::experiment::render_comparisons(
                &summary,
                args.baseline.as_deref(),
                args.treatment.as_deref(),
                args.metric.as_deref()
            )
        );
    }
    Ok(0)
}

pub async fn main(cli: Cli) -> i32 {
    init_logging();
    let outcome = match &cli.command {
        Command::Run(args) => cmd_run(&cli, args).await,
        Command::Replay(args) => cmd_replay(&cli, args).await,
        Command::Verify(args) => cmd_verify(&cli, args).await,
        Command::Counterfactual(args) => cmd_counterfactual(&cli, args).await,
        Command::Diff(args) => cmd_diff(&cli, args).await,
        Command::Experiment(ExperimentCommand::Run(args)) => cmd_experiment_run(&cli, args).await,
        Command::Experiment(ExperimentCommand::Compare(args)) => {
            cmd_experiment_compare(&cli, args).await
        }
    };
    match outcome {
        Ok(code) => code,
        Err(message) => {
            eprintln!("error: {message}");
            2
        }
    }
}

fn init_logging() {
    let filter = tracing_subscriber::EnvFilter::try_from_env("MESH_LOG")
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("error"));
    let builder = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr);
    if std::env::var("MESH_LOG_FORMAT").as_deref() == Ok("json") {
        let _ = builder.json().try_init();
    } else {
        let _ = builder.try_init();
    }
}
