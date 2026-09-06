export interface CanonicalEventEnvelope {
  event_id: string;
  event_type: string;
  schema_version: string;
  timestamp: number;
  source: string;
  subject_id: string;
  correlation_id: string;
  causation_id: string;
  provenance: string;
  payload?: Record<string, unknown>;
}

export interface OperatorStateTelemetry {
  heart_rate: number;
  hrv: number;
  arousal: number;
  attention: number;
  stress: number;
  confidence: number;
  cognitive_load: number;
  sensor_provenance: string;
  is_simulated: boolean;
  timestamp: number;
}

export interface AdaptiveState {
  state: "NORMAL" | "ELEVATED" | "HIGH" | "CRITICAL";
  stability: number;
  resonance: number;
  adaptation_rate: number;
  confidence: number;
  timestamp: number;
}

export interface AgentState {
  agent_id: string;
  state: "IDLE" | "EXECUTING" | "WAITING" | "ERROR";
  capabilities: string[];
  task_assignments: string[];
  priority: number;
  position: { x: number; y: number; z: number };
  velocity: { x: number; y: number; z: number };
  confidence: number;
  timestamp: number;
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

export interface ActionProposal {
  proposal_id: string;
  agent_id: string;
  action_type: string;
  parameters_json: string;
  target_position: { x: number; y: number; z: number };
  priority: number;
  timestamp: number;
  correlation_id: string;
  source_observation: string;
}
