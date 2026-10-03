"use client";

import React, { useMemo, useState } from "react";
import { Badge, Empty, ErrorNote, Loading, Panel, Select, cx } from "@/components/ui";
import type { MeshSource } from "@/lib/source";
import type { RunListing, ScenarioInfo, Topology, TopologyNode } from "@/lib/types";
import { useAsync } from "@/lib/useRun";

const COLUMNS: Record<string, number> = {
  EXPERIMENT: 0,
  ENVIRONMENT: 0,
  SENSOR: 1,
  HUMAN: 1,
  AGENT: 2,
  MODEL: 2,
  VALIDATOR: 3,
  JUDGE: 3,
  POLICY: 4,
  STATE_STORE: 5,
  VISUALIZER: 5,
};

const BOUNDARY: Record<string, { stroke: string; dash?: string; label: string }> = {
  LOCAL_DETERMINISTIC: { stroke: "rgb(var(--accent))", label: "local, deterministic" },
  LOCAL_NONDETERMINISTIC: { stroke: "rgb(var(--withhold))", dash: "4 3", label: "local, not reproducible" },
  REMOTE: { stroke: "rgb(var(--ai))", dash: "6 3", label: "crosses a network" },
  HUMAN: { stroke: "rgb(var(--review))", label: "a person (or simulated stand-in)" },
};

const NODE_W = 178;
const NODE_H = 40;
const COL_W = 222;

function layout(topology: Topology) {
  const columns: TopologyNode[][] = Array.from({ length: 6 }, () => []);
  for (const node of topology.nodes) columns[COLUMNS[node.kind] ?? 5].push(node);
  const maxRows = Math.max(...columns.map((c) => c.length), 1);
  const height = maxRows * (NODE_H + 18) + 20;
  const positions = new Map<string, { x: number; y: number }>();
  columns.forEach((nodes, col) => {
    const offset = (height - nodes.length * (NODE_H + 18)) / 2;
    nodes.forEach((node, row) => positions.set(node.id, { x: 10 + col * COL_W, y: offset + row * (NODE_H + 18) }));
  });
  return { positions, width: 10 + 6 * COL_W - (COL_W - NODE_W) + 10, height };
}

