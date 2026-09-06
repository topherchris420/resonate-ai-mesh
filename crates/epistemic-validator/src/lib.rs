use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Vector3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionProposal {
    pub proposal_id: String,
    pub agent_id: String,
    pub action_type: String,
    pub parameters_json: String,
    pub target_position: Vector3,
    pub priority: i32,
    pub timestamp: i64,
    pub correlation_id: String,
    pub source_observation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ValidationResult {
    pub proposal_id: String,
    pub agent_id: String,
    pub accepted: bool,
    pub feasibility: f64,
    pub contradictions: Vec<String>,
    pub confidence: f64,
    pub reasons: Vec<String>,
    pub provenance: String,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Default)]
pub struct EpistemicValidator {
    pub max_spatial_bound: f64,
    pub max_stale_ms: i64,
}

impl EpistemicValidator {
    pub fn new() -> Self {
        Self {
            max_spatial_bound: 10000.0,
            max_stale_ms: 30000,
        }
    }

    pub fn validate(&self, proposal: &ActionProposal, current_time: i64) -> ValidationResult {
        let mut contradictions = Vec::new();
        let mut reasons = Vec::new();
        let mut feasibility = 1.0;
        let mut confidence = 0.95;

        if proposal.target_position.x.abs() > self.max_spatial_bound
            || proposal.target_position.y.abs() > self.max_spatial_bound
            || proposal.target_position.z.abs() > self.max_spatial_bound
        {
            feasibility = 0.0;
            contradictions.push(format!(
                "Target position ({}, {}, {}) exceeds maximum spatial boundary {}",
                proposal.target_position.x,
                proposal.target_position.y,
                proposal.target_position.z,
                self.max_spatial_bound
            ));
        }

        if proposal.priority < 0 {
            feasibility *= 0.5;
            contradictions.push("Negative priority supplied".to_string());
        }

        let age_ms = current_time - proposal.timestamp;
        if age_ms > self.max_stale_ms {
            confidence *= 0.3;
            contradictions.push(format!(
                "Proposal observation is stale by {} ms (threshold: {} ms)",
                age_ms, self.max_stale_ms
            ));
        }

        let valid_actions = ["MOVE", "PATROL", "INSPECT", "STANDBY", "HOLD"];
        if !valid_actions.contains(&proposal.action_type.as_str()) {
            feasibility = 0.0;
            contradictions.push(format!(
                "Unknown or prohibited action type: {}",
                proposal.action_type
            ));
        }

        let accepted = feasibility > 0.5 && contradictions.is_empty() && confidence > 0.5;

        if accepted {
            reasons.push(
                "Action proposal passed deterministic feasibility and non-contradiction checks."
                    .to_string(),
            );
        } else {
            reasons.push("Action proposal failed epistemic validation checks.".to_string());
        }

        ValidationResult {
            proposal_id: proposal.proposal_id.clone(),
            agent_id: proposal.agent_id.clone(),
            accepted,
            feasibility,
            contradictions,
            confidence,
            reasons,
            provenance: format!("Validator:epistemic-v1:{}", proposal.source_observation),
            timestamp: chrono::Utc::now().timestamp_millis(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_proposal() {
        let validator = EpistemicValidator::new();
        let now = 100000;
        let proposal = ActionProposal {
            proposal_id: "prop_1".into(),
            agent_id: "agent_alpha".into(),
            action_type: "PATROL".into(),
            parameters_json: "{}".into(),
            target_position: Vector3 {
                x: 100.0,
                y: 50.0,
                z: 0.0,
            },
            priority: 1,
            timestamp: now - 1000,
            correlation_id: "corr_1".into(),
            source_observation: "obs_001".into(),
        };

        let res = validator.validate(&proposal, now);
        assert!(res.accepted);
        assert_eq!(res.feasibility, 1.0);
        assert!(res.contradictions.is_empty());
    }

    #[test]
    fn test_invalid_coordinates_rejected() {
        let validator = EpistemicValidator::new();
        let now = 100000;
        let proposal = ActionProposal {
            proposal_id: "prop_2".into(),
            agent_id: "agent_beta".into(),
            action_type: "MOVE".into(),
            parameters_json: "{}".into(),
            target_position: Vector3 {
                x: 999999.0,
                y: 0.0,
                z: 0.0,
            },
            priority: 1,
            timestamp: now - 1000,
            correlation_id: "corr_2".into(),
            source_observation: "obs_002".into(),
        };

        let res = validator.validate(&proposal, now);
        assert!(!res.accepted);
        assert_eq!(res.feasibility, 0.0);
        assert!(!res.contradictions.is_empty());
    }
}
