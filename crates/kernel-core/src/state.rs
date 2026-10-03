//! Authoritative state and the single interface that mutates it.
//!
//! [`AuthoritativeState::apply`] is the only function that changes agents,
//! positions, or declared constraints. It is crate-private; outside callers go
//! through `KernelEngine`, which only reaches it with a registration, a
//! constraint declaration, or a [`CommitAuthorization`]. A commit authorization
//! can only be built by the policy module from a `ValidatedProposal`, which in
//! turn can only come from a passing deterministic verdict.

use crate::policy::CommitBasis;
use epistemic_validator::{ActionProposal, AgentView, HazardZone, ValidatedProposal};
use event_bus::hash_canonical;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use spatial_state::{SpatialEntity, SpatialIndex, Vector3};
use std::collections::BTreeMap;
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AgentStatus {
    Idle,
    Busy,
    Executing,
    Failed,
    Completed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AuthoritativeAgentState {
    pub agent_id: String,
    pub status: AgentStatus,
    pub capabilities: Vec<String>,
    pub current_task: Option<String>,
    pub priority: i32,
    pub position: Vector3,
    /// Displacement applied by the most recent commit.
    pub velocity: Vector3,
    /// Deterministic confidence of the validation that authorized the last commit.
    pub confidence: f64,
    pub last_updated: i64,
}

impl AuthoritativeAgentState {
    pub fn idle(agent_id: &str, position: Vector3, now_ms: i64) -> Self {
        Self {
            agent_id: agent_id.to_string(),
            status: AgentStatus::Idle,
            capabilities: vec!["move".to_string()],
            current_task: None,
            priority: 1,
            position,
            velocity: Vector3::ZERO,
            confidence: 1.0,
            last_updated: now_ms,
        }
    }
}

/// Everything the kernel treats as authoritative. Ordered maps keep the hash stable.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct StateSnapshot {
    pub revision: u64,
    pub agents: BTreeMap<String, AuthoritativeAgentState>,
    pub hazards: BTreeMap<String, HazardZone>,
}

impl StateSnapshot {
    pub fn hash(&self) -> String {
        hash_canonical(self).expect("state snapshot serializes")
    }
}

/// Proof that the policy stage permitted this commit.
#[derive(Debug, Clone)]
pub struct CommitAuthorization {
    pub(crate) validated: ValidatedProposal,
    pub(crate) basis: CommitBasis,
    pub(crate) authorized_by: Vec<String>,
}

impl CommitAuthorization {
    pub fn proposal(&self) -> &ActionProposal {
        self.validated.proposal()
    }

    pub fn basis(&self) -> CommitBasis {
        self.basis
    }
}

