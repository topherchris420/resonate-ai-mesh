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
  JudgmentEnvelope,
  isJudgmentEnvelope,
} from "@pordenone/shared-types";
import {
  adaptiveStateFromTelemetry,
  alertsAreAggregated,
  buildProposalEnvelope,
  isAgentState,
  isOperatorStateTelemetry,
  isValidationResultPayload,
  localStandaloneDispatch,
  parseIncomingEnvelope,
  prependEvent,
  resolveHumanReview,
  upsertAgent,
  visualDensityScale,
} from "@/session-model";

const SpatialCanvas = dynamic(() => import("@/components/SpatialCanvas"), {
  ssr: false,
  loading: () => <div className="w-full h-full bg-[#090d13] flex items-center justify-center text-xs text-[#8b949e]">Loading 3D Spatial Canvas...</div>,
});

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
  const [latestJudgment, setLatestJudgment] = useState<JudgmentEnvelope | null>(null);
  const [humanDecision, setHumanDecision] = useState<"approve" | "reject" | null>(null);
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
          const envelope = parseIncomingEnvelope(event.data);
          setEvents((prev) => prependEvent(prev, envelope));

          const operatorTelemetry = envelope.payload?.operator_telemetry;
          if (envelope.event_type === "telemetry" && isOperatorStateTelemetry(operatorTelemetry)) {
            const t = operatorTelemetry;
            setTelemetry(t);
            setAdaptiveState(adaptiveStateFromTelemetry(t));
          }

          const validationResult = envelope.payload?.validation_result ?? envelope.payload;
          if (envelope.event_type === "validation" && isValidationResultPayload(validationResult)) {
            setLatestValidation(validationResult);
            setHumanDecision(null);
            if (!validationResult.accepted) {
              setLatestJudgment(null);
            }
          }

          const judgmentResult = envelope.payload?.judgment ?? envelope.payload;
          if (envelope.event_type === "judgment" && isJudgmentEnvelope(judgmentResult)) {
            setLatestJudgment(judgmentResult);
            setHumanDecision(null);
          }

          const agentState = envelope.payload?.agent_state;
          if (envelope.event_type === "agent_state" && isAgentState(agentState)) {
            const newAgent = agentState;
            setAgents((prev) => upsertAgent(prev, newAgent));
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
    const timestamp = Date.now();
    const proposalId = `prop_${timestamp}`;
    const agentId = selectedAgentId || "agent_alpha";
    const correlationId = `corr_${timestamp}`;

    const proposalEnvelope: CanonicalEventEnvelope = buildProposalEnvelope({
      proposalId,
      agentId,
      actionType,
      targetX,
      targetY,
      correlationId,
      timestamp,
    });

    if (wsRef.current && wsRef.current.readyState === WebSocket.OPEN) {
      wsRef.current.send(JSON.stringify(proposalEnvelope));
    } else {
      const fallback = localStandaloneDispatch({
        agents,
        proposalId,
        agentId,
        targetX,
        targetY,
        correlationId,
        timestamp,
      });
      setLatestValidation(fallback.validation);
      setHumanDecision(null);
      setLatestJudgment(fallback.judgment);
      setAgents(fallback.agents);
    }
  };

  const handleResolveHumanReview = (proposalId: string, decision: "approve" | "reject") => {
    const resolution = resolveHumanReview({
      judgment: latestJudgment,
      validation: latestValidation,
      agents,
      proposalId,
      decision,
      timestamp: Date.now(),
    });
    if (!resolution.applied || !resolution.event) {
      return;
    }
    setHumanDecision(decision);
    setEvents((prev) => prependEvent(prev, resolution.event as CanonicalEventEnvelope));
    if (wsRef.current && wsRef.current.readyState === WebSocket.OPEN) {
      wsRef.current.send(JSON.stringify(resolution.event));
    }
    setAgents(resolution.agents);
  };

  const visualScale = visualDensityScale(adaptiveState?.state);
  const aggregateAlerts = alertsAreAggregated(adaptiveState?.state);

  return (
    <div className="flex flex-col h-screen w-screen overflow-hidden bg-[#0d1117] text-[#c9d1d9]">
      <header className="h-12 border-b border-panelBorder bg-[#161b22] px-4 flex items-center justify-between shrink-0">
        <div className="flex items-center gap-3">
          <span className="font-extrabold tracking-widest text-cyanGlow text-sm">PORDENONE</span>
          <span className="text-xs text-[#8b949e]">| Deterministic validation · simulated telemetry</span>
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
            latestJudgment={latestJudgment}
            humanDecision={humanDecision}
            onDispatchProposal={handleDispatchProposal}
            onResolveHumanReview={handleResolveHumanReview}
          />
          <div className="flex-1 overflow-hidden">
            <EventFeed events={events} aggregateAlerts={aggregateAlerts} />
          </div>
        </div>
      </div>
    </div>
  );
}
