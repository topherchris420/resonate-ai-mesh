//! The mesh as a typed graph.
//!
//! The topology is derived from the configuration a run actually used, not
//! drawn by hand. It makes explicit which components observe, propose,
//! validate, judge, decide, and commit, which are deterministic or networked,
//! and that exactly one node may mutate authoritative state.

use crate::agents::AgentDescriptor;
use crate::config::{Behavior, JudgeProvider, ReviewMode, RunConfig};
use serde::{Deserialize, Serialize};

pub const TOPOLOGY_VERSION: &str = "resonate-ai-mesh.topology.v1";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NodeKind {
    Human,
    Sensor,
    Agent,
    Model,
    Validator,
    Judge,
    Environment,
    Policy,
    StateStore,
    Visualizer,
    Experiment,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TrustBoundary {
    /// Runs in the kernel process and is a pure function of its inputs.
    LocalDeterministic,
    /// Runs locally but its output is not guaranteed reproducible.
    LocalNondeterministic,
    /// Crosses a network boundary.
    Remote,
    /// A person, or a simulated stand-in for one.
    Human,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Health {
    Ok,
    Simulated,
    Disabled,
    Unavailable,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Node {
    pub id: String,
    pub kind: NodeKind,
    pub label: String,
    pub capabilities: Vec<String>,
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
    pub schemas: Vec<String>,
    pub trust_boundary: TrustBoundary,
    /// Median latency observed in the run, when measured.
    pub latency_ms: Option<f64>,
    pub provenance: String,
    pub health: Health,
    pub deterministic: bool,
    pub networked: bool,
    pub may_mutate_state: bool,
    pub data_mode: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EdgeKind {
    Observes,
    Senses,
    Proposes,
    Validates,
    Judges,
    Decides,
    Reviews,
    Commits,
    Configures,
    Records,
    Displays,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Edge {
    pub from: String,
    pub to: String,
    pub kind: EdgeKind,
    pub schema: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Topology {
    pub version: String,
    pub run_id: Option<String>,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
}

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| item.to_string()).collect()
}

#[allow(clippy::too_many_arguments)]
fn node(
    id: &str,
    kind: NodeKind,
    label: &str,
    capabilities: &[&str],
    inputs: &[&str],
    outputs: &[&str],
    trust_boundary: TrustBoundary,
    provenance: &str,
) -> Node {
    Node {
        id: id.to_string(),
        kind,
        label: label.to_string(),
        capabilities: strings(capabilities),
        inputs: strings(inputs),
        outputs: strings(outputs),
        schemas: Vec::new(),
        trust_boundary,
        latency_ms: None,
        provenance: provenance.to_string(),
        health: Health::Ok,
        deterministic: trust_boundary == TrustBoundary::LocalDeterministic,
        networked: trust_boundary == TrustBoundary::Remote,
        may_mutate_state: false,
        data_mode: None,
    }
}

fn edge(from: &str, to: &str, kind: EdgeKind, schema: &str) -> Edge {
    Edge {
        from: from.to_string(),
        to: to.to_string(),
        kind,
        schema: schema.to_string(),
    }
}

/// Build the topology of a run configuration.
pub fn from_config(
    config: &RunConfig,
    agents: Option<&[AgentDescriptor]>,
    pipeline_latency_ms: Option<f64>,
    with_visualizer: bool,
) -> Topology {
    let scenario = &config.scenario;
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    let experiment = format!("experiment.{}", config.experiment_id);
    let mut exp = node(
        &experiment,
        NodeKind::Experiment,
        &format!("{} / {}", config.experiment_id, config.condition_id),
        &[
            "configure run",
            "inject faults",
            "record events",
            "compute metrics",
        ],
        &["manifest", "scenario"],
        &["events.jsonl", "metrics.json"],
        TrustBoundary::LocalDeterministic,
        "mesh-lab runner",
    );
    exp.schemas = strings(&["RunConfig", "CanonicalEventEnvelope 1.2.0"]);
    nodes.push(exp);

    let environment = format!("environment.{}", scenario.id);
    let mut env = node(
        &environment,
        NodeKind::Environment,
        &format!("{} arena", scenario.id),
        &["hazard schedule", "points of interest"],
        &[],
        &["environment_change"],
        TrustBoundary::LocalDeterministic,
        &format!("scenario:{}", scenario.id),
    );
    env.health = Health::Simulated;
    env.data_mode = Some("SIMULATED".into());
    nodes.push(env);

    let mut sensors = node(
        "sensor.positions",
        NodeKind::Sensor,
        "Position sensors",
        &[
            "noisy position readings",
            "neighbour readings",
            "fault effects",
        ],
        &["authoritative positions", "declared hazards"],
        &["observation"],
        TrustBoundary::LocalDeterministic,
        "mesh.lab.sensors.v1",
    );
    sensors.health = Health::Simulated;
    sensors.data_mode = Some("SIMULATED".into());
    sensors.schemas = strings(&["AgentObservation"]);
    nodes.push(sensors);

    if scenario.human_state.enabled {
        let mut load = node(
            "sensor.operator_load",
            NodeKind::Sensor,
            "Operator load index",
            &["operator_load_index"],
            &["rejections", "pending reviews", "environment complexity"],
            &["human_state"],
            TrustBoundary::LocalDeterministic,
            crate::sim::OPERATOR_LOAD_SOURCE,
        );
        load.health = Health::Simulated;
        load.schemas = strings(&["HumanStateDatum"]);
        load.data_mode = Some(
            if scenario.human_state.trace.is_some() {
                "REPLAY"
            } else {
                "SIMULATED"
            }
            .into(),
        );
        nodes.push(load);
    }

    let review_simulated = scenario.human_review.mode != ReviewMode::Manual;
    let mut human = node(
        "human.operator",
        NodeKind::Human,
        if review_simulated {
            "Operator (simulated)"
        } else {
            "Operator"
        },
        &[
            "approve or reject proposals awaiting review",
            "issue commands (live sessions)",
        ],
        &["commitment awaiting_human_review"],
        &["human_resolution"],
        TrustBoundary::Human,
        match scenario.human_review.mode {
            ReviewMode::SimulatedOperator => "simulated operator rule",
            ReviewMode::Expire => "review expiry rule",
            ReviewMode::Manual => "operator console",
        },
    );
    human.health = if review_simulated {
        Health::Simulated
    } else {
        Health::Ok
    };
    human.deterministic = review_simulated;
    nodes.push(human);

    let validator_id = "pordenone.validator";
    let checks: Vec<&str> = epistemic_validator::CheckId::ALL
        .iter()
        .filter(|check| {
            check.is_core() || !scenario.kernel.validator.disabled_checks.contains(check)
        })
        .map(|check| check.as_str())
        .collect();
    let mut validator = node(
        validator_id,
        NodeKind::Validator,
        "Deterministic validator",
        &checks,
        &["proposal", "authoritative state snapshot"],
        &["validation"],
        TrustBoundary::LocalDeterministic,
        epistemic_validator::VALIDATOR_VERSION,
    );
    validator.schemas = strings(&["ActionProposal", "ValidationResult"]);
    validator.latency_ms = pipeline_latency_ms;
    nodes.push(validator);

    let judge_spec = &scenario.kernel.judgment;
    let judge_enabled = judge_spec.provider != JudgeProvider::Disabled;
    let judge_id = "pordenone.judge";
    let mut judge = node(
        judge_id,
        NodeKind::Judge,
        match judge_spec.provider {
            JudgeProvider::Disabled => "Judgment (off)",
            JudgeProvider::Mock => "Judgment (deterministic mock)",
            JudgeProvider::Typesafe => "Judgment (TypeSafe Jev)",
            JudgeProvider::Recorded => "Judgment (recorded)",
        },
        &[
            "proposal_support",
            "evidence_quality",
            "contradiction_present",
            "scope_violation",
            "human_review",
        ],
        &["privacy-minimized evidence package"],
        &["judgment"],
        if judge_spec.provider == JudgeProvider::Typesafe {
            TrustBoundary::Remote
        } else {
            TrustBoundary::LocalDeterministic
        },
        typed_judgment::QUESTION_SET_VERSION,
    );
    judge.health = if judge_enabled {
        Health::Ok
    } else {
        Health::Disabled
    };
    judge.schemas = strings(&["JudgmentEnvelope"]);
    nodes.push(judge);
    if judge_spec.provider == JudgeProvider::Typesafe {
        let mut model = node(
            "model.jev",
            NodeKind::Model,
            "TypeSafe Jev (remote)",
            &["typed answers with probabilities"],
            &["System One request"],
            &["System One response"],
            TrustBoundary::Remote,
            "https://api.typesafe.ai/v1/systemone",
        );
        model.health = Health::Unknown;
        nodes.push(model);
        edges.push(edge(
            judge_id,
            "model.jev",
            EdgeKind::Judges,
            "System One request",
        ));
    }

    let mut policy = node(
        "pordenone.policy",
        NodeKind::Policy,
        "Pordenone policy and commit gate",
        &[
            "interpret judgment",
            "operator-load gating",
            "judgment routing",
            "authorize commits",
        ],
        &[
            "validation",
            "judgment",
            "operator level",
            "human_resolution",
        ],
        &["commitment", "state_transition"],
        TrustBoundary::LocalDeterministic,
        kernel_core::KERNEL_POLICY_VERSION,
    );
    policy.may_mutate_state = true;
    policy.schemas = strings(&["PolicyDecision", "StateTransition"]);
    nodes.push(policy);

    let mut store = node(
        "pordenone.state",
        NodeKind::StateStore,
        "Authoritative state",
        &["agents", "positions", "declared hazards", "state hash"],
        &["commit authorization"],
        &["snapshot"],
        TrustBoundary::LocalDeterministic,
        "kernel-core state",
    );
    store.schemas = strings(&["StateSnapshot"]);
    nodes.push(store);

    if with_visualizer {
        let mut cockpit = node(
            "visualizer.cockpit",
            NodeKind::Visualizer,
            "Research cockpit",
            &["timeline", "evidence", "replay", "compare", "topology"],
            &["events", "metrics"],
            &["operator commands"],
            TrustBoundary::Human,
            "apps/c2-dashboard",
        );
        cockpit.deterministic = false;
        nodes.push(cockpit);
        edges.push(edge(
            "pordenone.state",
            "visualizer.cockpit",
            EdgeKind::Displays,
            "events",
        ));
        edges.push(edge(
            "visualizer.cockpit",
            "human.operator",
            EdgeKind::Displays,
            "evidence",
        ));
    }

    let agent_descriptors: Vec<AgentDescriptor> = match agents {
        Some(list) => list.to_vec(),
        None => scenario
            .agents
            .iter()
            .map(|spec| AgentDescriptor {
                agent_id: spec.id.clone(),
                adapter: format!("builtin:{}", crate::agents::behavior_name(spec.behavior)),
                deterministic: spec.behavior != Behavior::External,
                networked: false,
            })
            .collect(),
    };
    for descriptor in &agent_descriptors {
        let id = format!("agent.{}", descriptor.agent_id);
        let mut agent = node(
            &id,
            NodeKind::Agent,
            &descriptor.agent_id,
            &["observe", "propose", "explain", "receive outcome"],
            &["observation", "outcome"],
            &["proposal"],
            if descriptor.deterministic {
                TrustBoundary::LocalDeterministic
            } else {
                TrustBoundary::LocalNondeterministic
            },
            &descriptor.adapter,
        );
        agent.networked = descriptor.networked;
        agent.schemas = strings(&["MeshAgentAdapter", "AgentIntent"]);
        nodes.push(agent);
        edges.push(edge(
            "sensor.positions",
            &id,
            EdgeKind::Senses,
            "AgentObservation",
        ));
        edges.push(edge(
            &id,
            validator_id,
            EdgeKind::Proposes,
            "ActionProposal",
        ));
        edges.push(edge(
            "pordenone.policy",
            &id,
            EdgeKind::Decides,
            "ProposalOutcome",
        ));
    }

    edges.push(edge(
        &environment,
        "sensor.positions",
        EdgeKind::Observes,
        "hazards",
    ));
    edges.push(edge(
        "pordenone.state",
        "sensor.positions",
        EdgeKind::Observes,
        "positions",
    ));
    edges.push(edge(
        &environment,
        "pordenone.policy",
        EdgeKind::Configures,
        "hazard declarations",
    ));
    if scenario.human_state.enabled {
        edges.push(edge(
            "sensor.operator_load",
            "pordenone.policy",
            EdgeKind::Senses,
            "HumanStateDatum",
        ));
    }
    if judge_enabled {
        edges.push(edge(
            validator_id,
            judge_id,
            EdgeKind::Validates,
            "evidence package (validation passed)",
        ));
        edges.push(edge(
            judge_id,
            "pordenone.policy",
            EdgeKind::Judges,
            "JudgmentEnvelope",
        ));
    }
    edges.push(edge(
        validator_id,
        "pordenone.policy",
        EdgeKind::Validates,
        "ValidationResult",
    ));
    edges.push(edge(
        "human.operator",
        "pordenone.policy",
        EdgeKind::Reviews,
        "HumanReviewDecision",
    ));
    edges.push(edge(
        "pordenone.policy",
        "pordenone.state",
        EdgeKind::Commits,
        "CommitAuthorization",
    ));
    edges.push(edge(
        &experiment,
        &environment,
        EdgeKind::Configures,
        "Scenario",
    ));
    edges.push(edge(
        &experiment,
        "pordenone.policy",
        EdgeKind::Configures,
        "KernelSpec, agent registration",
    ));
    edges.push(edge(
        "pordenone.state",
        &experiment,
        EdgeKind::Records,
        "events",
    ));

    Topology {
        version: TOPOLOGY_VERSION.to_string(),
        run_id: Some(config.run_id.clone()),
        nodes,
        edges,
    }
}

/// Structural rules every topology must satisfy.
pub fn check(topology: &Topology) -> Vec<String> {
    let mut problems = Vec::new();
    let mutators: Vec<&Node> = topology
        .nodes
        .iter()
        .filter(|n| n.may_mutate_state)
        .collect();
    if mutators.len() != 1 || mutators[0].kind != NodeKind::Policy {
        problems.push(format!(
            "exactly one POLICY node may mutate state; found {:?}",
            mutators.iter().map(|n| &n.id).collect::<Vec<_>>()
        ));
    }
    let kind_of = |id: &str| topology.nodes.iter().find(|n| n.id == id).map(|n| n.kind);
    let node_of = |id: &str| topology.nodes.iter().find(|n| n.id == id);
    for e in &topology.edges {
        if node_of(&e.from).is_none() || node_of(&e.to).is_none() {
            problems.push(format!(
                "edge {} -> {} references an unknown node",
                e.from, e.to
            ));
            continue;
        }
        if e.kind == EdgeKind::Commits {
            let from = node_of(&e.from).expect("checked");
            if !from.may_mutate_state || from.networked {
                problems.push(format!("{} commits but may not mutate state", e.from));
            }
        }
        if kind_of(&e.to) == Some(NodeKind::StateStore) && e.kind != EdgeKind::Commits {
            problems.push(format!(
                "{} reaches the state store without a commit edge",
                e.from
            ));
        }
        if kind_of(&e.from) == Some(NodeKind::Judge)
            && !matches!(kind_of(&e.to), Some(NodeKind::Policy | NodeKind::Model))
        {
            problems.push(format!("judge output goes to {} instead of policy", e.to));
        }
        if kind_of(&e.from) == Some(NodeKind::Agent) && kind_of(&e.to) != Some(NodeKind::Validator)
        {
            problems.push(format!("agent {} bypasses validation", e.from));
        }
    }
    problems
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn config(provider: &str) -> RunConfig {
        let scenario: crate::config::Scenario = serde_yaml::from_str(&format!(
            r#"
id: t
description: t
ticks: 2
points_of_interest: [{{id: g, position: [5, 0]}}]
agents: [{{id: a, behavior: cautious, start: [0, 0], goal: g}}]
kernel: {{judgment: {{provider: {provider}}}}}
"#
        ))
        .unwrap();
        crate::config::resolve_with_overrides(&scenario, "e", "c", 0, 1, BTreeMap::new()).unwrap()
    }

    #[test]
    fn only_the_policy_gate_may_mutate_state() {
        for provider in ["disabled", "mock", "typesafe"] {
            let topology = from_config(&config(provider), None, None, true);
            assert!(
                check(&topology).is_empty(),
                "{provider}: {:?}",
                check(&topology)
            );
            let remote = topology
                .nodes
                .iter()
                .filter(|n| n.networked)
                .collect::<Vec<_>>();
            assert!(remote.iter().all(|n| !n.may_mutate_state));
            if provider == "typesafe" {
                assert!(topology
                    .nodes
                    .iter()
                    .any(|n| n.kind == NodeKind::Model && n.networked));
            }
        }
    }

    #[test]
    fn a_bypassing_edge_is_detected() {
        let mut topology = from_config(&config("mock"), None, None, false);
        topology.edges.push(edge(
            "pordenone.judge",
            "pordenone.state",
            EdgeKind::Commits,
            "x",
        ));
        assert!(!check(&topology).is_empty());
    }
}
