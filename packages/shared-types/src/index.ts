export interface Vector3 {
  x: number;
  y: number;
  z: number;
}

export interface OperatorStateTelemetry {
  operator_id: string;
  cognitive_load: number;
  arousal: number;
  hrv: number;
  heart_rate: number;
  attention: number;
  stress: number;
  confidence: number;
  timestamp: number;
  sensor_provenance: string;
  is_simulated: boolean;
}

export type CognitiveLoadLevel = 'NORMAL' | 'ELEVATED' | 'HIGH' | 'CRITICAL';

export interface AdaptiveState {
  state: CognitiveLoadLevel;
  stability: number;
  resonance: number;
  adaptation_rate: number;
  confidence: number;
  timestamp: number;
}

export type AgentStatus = 'IDLE' | 'BUSY' | 'EXECUTING' | 'FAILED' | 'COMPLETED';

export interface AgentState {
  agent_id: string;
  state: AgentStatus;
  capabilities: string[];
  task_assignments: string[];
  priority: number;
  position: Vector3;
  velocity: Vector3;
  confidence: number;
  timestamp: number;
}

export interface ActionProposal {
  proposal_id: string;
  agent_id: string;
  action_type: string;
  parameters_json: string;
  target_position: Vector3;
  priority: number;
  timestamp: number;
  correlation_id: string;
  source_observation: string;
}

export interface SpatialState {
  entity_id: string;
  position: Vector3;
  orientation: Vector3;
  velocity: Vector3;
  terrain_reference: string;
  coordinate_system: string;
  timestamp: number;
  confidence: number;
}

export interface ValidationResultPayload {
  proposal_id: string;
  agent_id: string;
  accepted: boolean;
  feasibility: number;
  contradictions: string[];
  confidence: number;
  reasons: string[];
  provenance: string;
  timestamp: number;
}

export interface CanonicalEventEnvelope {
  event_id: string;
  event_type: 'telemetry' | 'adaptive_state' | 'agent_state' | 'proposal' | 'validation' | 'spatial' | 'custom';
  schema_version: string;
  timestamp: number;
  source: string;
  subject_id: string;
  correlation_id: string;
  causation_id: string;
  provenance: string;
  payload: {
    operator_telemetry?: OperatorStateTelemetry;
    adaptive_state?: AdaptiveState;
    agent_state?: AgentState;
    action_proposal?: ActionProposal;
    spatial_state?: SpatialState;
    validation_result?: ValidationResultPayload;
    custom_json_payload?: string;
  };
}
