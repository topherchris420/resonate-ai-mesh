 "use client";

import React, { useEffect, useState, useRef } from "react";
import dynamic from "next/dynamic";
import OperatorPanel from "@/components/OperatorPanel";
import AgentPanel from "@/components/AgentPanel";
import EventFeed from "@/components/EventFeed";
import ValidationInspector from "@/components/ValidationInspector";
import {
  OperatorStateTelemetry,
  AdaptiveState,
  AgentState,
  CanonicalEventEnvelope,
  ValidationResultPayload,
} from "@pordenone/shared-types";

const SpatialCanvas = dynamic(() => import("@/components/SpatialCanvas"), {
  ssr: false,
  loading: () => <div className="w-full h-full bg-[#090d13] flex items-center justify-center text-xs text-[#8b949e]">Loading 3D Spatial Canvas...</div>,
});

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

function isVector3(value: unknown): value is { x: number; y: number; z: number } {
  return (
    isRecord(value) &&
    typeof value.x === "number" &&
    typeof value.y === "number" &&
    typeof value.z === "number"
  );
}

function isOperatorStateTelemetry(value: unknown): value is OperatorStateTelemetry {
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

function isAgentState(value: unknown): value is AgentState {
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

function isValidationResultPayload(value: unknown): value is ValidationResultPayload {
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

export default function C2DashboardPage() {
  const [wsConnected, setWsConnected] = useState(false);
  const [telemetry, setTelemetry] = useState<OperatorStateTelemetry | null>(null);
  const [adaptiveState, setAdaptiveState] = useState<AdaptiveState | null>(null);
  const [agents, setAgents] = useState<AgentState[]>([
    {
      agent_id: "agent_alpha",
      state: "EXECUTING",
      capabilities: ["SWARM", "RECON"],
      task_assignments: ["PATROL_SECTOR_0"],
      priority: 1,
      position: { x: 100, y: 50, z: 0 },
      velocity: { x: 2, y: 1, z: 0 },
      confidence: 0.95,
      timestamp: Date.now(),
    },
    {
      agent_id: "agent_beta",
      state: "EXECUTING",
      capabilities: ["SWARM", "SURVEILLANCE"],
      task_assignments: ["PATROL_SECTOR_1"],
      priority: 1,
      position: { x: -150, y: 120, z: 0 },
      velocity: { x: -1, y: 2, z: 0 },
      confidence: 0.92,
      timestamp: Date.now(),
    },
  ]);
  const [selectedAgentId, setSelectedAgentId] = useState<string | null>("agent_alpha");
  const [events, setEvents] = useState<CanonicalEventEnvelope[]>([]);
  const [latestValidation, setLatestValidation] = useState<ValidationResultPayload | null>(null);
  const [isRecording, setIsRecording] = useState(false);

  const wsRef = useRef<WebSocket | null>(null);

  useEffect(() => {
    const wsUrl = process.env.NEXT_PUBLIC_WS_URL || "ws://localhost:8080";
    let ws: WebSocket;

    try {
      ws = new WebSocket(wsUrl);
      wsRef.current = ws;

      ws.onopen = () => {
        setWsConnected(true);
      };

      ws.onclose = () => {
        setWsConnected(false);
      };

      ws.onmessage = (event) => {
        try {
          const envelope: CanonicalEventEnvelope = JSON.parse(event.data);
          setEvents((prev) => [envelope, ...prev.slice(0, 49)]);

          const operatorTelemetry = envelope.payload?.operator_telemetry;
          if (envelope.event_type === "telemetry" && isOperatorStateTelemetry(operatorTelemetry)) {
            const t = operatorTelemetry;
            setTelemetry(t);

            const cogLoad = t.cognitive_load;
            const policyLevel = cogLoad > 0.85 ? "CRITICAL" : cogLoad > 0.65 ? "HIGH" : cogLoad > 0.4 ? "ELEVATED" : "NORMAL";
            setAdaptiveState({
              state: policyLevel,
              stability: 1.0 - cogLoad * 0.5,
              resonance: 1.0,
              adaptation_rate: 0.05,
              confidence: t.confidence,
              timestamp: t.timestamp,
            });
          }

          const validationResult = envelope.payload?.validation_result;
          if (envelope.event_type === "validation" && isValidationResultPayload(validationResult)) {
            setLatestValidation(validationResult);
          }

          const agentState = envelope.payload?.agent_state;
          if (envelope.event_type === "agent_state" && isAgentState(agentState)) {
            const newAgent = agentState;
            setAgents((prev) => {
              const idx = prev.findIndex((a) => a.agent_id === newAgent.agent_id);
              if (idx >= 0) {
                const copy = [...prev];
                copy[idx] = newAgent;
                return copy;
              }
              return [...prev, newAgent];
            });
          }
        } catch (e) {
          console.warn("Error parsing WebSocket message", e);
        }
      };
    } catch (e) {
      console.warn("WebSocket setup failed", e);
    }

    return () => {
      wsRef.current?.close();
    };
  }, []);

  const handleDispatchProposal = (actionType: string, targetX: number, targetY: number) => {
    const proposalId = `prop_${Date.now()}`;
    const agentId = selectedAgentId || "agent_alpha";
    const correlationId = `corr_${Date.now()}`;

    const proposalEnvelope: CanonicalEventEnvelope = {
      event_id: proposalId,
      event_type: "proposal",
      schema_version: "1.0.0",
      timestamp: Date.now(),
      source: "c2_dashboard",
      subject_id: agentId,
      correlation_id: correlationId,
      causation_id: proposalId,
      provenance: "C2Dashboard:UserAction",
      payload: {
        action_proposal: {
          proposal_id: proposalId,
          agent_id: agentId,
          action_type: actionType,
          parameters_json: "{}",
          target_position: { x: targetX, y: targetY, z: 0 },
          priority: 1,
          timestamp: Date.now(),
          correlation_id: correlationId,
          source_observation: "user_ui_command",
        },
      },
    };

    if (wsRef.current && wsRef.current.readyState === WebSocket.OPEN) {
      wsRef.current.send(JSON.stringify(proposalEnvelope));
    } else {
      const isOutOfBounds = Math.abs(targetX) > 10000 || Math.abs(targetY) > 10000;
      const validationPayload: ValidationResultPayload = {
        proposal_id: proposalId,
        agent_id: agentId,
        accepted: !isOutOfBounds,
        feasibility: isOutOfBounds ? 0.0 : 1.0,
        contradictions: isOutOfBounds ? [`Target coordinates (${targetX}, ${targetY}) exceed bounds`] : [],
        confidence: 0.95,
        reasons: isOutOfBounds ? ["Proposal failed epistemic validation."] : ["Passed feasibility and non-contradiction checks."],
        provenance: "LocalFallbackValidator",
        timestamp: Date.now(),
      };

      setLatestValidation(validationPayload);

      if (!isOutOfBounds) {
        setAgents((prev) =>
          prev.map((a) =>
            a.agent_id === agentId
              ? { ...a, state: "EXECUTING", position: { x: targetX, y: targetY, z: 0 } }
              : a
          )
        );
      }
    }
  };

  const visualScale = adaptiveState?.state === "CRITICAL" ? 0.2 : adaptiveState?.state === "HIGH" ? 0.5 : 1.0;
  const aggregateAlerts = adaptiveState?.state === "HIGH" || adaptiveState?.state === "CRITICAL";

  return (
    <div className="flex flex-col h-screen w-screen overflow-hidden bg-[#0d1117] text-[#c9d1d9]">
      <header className="h-12 border-b border-panelBorder bg-[#161b22] px-4 flex items-center justify-between shrink-0">
        <div className="flex items-center gap-3">
          <span className="font-extrabold tracking-widest text-cyanGlow text-sm">PORDENONE NEXUS C2</span>
          <span className="text-xs text-[#8b949e]">| Cognitive Cyber-Physical Command Platform</span>
        </div>

        <div className="flex items-center gap-4 text-xs">
          <div className="flex items-center gap-1.5">
            <span className={`w-2 h-2 rounded-full ${wsConnected ? "bg-greenOk" : "bg-redAlert"}`} />
            <span>{wsConnected ? "Bridge Connected" : "Local Standalone Mode"}</span>
          </div>

          <button
            onClick={() => setIsRecording(!isRecording)}
            className={`px-2 py-0.5 rounded text-[11px] font-bold border transition-colors ${
              isRecording ? "bg-redAlert text-white border-redAlert animate-pulse" : "bg-[#21262d] text-[#8b949e] border-panelBorder hover:text-white"
            }`}
          >
            {isRecording ? "● RECORDING SESSION" : "RECORD SESSION"}
          </button>
        </div>
      </header>

      <div className="flex-1 grid grid-cols-12 gap-2 p-2 overflow-hidden">
        <div className="col-span-3 flex flex-col gap-2 overflow-hidden">
          <OperatorPanel
            telemetry={telemetry}
            adaptiveState={adaptiveState}
            onToggleSimulated={(sim) => {
              if (telemetry) {
                setTelemetry({ ...telemetry, is_simulated: sim });
              }
            }}
          />
          <div className="flex-1 overflow-hidden">
            <AgentPanel
              agents={agents}
              selectedAgentId={selectedAgentId}
              onSelectAgent={setSelectedAgentId}
            />
          </div>
        </div>

        <div className="col-span-6 rounded border border-panelBorder overflow-hidden relative">
          <SpatialCanvas
            agents={agents}
            selectedAgentId={selectedAgentId}
            onSelectAgent={setSelectedAgentId}
            visualDensityScale={visualScale}
          />
        </div>

        <div className="col-span-3 flex flex-col gap-2 overflow-hidden">
          <ValidationInspector
            latestValidation={latestValidation}
            onDispatchProposal={handleDispatchProposal}
          />
          <div className="flex-1 overflow-hidden">
            <EventFeed events={events} aggregateAlerts={aggregateAlerts} />
          </div>
        </div>
      </div>
    </div>
  );
}
