//! Agents participate in the mesh through [`MeshAgentAdapter`].
//!
//! An adapter observes, proposes an *intent*, explains it, and receives the
//! kernel's outcome. It never sees authoritative state directly and cannot
//! commit anything: the runner turns an intent into an `ActionProposal` and
//! submits it to the Pordenone kernel, which validates and decides.
//!
//! Built-in behaviors are deterministic functions of their observations, the
//! outcomes they receive, and a seeded random stream. [`ExternalProcessAgent`]
//! connects any program (for example a R.A.I.N. deliberative agent) over a
//! JSON-lines protocol on stdin/stdout. Its intents are recorded, and replay
//! substitutes the recording instead of re-running the process.

use crate::config::{point, AgentSpec, Behavior, ScriptedIntent};
use epistemic_validator::{HazardZone, Vector3};
use event_bus::{quantize, SignalQuality};
use kernel_core::PolicyOutcome;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

pub const AGENT_PROTOCOL: &str = "mesh-agent/1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Neighbor {
    pub agent_id: String,
    pub position: Vector3,
}

/// What a sensor delivered to one agent.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentObservation {
    pub observation_id: String,
    pub tick: u64,
    pub observed_at: i64,
    pub own_position: Vector3,
    pub goal: Option<Vector3>,
    pub known_hazards: Vec<HazardZone>,
    pub neighbors: Vec<Neighbor>,
    pub quality: SignalQuality,
}

/// Published rules an agent may (or may not) respect.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Constraints {
    pub max_step: f64,
    pub min_separation: f64,
    pub allowed_actions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProposalContext {
    pub tick: u64,
    pub now_ms: i64,
    pub constraints: Constraints,
}

/// What an agent wants to do. The runner assigns ids and timestamps.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentIntent {
    pub action_type: String,
    pub target: Vector3,
    pub priority: i32,
    /// Free-text reason, recorded with the proposal. Never used for authority.
    pub rationale: String,
}

/// What the kernel did with the agent's last proposal.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProposalOutcome {
    pub proposal_id: String,
    pub outcome: PolicyOutcome,
    pub committed: bool,
    pub reasons: Vec<String>,
    pub committed_position: Option<Vector3>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentDescriptor {
    pub agent_id: String,
    pub adapter: String,
    pub deterministic: bool,
    pub networked: bool,
}

pub trait MeshAgentAdapter: Send {
    fn descriptor(&self) -> AgentDescriptor;
    fn observe(&mut self, observation: &AgentObservation);
    fn propose(&mut self, context: &ProposalContext) -> Option<AgentIntent>;
    /// Explanation of the most recent proposal.
    fn explain(&self) -> Option<String>;
    fn receive_outcome(&mut self, outcome: &ProposalOutcome);
    /// Non-fatal problems (timeouts, protocol errors) since the last call.
    fn take_faults(&mut self) -> Vec<String> {
        Vec::new()
    }
}

// Rotation constants for fixed detour angles. Literal values keep the
// simulation free of platform-dependent trigonometric functions.
const DETOURS: [(f64, f64); 9] = [
    (1.0, 0.0),
    (0.866_025_403_784_438_6, 0.5),
    (0.866_025_403_784_438_6, -0.5),
    (0.5, 0.866_025_403_784_438_6),
    (0.5, -0.866_025_403_784_438_6),
    (0.0, 1.0),
    (0.0, -1.0),
    (-0.5, 0.866_025_403_784_438_6),
    (-0.5, -0.866_025_403_784_438_6),
];

fn sub(a: &Vector3, b: &Vector3) -> Vector3 {
    Vector3::new(a.x - b.x, a.y - b.y, a.z - b.z)
}

fn add_scaled(a: &Vector3, dir: &Vector3, scale: f64) -> Vector3 {
    Vector3::new(
        a.x + dir.x * scale,
        a.y + dir.y * scale,
        a.z + dir.z * scale,
    )
}

fn length(v: &Vector3) -> f64 {
    (v.x * v.x + v.y * v.y + v.z * v.z).sqrt()
}

fn rotate(v: &Vector3, (cos, sin): (f64, f64)) -> Vector3 {
    Vector3::new(v.x * cos - v.y * sin, v.x * sin + v.y * cos, v.z)
}

pub fn q4(v: Vector3) -> Vector3 {
    Vector3::new(quantize(v.x, 4), quantize(v.y, 4), quantize(v.z, 4))
}

