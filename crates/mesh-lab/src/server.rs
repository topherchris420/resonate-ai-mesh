//! `mesh serve`: the cockpit API, live sessions, and the hardened ingest.
//!
//! Security defaults:
//! * binds to 127.0.0.1; a non-loopback bind without `MESH_API_TOKEN` is refused;
//! * with a token, every `/api`, `/ws`, and `/ingest` request must present it
//!   (`Authorization: Bearer <token>`, or `?token=` for browser WebSockets);
//! * CORS allows only the configured origins;
//! * request bodies are limited to 256 KiB and ingest messages to 64 KiB;
//! * run ids are validated before any path is built and only bundle files on an
//!   allow-list are served;
//! * `/ingest` admits only `human_state` and `observation` events, never kernel
//!   events, and refuses LIVE data from unregistered sources.

use crate::capabilities::{self, LiveFacts, Probe};
use crate::cli::Cli;
use crate::config;
use crate::runner::{ControlCommand, RunControl, RunOptions};
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{DefaultBodyLimit, Path as UrlPath, Query, State};
use axum::http::{header, HeaderMap, HeaderValue, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use clap::Args;
use event_bus::{EventBus, HumanStateDatum};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use telemetry_bridge::{admit, IngestPolicy, RateLimiter};
use tokio::sync::mpsc;
use tower_http::cors::{AllowOrigin, CorsLayer};

#[derive(Args, Clone)]
pub struct ServeArgs {
    /// Address to bind. Non-loopback addresses require MESH_API_TOKEN.
    #[arg(long, default_value = "127.0.0.1:7878", env = "MESH_BIND")]
    pub bind: SocketAddr,
    /// Allowed browser origins (repeatable).
    #[arg(long = "allow-origin", default_values_t = vec!["http://localhost:3000".to_string(), "http://127.0.0.1:3000".to_string()])]
    pub allow_origin: Vec<String>,
    /// Default pacing of live sessions in milliseconds per tick.
    #[arg(long, default_value_t = 250)]
    pub tick_ms: u64,
}

const EXPERIMENT_FILES: &[&str] = &["summary.json", "report.md", "runs.csv"];

const SERVED_FILES: &[&str] = &[
    "manifest.json",
    "events.jsonl",
    "decisions.jsonl",
    "intents.jsonl",
    "metrics.json",
    "environment.json",
    "topology.json",
    "provenance.json",
    "replay.json",
    "timing.json",
    "report.md",
    "divergence.json",
];

#[derive(Default)]
struct Counters {
    events_streamed: AtomicU64,
    ws_lagged: AtomicU64,
    ingest_accepted: AtomicU64,
    ingest_rejected: Mutex<BTreeMap<String, u64>>,
    runs: AtomicU64,
    replays: AtomicU64,
    replay_divergences: AtomicU64,
    live_ticks: AtomicU64,
}

struct LiveSession {
    run_id: String,
    /// The resolved scenario, so a client can draw the arena before the
    /// bundle is written.
    scenario: Value,
    commands: mpsc::UnboundedSender<ControlCommand>,
    stop: Arc<AtomicBool>,
    tick: Arc<AtomicU64>,
    finished: Arc<AtomicBool>,
    started: Instant,
    human_state_source: String,
}

struct AppState {
    root: PathBuf,
    artifacts: PathBuf,
    token: Option<String>,
    bus: EventBus,
    live: tokio::sync::Mutex<Option<LiveSession>>,
    latest_ingest: IngestSlot,
    live_sources: Mutex<BTreeMap<String, Instant>>,
    ingest_policy: IngestPolicy,
    counters: Counters,
    started: Instant,
    tick_ms: u64,
    last_timing: Mutex<Option<crate::record::Timing>>,
}

type Shared = Arc<AppState>;
/// Latest human-state datum received on /ingest, with its arrival time.
type IngestSlot = Arc<Mutex<Option<(HumanStateDatum, Instant)>>>;

fn error(status: StatusCode, message: impl Into<String>) -> Response {
    (status, Json(json!({ "error": message.into() }))).into_response()
}

fn authorized(state: &AppState, headers: &HeaderMap, query: &BTreeMap<String, String>) -> bool {
    let Some(token) = &state.token else {
        return true;
    };
    let bearer = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));
    let supplied = bearer.or(query.get("token").map(String::as_str));
    // Constant-time comparison.
    match supplied {
        Some(candidate) if candidate.len() == token.len() => {
            candidate
                .bytes()
                .zip(token.bytes())
                .fold(0u8, |acc, (a, b)| acc | (a ^ b))
                == 0
        }
        _ => false,
    }
}

macro_rules! guard {
    ($state:expr, $headers:expr, $query:expr) => {
        if !authorized(&$state, &$headers, &$query) {
            return error(StatusCode::UNAUTHORIZED, "missing or invalid API token");
        }
    };
}

