 "use client";

import React from "react";
import { ValidationResultPayload } from "@pordenone/shared-types";

interface ValidationInspectorProps {
  latestValidation: ValidationResultPayload | null;
  onDispatchProposal: (actionType: string, targetX: number, targetY: number) => void;
}

export default function ValidationInspector({
  latestValidation,
  onDispatchProposal,
}: ValidationInspectorProps) {
  const [actionType, setActionType] = React.useState("PATROL");
  const [targetX, setTargetX] = React.useState(200);
  const [targetY, setTargetY] = React.useState(150);

  return (
    <div className="bg-panel border border-panelBorder p-3 rounded flex flex-col gap-3 text-xs">
      <div className="font-bold tracking-wider text-cyanGlow uppercase border-b border-panelBorder pb-2">
        Epistemic Validation Inspector
      </div>

      {latestValidation ? (
        <div className="bg-[#0d1117] p-2.5 rounded border border-panelBorder flex flex-col gap-1.5">
          <div className="flex justify-between items-center">
            <span className="font-bold text-white">Agent: {latestValidation.agent_id}</span>
            <span
              className={`px-2 py-0.5 rounded font-bold text-[10px] ${
                latestValidation.accepted ? "bg-greenOk/20 text-greenOk border border-greenOk/40" : "bg-redAlert/20 text-redAlert border border-redAlert/40"
              }`}
            >
              {latestValidation.accepted ? "ACCEPTED" : "REJECTED"}
            </span>
          </div>

          <div className="grid grid-cols-2 gap-2 text-[10px] text-[#8b949e]">
            <div>Feasibility: {(latestValidation.feasibility * 100).toFixed(0)}%</div>
            <div>Confidence: {(latestValidation.confidence * 100).toFixed(0)}%</div>
          </div>

          {latestValidation.reasons && latestValidation.reasons.length > 0 && (
            <div className="text-[10px] text-white bg-[#161b22] p-1.5 rounded">
              {latestValidation.reasons.join(" | ")}
            </div>
          )}

          {latestValidation.contradictions && latestValidation.contradictions.length > 0 && (
            <div className="text-[10px] text-redAlert bg-red-950/20 p-1.5 rounded border border-red-800/40">
              Contradictions: {latestValidation.contradictions.join(", ")}
            </div>
          )}

          <div className="text-[9px] text-[#8b949e] truncate">
            Provenance: {latestValidation.provenance}
          </div>
        </div>
      ) : (
        <div className="text-[#8b949e] text-[11px] italic bg-[#0d1117] p-2 rounded border border-panelBorder">
          No proposals validated yet. Dispatch a proposal below to run Epistemic Validation.
        </div>
      )}

      <div className="border-t border-panelBorder pt-2 flex flex-col gap-2">
        <div className="font-bold text-[#8b949e] text-[10px]">Dispatch Agent Proposal</div>
        <div className="grid grid-cols-3 gap-2">
          <select
            value={actionType}
            onChange={(e) => setActionType(e.target.value)}
            className="bg-[#0d1117] border border-panelBorder rounded p-1 text-[#c9d1d9]"
          >
            <option value="PATROL">PATROL</option>
            <option value="MOVE">MOVE</option>
            <option value="INSPECT">INSPECT</option>
            <option value="INVALID_OUT_OF_BOUNDS">INVALID BOUNDS</option>
          </select>

          <input
            type="number"
            value={targetX}
            onChange={(e) => setTargetX(Number(e.target.value))}
            placeholder="Target X"
            className="bg-[#0d1117] border border-panelBorder rounded p-1 text-[#c9d1d9]"
          />

          <input
            type="number"
            value={targetY}
            onChange={(e) => setTargetY(Number(e.target.value))}
            placeholder="Target Y"
            className="bg-[#0d1117] border border-panelBorder rounded p-1 text-[#c9d1d9]"
          />
        </div>

        <button
          onClick={() => {
            const x = actionType === "INVALID_OUT_OF_BOUNDS" ? 999999 : targetX;
            onDispatchProposal(actionType === "INVALID_OUT_OF_BOUNDS" ? "MOVE" : actionType, x, targetY);
          }}
          className="bg-cyanGlow/20 hover:bg-cyanGlow/30 border border-cyanGlow/50 text-cyanGlow font-bold py-1 px-3 rounded text-xs transition-colors"
        >
          Dispatch Proposal to Kernel
        </button>
      </div>
    </div>
  );
}
