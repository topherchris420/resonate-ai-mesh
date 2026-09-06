 "use client";

import React from "react";
import { CanonicalEventEnvelope } from "@pordenone/shared-types";

interface EventFeedProps {
  events: CanonicalEventEnvelope[];
  aggregateAlerts: boolean;
}

export default function EventFeed({ events, aggregateAlerts }: EventFeedProps) {
  const displayEvents = React.useMemo(() => {
    if (!aggregateAlerts) return events;
    const filtered: CanonicalEventEnvelope[] = [];
    let telemetryCount = 0;

    for (const evt of events) {
      if (evt.event_type === "telemetry") {
        telemetryCount++;
        if (telemetryCount <= 3) filtered.push(evt);
      } else {
        filtered.push(evt);
      }
    }
    return filtered;
  }, [events, aggregateAlerts]);

  return (
    <div className="bg-panel border border-panelBorder p-3 rounded flex flex-col gap-2 text-xs h-full overflow-hidden">
      <div className="font-bold tracking-wider text-cyanGlow uppercase border-b border-panelBorder pb-2 flex justify-between items-center">
        <span>NEXUS Event Feed</span>
        {aggregateAlerts && (
          <span className="text-[10px] text-amberAlert font-normal border border-amberAlert/40 px-1 rounded">
            Alert Aggregation Active
          </span>
        )}
      </div>

      <div className="flex-1 overflow-y-auto space-y-1.5 pr-1 text-[11px] font-mono">
        {displayEvents.map((evt, idx) => {
          const dateStr = new Date(evt.timestamp).toLocaleTimeString();
          const isValidation = evt.event_type === "validation";
          const isTelemetry = evt.event_type === "telemetry";

          return (
            <div
              key={evt.event_id || idx}
              className={`p-1.5 rounded border ${
                isValidation
                  ? "bg-purple-950/20 border-purple-800/40 text-purple-200"
                  : isTelemetry
                  ? "bg-[#0d1117] border-panelBorder text-[#8b949e]"
                  : "bg-[#1f242d] border-panelBorder text-white"
              }`}
            >
              <div className="flex justify-between items-center text-[10px]">
                <span className="text-cyanGlow font-bold">[{evt.event_type.toUpperCase()}]</span>
                <span className="text-[#8b949e]">{dateStr}</span>
              </div>
              <div className="mt-0.5 truncate text-white">
                Src: {evt.source} | Subj: {evt.subject_id}
              </div>
              <div className="text-[9px] text-[#8b949e] truncate">
                CorrID: {evt.correlation_id}
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}