fn segment_clear(from: &Vector3, to: &Vector3, zone: &HazardZone, margin: f64) -> bool {
    epistemic_validator::segment_point_distance(from, to, &zone.center) >= zone.radius + margin
}

/// Deterministic step planner used by cooperative behaviors.
fn plan_step(
    position: &Vector3,
    goal: &Vector3,
    step_limit: f64,
    hazards: &[HazardZone],
    neighbors: &[Neighbor],
    min_separation: f64,
) -> Option<(Vector3, usize)> {
    let delta = sub(goal, position);
    let distance = length(&delta);
    if distance < 1e-9 {
        return None;
    }
    let direction = Vector3::new(delta.x / distance, delta.y / distance, 0.0);
    for scale in [1.0, 0.5] {
        let step = (step_limit * scale).min(distance);
        for (index, rotation) in DETOURS.iter().enumerate() {
            let heading = rotate(&direction, *rotation);
            let candidate = q4(add_scaled(position, &heading, step));
            let hazard_ok = hazards.iter().all(|zone| {
                let inside_now = zone.center.distance(position) < zone.radius;
                if inside_now {
                    zone.center.distance(&candidate) >= zone.radius + 0.5
                } else {
                    segment_clear(position, &candidate, zone, 0.75)
                }
            });
            let spacing_ok = neighbors
                .iter()
                .all(|other| other.position.distance(&candidate) >= min_separation + 0.5);
            if hazard_ok && spacing_ok {
                return Some((candidate, index));
            }
        }
    }
    None
}

struct Belief {
    position: Vector3,
    observation: Option<AgentObservation>,
}

/// Behaviors implemented in this repository.
pub struct BuiltinAgent {
    spec: AgentSpec,
    goal: Option<Vector3>,
    home: Vector3,
    belief: Belief,
    step_scale: f64,
    detour_ticks: u64,
    hold_ticks: u64,
    proposals_made: u64,
    patrol_outbound: bool,
    last_explanation: Option<String>,
    script: BTreeMap<u64, ScriptedIntent>,
    goal_radius: f64,
    awaiting_review: Option<String>,
}

impl BuiltinAgent {
    pub fn new(spec: &AgentSpec, goal: Option<Vector3>, goal_radius: f64) -> Self {
        let start = point(spec.start);
        Self {
            spec: spec.clone(),
            goal,
            home: start,
            belief: Belief {
                position: start,
                observation: None,
            },
            step_scale: 1.0,
            detour_ticks: 0,
            hold_ticks: 0,
            proposals_made: 0,
            patrol_outbound: true,
            last_explanation: None,
            script: spec
                .script
                .iter()
                .map(|intent| (intent.tick, intent.clone()))
                .collect(),
            goal_radius,
            awaiting_review: None,
        }
    }

    fn intent(
        &mut self,
        action: &str,
        target: Vector3,
        priority: i32,
        why: String,
    ) -> Option<AgentIntent> {
        self.proposals_made += 1;
        self.last_explanation = Some(why.clone());
        Some(AgentIntent {
            action_type: action.to_string(),
            target,
            priority,
            rationale: why,
        })
    }

    fn cooperative(
        &mut self,
        context: &ProposalContext,
        action: &str,
        priority: i32,
        speed: f64,
    ) -> Option<AgentIntent> {
        let observation = self.belief.observation.clone()?;
        let goal = self.current_goal()?;
        let position = self.belief.position;
        if position.distance(&goal) <= self.goal_radius * 0.5 {
            if self.spec.behavior == Behavior::Patrol {
                self.patrol_outbound = !self.patrol_outbound;
            } else {
                self.last_explanation = Some("at goal; holding without a proposal".into());
                return None;
            }
        }
        let goal = self.current_goal()?;
        let step_limit = speed.min(context.constraints.max_step * 0.95);
        match plan_step(
            &position,
            &goal,
            step_limit,
            &observation.known_hazards,
            &observation.neighbors,
            context.constraints.min_separation,
        ) {
            Some((target, detour)) => {
                let why = if detour == 0 {
                    format!("step toward goal ({:.1} away)", position.distance(&goal))
                } else {
                    format!("detour {detour} around a known hazard or neighbour")
                };
                self.intent(action, target, priority, why)
            }
            None => {
                self.last_explanation = Some("no clear step; holding".into());
                None
            }
        }
    }

