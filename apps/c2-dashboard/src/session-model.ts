import {
  AdaptiveState,
  AgentState,
  CanonicalEventEnvelope,
  JudgmentEnvelope,
  OperatorStateTelemetry,
  ValidationResultPayload,
  disabledJudgmentEnvelope,
} from "@pordenone/shared-types";

/** Local standalone fallback uses the same spatial bound as EpistemicValidator. */
export const LOCAL_SPATIAL_BOUND = 10000;
export const EVENT_FEED_LIMIT = 50;

export function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

export function isVector3(value: unknown): value is { x: number; y: number; z: number } {
  return (
    isRecord(value) &&
    typeof value.x === "number" &&
    typeof value.y === "number" &&
    typeof value.z === "number"
  );
}

export function isOperatorStateTelemetry(value: unknown): value is OperatorStateTelemetry {
  return (
    isRecord(value) &&
    typeof value.heart_rate === "number" &&
    typeof value.hrv === "number" &&
    typeof value.arousal === "number" &&
    typeof value.attention === "number" &&
    typeof value.stress === "number" &&
    typeof value.confidence === "number" &&
    typeof value.cognitive_load === "number" &&
    typeof value.sensor_provenance === "string" &&
    typeof value.is_simulated === "boolean" &&
    typeof value.timestamp === "number"
  );
}

export function isAgentState(value: unknown): value is AgentState {
  return (
    isRecord(value) &&
    typeof value.agent_id === "string" &&
    typeof value.state === "string" &&
    Array.isArray(value.capabilities) &&
    value.capabilities.every((item) => typeof item === "string") &&
    Array.isArray(value.task_assignments) &&
    value.task_assignments.every((item) => typeof item === "string") &&
    typeof value.priority === "number" &&
    isVector3(value.position) &&
    isVector3(value.velocity) &&
    typeof value.confidence === "number" &&
    typeof value.timestamp === "number"
  );
}

export function isValidationResultPayload(value: unknown): value is ValidationResultPayload {
  return (
    isRecord(value) &&
    typeof value.proposal_id === "string" &&
    typeof value.agent_id === "string" &&
    typeof value.accepted === "boolean" &&
    typeof value.feasibility === "number" &&
    Array.isArray(value.contradictions) &&
    value.contradictions.every((item) => typeof item === "string") &&
    typeof value.confidence === "number" &&
    Array.isArray(value.reasons) &&
    value.reasons.every((item) => typeof item === "string") &&
    typeof value.provenance === "string" &&
    typeof value.timestamp === "number"
  );
}

export function parseIncomingEnvelope(raw: string): CanonicalEventEnvelope {
  const parsed: CanonicalEventEnvelope = JSON.parse(raw);
  if (!parsed.payload && typeof parsed.payload_json === "string") {
    try {
      const payload = JSON.parse(parsed.payload_json) as unknown;
      if (typeof payload === "object" && payload !== null) {
        parsed.payload = payload as Record<string, unknown>;
      }
    } catch {
      parsed.payload = undefined;
    }
  }
  return parsed;
}

export function policyLevelFromCognitiveLoad(cognitiveLoad: number): AdaptiveState["state"] {
  if (cognitiveLoad > 0.85) return "CRITICAL";
  if (cognitiveLoad > 0.65) return "HIGH";
  if (cognitiveLoad > 0.4) return "ELEVATED";
  return "NORMAL";
}

export function adaptiveStateFromTelemetry(telemetry: OperatorStateTelemetry): AdaptiveState {
  const cognitiveLoad = telemetry.cognitive_load;
  return {
    state: policyLevelFromCognitiveLoad(cognitiveLoad),
    stability: 1.0 - cognitiveLoad * 0.5,
    resonance: 1.0,
    adaptation_rate: 0.05,
    confidence: telemetry.confidence,
    timestamp: telemetry.timestamp,
  };
}

export function isOutOfBounds(targetX: number, targetY: number): boolean {
  return Math.abs(targetX) > LOCAL_SPATIAL_BOUND || Math.abs(targetY) > LOCAL_SPATIAL_BOUND;
}

export function localFallbackValidation(input: {
  proposalId: string;
  agentId: string;
  targetX: number;
  targetY: number;
  timestamp: number;
}): ValidationResultPayload {
  const outOfBounds = isOutOfBounds(input.targetX, input.targetY);
  return {
    proposal_id: input.proposalId,
    agent_id: input.agentId,
    accepted: !outOfBounds,
    feasibility: outOfBounds ? 0.0 : 1.0,
    contradictions: outOfBounds
      ? [`Target coordinates (${input.targetX}, ${input.targetY}) exceed bounds`]
      : [],
    confidence: 0.95,
    reasons: outOfBounds
      ? ["Proposal failed epistemic validation."]
      : ["Passed feasibility and non-contradiction checks."],
    provenance: "LocalFallbackValidator",
    timestamp: input.timestamp,
  };
}

export interface LocalStandaloneDispatch {
  validation: ValidationResultPayload;
  judgment: JudgmentEnvelope | null;
  agents: AgentState[];
}