export default function TopologyView({ source, runs, scenarios }: { source: MeshSource; runs: RunListing[]; scenarios: ScenarioInfo[] }) {
  const [scenario, setScenario] = useState("perturbed-mesh");
  const runId = runs.find((r) => r.kind === "recorded")?.id ?? null;
  const topology = useAsync(() => source.topology(scenario, runId), [source, scenario, runId]);
  const [selectedId, setSelectedId] = useState<string | null>("pordenone.policy");
  const graph = useMemo(() => (topology.value ? layout(topology.value) : null), [topology.value]);

  if (topology.loading && !topology.value) return <Loading what="topology" />;
  if (!topology.value || !graph) return <><ErrorNote error={topology.error} /><Empty title="No topology available" /></>;
  const t = topology.value;
  const selected = t.nodes.find((n) => n.id === selectedId) ?? null;
  const connected = new Set(t.edges.filter((e) => e.from === selectedId || e.to === selectedId).flatMap((e) => [e.from, e.to]));
  const mutators = t.nodes.filter((n) => n.may_mutate_state);
  const networked = t.nodes.filter((n) => n.networked);

  return (
    <div className="space-y-4">
      <Panel
        title="Mesh topology"
        subtitle={source.kind === "server" ? "Derived from the scenario configuration." : `Recorded with run ${t.run_id ?? "—"}.`}
        actions={source.kind === "server" && <Select label="scenario" value={scenario} onChange={setScenario} options={scenarios.map((s) => ({ value: s.id, label: s.id }))} />}
      >
        <div className="mb-3 flex flex-wrap gap-2">
          <Badge tone={mutators.length === 1 ? "commit" : "reject"}>may mutate state: {mutators.map((n) => n.id).join(", ") || "none"}</Badge>
          <Badge tone={networked.length ? "ai" : "commit"}>networked: {networked.map((n) => n.id).join(", ") || "none"}</Badge>
          {Object.entries(BOUNDARY).map(([key, b]) => (
            <span key={key} className="flex items-center gap-1 text-[11px] text-muted">
              <svg width="18" height="8">
                <line x1="0" y1="4" x2="18" y2="4" stroke={b.stroke} strokeWidth="2" strokeDasharray={b.dash} />
              </svg>
              {b.label}
            </span>
          ))}
        </div>
        <div className="overflow-x-auto">
          <svg viewBox={`0 0 ${graph.width} ${graph.height}`} className="min-w-[1000px]" role="img" aria-label="Mesh topology graph">
            <defs>
              <marker id="topo-arrow" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="6" markerHeight="6" orient="auto">
                <path d="M 0 0 L 10 5 L 0 10 z" fill="rgb(var(--muted))" />
              </marker>
            </defs>
            {t.edges.map((e, i) => {
              const a = graph.positions.get(e.from);
              const b = graph.positions.get(e.to);
              if (!a || !b) return null;
              const forward = b.x >= a.x;
              const x1 = forward ? a.x + NODE_W : a.x;
              const x2 = forward ? b.x : b.x + NODE_W;
              const y1 = a.y + NODE_H / 2;
              const y2 = b.y + NODE_H / 2;
              const bend = a.x === b.x ? 60 : 0;
              const active = e.from === selectedId || e.to === selectedId;
              return (
                <path
                  key={i}
                  d={`M ${x1} ${y1} C ${(x1 + x2) / 2 + bend} ${y1}, ${(x1 + x2) / 2 + bend} ${y2}, ${x2} ${y2}`}
                  fill="none"
                  stroke={active ? "rgb(var(--text) / 0.8)" : "rgb(var(--muted) / 0.25)"}
                  strokeWidth={active ? 1.5 : 1}
                  markerEnd="url(#topo-arrow)"
                >
                  <title>{`${e.from} ${e.kind} ${e.to} (${e.schema})`}</title>
                </path>
              );
            })}
            {t.nodes.map((n) => {
              const p = graph.positions.get(n.id)!;
              const b = BOUNDARY[n.trust_boundary] ?? BOUNDARY.LOCAL_DETERMINISTIC;
              const dim = selectedId && !connected.has(n.id) && n.id !== selectedId;
              return (
                <g key={n.id} className="cursor-pointer" opacity={dim ? 0.45 : 1} onClick={() => setSelectedId(n.id)}>
                  <rect x={p.x} y={p.y} width={NODE_W} height={NODE_H} rx={6} fill={n.id === selectedId ? "rgb(var(--raised))" : "rgb(var(--panel))"} stroke={b.stroke} strokeWidth={n.may_mutate_state ? 2.5 : 1.2} strokeDasharray={b.dash} />
                  <text x={p.x + 8} y={p.y + 16} fontSize={11} fill="rgb(var(--text))">
                    {n.label.length > 27 ? `${n.label.slice(0, 26)}…` : n.label}
                  </text>
                  <text x={p.x + 8} y={p.y + 31} fontSize={9.5} fill="rgb(var(--muted))">
                    {n.kind.toLowerCase().replace("_", " ")} · {n.health.toLowerCase()}
                  </text>
                </g>
              );
            })}
          </svg>
        </div>
      </Panel>

      {selected && (
        <Panel title={selected.label} subtitle={selected.id}>
          <div className="mb-3 flex flex-wrap gap-2">
            <Badge tone={selected.deterministic ? "commit" : "withhold"}>{selected.deterministic ? "deterministic" : "not deterministic"}</Badge>
            <Badge tone={selected.networked ? "ai" : "neutral"}>{selected.networked ? "networked" : "no network"}</Badge>
            <Badge tone={selected.may_mutate_state ? "withhold" : "neutral"}>{selected.may_mutate_state ? "may mutate authoritative state" : "cannot mutate state"}</Badge>
            <Badge>{BOUNDARY[selected.trust_boundary]?.label ?? selected.trust_boundary}</Badge>
            {selected.data_mode && <Badge tone="accent">{selected.data_mode}</Badge>}
            {selected.latency_ms !== null && <Badge>p50 latency {selected.latency_ms} ms</Badge>}
          </div>
          <dl className="grid gap-x-6 gap-y-2 md:grid-cols-2">
            {(
              [
                ["Capabilities", selected.capabilities],
                ["Inputs", selected.inputs],
                ["Outputs", selected.outputs],
                ["Schemas", selected.schemas],
              ] as const
            ).map(([label, items]) => (
              <div key={label}>
                <dt className="text-[11px] uppercase tracking-wide text-muted">{label}</dt>
                <dd className={cx(items.length === 0 && "text-muted")}>{items.length ? items.join(", ") : "—"}</dd>
              </div>
            ))}
            <div>
              <dt className="text-[11px] uppercase tracking-wide text-muted">Provenance</dt>
              <dd className="font-mono text-[12px]">{selected.provenance}</dd>
            </div>
            <div>
              <dt className="text-[11px] uppercase tracking-wide text-muted">Connections</dt>
              <dd>
                <ul>
                  {t.edges
                    .filter((e) => e.from === selected.id || e.to === selected.id)
                    .map((e, i) => (
                      <li key={i}>
                        {e.from === selected.id ? "→" : "←"} <span className="font-mono text-[12px]">{e.from === selected.id ? e.to : e.from}</span>{" "}
                        <span className="text-muted">
                          {e.kind}: {e.schema}
                        </span>
                      </li>
                    ))}
                </ul>
              </dd>
            </div>
          </dl>
        </Panel>
      )}
    </div>
  );
}
