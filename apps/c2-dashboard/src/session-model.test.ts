import { describe, expect, it } from "vitest";
import { disabledJudgmentEnvelope, type AgentState, type OperatorStateTelemetry } from "@pordenone/shared-types";
import {
  LOCAL_SPATIAL_BOUND,
  adaptiveStateFromTelemetry,
  agentsAtVisualScale,
  alertsAreAggregated,
  buildProposalEnvelope,
  isAgentState,
  isOperatorStateTelemetry,
  isValidationResultPayload,
  localStandaloneDispatch,
  parseIncomingEnvelope,
  policyLevelFromCognitiveLoad,
  prependEvent,
  resolveHumanReview,
  upsertAgent,
  visualDensityScale,
} from "./session-model";

function telemetry(cognitiveLoad: number, simulated = true): OperatorStateTelemetry {
  return {
    heart_rate: 72,
    hrv: 60,
    arousal: 0.4,
    attention: 0.8,
    stress: 0.2,
    confidence: 0.9,
    cognitive_load: cognitiveLoad,
    sensor_provenance: "CIRCLE:SIMULATION:synthetic_v1",
    is_simulated: simulated,
    timestamp: 1700000000000,
  };
}

function agent(id = "agent_alpha"): AgentState {
  return {
    agent_id: id,
    state: "IDLE",
    capabilities: ["SWARM"],
    task_assignments: ["PATROL_SECTOR_0"],
    priority: 1,
    position: { x: 1, y: 2, z: 3 },
    velocity: { x: 0, y: 0, z: 0 },
    confidence: 0.9,
    timestamp: 1,
  };
}

describe("incoming envelopes", () => {
  it("parses a payload carried as payload_json", () => {
    const raw = JSON.stringify({
      event_id: "e1",
      event_type: "telemetry",
      schema_version: "1.0.0",
      timestamp: 10,
      source: "biometric",
      subject_id: "human",
      correlation_id: "c1",
      causation_id: "c1",
      provenance: "sim",
      payload_json: JSON.stringify({ operator_telemetry: telemetry(0.2) }),
    });
    const envelope = parseIncomingEnvelope(raw);
    expect(isOperatorStateTelemetry(envelope.payload?.operator_telemetry)).toBe(true);
    expect(envelope.payload?.operator_telemetry).toMatchObject({ cognitive_load: 0.2, is_simulated: true });
  });

  it("drops a payload_json value that is not an object", () => {
    const raw = JSON.stringify({
      event_id: "e1",
      event_type: "telemetry",
      schema_version: "1.0.0",
      timestamp: 10,
      source: "biometric",
      subject_id: "human",
      correlation_id: "c1",
      causation_id: "c1",
      provenance: "sim",
      payload_json: JSON.stringify("not-an-object"),
    });
    expect(parseIncomingEnvelope(raw).payload).toBeUndefined();
  });

  it("leaves payload unset when payload_json is malformed", () => {
    const raw = JSON.stringify({
      event_id: "e1",
      event_type: "validation",
      schema_version: "1.0.0",
      timestamp: 10,
      source: "kernel",
      subject_id: "agent_alpha",
      correlation_id: "c1",
      causation_id: "c1",
      provenance: "kernel",
      payload_json: "{",
    });
    expect(parseIncomingEnvelope(raw).payload).toBeUndefined();
  });

  it("rejects partial telemetry, agents, and validation payloads", () => {
    expect(isOperatorStateTelemetry({ heart_rate: 70, is_simulated: true })).toBe(false);
    expect(isAgentState({ agent_id: "a", state: "IDLE" })).toBe(false);
    expect(isValidationResultPayload({ proposal_id: "p", accepted: true })).toBe(false);
    expect(isAgentState(agent())).toBe(true);
  });
});

describe("policy level from simulated cognitive load", () => {
  it("uses strict greater-than thresholds", () => {
    expect(policyLevelFromCognitiveLoad(0.4)).toBe("NORMAL");
    expect(policyLevelFromCognitiveLoad(0.400001)).toBe("ELEVATED");
    expect(policyLevelFromCognitiveLoad(0.65)).toBe("ELEVATED");
    expect(policyLevelFromCognitiveLoad(0.650001)).toBe("HIGH");
    expect(policyLevelFromCognitiveLoad(0.85)).toBe("HIGH");
    expect(policyLevelFromCognitiveLoad(0.850001)).toBe("CRITICAL");
  });

  it("derives adaptive state without calling a remote model", () => {
    expect(adaptiveStateFromTelemetry(telemetry(0.5))).toEqual({
      state: "ELEVATED",
      stability: 0.75,
      resonance: 1.0,
      adaptation_rate: 0.05,
      confidence: 0.9,
      timestamp: 1700000000000,
    });
  });
});