pub(crate) enum StateMutation {
    RegisterAgent(AuthoritativeAgentState),
    DeclareHazard(HazardZone),
    RetireHazard(String),
    Commit(Box<CommitAuthorization>),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StateTransition {
    pub revision: u64,
    pub mutation: String,
    pub subject_id: String,
    pub before_hash: String,
    pub after_hash: String,
    pub proposal_id: Option<String>,
    pub basis: Option<CommitBasis>,
    /// Event ids whose decisions authorized this transition.
    pub authorized_by: Vec<String>,
    pub change: Value,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum MutationError {
    #[error("agent `{0}` is already registered")]
    DuplicateAgent(String),
    #[error("agent `{0}` is not registered")]
    UnknownAgent(String),
    #[error("agent `{0}` has a non-finite position")]
    InvalidAgent(String),
    #[error("hazard `{0}` must have a finite center and a positive radius")]
    InvalidHazard(String),
    #[error("hazard `{0}` is not declared")]
    UnknownHazard(String),
    #[error("identifier is empty or longer than 128 characters")]
    InvalidIdentifier,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct AuthoritativeState {
    snapshot: StateSnapshot,
    spatial: SpatialIndex,
}

impl AuthoritativeState {
    pub(crate) fn snapshot(&self) -> &StateSnapshot {
        &self.snapshot
    }

    pub(crate) fn agent_view(&self, agent_id: &str) -> Option<AgentView> {
        self.snapshot.agents.get(agent_id).map(|agent| AgentView {
            agent_id: agent.agent_id.clone(),
            position: agent.position,
        })
    }

    pub(crate) fn other_views(&self, agent_id: &str) -> Vec<AgentView> {
        self.spatial
            .list_entities()
            .into_iter()
            .filter(|entity| entity.entity_id != agent_id)
            .map(|entity| AgentView {
                agent_id: entity.entity_id,
                position: entity.position,
            })
            .collect()
    }

    pub(crate) fn hazards(&self) -> Vec<HazardZone> {
        self.snapshot.hazards.values().cloned().collect()
    }

    /// The single mutation path. On error, state is unchanged.
    pub(crate) fn apply(
        &mut self,
        mutation: StateMutation,
        now_ms: i64,
    ) -> Result<StateTransition, MutationError> {
        let before_hash = self.snapshot.hash();
        let (kind, subject, proposal_id, basis, authorized_by, change) = match mutation {
            StateMutation::RegisterAgent(agent) => {
                check_identifier(&agent.agent_id)?;
                if !agent.position.is_finite() {
                    return Err(MutationError::InvalidAgent(agent.agent_id));
                }
                if self.snapshot.agents.contains_key(&agent.agent_id) {
                    return Err(MutationError::DuplicateAgent(agent.agent_id));
                }
                let change = json!({ "position": agent.position, "status": agent.status });
                let id = agent.agent_id.clone();
                self.spatial.upsert_entity(entity(&agent, now_ms));
                self.snapshot.agents.insert(id.clone(), agent);
                ("register_agent", id, None, None, Vec::new(), change)
            }
            StateMutation::DeclareHazard(zone) => {
                check_identifier(&zone.id)?;
                if !zone.center.is_finite() || !zone.radius.is_finite() || zone.radius <= 0.0 {
                    return Err(MutationError::InvalidHazard(zone.id));
                }
                let change = json!({ "center": zone.center, "radius": zone.radius });
                let id = zone.id.clone();
                self.snapshot.hazards.insert(id.clone(), zone);
                ("declare_hazard", id, None, None, Vec::new(), change)
            }
            StateMutation::RetireHazard(id) => {
                if self.snapshot.hazards.remove(&id).is_none() {
                    return Err(MutationError::UnknownHazard(id));
                }
                ("retire_hazard", id, None, None, Vec::new(), Value::Null)
            }
            StateMutation::Commit(authorization) => {
                let CommitAuthorization {
                    validated,
                    basis,
                    authorized_by,
                } = *authorization;
                let (proposal, validation) = validated.into_parts();
                let agent = self
                    .snapshot
                    .agents
                    .get_mut(&proposal.agent_id)
                    .ok_or_else(|| MutationError::UnknownAgent(proposal.agent_id.clone()))?;
                let from = agent.position;
                let to = proposal.target_position;
                agent.velocity = Vector3::new(to.x - from.x, to.y - from.y, to.z - from.z);
                agent.position = to;
                agent.status = AgentStatus::Executing;
                agent.current_task = Some(proposal.action_type.clone());
                agent.priority = proposal.priority;
                agent.confidence = validation.confidence;
                agent.last_updated = now_ms;
                let snapshot_agent = agent.clone();
                self.spatial.upsert_entity(entity(&snapshot_agent, now_ms));
                let change = json!({
                    "from": from,
                    "to": to,
                    "action_type": proposal.action_type,
                });
                (
                    "commit",
                    proposal.agent_id.clone(),
                    Some(proposal.proposal_id.clone()),
                    Some(basis),
                    authorized_by,
                    change,
                )
            }
        };
        self.snapshot.revision += 1;
        Ok(StateTransition {
            revision: self.snapshot.revision,
            mutation: kind.to_string(),
            subject_id: subject,
            before_hash,
            after_hash: self.snapshot.hash(),
            proposal_id,
            basis,
            authorized_by,
            change,
        })
    }
}

fn check_identifier(id: &str) -> Result<(), MutationError> {
    if id.trim().is_empty() || id.chars().count() > 128 || id.chars().any(char::is_control) {
        Err(MutationError::InvalidIdentifier)
    } else {
        Ok(())
    }
}

fn entity(agent: &AuthoritativeAgentState, now_ms: i64) -> SpatialEntity {
    SpatialEntity {
        entity_id: agent.agent_id.clone(),
        position: agent.position,
        orientation: Vector3::ZERO,
        velocity: agent.velocity,
        terrain_reference: "arena".to_string(),
        coordinate_system: "local_sim".to_string(),
        timestamp: now_ms,
        confidence: agent.confidence,
    }
}
