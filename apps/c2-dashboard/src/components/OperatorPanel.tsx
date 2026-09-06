 "use client";

import React from "react";
import { OperatorStateTelemetry, AdaptiveState } from "@pordenone/shared-types";

interface OperatorPanelProps {
  telemetry: OperatorStateTelemetry | null;
  adaptiveState: AdaptiveState | null;
  onToggleSimulated: (sim: boolean) => void;
}

export default function OperatorPanel({
  telemetry,
  adaptiveState,
  onToggleSimulated,
}: OperatorPanelProps) {
  const isSimulated = telemetry?.is_simulated ?? true;
  const cogLoad = telemetry?.cognitive_load ?? 0.0;
  const policyLevel = adaptiveState?.state ?? "NORMAL";

  const getPolicyColor = (level: string) => {
    switch (level) {
      case "CRITICAL":
        return "bg-redAlert text-white";
      case "HIGH":
        return "bg-amberAlert text-black";
      case "ELEVATED":
        return "bg-yellow-500 text-black";
      default:
        return "bg-greenOk text-white";
    }
  };

  return (
    <div className="bg-panel border border-panelBorder p-3 rounded flex flex-col gap-3 text-xs">
      <div className="flex items-center justify-between border-b border-panelBorder pb-2">
        <span className="font-bold tracking-wider text-cyanGlow uppercase">
          Operator Telemetry
        </span>
        <div className="flex items-center gap-2">
          <span
            className={`px-2 py-0.5 rounded font-bold text-[10px] ${
              isSimulated ? "bg-amberAlert/20 text-amberAlert border border-amberAlert/40" : "bg-greenOk/20 text-greenOk border border-greenOk/40"
            }`}
          >
            {isSimulated ? "[SIMULATED DATA]" : "[LIVE SENSOR]"}
          </span>
          <button
            onClick={() => onToggleSimulated(!isSimulated)}
            className="hover:text-cyanGlow text-[10px] underline text-[#8b949e]"
          >
            Switch to {isSimulated ? "LIVE" : "SIM"}
          </button>
        </div>
      </div>

      <div className="flex items-center justify-between bg-[#0d1117] p-2 rounded border border-panelBorder">
        <span className="text-[#8b949e]">Adaptive Policy Level:</span>
        <span className={`px-2 py-0.5 rounded font-bold text-[11px] ${getPolicyColor(policyLevel)}`}>
          {policyLevel}
        </span>
      </div>

      <div className="grid grid-cols-2 gap-2">
        <div className="bg-[#0d1117] p-2 rounded border border-panelBorder">
          <div className="text-[#8b949e] text-[10px]">Cognitive Load</div>
          <div className="text-lg font-bold text-cyanGlow">{(cogLoad * 100).toFixed(1)}%</div>
          <div className="w-full bg-[#30363d] h-1.5 rounded mt-1 overflow-hidden">
            <div
              className={`h-full ${cogLoad > 0.8 ? "bg-redAlert" : cogLoad > 0.6 ? "bg-amberAlert" : "bg-cyanGlow"}`}
              style={{ width: `${Math.min(100, cogLoad * 100)}%` }}
            />
          </div>
        </div>

        <div className="bg-[#0d1117] p-2 rounded border border-panelBorder">
          <div className="text-[#8b949e] text-[10px]">Heart Rate / HRV</div>
          <div className="text-base font-bold text-white">
            {telemetry?.heart_rate ?? 72} BPM <span className="text-xs text-[#8b949e]">({telemetry?.hrv ?? 60} ms)</span>
          </div>
          <div className="text-[10px] text-[#8b949e] mt-1">
            Arousal: {((telemetry?.arousal ?? 0.5) * 100).toFixed(0)}%
          </div>
        </div>
      </div>

      <div className="grid grid-cols-3 gap-1 text-[11px] bg-[#0d1117] p-2 rounded border border-panelBorder">
        <div>
          <span className="text-[#8b949e]">Attention:</span>{" "}
          <span className="font-semibold text-white">{((telemetry?.attention ?? 0.8) * 100).toFixed(0)}%</span>
        </div>
        <div>
          <span className="text-[#8b949e]">Stress:</span>{" "}
          <span className="font-semibold text-white">{((telemetry?.stress ?? 0.2) * 100).toFixed(0)}%</span>
        </div>
        <div>
          <span className="text-[#8b949e]">Confidence:</span>{" "}
          <span className="font-semibold text-white">{((telemetry?.confidence ?? 0.9) * 100).toFixed(0)}%</span>
        </div>
      </div>

      <div className="text-[10px] text-[#8b949e] truncate">
        Provenance: {telemetry?.sensor_provenance ?? "CIRCLE:SIMULATION:synthetic_v1"}
      </div>
    </div>
  );
}