describe("local standalone dispatch", () => {
  it("accepts a target on the spatial bound and skips judgment", () => {
    const result = localStandaloneDispatch({
      agents: [agent()],
      proposalId: "prop_1",
      agentId: "agent_alpha",
      targetX: LOCAL_SPATIAL_BOUND,
      targetY: -LOCAL_SPATIAL_BOUND,
      correlationId: "corr_1",
      timestamp: 42,
    });
    expect(result.validation.accepted).toBe(true);
    expect(result.validation.feasibility).toBe(1);
    expect(result.validation.provenance).toBe("LocalFallbackValidator");
    expect(result.judgment).toMatchObject({
      disposition: "SKIPPED",
      provider_status: "disabled",
      reason_codes: ["JUDGMENT_DISABLED"],
      simulation_label: "SIMULATED",
      proposal_id: "prop_1",
    });
    expect(result.agents[0]).toMatchObject({
      state: "EXECUTING",
      position: { x: LOCAL_SPATIAL_BOUND, y: -LOCAL_SPATIAL_BOUND, z: 0 },
    });
  });

  it("rejects an out-of-bounds target and does not move the agent or request judgment", () => {
    const before = [agent()];
    const result = localStandaloneDispatch({
      agents: before,
      proposalId: "prop_2",
      agentId: "agent_alpha",
      targetX: LOCAL_SPATIAL_BOUND + 1,
      targetY: 0,
      correlationId: "corr_2",
      timestamp: 43,
    });
    expect(result.validation.accepted).toBe(false);
    expect(result.validation.feasibility).toBe(0);
    expect(result.validation.contradictions[0]).toContain("10001");
    expect(result.judgment).toBeNull();
    expect(result.agents).toBe(before);
  });

  it("builds a proposal envelope the kernel bridge can parse", () => {
    const envelope = buildProposalEnvelope({
      proposalId: "prop_3",
      agentId: "agent_beta",
      actionType: "INSPECT",
      targetX: 4,
      targetY: 5,
      correlationId: "corr_3",
      timestamp: 99,
    });
    expect(envelope).toMatchObject({
      event_id: "prop_3",
      event_type: "proposal",
      source: "c2_dashboard",
      subject_id: "agent_beta",
      correlation_id: "corr_3",
    });
    expect(envelope.payload?.action_proposal).toMatchObject({
      action_type: "INSPECT",
      target_position: { x: 4, y: 5, z: 0 },
      source_observation: "user_ui_command",
    });
  });
});

describe("human review resolution", () => {
  const judgment = {
    ...disabledJudgmentEnvelope({
      proposal_id: "prop_h",
      correlation_id: "corr_h",
      causation_id: "cause_h",
      timestamp: 7,
    }),
    judgment_id: "jdg_h",
    disposition: "HUMAN_REVIEW" as const,
  };
  const validation = {
    proposal_id: "prop_h",
    agent_id: "agent_alpha",
    accepted: true,
    feasibility: 1,
    contradictions: [],
    confidence: 0.95,
    reasons: ["Passed feasibility and non-contradiction checks."],
    provenance: "LocalFallbackValidator",
    timestamp: 7,
  };

  it("ignores a decision when judgment is not waiting on a human", () => {
    const skipped = disabledJudgmentEnvelope({
      proposal_id: "prop_h",
      correlation_id: "corr_h",
      causation_id: "cause_h",
      timestamp: 7,
    });
    const result = resolveHumanReview({
      judgment: skipped,
      validation,
      agents: [agent()],
      proposalId: "prop_h",
      decision: "approve",
      timestamp: 8,
    });
    expect(result.applied).toBe(false);
    expect(result.event).toBeUndefined();
    expect(result.agents[0].state).toBe("IDLE");
  });

  it("records approval without rewriting the model disposition", () => {
    const result = resolveHumanReview({
      judgment,
      validation,
      agents: [agent()],
      proposalId: "prop_h",
      decision: "approve",
      timestamp: 8,
    });
    expect(result.applied).toBe(true);
    expect(judgment.disposition).toBe("HUMAN_REVIEW");
    expect(result.event).toMatchObject({
      event_type: "human_resolution",
      schema_version: "1.1.0",
      correlation_id: "corr_h",
      causation_id: "jdg_h",
      payload: {
        human_resolution: {
          proposal_id: "prop_h",
          decision: "approve",
          committed: true,
          judgment_id: "jdg_h",
        },
      },
    });
    expect(result.agents[0].state).toBe("EXECUTING");
  });

  it("records rejection without moving the agent to executing", () => {
    const result = resolveHumanReview({
      judgment,
      validation,
      agents: [agent()],
      proposalId: "prop_h",
      decision: "reject",
      timestamp: 9,
    });
    expect(result.event?.payload?.human_resolution).toMatchObject({ decision: "reject", committed: false });
    expect(result.agents[0].state).toBe("IDLE");
  });
});

describe("feed, agents, and canvas density", () => {
  it("keeps the newest 50 events", () => {
    const existing = Array.from({ length: 50 }, (_, index) => ({ id: index }));
    const next = prependEvent(existing, { id: 50 });
    expect(next).toHaveLength(50);
    expect(next[0]).toEqual({ id: 50 });
    expect(next.at(-1)).toEqual({ id: 48 });
  });

  it("replaces an agent by id and appends an unknown one", () => {
    const replaced = upsertAgent([agent("a"), agent("b")], { ...agent("b"), state: "ERROR" });
    expect(replaced.map((item) => item.agent_id)).toEqual(["a", "b"]);
    expect(replaced[1].state).toBe("ERROR");
    expect(upsertAgent(replaced, agent("c"))).toHaveLength(3);
  });

  it("reduces marker density only at high and critical load", () => {
    expect(visualDensityScale("NORMAL")).toBe(1);
    expect(visualDensityScale("ELEVATED")).toBe(1);
    expect(visualDensityScale("HIGH")).toBe(0.5);
    expect(visualDensityScale("CRITICAL")).toBe(0.2);
    expect(alertsAreAggregated("ELEVATED")).toBe(false);
    expect(alertsAreAggregated("HIGH")).toBe(true);
    expect(alertsAreAggregated(undefined)).toBe(false);
    expect(agentsAtVisualScale(["a", "b", "c", "d", "e", "f", "g", "h", "i", "j"], 0.2)).toEqual([
      "a",
      "b",
      "c",
    ]);
    expect(agentsAtVisualScale(["a", "b"], 1)).toEqual(["a", "b"]);
  });
});