    fn current_goal(&self) -> Option<Vector3> {
        match self.spec.behavior {
            Behavior::Patrol if !self.patrol_outbound => Some(self.home),
            _ => self.goal,
        }
    }
}

impl MeshAgentAdapter for BuiltinAgent {
    fn descriptor(&self) -> AgentDescriptor {
        AgentDescriptor {
            agent_id: self.spec.id.clone(),
            adapter: format!("builtin:{}", behavior_name(self.spec.behavior)),
            deterministic: true,
            networked: false,
        }
    }

    fn observe(&mut self, observation: &AgentObservation) {
        if observation.quality == SignalQuality::Invalid {
            // Built-in agents distrust invalid readings and keep their belief.
            return;
        }
        self.belief.position = observation.own_position;
        self.belief.observation = Some(observation.clone());
    }

    fn propose(&mut self, context: &ProposalContext) -> Option<AgentIntent> {
        if let Some(pending) = &self.awaiting_review {
            self.last_explanation = Some(format!("waiting for the human decision on {pending}"));
            return None;
        }
        if self.hold_ticks > 0 {
            self.hold_ticks -= 1;
            self.last_explanation = Some("waiting after a separation rejection".into());
            return None;
        }
        let priority = self.spec.priority;
        match self.spec.behavior {
            Behavior::Scripted => {
                let scripted = self.script.get(&context.tick)?.clone();
                self.intent(
                    &scripted.action_type,
                    point(scripted.target),
                    scripted.priority,
                    "scripted".into(),
                )
            }
            Behavior::Cautious => self.cooperative(context, "MOVE", priority, self.spec.speed),
            Behavior::Patrol => self.cooperative(context, "PATROL", 0, self.spec.speed),
            Behavior::Malformed => {
                let n = self.proposals_made + 1;
                if n.is_multiple_of(self.spec.malformed_every) {
                    let position = self.belief.position;
                    let (action, target, priority, why) = match (n / self.spec.malformed_every) % 5
                    {
                        0 => ("OVERRIDE", position, priority, "unknown action"),
                        1 => (
                            "MOVE",
                            Vector3::new(f64::NAN, position.y, 0.0),
                            priority,
                            "non-finite target",
                        ),
                        2 => (
                            "MOVE",
                            Vector3::new(1.0e6, 0.0, 0.0),
                            priority,
                            "far out of bounds",
                        ),
                        3 => ("MOVE", position, -1, "negative priority"),
                        _ => (
                            "MOVE",
                            add_scaled(&position, &Vector3::new(1.0, 0.0, 0.0), 500.0),
                            priority,
                            "teleport",
                        ),
                    };
                    self.intent(
                        action,
                        target,
                        priority,
                        format!("deliberately invalid: {why}"),
                    )
                } else {
                    self.cooperative(context, "MOVE", priority, self.spec.speed)
                }
            }
            Behavior::Greedy => {
                if self.detour_ticks > 0 {
                    self.detour_ticks -= 1;
                    let speed = self.spec.speed * self.step_scale;
                    return self.cooperative(context, "MOVE", priority, speed);
                }
                self.belief.observation.as_ref()?;
                let goal = self.goal?;
                let position = self.belief.position;
                let delta = sub(&goal, &position);
                let distance = length(&delta);
                if distance <= self.goal_radius * 0.5 {
                    self.last_explanation = Some("at goal".into());
                    return None;
                }
                let step = (self.spec.speed * self.step_scale).min(distance);
                let target = q4(add_scaled(
                    &position,
                    &Vector3::new(delta.x / distance, delta.y / distance, 0.0),
                    step,
                ));
                self.intent(
                    "MOVE",
                    target,
                    priority,
                    format!("straight line, step {step:.2}"),
                )
            }
            Behavior::Oscillating => {
                self.belief.observation.as_ref()?;
                let goal = self.goal?;
                let position = self.belief.position;
                let delta = sub(&goal, &position);
                let distance = length(&delta);
                if distance <= self.goal_radius * 0.25 {
                    self.last_explanation = Some("settled at goal".into());
                    return None;
                }
                let mut shift =
                    Vector3::new(delta.x * self.spec.gain, delta.y * self.spec.gain, 0.0);
                let shift_length = length(&shift);
                if shift_length > self.spec.speed {
                    let scale = self.spec.speed / shift_length;
                    shift = Vector3::new(shift.x * scale, shift.y * scale, 0.0);
                }
                let target = q4(add_scaled(&position, &shift, 1.0));
                self.intent(
                    "MOVE",
                    target,
                    priority,
                    format!("gain {:.2} correction", self.spec.gain),
                )
            }
            Behavior::External => None,
        }
    }