/** Offline path used when the telemetry bridge socket is not open. */
export function localStandaloneDispatch(input: {
  agents: AgentState[];
  proposalId: string;
  agentId: string;
  targetX: number;
  targetY: number;
  correlationId: string;
  timestamp: number;
}): LocalStandaloneDispatch {
  const outOfBounds = isOutOfBounds(input.targetX, input.targetY);
  const validation = localFallbackValidation({
    proposalId: input.proposalId,
    agentId: input.agentId,
    targetX: input.targetX,
    targetY: input.targetY,
    timestamp: input.timestamp,
  });
  const judgment = outOfBounds
    ? null
    : disabledJudgmentEnvelope({
        proposal_id: input.proposalId,
        correlation_id: input.correlationId,
        causation_id: input.proposalId,
        timestamp: input.timestamp,
      });
  const agents = outOfBounds
    ? input.agents
    : input.agents.map((agent) =>
        agent.agent_id === input.agentId
          ? { ...agent, state: "EXECUTING" as const, position: { x: input.targetX, y: input.targetY, z: 0 } }
          : agent
      );
  return { validation, judgment, agents };
}

export function buildProposalEnvelope(input: {
  proposalId: string;
  agentId: string;
  actionType: string;
  targetX: number;
  targetY: number;
  correlationId: string;
  timestamp: number;
}): CanonicalEventEnvelope {
  return {
    event_id: input.proposalId,
    event_type: "proposal",
    schema_version: "1.0.0",
    timestamp: input.timestamp,
    source: "c2_dashboard",
    subject_id: input.agentId,
    correlation_id: input.correlationId,
    causation_id: input.proposalId,
    provenance: "C2Dashboard:UserAction",
    payload: {
      action_proposal: {
        proposal_id: input.proposalId,
        agent_id: input.agentId,
        action_type: input.actionType,
        parameters_json: "{}",
        target_position: { x: input.targetX, y: input.targetY, z: 0 },
        priority: 1,
        timestamp: input.timestamp,
        correlation_id: input.correlationId,
        source_observation: "user_ui_command",
      },
    },
  };
}

export function visualDensityScale(level: AdaptiveState["state"] | undefined): number {
  if (level === "CRITICAL") return 0.2;
  if (level === "HIGH") return 0.5;
  return 1.0;
}

export function alertsAreAggregated(level: AdaptiveState["state"] | undefined): boolean {
  return level === "HIGH" || level === "CRITICAL";
}

export function upsertAgent(agents: AgentState[], next: AgentState): AgentState[] {
  const idx = agents.findIndex((agent) => agent.agent_id === next.agent_id);
  if (idx >= 0) {
    const copy = [...agents];
    copy[idx] = next;
    return copy;
  }
  return [...agents, next];
}

export function prependEvent<T>(events: T[], event: T, limit = EVENT_FEED_LIMIT): T[] {
  return [event, ...events.slice(0, limit - 1)];
}

export function humanReviewApplies(
  judgment: JudgmentEnvelope | null,
  proposalId: string
): judgment is JudgmentEnvelope {
  return Boolean(
    judgment && judgment.proposal_id === proposalId && judgment.disposition === "HUMAN_REVIEW"
  );
}

export interface HumanResolutionResult {
  applied: boolean;
  event?: CanonicalEventEnvelope;
  agents: AgentState[];
}

export function resolveHumanReview(input: {
  judgment: JudgmentEnvelope | null;
  validation: ValidationResultPayload | null;
  agents: AgentState[];
  proposalId: string;
  decision: "approve" | "reject";
  timestamp: number;
}): HumanResolutionResult {
  if (!humanReviewApplies(input.judgment, input.proposalId)) {
    return { applied: false, agents: input.agents };
  }
  const judgment = input.judgment;
  const event: CanonicalEventEnvelope = {
    event_id: `human_${input.timestamp}`,
    event_type: "human_resolution",
    schema_version: "1.1.0",
    timestamp: input.timestamp,
    source: "c2_dashboard",
    subject_id: input.validation?.agent_id ?? "operator_console",
    correlation_id: judgment.correlation_id,
    causation_id: judgment.judgment_id,
    provenance: "HumanReview:operator_console",
    payload: {
      human_resolution: {
        proposal_id: input.proposalId,
        decision: input.decision,
        operator_ref: "operator_console",
        judgment_id: judgment.judgment_id,
        committed: input.decision === "approve",
      },
    },
  };
  let agents = input.agents;
  if (input.decision === "approve" && input.validation?.accepted) {
    const agentId = input.validation.agent_id;
    agents = agents.map((agent) =>
      agent.agent_id === agentId ? { ...agent, state: "EXECUTING" as const } : agent
    );
  }
  return { applied: true, event, agents };
}

export function agentsAtVisualScale<T>(agents: T[], scale: number): T[] {
  if (scale <= 0.3) {
    return agents.slice(0, Math.max(1, Math.floor(agents.length * 0.3)));
  }
  return agents;
}
