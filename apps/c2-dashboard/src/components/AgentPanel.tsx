 "use client";

import React from "react";
import { AgentState } from "@pordenone/shared-types";

interface AgentPanelProps {
  agents: AgentState[];
  selectedAgentId: string | null;
  onSelectAgent: (id: string) => void;
}

export default function AgentPanel({
  agents,
  selectedAgentId,
  onSelectAgent,
}: AgentPanelProps) {
  return (
    <div className="bg-panel border border-panelBorder p-3 rounded flex flex-col gap-2 text-xs h-full overflow-hidden">
      <div className="font-bold tracking-wider text-cyanGlow uppercase border-b border-panelBorder pb-2 flex justify-between items-center">
        <span>Active Agents ({agents.length})</span>
      </div>

      <div className="flex-1 overflow-y-auto space-y-2 pr-1">
        {agents.map((agent) => {
          const isSelected = agent.agent_id === selectedAgentId;
          return (
            <div
              key={agent.agent_id}
              onClick={() => onSelectAgent(agent.agent_id)}
              className={`p-2 rounded border cursor-pointer transition-colors ${
                isSelected
                  ? "bg-cyanGlow/10 border-cyanGlow text-white"
                  : "bg-[#0d1117] border-panelBorder text-[#c9d1d9] hover:border-[#58a6ff]"
              }`}
            >
              <div className="flex justify-between items-center font-bold">
                <span className="text-cyanGlow">{agent.agent_id}</span>
                <span className="px-1.5 py-0.5 rounded text-[10px] bg-[#21262d] text-greenOk">
                  {agent.state}
                </span>
              </div>

              <div className="mt-1 text-[11px] text-[#8b949e]">
                Task: {agent.task_assignments?.[0] ?? "PATROL"}
              </div>

              <div className="flex justify-between items-center mt-1 text-[10px] text-[#8b949e]">
                <span>Pos: ({agent.position?.x?.toFixed(0)}, {agent.position?.y?.toFixed(0)})</span>
                <span>Conf: {((agent.confidence ?? 0.9) * 100).toFixed(0)}%</span>
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}
