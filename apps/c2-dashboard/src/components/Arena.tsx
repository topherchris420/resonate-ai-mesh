"use client";

import React from "react";
import type { DecisionView, RunModel } from "@/lib/model";
import { OUTCOME_COLOR } from "./ui";

const PAD = 8;

function finite(target: (number | null)[] | null): [number, number] | null {
  if (!target) return null;
  const [x, y] = target;
  return typeof x === "number" && typeof y === "number" ? [x, y] : null;
}

/** Top-down view of the arena at one tick, drawn from recorded events only. */
export default function Arena({
  model,
  tick,
  selected,
  onSelect,
}: {
  model: RunModel;
  tick: number;
  selected: DecisionView | null;
  onSelect: (decision: DecisionView) => void;
}) {
  const half = model.scenario?.arena_half_extent ?? 100;
  const extent = half + PAD;
  const y = (v: number) => -v; // world y points up
  const decisions = model.decisionsByTick[tick] ?? [];
  const activeFaults = model.faults.filter((f) => tick >= f.start && tick < f.end);
  const faulted = (agent: string) => activeFaults.filter((f) => !f.target || f.target === "all" || f.target === agent);
  const grid: number[] = [];
  for (let v = -Math.floor(half / 25) * 25; v <= half; v += 25) grid.push(v);

  return (
    <svg
      viewBox={`${-extent} ${-extent} ${2 * extent} ${2 * extent}`}
      className="h-full w-full select-none"
      role="img"
      aria-label={`Arena at tick ${tick}`}
    >
      <defs>
        {Object.entries(OUTCOME_COLOR).map(([outcome, color]) => (
          <marker key={outcome} id={`arrow-${outcome}`} viewBox="0 0 10 10" refX="8" refY="5" markerWidth="5" markerHeight="5" orient="auto-start-reverse">
            <path d="M 0 0 L 10 5 L 0 10 z" fill={color} />
          </marker>
        ))}
      </defs>
      <rect x={-half} y={-half} width={2 * half} height={2 * half} fill="rgb(var(--panel))" stroke="rgb(var(--line))" strokeWidth={0.4} />
      {grid.map((v) => (
        <g key={v} stroke="rgb(var(--line))" strokeWidth={0.15}>
          <line x1={v} y1={-half} x2={v} y2={half} />
          <line x1={-half} y1={v} x2={half} y2={v} />
        </g>
      ))}

      {model.hazards.map((h) => {
        const active = h.activeFrom <= tick && (h.activeUntil === null || tick < h.activeUntil);
        const future = h.activeFrom > tick;
        return (
          <g key={h.id} opacity={active ? 1 : 0.45}>
            <circle
              cx={h.center[0]}
              cy={y(h.center[1])}
              r={h.radius}
              fill={active ? "rgb(var(--hazard) / 0.16)" : "none"}
              stroke="rgb(var(--hazard))"
              strokeWidth={active ? 0.5 : 0.3}
              strokeDasharray={active ? undefined : "1.5 1.5"}
            />
            <text x={h.center[0]} y={y(h.center[1]) + 1.2} textAnchor="middle" fontSize={3.2} fill="rgb(var(--hazard))">
              {h.id}
              {future ? ` · t${h.activeFrom}` : ""}
            </text>
          </g>
        );
      })}

      {model.scenario?.points_of_interest.map((p) => (
        <g key={p.id}>
          <rect x={p.position[0] - 1.4} y={y(p.position[1]) - 1.4} width={2.8} height={2.8} transform={`rotate(45 ${p.position[0]} ${y(p.position[1])})`} fill="none" stroke="rgb(var(--accent))" strokeWidth={0.4} />
          <text x={p.position[0]} y={y(p.position[1]) - 3} textAnchor="middle" fontSize={2.8} fill="rgb(var(--muted))">
            {p.id}
          </text>
        </g>
      ))}

      {model.agentIds.map((agent) => {
        const trail = model.positions[agent]?.slice(0, tick + 1).filter((p): p is [number, number] => p !== null) ?? [];
        return (
          <polyline
            key={`trail-${agent}`}
            points={trail.map(([px, py]) => `${px},${y(py)}`).join(" ")}
            fill="none"
            stroke="rgb(var(--text) / 0.18)"
            strokeWidth={0.35}
          />
        );
      })}

      {decisions
        .filter((d) => d.kind === "proposal")
        .map((d) => {
          const to = finite(d.target);
          if (!to || !d.from) return null;
          const outcome = d.policy?.outcome ?? "pending";
          const color = OUTCOME_COLOR[outcome] ?? "rgb(var(--muted))";
          const isSelected = selected?.key === d.key;
          return (
            <g key={d.key} className="cursor-pointer" onClick={() => onSelect(d)}>
              <title>{`${d.agentId} ${d.actionType} → ${outcome}${d.validation && !d.validation.accepted ? ` (${d.validation.reasons.join(", ")})` : ""}`}</title>
              <line
                x1={d.from[0]}
                y1={y(d.from[1])}
                x2={to[0]}
                y2={y(to[1])}
                stroke={color}
                strokeWidth={isSelected ? 1 : 0.55}
                strokeDasharray={outcome === "rejected_deterministic" ? "1.2 0.8" : undefined}
                markerEnd={`url(#arrow-${outcome in OUTCOME_COLOR ? outcome : "committed"})`}
              />
              {isSelected && <circle cx={to[0]} cy={y(to[1])} r={2.4} fill="none" stroke={color} strokeWidth={0.5} />}
              <line x1={d.from[0]} y1={y(d.from[1])} x2={to[0]} y2={y(to[1])} stroke="transparent" strokeWidth={4} />
            </g>
          );
        })}

      {model.agentIds.map((agent) => {
        const at = model.positions[agent]?.[tick];
        if (!at) return null;
        const faults = faulted(agent);
        const isSelected = selected?.agentId === agent;
        return (
          <g key={agent}>
            {faults.length > 0 && (
              <circle cx={at[0]} cy={y(at[1])} r={4.2} fill="none" stroke="rgb(var(--withhold))" strokeWidth={0.45} strokeDasharray="1 1">
                <title>{faults.map((f) => f.label).join(", ")}</title>
              </circle>
            )}
            <circle cx={at[0]} cy={y(at[1])} r={isSelected ? 2.3 : 1.8} fill="rgb(var(--text))" stroke="rgb(var(--ink))" strokeWidth={0.4} />
            <text x={at[0] + 3} y={y(at[1]) + 1} fontSize={3} fill="rgb(var(--text) / 0.85)">
              {agent}
            </text>
          </g>
        );
      })}
    </svg>
  );
}