pub async fn serve(cli: &Cli, args: &ServeArgs) -> Result<i32, String> {
    let token = std::env::var("MESH_API_TOKEN")
        .ok()
        .filter(|t| !t.trim().is_empty());
    if !args.bind.ip().is_loopback() && token.is_none() {
        return Err(format!(
            "refusing to bind {} without MESH_API_TOKEN: a non-loopback address would expose run control to the network",
            args.bind
        ));
    }
    let state: Shared = Arc::new(AppState {
        root: cli.root.clone(),
        artifacts: cli.artifacts.clone(),
        token,
        bus: EventBus::new(4096),
        live: tokio::sync::Mutex::new(None),
        latest_ingest: Arc::new(Mutex::new(None)),
        live_sources: Mutex::new(BTreeMap::new()),
        ingest_policy: IngestPolicy::from_env(),
        counters: Counters::default(),
        started: Instant::now(),
        tick_ms: args.tick_ms.clamp(10, 5_000),
        last_timing: Mutex::new(None),
    });
    let origins: Vec<HeaderValue> = args
        .allow_origin
        .iter()
        .filter_map(|origin| HeaderValue::from_str(origin).ok())
        .collect();
    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::list(origins))
        .allow_methods([Method::GET, Method::POST])
        .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE]);
    let app = Router::new()
        .route("/health", get(health))
        .route("/metrics", get(prometheus))
        .route("/api/capabilities", get(api_capabilities))
        .route("/api/scenarios", get(api_scenarios))
        .route("/api/experiments", get(api_experiments))
        .route("/api/experiments/:id/summary", get(api_experiment_summary))
        .route("/api/experiments/:id/run", post(api_experiment_run))
        .route("/api/experiments/:id/files/:file", get(api_experiment_file))
        .route("/api/metrics/definitions", get(api_metric_definitions))
        .route("/api/topology", get(api_topology))
        .route("/api/claims", get(api_claims))
        .route("/api/runs", get(api_runs).post(api_create_run))
        .route("/api/runs/:id/files/:file", get(api_run_file))
        .route("/api/runs/:id/replay", post(api_replay))
        .route("/api/runs/:id/counterfactual", post(api_counterfactual))
        .route("/api/runs/:id/explain/:target", get(api_explain))
        .route("/api/live", get(api_live_status))
        .route("/api/live/start", post(api_live_start))
        .route("/api/live/stop", post(api_live_stop))
        .route("/api/live/command", post(api_live_command))
        .route("/ws", get(ws_events))
        .route("/ingest", get(ws_ingest))
        .layer(DefaultBodyLimit::max(256 * 1024))
        .layer(cors)
        .with_state(state.clone());
    let listener = tokio::net::TcpListener::bind(args.bind)
        .await
        .map_err(|e| format!("cannot bind {}: {e}", args.bind))?;
    eprintln!(
        "mesh serve listening on http://{} (token {}; origins {})",
        args.bind,
        if state.token.is_some() {
            "required"
        } else {
            "not required on loopback"
        },
        args.allow_origin.join(", ")
    );
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
        .map_err(|e| e.to_string())?;
    Ok(0)
}

async fn health(State(state): State<Shared>) -> Response {
    let live = state.live.lock().await;
    Json(json!({
        "status": "ok",
        "service": "mesh",
        "software_version": event_bus::SOFTWARE_VERSION,
        "uptime_s": state.started.elapsed().as_secs(),
        "live_session": live.as_ref().map(|s| json!({
            "run_id": s.run_id,
            "tick": s.tick.load(Ordering::Relaxed),
            "running": !s.finished.load(Ordering::Relaxed),
        })),
        "components": {
            "kernel": "in-process",
            "judgment": if std::env::var(typed_judgment::API_KEY_ENV).is_ok() { "remote available (opt-in per run)" } else { "local only" },
            "event_bus": { "subscribers": state.bus.subscriber_count(), "queue_depth": state.bus.queue_depth() },
        },
    }))
    .into_response()
}

async fn prometheus(State(state): State<Shared>) -> Response {
    use std::fmt::Write as _;
    let c = &state.counters;
    let mut out = String::new();
    let mut metric = |name: &str, kind: &str, help: &str, value: f64| {
        let _ = writeln!(
            out,
            "# HELP {name} {help}\n# TYPE {name} {kind}\n{name} {value}"
        );
    };
    metric(
        "mesh_uptime_seconds",
        "gauge",
        "Seconds since the server started.",
        state.started.elapsed().as_secs_f64(),
    );
    metric(
        "mesh_events_published_total",
        "counter",
        "Events published on the live bus.",
        state.bus.published_count() as f64,
    );
    metric(
        "mesh_events_streamed_total",
        "counter",
        "Events delivered to WebSocket clients.",
        c.events_streamed.load(Ordering::Relaxed) as f64,
    );
    metric(
        "mesh_ws_dropped_events_total",
        "counter",
        "Events skipped by slow WebSocket clients (lagged).",
        c.ws_lagged.load(Ordering::Relaxed) as f64,
    );
    metric(
        "mesh_event_bus_queue_depth",
        "gauge",
        "Events queued for the slowest bus subscriber.",
        state.bus.queue_depth() as f64,
    );
    metric(
        "mesh_ingest_accepted_total",
        "counter",
        "Ingest messages admitted.",
        c.ingest_accepted.load(Ordering::Relaxed) as f64,
    );
    metric(
        "mesh_runs_total",
        "counter",
        "Runs executed through the API.",
        c.runs.load(Ordering::Relaxed) as f64,
    );
    metric(
        "mesh_replays_total",
        "counter",
        "Replays executed through the API.",
        c.replays.load(Ordering::Relaxed) as f64,
    );
    metric(
        "mesh_replay_divergences_total",
        "counter",
        "Replays that did not verify.",
        c.replay_divergences.load(Ordering::Relaxed) as f64,
    );
    metric(
        "mesh_live_ticks_total",
        "counter",
        "Ticks executed by live sessions.",
        c.live_ticks.load(Ordering::Relaxed) as f64,
    );
    if let Some(timing) = state.last_timing.lock().expect("timing lock").clone() {
        metric(
            "mesh_last_run_ticks_per_second",
            "gauge",
            "Simulation tick rate of the last completed run.",
            timing.ticks_per_second,
        );
        metric(
            "mesh_last_run_pipeline_latency_p50_ms",
            "gauge",
            "Median kernel pipeline latency per proposal in the last run.",
            timing.pipeline_latency_ms_p50,
        );
        metric(
            "mesh_last_run_pipeline_latency_p95_ms",
            "gauge",
            "95th percentile kernel pipeline latency per proposal in the last run.",
            timing.pipeline_latency_ms_p95,
        );
    }
    let _ = writeln!(out, "# HELP mesh_ingest_rejected_total Ingest messages refused, by reason.\n# TYPE mesh_ingest_rejected_total counter");
    for (reason, count) in c.ingest_rejected.lock().expect("counter lock").iter() {
        let _ = writeln!(
            out,
            "mesh_ingest_rejected_total{{reason=\"{reason}\"}} {count}"
        );
    }
    ([(header::CONTENT_TYPE, "text/plain; version=0.0.4")], out).into_response()
}

