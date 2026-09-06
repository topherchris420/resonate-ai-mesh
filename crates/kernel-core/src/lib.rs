use epistemic_validator::{ActionProposal, EpistemicValidator, ValidationResult};
use event_bus::{EventBus, EventEnvelope};
use serde::{Deserialize, Serialize};
use spatial_state::{SpatialEntity, SpatialIndex, Vector3 as SpatialVector3};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info, warn};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AgentStatus {
    Idle,
    Busy,
    Executing,
    Failed,
    Completed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthoritativeAgentState {
    pub agent_id: String,
    pub status: AgentStatus,
    pub capabilities: Vec<String>,
    pub current_task: Option<String>,
    pub priority: i32,
    pub position: SpatialVector3,
    pub velocity: SpatialVector3,
    pub confidence: f64,
    pub last_updated: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum CognitiveLoadPolicyLevel {
    Normal,
    Elevated,
    High,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyConfiguration {
    pub level: CognitiveLoadPolicyLevel,
    pub update_frequency_ms: u64,
    pub visual_density_scale: f64,
    pub aggregate_alerts: bool,
    pub defer_background_tasks: bool,
}

impl Default for PolicyConfiguration {
    fn default() -> Self {
        Self {
            level: CognitiveLoadPolicyLevel::Normal,
            update_frequency_ms: 100,
            visual_density_scale: 1.0,
            aggregate_alerts: false,
            defer_background_tasks: false,
        }
    }
}

pub struct KernelEngine {
    event_bus: EventBus,
    validator: EpistemicValidator,
    agent_states: Arc<RwLock<HashMap<String, AuthoritativeAgentState>>>,
    spatial_index: Arc<RwLock<SpatialIndex>>,
    policy_config: Arc<RwLock<PolicyConfiguration>>,
}

impl KernelEngine {
    pub fn new(event_bus: EventBus) -> Self {
        Self {
            event_bus,
            validator: EpistemicValidator::new(),
            agent_states: Arc::new(RwLock::new(HashMap::new())),
            spatial_index: Arc::new(RwLock::new(SpatialIndex::new())),
            policy_config: Arc::new(RwLock::new(PolicyConfiguration::default())),
        }
    }

    pub async fn register_agent(&self, state: AuthoritativeAgentState) {
        let mut map = self.agent_states.write().await;
        map.insert(state.agent_id.clone(), state);
    }

    pub async fn get_agent(&self, agent_id: &str) -> Option<AuthoritativeAgentState> {
        let map = self.agent_states.read().await;
        map.get(agent_id).cloned()
    }

    pub async fn update_policy_for_cognitive_load(
        &self,
        cognitive_load: f64,
    ) -> PolicyConfiguration {
        let level = if cognitive_load < 0.4 {
            CognitiveLoadPolicyLevel::Normal
        } else if cognitive_load < 0.65 {
            CognitiveLoadPolicyLevel::Elevated
        } else if cognitive_load < 0.85 {
            CognitiveLoadPolicyLevel::High
        } else {
            CognitiveLoadPolicyLevel::Critical
        };

        let config = match level {
            CognitiveLoadPolicyLevel::Normal => PolicyConfiguration {
                level: CognitiveLoadPolicyLevel::Normal,
                update_frequency_ms: 100,
                visual_density_scale: 1.0,
                aggregate_alerts: false,
                defer_background_tasks: false,
            },
            CognitiveLoadPolicyLevel::Elevated => PolicyConfiguration {
                level: CognitiveLoadPolicyLevel::Elevated,
                update_frequency_ms: 250,
                visual_density_scale: 0.8,
                aggregate_alerts: false,
                defer_background_tasks: false,
            },
            CognitiveLoadPolicyLevel::High => PolicyConfiguration {
                level: CognitiveLoadPolicyLevel::High,
                update_frequency_ms: 500,
                visual_density_scale: 0.5,
                aggregate_alerts: true,
                defer_background_tasks: true,
            },
            CognitiveLoadPolicyLevel::Critical => PolicyConfiguration {
                level: CognitiveLoadPolicyLevel::Critical,
                update_frequency_ms: 1000,
                visual_density_scale: 0.2,
                aggregate_alerts: true,
                defer_background_tasks: true,
            },
        };

        let mut p = self.policy_config.write().await;
        *p = config.clone();

        info!(
            policy_level = ?config.level,
            cognitive_load = cognitive_load,
            "Updated Kernel Policy Level"
        );

        config
    }

    pub async fn process_action_proposal(&self, proposal: ActionProposal) -> ValidationResult {
        let current_time = chrono::Utc::now().timestamp_millis();

        let agent_exists = self.get_agent(&proposal.agent_id).await.is_some();
        if !agent_exists {
            warn!(agent_id = %proposal.agent_id, "Proposal for unknown agent; auto-registering stub");
            self.register_agent(AuthoritativeAgentState {
                agent_id: proposal.agent_id.clone(),
                status: AgentStatus::Idle,
                capabilities: vec!["generic".to_string()],
                current_task: None,
                priority: proposal.priority,
                position: SpatialVector3 {
                    x: 0.0,
                    y: 0.0,
                    z: 0.0,
                },
                velocity: SpatialVector3 {
                    x: 0.0,
                    y: 0.0,
                    z: 0.0,
                },
                confidence: 0.9,
                last_updated: current_time,
            })
            .await;
        }

        let val_result = self.validator.validate(&proposal, current_time);

        if val_result.accepted {
            let mut map = self.agent_states.write().await;
            if let Some(agent) = map.get_mut(&proposal.agent_id) {
                agent.status = AgentStatus::Executing;
                agent.current_task = Some(proposal.action_type.clone());
                agent.position = SpatialVector3 {
                    x: proposal.target_position.x,
                    y: proposal.target_position.y,
                    z: proposal.target_position.z,
                };
                agent.last_updated = current_time;
            }

            let mut s = self.spatial_index.write().await;
            s.upsert_entity(SpatialEntity {
                entity_id: proposal.agent_id.clone(),
                position: SpatialVector3 {
                    x: proposal.target_position.x,
                    y: proposal.target_position.y,
                    z: proposal.target_position.z,
                },
                orientation: SpatialVector3 {
                    x: 0.0,
                    y: 0.0,
                    z: 0.0,
                },
                velocity: SpatialVector3 {
                    x: 0.0,
                    y: 0.0,
                    z: 0.0,
                },
                terrain_reference: "default".into(),
                coordinate_system: "ECEF".into(),
                timestamp: current_time,
                confidence: val_result.confidence,
            });

            info!(
                proposal_id = %proposal.proposal_id,
                agent_id = %proposal.agent_id,
                "Action Proposal VALIDATED and COMMITTED to authoritative state"
            );
        } else {
            error!(
                proposal_id = %proposal.proposal_id,
                agent_id = %proposal.agent_id,
                reasons = ?val_result.reasons,
                contradictions = ?val_result.contradictions,
                "Action Proposal REJECTED by Epistemic Validator"
            );
        }

        let envelope = EventEnvelope::new(
            "validation",
            "kernel_core",
            &proposal.agent_id,
            &proposal.correlation_id,
            &proposal.proposal_id,
            &val_result.provenance,
            serde_json::to_string(&val_result).unwrap_or_default(),
        );

        let _ = self.event_bus.publish(envelope);

        val_result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_kernel_lifecycle_commit_and_rejection() {
        let bus = EventBus::new(10);
        let mut rx = bus.subscribe();
        let kernel = KernelEngine::new(bus);

        let valid_proposal = ActionProposal {
            proposal_id: "prop_valid".into(),
            agent_id: "agent_001".into(),
            action_type: "MOVE".into(),
            parameters_json: "{}".into(),
            target_position: epistemic_validator::Vector3 {
                x: 100.0,
                y: 200.0,
                z: 0.0,
            },
            priority: 1,
            timestamp: chrono::Utc::now().timestamp_millis(),
            correlation_id: "corr_001".into(),
            source_observation: "obs_001".into(),
        };

        let res = kernel.process_action_proposal(valid_proposal).await;
        assert!(res.accepted);

        let agent = kernel.get_agent("agent_001").await.unwrap();
        assert_eq!(agent.status, AgentStatus::Executing);
        assert_eq!(agent.position.x, 100.0);

        let evt = rx.recv().await.unwrap();
        assert_eq!(evt.event_type, "validation");
        assert_eq!(evt.correlation_id, "corr_001");

        let invalid_proposal = ActionProposal {
            proposal_id: "prop_invalid".into(),
            agent_id: "agent_001".into(),
            action_type: "MOVE".into(),
            parameters_json: "{}".into(),
            target_position: epistemic_validator::Vector3 {
                x: 999999.0,
                y: 0.0,
                z: 0.0,
            },
            priority: 1,
            timestamp: chrono::Utc::now().timestamp_millis(),
            correlation_id: "corr_002".into(),
            source_observation: "obs_002".into(),
        };

        let res2 = kernel.process_action_proposal(invalid_proposal).await;
        assert!(!res2.accepted);

        let agent_after = kernel.get_agent("agent_001").await.unwrap();
        assert_eq!(agent_after.position.x, 100.0);
    }
}