    fn explain(&self) -> Option<String> {
        self.last_explanation.clone()
    }

    fn receive_outcome(&mut self, outcome: &ProposalOutcome) {
        if outcome.outcome == PolicyOutcome::AwaitingHumanReview {
            self.awaiting_review = Some(outcome.proposal_id.clone());
        } else if self.awaiting_review.as_deref() == Some(outcome.proposal_id.as_str()) {
            self.awaiting_review = None;
        }
        if let Some(position) = outcome.committed_position {
            self.belief.position = position;
        }
        if self.spec.behavior == Behavior::Greedy && !outcome.committed {
            if outcome.reasons.iter().any(|r| r == "STEP_TOO_LARGE") {
                self.step_scale = (self.step_scale * 0.5).max(0.1);
            }
            if outcome.reasons.iter().any(|r| r == "HAZARD_INTERSECTION") {
                self.detour_ticks = 5;
            }
        }
        if outcome.reasons.iter().any(|r| r == "SEPARATION_VIOLATION") {
            self.hold_ticks = 1;
        }
    }
}

pub fn behavior_name(behavior: Behavior) -> &'static str {
    match behavior {
        Behavior::Cautious => "cautious",
        Behavior::Greedy => "greedy",
        Behavior::Oscillating => "oscillating",
        Behavior::Patrol => "patrol",
        Behavior::Malformed => "malformed",
        Behavior::Scripted => "scripted",
        Behavior::External => "external",
    }
}

/// What a non-deterministic agent did at one tick, as recorded.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RecordedTurn {
    pub intent: Option<AgentIntent>,
    pub explanation: Option<String>,
    pub faults: Vec<String>,
}

/// Replays an agent's recorded turns. Used in replay for agents that are not
/// deterministic (external processes); it reports the original descriptor so
/// the replayed events are identical, and replay lists the substitution.
pub struct RecordedAgent {
    descriptor: AgentDescriptor,
    turns: BTreeMap<u64, RecordedTurn>,
    tick: u64,
    pending_faults: Vec<String>,
}

impl RecordedAgent {
    pub fn new(descriptor: AgentDescriptor, turns: BTreeMap<u64, RecordedTurn>) -> Self {
        Self {
            descriptor,
            turns,
            tick: 0,
            pending_faults: Vec::new(),
        }
    }
}

impl MeshAgentAdapter for RecordedAgent {
    fn descriptor(&self) -> AgentDescriptor {
        self.descriptor.clone()
    }
    fn observe(&mut self, observation: &AgentObservation) {
        self.tick = observation.tick;
    }
    fn propose(&mut self, context: &ProposalContext) -> Option<AgentIntent> {
        self.tick = context.tick;
        let turn = self.turns.get(&context.tick).cloned().unwrap_or_default();
        self.pending_faults = turn.faults;
        turn.intent
    }
    fn explain(&self) -> Option<String> {
        self.turns
            .get(&self.tick)
            .and_then(|turn| turn.explanation.clone())
    }
    fn receive_outcome(&mut self, _outcome: &ProposalOutcome) {}
    fn take_faults(&mut self) -> Vec<String> {
        std::mem::take(&mut self.pending_faults)
    }
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ToAgent<'a> {
    Hello {
        protocol: &'a str,
        agent_id: &'a str,
    },
    Observe {
        observation: &'a AgentObservation,
    },
    Propose {
        context: &'a ProposalContext,
    },
    Outcome {
        outcome: &'a ProposalOutcome,
    },
    Shutdown,
}

#[derive(Deserialize)]
struct HelloReply {
    protocol: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    deterministic: bool,
}

#[derive(Deserialize)]
struct ProposeReply {
    intent: Option<AgentIntent>,
    #[serde(default)]
    explanation: Option<String>,
}