fn live_facts(state: &AppState, running: bool) -> LiveFacts {
    let recent: Vec<String> = state
        .live_sources
        .lock()
        .expect("sources lock")
        .iter()
        .filter(|(_, seen)| seen.elapsed().as_secs() < 10)
        .map(|(source, _)| source.clone())
        .collect();
    LiveFacts {
        server: true,
        live_sources_connected: recent,
        live_session_running: running,
        static_export: false,
    }
}

async fn api_capabilities(
    State(state): State<Shared>,
    headers: HeaderMap,
    Query(q): Query<BTreeMap<String, String>>,
) -> Response {
    guard!(state, headers, q);
    let running = state
        .live
        .lock()
        .await
        .as_ref()
        .is_some_and(|s| !s.finished.load(Ordering::Relaxed));
    let report = capabilities::discover(
        &Probe {
            root: state.root.clone(),
            artifacts: state.artifacts.clone(),
        },
        &live_facts(&state, running),
    );
    Json(report).into_response()
}

async fn api_scenarios(
    State(state): State<Shared>,
    headers: HeaderMap,
    Query(q): Query<BTreeMap<String, String>>,
) -> Response {
    guard!(state, headers, q);
    let list: Vec<Value> = capabilities::scenario_files(&state.root)
        .iter()
        .filter_map(|path| config::load_scenario(path).ok())
        .map(|s| {
            json!({
                "id": s.id,
                "description": s.description,
                "agents": s.agents.len(),
                "ticks": s.ticks,
                "faults": s.faults.iter().map(|f| f.label()).collect::<Vec<_>>(),
                "judgment": s.kernel.judgment.provider,
            })
        })
        .collect();
    Json(list).into_response()
}

async fn api_experiments(
    State(state): State<Shared>,
    headers: HeaderMap,
    Query(q): Query<BTreeMap<String, String>>,
) -> Response {
    guard!(state, headers, q);
    let list: Vec<Value> = capabilities::manifest_files(&state.root)
        .iter()
        .filter_map(|path| config::load_manifest(path).ok())
        .map(|(m, _)| {
            let has_summary = state.artifacts.join("experiments").join(&m.id).join("summary.json").is_file();
            json!({
                "id": m.id,
                "title": m.title,
                "question": m.question.trim(),
                "hypothesis": m.hypothesis.statement.trim(),
                "prediction": m.hypothesis.prediction,
                "repetitions": m.repetitions,
                "conditions": m.conditions.iter().map(|c| json!({"id": c.id, "description": c.description, "set": c.set, "requires": c.requires})).collect::<Vec<_>>(),
                "has_summary": has_summary,
            })
        })
        .collect();
    Json(list).into_response()
}

/// Overrides from a network client may tune a scenario but may not choose a
/// program to execute (an external agent's `command`) or a file to read (a
/// recorded judge's `recorded_from`). Checked on the resolved scenario, so no
/// spelling of the override path gets around it.
fn guard_overrides(base: &config::Scenario, set: &BTreeMap<String, Value>) -> Result<(), String> {
    let mut fields = set.clone();
    fields.remove(config::REMOVE_AGENTS_KEY);
    let candidate: config::Scenario =
        config::apply_overrides(base, &fields).map_err(|e| e.to_string())?;
    for agent in &candidate.agents {
        let launches = agent.behavior == config::Behavior::External || !agent.command.is_empty();
        let unchanged = base.agents.iter().any(|original| {
            original.id == agent.id
                && original.behavior == agent.behavior
                && original.command == agent.command
        });
        if launches && !unchanged {
            return Err(format!(
                "the API cannot change which program agent `{}` runs; edit the scenario file instead",
                agent.id
            ));
        }
    }
    if candidate.kernel.judgment.recorded_from != base.kernel.judgment.recorded_from {
        return Err("the API cannot point a recorded judge at a file".into());
    }
    Ok(())
}

fn guard_scenario_file(
    path: &std::path::Path,
    set: &BTreeMap<String, Value>,
) -> Result<(), String> {
    let base = config::load_scenario(path).map_err(|e| e.to_string())?;
    guard_overrides(&base, set)
}

fn scenario_path(state: &AppState, id: &str) -> Option<PathBuf> {
    if !config::valid_id(id) {
        return None;
    }
    let path = state.root.join("scenarios").join(format!("{id}.yaml"));
    path.is_file().then_some(path)
}

fn manifest_path(state: &AppState, id: &str) -> Option<PathBuf> {
    if !config::valid_id(id) {
        return None;
    }
    let path = state
        .root
        .join("experiments")
        .join(id)
        .join("manifest.yaml");
    path.is_file().then_some(path)
}

