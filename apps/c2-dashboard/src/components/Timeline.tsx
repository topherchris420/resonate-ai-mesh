"use client";

import React from "react";
import type { DecisionView, RunModel } from "@/lib/model";
import { OUTCOME_COLOR } from "./ui";
import { useWidth } from "./useWidth";

const LABEL_W = 92;
const ROW_H = 22;
const LOAD_H = 40;
const FAULT_H = 20;
const AXIS_H = 18;


/**
 * Every decision in the run on one time axis: operator load and gating level,
 * injected faults, and each agent's decisions colored by outcome. Selecting a
 * decision highlights every decision that shares its proposal (a withheld
 * proposal and its later human resolution) and links them.
 */
export default function Timeline({
  model,
  tick,
  onTick,
  selected,
  onSelect,
}: {
  model: RunModel;
  tick: number;
  onTick: (tick: number) => void;
  selected: DecisionView | null;
  onSelect: (decision: DecisionView) => void;
}) {
  const [ref, width] = useWidth<HTMLDivElement>();
  const plotW = width - LABEL_W - 8;
  const x = (t: number) => LABEL_W + ((t + 0.5) / model.ticks) * plotW;
  const rows = model.agentIds;
  const top = AXIS_H;
  // Overlapping faults stack in lanes so their labels stay readable.
  const laneEnds: number[] = [];
  const faultLanes = model.faults.map((f) => {
    let lane = laneEnds.findIndex((end) => end <= f.start);
    if (lane < 0) lane = laneEnds.push(0) - 1;
    laneEnds[lane] = f.end;
    return lane;
  });
  const faultRows = Math.max(1, laneEnds.length);
  const faultTop = top + LOAD_H + 6;
  const agentTop = faultTop + faultRows * FAULT_H + 6;
  const height = agentTop + rows.length * ROW_H + 6;
  const thresholds = model.scenario?.kernel.policy.adaptive.thresholds ?? [];
  const related = selected ? model.decisions.filter((d) => d.proposalId === selected.proposalId) : [];
  const rowOf = (agent: string) => rows.indexOf(agent);
  const loadPoints = model.load
    .map((v, t) => (v === null ? null : `${x(t)},${top + LOAD_H - v * LOAD_H}`))
    .filter(Boolean)
    .join(" ");

  const pick = (event: React.MouseEvent<SVGSVGElement>) => {
    const box = event.currentTarget.getBoundingClientRect();
    const px = event.clientX - box.left;
    if (px < LABEL_W) return;
    const t = Math.floor(((px - LABEL_W) / plotW) * model.ticks);
    onTick(Math.max(0, Math.min(model.ticks - 1, t)));
  };

  return (
    <div ref={ref} className="w-full">
      <svg width={width} height={height} className="block cursor-crosshair select-none" onClick={pick} role="img" aria-label="Causal timeline">
        {Array.from({ length: Math.floor((model.ticks - 1) / 10) + 1 }, (_, i) => i * 10).map((t) => (
          <g key={t}>
            <line x1={x(t)} y1={AXIS_H - 4} x2={x(t)} y2={height} stroke="rgb(var(--line))" strokeWidth={1} />
            <text x={x(t)} y={11} textAnchor="middle" fontSize={10} fill="rgb(var(--muted))">
              t{t}
            </text>
          </g>
        ))}

        <text x={4} y={top + 14} fontSize={10} fill="rgb(var(--muted))">
          operator load
        </text>
        <text x={4} y={top + 27} fontSize={9} fill="rgb(var(--muted) / 0.7)">
          (simulated)
        </text>
        {model.levels.map((level, t) =>
          level === "HIGH" || level === "CRITICAL" ? (
            <rect key={t} x={x(t) - plotW / model.ticks / 2} y={top} width={plotW / model.ticks} height={LOAD_H} fill={`rgb(var(--withhold) / ${level === "CRITICAL" ? 0.22 : 0.12})`}>
              <title>{`t${t}: operator level ${level} — background work deferred`}</title>
            </rect>
          ) : null,
        )}
        {thresholds.map((th) => (
          <line key={th} x1={LABEL_W} x2={LABEL_W + plotW} y1={top + LOAD_H - th * LOAD_H} y2={top + LOAD_H - th * LOAD_H} stroke="rgb(var(--line))" strokeDasharray="2 3" />
        ))}
        <polyline points={loadPoints} fill="none" stroke="rgb(var(--accent))" strokeWidth={1.4} />
        {model.load.map((v, t) =>
          v === null && t > 0 ? <rect key={`gap-${t}`} x={x(t) - 1} y={top + LOAD_H - 3} width={2} height={3} fill="rgb(var(--reject))"><title>{`t${t}: no usable operator-load signal`}</title></rect> : null,
        )}

        <text x={4} y={faultTop + 14} fontSize={10} fill="rgb(var(--muted))">
          faults
        </text>
        {model.faults.map((f, i) => (
          <g key={`${f.label}-${i}`}>
            <rect x={x(f.start) - plotW / model.ticks / 2} y={faultTop + faultLanes[i] * FAULT_H + 3} width={Math.max(3, ((f.end - f.start) / model.ticks) * plotW)} height={FAULT_H - 6} rx={3} fill="rgb(var(--withhold) / 0.25)" stroke="rgb(var(--withhold) / 0.7)">
              <title>{f.label}</title>
            </rect>
            <text x={x(f.start) - plotW / model.ticks / 2 + 4} y={faultTop + faultLanes[i] * FAULT_H + 14} fontSize={9.5} fill="rgb(var(--withhold))">
              {f.kind}
              {f.target ? `:${f.target}` : ""}
            </text>
          </g>
        ))}
        {model.hazards
          .filter((h) => h.activeFrom > 0)
          .map((h) => (
            <g key={h.id}>
              <path d={`M ${x(h.activeFrom)} ${faultTop + faultRows * FAULT_H - 2} l -4 6 l 8 0 z`} fill="rgb(var(--hazard))">
                <title>{`t${h.activeFrom}: hazard ${h.id} appeared`}</title>
              </path>
            </g>
          ))}

        {rows.map((agent, i) => (
          <g key={agent}>
            <line x1={LABEL_W} x2={LABEL_W + plotW} y1={agentTop + i * ROW_H + ROW_H / 2} y2={agentTop + i * ROW_H + ROW_H / 2} stroke="rgb(var(--line) / 0.6)" />
            <text x={4} y={agentTop + i * ROW_H + ROW_H / 2 + 3.5} fontSize={10.5} fill="rgb(var(--text) / 0.85)">
              {agent}
            </text>
          </g>
        ))}

        {related.length > 1 &&
          related.slice(1).map((d, i) => {
            const a = related[i];
            const ra = rowOf(a.agentId);
            const rb = rowOf(d.agentId);
            if (ra < 0 || rb < 0) return null;
            const y0 = agentTop + ra * ROW_H + ROW_H / 2;
            const y1 = agentTop + rb * ROW_H + ROW_H / 2;
            const midX = (x(a.tick) + x(d.tick)) / 2;
            return <path key={d.key} d={`M ${x(a.tick)} ${y0} Q ${midX} ${y0 - 16} ${x(d.tick)} ${y1}`} fill="none" stroke="rgb(var(--review))" strokeWidth={1.2} strokeDasharray="3 2" />;
          })}

        {model.decisions.map((d) => {
          const r = rowOf(d.agentId);
          if (r < 0) return null;
          const cx = x(d.tick);
          const cy = agentTop + r * ROW_H + ROW_H / 2;
          const outcome = d.policy?.outcome ?? "pending";
          const color = OUTCOME_COLOR[outcome] ?? "rgb(var(--muted))";
          const isSelected = selected?.key === d.key;
          const isRelated = related.some((x) => x.key === d.key);
          const size = isSelected ? 5 : 3.4;
          return (
            <g
              key={d.key}
              className="cursor-pointer"
              onClick={(event) => {
                event.stopPropagation();
                onSelect(d);
                onTick(d.tick);
              }}
            >
              <title>{`t${d.tick} ${d.agentId} ${d.kind === "human_resolution" ? `human ${d.humanDecision}` : d.actionType} → ${outcome}`}</title>
              {d.kind === "human_resolution" ? (
                <path d={`M ${cx} ${cy - size - 1} L ${cx + size + 1} ${cy} L ${cx} ${cy + size + 1} L ${cx - size - 1} ${cy} z`} fill={color} stroke="rgb(var(--review))" strokeWidth={1.2} />
              ) : (
                <circle cx={cx} cy={cy} r={size} fill={color} opacity={d.duplicate ? 0.5 : 1} />
              )}
              {(isSelected || isRelated) && <circle cx={cx} cy={cy} r={size + 3} fill="none" stroke="rgb(var(--text))" strokeWidth={1} />}
            </g>
          );
        })}

        <line x1={x(tick)} x2={x(tick)} y1={AXIS_H - 4} y2={height} stroke="rgb(var(--accent))" strokeWidth={1.5} />
      </svg>
    </div>
  );
}