/// A deliberative agent in another process. Protocol (`mesh-agent/1`), one
/// JSON object per line:
///
/// ```text
/// -> {"type":"hello","protocol":"mesh-agent/1","agent_id":"rain_01"}
/// <- {"protocol":"mesh-agent/1","name":"rain-stub","deterministic":true}
/// -> {"type":"observe","observation":{...}}                 (no reply)
/// -> {"type":"propose","context":{...}}
/// <- {"intent":{"action_type":"MOVE","target":{"x":..,"y":..,"z":0},"priority":1,"rationale":".."} | null,
///     "explanation":".."}
/// -> {"type":"outcome","outcome":{...}}                     (no reply)
/// -> {"type":"shutdown"}
/// ```
///
/// A reply that does not arrive within the timeout counts as no proposal and
/// is reported as a fault. The process never talks to the kernel.
pub struct ExternalProcessAgent {
    agent_id: String,
    command: Vec<String>,
    child: Option<Child>,
    stdin: Option<ChildStdin>,
    lines: Option<Receiver<String>>,
    timeout: Duration,
    name: String,
    deterministic: bool,
    explanation: Option<String>,
    faults: Vec<String>,
}

impl ExternalProcessAgent {
    pub fn start(agent_id: &str, command: &[String], timeout_ms: u64) -> Self {
        let mut agent = Self {
            agent_id: agent_id.to_string(),
            command: command.to_vec(),
            child: None,
            stdin: None,
            lines: None,
            timeout: Duration::from_millis(timeout_ms.clamp(10, 60_000)),
            name: "unknown".into(),
            deterministic: false,
            explanation: None,
            faults: Vec::new(),
        };
        if let Err(error) = agent.spawn() {
            agent
                .faults
                .push(format!("agent process failed to start: {error}"));
        }
        agent
    }

    fn spawn(&mut self) -> Result<(), String> {
        let (program, args) = self.command.split_first().ok_or("empty command")?;
        let mut child = Command::new(program)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| error.to_string())?;
        let stdout = child.stdout.take().ok_or("no stdout")?;
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                if line.len() > 64 * 1024 || sender.send(line).is_err() {
                    break;
                }
            }
        });
        self.stdin = child.stdin.take();
        self.child = Some(child);
        self.lines = Some(receiver);
        let hello = serde_json::to_string(&ToAgent::Hello {
            protocol: AGENT_PROTOCOL,
            agent_id: &self.agent_id,
        })
        .map_err(|error| error.to_string())?;
        self.send_line(&hello)?;
        // Interpreter start-up can be slow on a loaded machine; allow more time
        // for the handshake than for each proposal.
        let reply = self
            .lines
            .as_ref()
            .and_then(|lines| {
                lines
                    .recv_timeout(self.timeout.max(Duration::from_secs(15)))
                    .ok()
            })
            .ok_or("no hello reply")?;
        let hello: HelloReply = serde_json::from_str(&reply).map_err(|error| error.to_string())?;
        if hello.protocol != AGENT_PROTOCOL {
            return Err(format!("unsupported protocol `{}`", hello.protocol));
        }
        self.name = hello.name.chars().take(64).collect();
        self.deterministic = hello.deterministic;
        Ok(())
    }

    fn send_line(&mut self, line: &str) -> Result<(), String> {
        let stdin = self.stdin.as_mut().ok_or("agent process is not running")?;
        stdin
            .write_all(line.as_bytes())
            .and_then(|_| stdin.write_all(b"\n"))
            .and_then(|_| stdin.flush())
            .map_err(|error| error.to_string())
    }

    fn read_line(&mut self) -> Option<String> {
        self.lines.as_ref()?.recv_timeout(self.timeout).ok()
    }

    fn send(&mut self, message: &ToAgent<'_>) {
        if self.stdin.is_none() {
            return;
        }
        let line = serde_json::to_string(message).expect("protocol messages serialize");
        if let Err(error) = self.send_line(&line) {
            self.faults.push(format!("agent write failed: {error}"));
            self.stdin = None;
        }
    }
}

impl MeshAgentAdapter for ExternalProcessAgent {
    fn descriptor(&self) -> AgentDescriptor {
        AgentDescriptor {
            agent_id: self.agent_id.clone(),
            adapter: format!("external:{}", self.name),
            // Determinism is self-reported; replay never relies on it.
            deterministic: false,
            networked: false,
        }
    }

    fn observe(&mut self, observation: &AgentObservation) {
        self.send(&ToAgent::Observe { observation });
    }