async fn api_experiment_summary(
    State(state): State<Shared>,
    headers: HeaderMap,
    Query(q): Query<BTreeMap<String, String>>,
    UrlPath(id): UrlPath<String>,
) -> Response {
    guard!(state, headers, q);
    if !config::valid_id(&id) {
        return error(StatusCode::BAD_REQUEST, "invalid experiment id");
    }
    let path = state
        .artifacts
        .join("experiments")
        .join(&id)
        .join("summary.json");
    match crate::record::read_json::<Value>(&path) {
        Ok(summary) => Json(summary).into_response(),
        Err(_) => error(
            StatusCode::NOT_FOUND,
            format!("experiment `{id}` has not been run"),
        ),
    }
}

async fn api_experiment_file(
    State(state): State<Shared>,
    headers: HeaderMap,
    Query(q): Query<BTreeMap<String, String>>,
    UrlPath((id, file)): UrlPath<(String, String)>,
) -> Response {
    guard!(state, headers, q);
    if !config::valid_id(&id) || !EXPERIMENT_FILES.contains(&file.as_str()) {
        return error(StatusCode::NOT_FOUND, "not an experiment file");
    }
    let path = state.artifacts.join("experiments").join(&id).join(&file);
    match crate::record::read_text(&path) {
        Ok(text) => {
            let kind = match file.rsplit('.').next() {
                Some("md") => "text/markdown; charset=utf-8",
                Some("csv") => "text/csv; charset=utf-8",
                _ => "application/json",
            };
            ([(header::CONTENT_TYPE, kind)], text).into_response()
        }
        Err(_) => error(
            StatusCode::NOT_FOUND,
            format!("experiment `{id}` has not been run"),
        ),
    }
}

async fn api_metric_definitions(
    State(state): State<Shared>,
    headers: HeaderMap,
    Query(q): Query<BTreeMap<String, String>>,
) -> Response {
    guard!(state, headers, q);
    Json(crate::metrics::definitions_json()).into_response()
}

#[derive(Deserialize)]
struct ExperimentRunRequest {
    #[serde(default)]
    repetitions: Option<u32>,
}

async fn api_experiment_run(
    State(state): State<Shared>,
    headers: HeaderMap,
    Query(q): Query<BTreeMap<String, String>>,
    UrlPath(id): UrlPath<String>,
    Json(request): Json<ExperimentRunRequest>,
) -> Response {
    guard!(state, headers, q);
    let Some(path) = manifest_path(&state, &id) else {
        return error(StatusCode::NOT_FOUND, "unknown experiment");
    };
    let (manifest, scenario) = match config::load_manifest(&path) {
        Ok(loaded) => loaded,
        Err(e) => return error(StatusCode::BAD_REQUEST, e.to_string()),
    };
    let out_dir = state.artifacts.join("experiments").join(&id);
    let reps = request.repetitions.map(|r| r.clamp(1, 1000));
    let manifest_rel = path
        .strip_prefix(&state.root)
        .unwrap_or(&path)
        .to_path_buf();
    let outcome = tokio::task::spawn_blocking(move || {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| e.to_string())?
            .block_on(crate::experiment::run_experiment(
                &manifest,
                &scenario,
                &crate::experiment::ExperimentOptions {
                    out_dir,
                    repetitions: reps,
                    keep: crate::experiment::Keep::First,
                    allow_network: false,
                    manifest_path: manifest_rel,
                    force: true,
                    progress: false,
                },
            ))
    })
    .await;
    match outcome {
        Ok(Ok(outcome)) => Json(outcome.summary).into_response(),
        Ok(Err(e)) => error(StatusCode::BAD_REQUEST, e),
        Err(e) => error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

async fn api_topology(
    State(state): State<Shared>,
    headers: HeaderMap,
    Query(q): Query<BTreeMap<String, String>>,
) -> Response {
    guard!(state, headers, q);
    let scenario = q
        .get("scenario")
        .map(String::as_str)
        .unwrap_or("perturbed-mesh");
    let Some(path) = scenario_path(&state, scenario) else {
        return error(StatusCode::NOT_FOUND, "unknown scenario");
    };
    let target = match crate::cli::load_target(&path) {
        Ok(t) => t,
        Err(e) => return error(StatusCode::BAD_REQUEST, e),
    };
    match crate::cli::resolve_target(&target, None, 0, None, &BTreeMap::new()) {
        Ok(config) => Json(crate::topology::from_config(&config, None, None, true)).into_response(),
        Err(e) => error(StatusCode::BAD_REQUEST, e),
    }
}

async fn api_claims(
    State(state): State<Shared>,
    headers: HeaderMap,
    Query(q): Query<BTreeMap<String, String>>,
) -> Response {
    guard!(state, headers, q);
    match crate::claims::check_claims(&state.root.join("claims"), &state.artifacts) {
        Ok(results) => Json(results).into_response(),
        Err(e) => error(StatusCode::INTERNAL_SERVER_ERROR, e),
    }
}

async fn api_runs(
    State(state): State<Shared>,
    headers: HeaderMap,
    Query(q): Query<BTreeMap<String, String>>,
) -> Response {
    guard!(state, headers, q);
    let runs: Vec<Value> = capabilities::recorded_runs(&state.artifacts)
        .into_iter()
        .take(200)
        .filter_map(|(dir, _)| {
            let replay: crate::record::ReplayInfo =
                crate::record::read_json(&dir.join("replay.json")).ok()?;
            let provenance: Value = crate::record::read_json(&dir.join("provenance.json")).ok()?;
            Some(json!({
                "id": replay.run_id,
                "events": replay.event_count,
                "ticks": replay.ticks_run,
                "termination": replay.termination,
                "head_hash": replay.head_hash,
                "parent": provenance["parent"],
                "has_divergence": dir.join("divergence.json").is_file(),
            }))
        })
        .collect();
    Json(runs).into_response()
}

fn run_dir(state: &AppState, id: &str) -> Result<PathBuf, (StatusCode, String)> {
    if !config::valid_run_id(id) {
        return Err((StatusCode::BAD_REQUEST, "invalid run id".to_string()));
    }
    crate::record::resolve_run_dir(id, &state.artifacts)
        .map_err(|e| (StatusCode::NOT_FOUND, e.to_string()))
}

async fn api_run_file(
    State(state): State<Shared>,
    headers: HeaderMap,
    Query(q): Query<BTreeMap<String, String>>,
    UrlPath((id, file)): UrlPath<(String, String)>,
) -> Response {
    guard!(state, headers, q);
    if !SERVED_FILES.contains(&file.as_str()) {
        return error(StatusCode::NOT_FOUND, "not a bundle file");
    }
    let dir = match run_dir(&state, &id) {
        Ok(dir) => dir,
        Err((status, message)) => return error(status, message),
    };
    match crate::record::read_text(&dir.join(&file)) {
        Ok(text) => {
            let kind = if file.ends_with(".md") {
                "text/markdown; charset=utf-8"
            } else if file.ends_with(".jsonl") {
                "application/x-ndjson"
            } else {
                "application/json"
            };
            ([(header::CONTENT_TYPE, kind)], text).into_response()
        }
        Err(_) => error(StatusCode::NOT_FOUND, "file not present in this bundle"),
    }
}

#[derive(Deserialize)]
struct CreateRun {
    scenario: String,
    #[serde(default)]
    seed: Option<u64>,
    #[serde(default)]
    set: BTreeMap<String, Value>,
}

async fn api_create_run(
    State(state): State<Shared>,
    headers: HeaderMap,
    Query(q): Query<BTreeMap<String, String>>,
    Json(request): Json<CreateRun>,
) -> Response {
    guard!(state, headers, q);
    let Some(path) = scenario_path(&state, &request.scenario) else {
        return error(StatusCode::NOT_FOUND, "unknown scenario");
    };
    if let Err(e) = guard_scenario_file(&path, &request.set) {
        return error(StatusCode::FORBIDDEN, e);
    }
    let target = match crate::cli::load_target(&path) {
        Ok(t) => t,
        Err(e) => return error(StatusCode::BAD_REQUEST, e),
    };
    let config = match crate::cli::resolve_target(&target, None, 0, request.seed, &request.set) {
        Ok(c) => c,
        Err(e) => return error(StatusCode::BAD_REQUEST, e),
    };
    let result = match crate::runner::run(&config, RunOptions::default()).await {
        Ok(result) => result,
        Err(e) => return error(StatusCode::BAD_REQUEST, e.to_string()),
    };
    state.counters.runs.fetch_add(1, Ordering::Relaxed);
    *state.last_timing.lock().expect("timing lock") = Some(result.timing.clone());
    let dir = state.artifacts.join("runs").join(&config.run_id);
    let reproduce = vec![crate::cli::reproduce_command(
        path.strip_prefix(&state.root).unwrap_or(&path),
        &config,
        false,
    )];
    if let Err(e) = crate::bundle::save_run(
        &result,
        &dir,
        crate::bundle::SaveOptions {
            force: true,
            command: vec!["mesh".into(), "serve".into(), "POST /api/runs".into()],
            reproduce,
            parent: None,
            started_at_wall: chrono::Utc::now().to_rfc3339(),
            limitations: &[],
        },
    ) {
        return error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
    }
    Json(json!({ "run_id": config.run_id, "head_hash": result.head_hash, "metrics": result.metrics })).into_response()
}

async fn api_replay(
    State(state): State<Shared>,
    headers: HeaderMap,
    Query(q): Query<BTreeMap<String, String>>,
    UrlPath(id): UrlPath<String>,
) -> Response {
    guard!(state, headers, q);
    let dir = match run_dir(&state, &id) {
        Ok(dir) => dir,
        Err((status, message)) => return error(status, message),
    };
    let bundle = match crate::record::Bundle::load(&dir) {
        Ok(b) => b,
        Err(e) => return error(StatusCode::BAD_REQUEST, e.to_string()),
    };
    let report = crate::replay::replay_bundle(&bundle).await;
    state.counters.replays.fetch_add(1, Ordering::Relaxed);
    if !report.verified {
        state
            .counters
            .replay_divergences
            .fetch_add(1, Ordering::Relaxed);
    }
    Json(json!({ "report": report, "text": crate::replay::render(&report) })).into_response()
}

#[derive(Deserialize)]
struct CounterfactualRequest {
    set: BTreeMap<String, Value>,
    #[serde(default = "default_label")]
    label: String,
}

fn default_label() -> String {
    "cf".into()
}

async fn api_counterfactual(
    State(state): State<Shared>,
    headers: HeaderMap,
    Query(q): Query<BTreeMap<String, String>>,
    UrlPath(id): UrlPath<String>,
    Json(request): Json<CounterfactualRequest>,
) -> Response {
    guard!(state, headers, q);
    let dir = match run_dir(&state, &id) {
        Ok(dir) => dir,
        Err((status, message)) => return error(status, message),
    };
    let bundle = match crate::record::Bundle::load(&dir) {
        Ok(b) => b,
        Err(e) => return error(StatusCode::BAD_REQUEST, e.to_string()),
    };
    if let Err(e) = guard_overrides(&bundle.config.scenario, &request.set) {
        return error(StatusCode::FORBIDDEN, e);
    }
    let branch = match crate::counterfactual::run_branch(
        &bundle,
        request.set.clone(),
        &request.label,
        false,
    )
    .await
    {
        Ok(branch) => branch,
        Err(e) => return error(StatusCode::BAD_REQUEST, e),
    };
    let branch_dir = state
        .artifacts
        .join("runs")
        .join(&branch.result.config.run_id);
    let saved = crate::bundle::save_run(
        &branch.result,
        &branch_dir,
        crate::bundle::SaveOptions {
            force: true,
            command: vec!["mesh".into(), "serve".into(), "POST counterfactual".into()],
            reproduce: vec![format!(
                "mesh counterfactual {id} --label {}",
                request.label
            )],
            parent: Some(branch.parent.clone()),
            started_at_wall: chrono::Utc::now().to_rfc3339(),
            limitations: &[],
        },
    )
    .and_then(|_| {
        crate::record::write_json(&branch_dir.join("divergence.json"), &branch.comparison)
    });
    if let Err(e) = saved {
        return error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
    }
    Json(json!({ "run_id": branch.result.config.run_id, "comparison": branch.comparison }))
        .into_response()
}

async fn api_explain(
    State(state): State<Shared>,
    headers: HeaderMap,
    Query(q): Query<BTreeMap<String, String>>,
    UrlPath((id, target)): UrlPath<(String, String)>,
) -> Response {
    guard!(state, headers, q);
    let dir = match run_dir(&state, &id) {
        Ok(dir) => dir,
        Err((status, message)) => return error(status, message),
    };
    let events: Vec<event_bus::ChainedEvent> =
        match crate::record::read_jsonl(&dir.join("events.jsonl")) {
            Ok(events) => events,
            Err(e) => return error(StatusCode::BAD_REQUEST, e.to_string()),
        };
    match crate::explain::explain(&events, &target) {
        Some(explanation) => Json(explanation).into_response(),
        None => error(
            StatusCode::NOT_FOUND,
            "no event, proposal, or correlation matches",
        ),
    }
}

struct LiveControl {
    commands: mpsc::UnboundedReceiver<ControlCommand>,
    stop: Arc<AtomicBool>,
    tick: Arc<AtomicU64>,
    ingest: Option<IngestSlot>,
    bus: EventBus,
    run_id: String,
}

impl RunControl for LiveControl {
    fn commands(&mut self, _tick: u64) -> Vec<ControlCommand> {
        let mut out = Vec::new();
        while let Ok(command) = self.commands.try_recv() {
            out.push(command);
        }
        out
    }

    fn on_tick(&mut self, tick: u64, _events: &[event_bus::ChainedEvent]) {
        self.tick.store(tick, Ordering::Relaxed);
        let mut status = event_bus::EventEnvelope::new(
            "session_status",
            "mesh.serve",
            &self.run_id,
            &self.run_id,
            &self.run_id,
            "mesh.serve.live",
            json!({ "tick": tick, "running": true }),
        );
        status.run_id = self.run_id.clone();
        self.bus.publish(status);
    }

    fn should_stop(&self) -> bool {
        self.stop.load(Ordering::Relaxed)
    }

    fn human_state(&mut self, _tick: u64) -> Option<HumanStateDatum> {
        let slot = self.ingest.as_ref()?;
        let latest = slot.lock().expect("ingest lock");
        // Data older than two seconds is not presented as current.
        latest
            .as_ref()
            .filter(|(_, at)| at.elapsed().as_millis() < 2_000)
            .map(|(datum, _)| datum.clone())
    }
}

#[derive(Deserialize)]
struct LiveStart {
    scenario: String,
    #[serde(default)]
    seed: Option<u64>,
    #[serde(default)]
    set: BTreeMap<String, Value>,
    #[serde(default)]
    tick_ms: Option<u64>,
    /// `simulated` (default) or `ingest` (use data from /ingest).
    #[serde(default)]
    human_state: Option<String>,
}

async fn api_live_status(
    State(state): State<Shared>,
    headers: HeaderMap,
    Query(q): Query<BTreeMap<String, String>>,
) -> Response {
    guard!(state, headers, q);
    let live = state.live.lock().await;
    Json(match live.as_ref() {
        Some(session) => json!({
            "run_id": session.run_id,
            "tick": session.tick.load(Ordering::Relaxed),
            "running": !session.finished.load(Ordering::Relaxed),
            "elapsed_s": session.started.elapsed().as_secs(),
            "human_state_source": session.human_state_source,
            "scenario": session.scenario,
        }),
        None => json!({ "running": false }),
    })
    .into_response()
}

async fn api_live_start(
    State(state): State<Shared>,
    headers: HeaderMap,
    Query(q): Query<BTreeMap<String, String>>,
    Json(request): Json<LiveStart>,
) -> Response {
    guard!(state, headers, q);
    let mut live = state.live.lock().await;
    if live
        .as_ref()
        .is_some_and(|s| !s.finished.load(Ordering::Relaxed))
    {
        return error(StatusCode::CONFLICT, "a live session is already running");
    }
    let Some(path) = scenario_path(&state, &request.scenario) else {
        return error(StatusCode::NOT_FOUND, "unknown scenario");
    };
    if let Err(e) = guard_scenario_file(&path, &request.set) {
        return error(StatusCode::FORBIDDEN, e);
    }
    let mut set = request.set.clone();
    // A person at the console decides reviews unless the request says otherwise.
    set.entry("human_review.mode".to_string())
        .or_insert(json!("manual"));
    let target = match crate::cli::load_target(&path) {
        Ok(t) => t,
        Err(e) => return error(StatusCode::BAD_REQUEST, e),
    };
    let mut config = match crate::cli::resolve_target(&target, None, 0, request.seed, &set) {
        Ok(c) => c,
        Err(e) => return error(StatusCode::BAD_REQUEST, e),
    };
    config.run_id = format!(
        "live.{}.{}",
        config.scenario.id,
        chrono::Utc::now().format("%Y%m%dT%H%M%S")
    );
    config.condition_id = "live".to_string();
    let use_ingest = request.human_state.as_deref() == Some("ingest");
    let (tx, rx) = mpsc::unbounded_channel();
    let stop = Arc::new(AtomicBool::new(false));
    let tick = Arc::new(AtomicU64::new(0));
    let finished = Arc::new(AtomicBool::new(false));
    let control = LiveControl {
        commands: rx,
        stop: stop.clone(),
        tick: tick.clone(),
        ingest: use_ingest.then(|| state.latest_ingest.clone()),
        bus: state.bus.clone(),
        run_id: config.run_id.clone(),
    };
    let delay =
        std::time::Duration::from_millis(request.tick_ms.unwrap_or(state.tick_ms).clamp(10, 5_000));
    let worker_state = state.clone();
    let worker_finished = finished.clone();
    let run_config = config.clone();
    let source = path
        .strip_prefix(&state.root)
        .unwrap_or(&path)
        .to_path_buf();
    tokio::spawn(async move {
        let options = RunOptions {
            bus: Some(worker_state.bus.clone()),
            control: Some(Box::new(control)),
            tick_delay: Some(delay),
            ..RunOptions::default()
        };
        match crate::runner::run(&run_config, options).await {
            Ok(result) => {
                worker_state
                    .counters
                    .live_ticks
                    .fetch_add(result.ticks_run, Ordering::Relaxed);
                *worker_state.last_timing.lock().expect("timing lock") =
                    Some(result.timing.clone());
                let dir = worker_state.artifacts.join("runs").join(&run_config.run_id);
                let _ = crate::bundle::save_run(
                    &result,
                    &dir,
                    crate::bundle::SaveOptions {
                        force: true,
                        command: vec!["mesh".into(), "serve".into(), "live".into()],
                        reproduce: vec![format!(
                            "{}  # live sessions replay with `mesh replay {}`",
                            crate::cli::reproduce_command(&source, &run_config, false),
                            run_config.run_id
                        )],
                        parent: None,
                        started_at_wall: chrono::Utc::now().to_rfc3339(),
                        limitations: &["Interactive session: operator commands were issued by a person and are replayed from the recording.".to_string()],
                    },
                );
            }
            Err(e) => tracing::warn!(error = %e, "live session failed"),
        }
        worker_finished.store(true, Ordering::Relaxed);
        let mut done = event_bus::EventEnvelope::new(
            "session_status",
            "mesh.serve",
            &run_config.run_id,
            &run_config.run_id,
            &run_config.run_id,
            "mesh.serve.live",
            json!({ "running": false, "bundle": format!("runs/{}", run_config.run_id) }),
        );
        done.run_id = run_config.run_id.clone();
        worker_state.bus.publish(done);
    });
    let scenario = serde_json::to_value(&config.scenario).unwrap_or(Value::Null);
    *live = Some(LiveSession {
        run_id: config.run_id.clone(),
        scenario: scenario.clone(),
        commands: tx,
        stop,
        tick,
        finished,
        started: Instant::now(),
        human_state_source: if use_ingest {
            "ingest".into()
        } else {
            "simulated".into()
        },
    });
    Json(json!({ "run_id": config.run_id, "tick_ms": delay.as_millis() as u64, "scenario": scenario }))
        .into_response()
}

async fn api_live_stop(
    State(state): State<Shared>,
    headers: HeaderMap,
    Query(q): Query<BTreeMap<String, String>>,
) -> Response {
    guard!(state, headers, q);
    let live = state.live.lock().await;
    match live.as_ref() {
        Some(session) => {
            session.stop.store(true, Ordering::Relaxed);
            Json(json!({ "stopping": session.run_id })).into_response()
        }
        None => error(StatusCode::NOT_FOUND, "no live session"),
    }
}

async fn api_live_command(
    State(state): State<Shared>,
    headers: HeaderMap,
    Query(q): Query<BTreeMap<String, String>>,
    Json(command): Json<ControlCommand>,
) -> Response {
    guard!(state, headers, q);
    if let ControlCommand::InjectFault { fault } = &command {
        if fault.duration_ticks == 0 || fault.duration_ticks > 1_000 || !fault.magnitude.is_finite()
        {
            return error(
                StatusCode::BAD_REQUEST,
                "fault duration must be 1..=1000 ticks and magnitude finite",
            );
        }
    }
    if let ControlCommand::OperatorProposal { agent_id, intent } = &command {
        if !config::valid_id(agent_id)
            || intent.rationale.len() > 512
            || intent.action_type.len() > 64
        {
            return error(StatusCode::BAD_REQUEST, "invalid operator proposal");
        }
    }
    let live = state.live.lock().await;
    match live.as_ref() {
        Some(session) if !session.finished.load(Ordering::Relaxed) => match session.commands.send(command) {
            Ok(()) => Json(json!({ "queued": true, "applies_at_tick": session.tick.load(Ordering::Relaxed) + 1 })).into_response(),
            Err(_) => error(StatusCode::GONE, "live session has ended"),
        },
        _ => error(StatusCode::NOT_FOUND, "no live session is running"),
    }
}

async fn ws_events(
    State(state): State<Shared>,
    headers: HeaderMap,
    Query(q): Query<BTreeMap<String, String>>,
    upgrade: WebSocketUpgrade,
) -> Response {
    guard!(state, headers, q);
    upgrade
        .max_message_size(4 * 1024)
        .on_upgrade(move |socket| stream_events(socket, state))
}

async fn stream_events(mut socket: WebSocket, state: Shared) {
    let mut rx = state.bus.subscribe();
    loop {
        tokio::select! {
            received = rx.recv() => match received {
                Ok(event) => {
                    let text = json!({ "type": "event", "event": event }).to_string();
                    if socket.send(Message::Text(text)).await.is_err() {
                        break;
                    }
                    state.counters.events_streamed.fetch_add(1, Ordering::Relaxed);
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                    state.counters.ws_lagged.fetch_add(skipped, Ordering::Relaxed);
                    let note = json!({ "type": "lagged", "skipped": skipped }).to_string();
                    if socket.send(Message::Text(note)).await.is_err() {
                        break;
                    }
                }
                Err(_) => break,
            },
            incoming = socket.recv() => match incoming {
                // The event stream is read-only; anything other than close/ping is ignored.
                Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                Some(Ok(_)) => {}
            }
        }
    }
}

async fn ws_ingest(
    State(state): State<Shared>,
    headers: HeaderMap,
    Query(q): Query<BTreeMap<String, String>>,
    upgrade: WebSocketUpgrade,
) -> Response {
    guard!(state, headers, q);
    let limit = state.ingest_policy.max_message_bytes;
    upgrade
        .max_message_size(limit)
        .on_upgrade(move |socket| ingest(socket, state))
}

fn reject(state: &AppState, code: &str) {
    *state
        .counters
        .ingest_rejected
        .lock()
        .expect("counter lock")
        .entry(code.to_string())
        .or_insert(0) += 1;
}

async fn ingest(mut socket: WebSocket, state: Shared) {
    let started = Instant::now();
    let mut limiter = RateLimiter::new(state.ingest_policy.max_messages_per_second, 0);
    while let Some(Ok(message)) = socket.recv().await {
        let Message::Text(text) = message else {
            if matches!(message, Message::Close(_)) {
                break;
            }
            continue;
        };
        let now_ms = started.elapsed().as_millis() as i64;
        let verdict = limiter
            .try_acquire(now_ms)
            .and_then(|_| admit(&text, &state.ingest_policy));
        let reply = match verdict {
            Ok(admitted) => {
                state
                    .counters
                    .ingest_accepted
                    .fetch_add(1, Ordering::Relaxed);
                if let Some(datum) = &admitted.datum {
                    state
                        .live_sources
                        .lock()
                        .expect("sources lock")
                        .insert(datum.source.clone(), Instant::now());
                    *state.latest_ingest.lock().expect("ingest lock") =
                        Some((datum.clone(), Instant::now()));
                }
                let mut observed = admitted.envelope.clone();
                observed.source = format!("ingest:{}", observed.source);
                state.bus.publish(observed);
                json!({ "accepted": true, "event_id": admitted.envelope.event_id })
            }
            Err(e) => {
                reject(&state, e.code());
                json!({ "accepted": false, "code": e.code(), "error": e.to_string() })
            }
        };
        if socket.send(Message::Text(reply.to_string())).await.is_err() {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::guard_overrides;
    use serde_json::json;
    use std::collections::BTreeMap;

    fn scenario(name: &str) -> crate::config::Scenario {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios");
        crate::config::load_scenario(&root.join(format!("{name}.yaml"))).expect("scenario loads")
    }

    fn set(pairs: &[(&str, serde_json::Value)]) -> BTreeMap<String, serde_json::Value> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect()
    }

    #[test]
    fn ordinary_tuning_is_allowed() {
        let base = scenario("perturbed-mesh");
        assert!(guard_overrides(
            &base,
            &set(&[("kernel.judgment.provider", json!("disabled"))])
        )
        .is_ok());
        assert!(guard_overrides(
            &base,
            &set(&[
                ("ticks", json!(20)),
                ("kernel.validator.min_separation", json!(4))
            ])
        )
        .is_ok());
        assert!(guard_overrides(&base, &set(&[("agents.scout_01.speed", json!(2.0))])).is_ok());
    }

    #[test]
    fn no_override_can_choose_a_program_to_run() {
        let base = scenario("perturbed-mesh");
        let attempts = [
            set(&[
                ("agents.scout_01.behavior", json!("external")),
                ("agents.scout_01.command", json!(["sh", "-c", "id"])),
            ]),
            set(&[("agents.0.command", json!(["sh", "-c", "id"]))]),
            set(&[(
                "agents",
                json!([{"id": "x", "behavior": "external", "start": [0.0, 0.0], "command": ["sh"]}]),
            )]),
        ];
        for attempt in attempts {
            assert!(guard_overrides(&base, &attempt).is_err(), "{attempt:?}");
        }
        // An external agent defined in the scenario file itself still runs as written.
        let external = scenario("external-agent");
        assert!(guard_overrides(&external, &set(&[("ticks", json!(5))])).is_ok());
        let swapped = set(&[("agents.rain_01.command", json!(["sh", "-c", "id"]))]);
        assert!(guard_overrides(&external, &swapped).is_err());
    }

    #[test]
    fn no_override_can_choose_a_file_to_read() {
        let base = scenario("perturbed-mesh");
        let attempts = [
            set(&[("kernel.judgment.recorded_from", json!("/etc"))]),
            set(&[(
                "kernel.judgment",
                json!({"provider": "recorded", "recorded_from": "/etc"}),
            )]),
        ];
        for attempt in attempts {
            assert!(guard_overrides(&base, &attempt).is_err(), "{attempt:?}");
        }
    }
}