    fn propose(&mut self, context: &ProposalContext) -> Option<AgentIntent> {
        self.explanation = None;
        self.stdin.as_ref()?;
        self.send(&ToAgent::Propose { context });
        let Some(line) = self.read_line() else {
            self.faults
                .push("agent did not reply within the timeout".into());
            return None;
        };
        match serde_json::from_str::<ProposeReply>(&line) {
            Ok(reply) => {
                self.explanation = reply
                    .explanation
                    .map(|text| text.chars().take(512).collect());
                reply.intent.map(|mut intent| {
                    intent.rationale = intent.rationale.chars().take(512).collect();
                    intent
                })
            }
            Err(error) => {
                self.faults
                    .push(format!("agent reply was not valid: {error}"));
                None
            }
        }
    }

    fn explain(&self) -> Option<String> {
        self.explanation.clone()
    }

    fn receive_outcome(&mut self, outcome: &ProposalOutcome) {
        self.send(&ToAgent::Outcome { outcome });
    }

    fn take_faults(&mut self) -> Vec<String> {
        std::mem::take(&mut self.faults)
    }
}

impl Drop for ExternalProcessAgent {
    fn drop(&mut self) {
        self.send(&ToAgent::Shutdown);
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(behavior: Behavior) -> AgentSpec {
        serde_yaml::from_str(&format!(
            "{{id: a, behavior: {}, start: [0, 0], goal: g, speed: 4}}",
            behavior_name(behavior)
        ))
        .unwrap()
    }

    fn observation(position: Vector3, hazards: Vec<HazardZone>) -> AgentObservation {
        AgentObservation {
            observation_id: "obs".into(),
            tick: 0,
            observed_at: 0,
            own_position: position,
            goal: Some(Vector3::new(20.0, 0.0, 0.0)),
            known_hazards: hazards,
            neighbors: vec![],
            quality: SignalQuality::Good,
        }
    }

    fn context() -> ProposalContext {
        ProposalContext {
            tick: 0,
            now_ms: 0,
            constraints: Constraints {
                max_step: 10.0,
                min_separation: 2.0,
                allowed_actions: vec!["MOVE".into()],
            },
        }
    }

    #[test]
    fn cautious_agent_detours_around_a_known_hazard() {
        let mut agent = BuiltinAgent::new(
            &spec(Behavior::Cautious),
            Some(Vector3::new(20.0, 0.0, 0.0)),
            1.5,
        );
        let hazard = HazardZone {
            id: "h".into(),
            center: Vector3::new(4.0, 0.0, 0.0),
            radius: 2.0,
        };
        agent.observe(&observation(Vector3::ZERO, vec![hazard.clone()]));
        let intent = agent.propose(&context()).unwrap();
        assert!(
            epistemic_validator::segment_point_distance(
                &Vector3::ZERO,
                &intent.target,
                &hazard.center
            ) >= hazard.radius
        );
        assert!(agent.explain().unwrap().contains("detour"));
    }

    #[test]
    fn greedy_agent_learns_from_step_rejections() {
        let mut spec = spec(Behavior::Greedy);
        spec.speed = 16.0;
        let mut agent = BuiltinAgent::new(&spec, Some(Vector3::new(40.0, 0.0, 0.0)), 1.5);
        agent.observe(&observation(Vector3::ZERO, vec![]));
        let first = agent.propose(&context()).unwrap();
        assert_eq!(first.target.x, 16.0);
        agent.receive_outcome(&ProposalOutcome {
            proposal_id: "p".into(),
            outcome: PolicyOutcome::RejectedDeterministic,
            committed: false,
            reasons: vec!["STEP_TOO_LARGE".into()],
            committed_position: None,
        });
        assert_eq!(agent.propose(&context()).unwrap().target.x, 8.0);
    }

    #[test]
    fn agents_hold_without_an_observation() {
        let mut agent = BuiltinAgent::new(
            &spec(Behavior::Cautious),
            Some(Vector3::new(20.0, 0.0, 0.0)),
            1.5,
        );
        assert!(agent.propose(&context()).is_none());
    }

    #[test]
    fn malformed_agent_emits_invalid_intents_on_schedule() {
        let mut spec = spec(Behavior::Malformed);
        spec.malformed_every = 2;
        let mut agent = BuiltinAgent::new(&spec, Some(Vector3::new(20.0, 0.0, 0.0)), 1.5);
        agent.observe(&observation(Vector3::ZERO, vec![]));
        let first = agent.propose(&context()).unwrap();
        assert_eq!(first.action_type, "MOVE");
        let second = agent.propose(&context()).unwrap();
        assert!(second.rationale.starts_with("deliberately invalid"));
    }
}
