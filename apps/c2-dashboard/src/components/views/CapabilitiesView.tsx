"use client";

import React from "react";
import { Badge, CopyCommand, Panel, Table } from "@/components/ui";
import type { MeshSource } from "@/lib/source";
import type { CapabilityReport, ScenarioInfo } from "@/lib/types";

const CONTEXT: Record<CapabilityReport["context"], string> = {
  "static-export": "This page is a static export. It holds real recorded runs and experiment results; the commands below need a local installation.",
  server: "Connected to mesh serve. These are the capabilities of that installation, probed just now.",
  cli: "Capabilities of the installation that produced this report.",
};

const BROWSER = [
  "Verify each recorded run's hash chain with SHA-256 in your browser",
  "Scrub through a run tick by tick and inspect any decision's deterministic checks",
  "Walk the causal chain behind a decision (why did this happen?)",
  "Compare a recorded counterfactual branch with its original",
  "Read experiment results, paired statistics, and raw per-run data",
  "Check each research claim against the recorded evidence",
];

export default function CapabilitiesView({ source, report, scenarios }: { source: MeshSource; report: CapabilityReport; scenarios: ScenarioInfo[] }) {
  const available = report.capabilities.filter((c) => c.available);
  const unavailable = report.capabilities.filter((c) => !c.available);
  return (
    <div className="space-y-4">
      <Panel title="What this installation can do" subtitle={CONTEXT[report.context] ?? ""}>
        <div className="grid gap-3 md:grid-cols-4">
          {(
            [
              ["Human-state input", report.status.human_state_input],
              ["Remote judgment", report.status.remote_judgment],
              ["Physical control", report.status.physical_control],
              ["Data mode", report.status.data_mode],
            ] as const
          ).map(([label, value]) => (
            <div key={label} className="rounded border border-line bg-raised/50 px-3 py-2">
              <div className="text-[11px] uppercase tracking-wide text-muted">{label}</div>
              <div className="mt-0.5 text-[13px]">{value}</div>
            </div>
          ))}
        </div>
      </Panel>

      {source.kind === "static" && (
        <Panel title="In this browser, now">
          <ul className="grid gap-1 md:grid-cols-2">
            {BROWSER.map((item) => (
              <li key={item} className="flex gap-2">
                <span className="text-commit">✓</span>
                {item}
              </li>
            ))}
            <li className="flex gap-2 text-muted">
              <span>–</span>Run new simulations, experiments, live sessions, or counterfactuals: start <code className="font-mono">mesh serve</code> locally.
            </li>
          </ul>
        </Panel>
      )}

      <div className="grid gap-4 xl:grid-cols-2">
        <Panel title={source.kind === "static" ? "With a local installation" : "Available"}>
          <ul className="space-y-3">
            {available.map((c) => (
              <li key={c.id}>
                <div className="flex items-baseline gap-2">
                  <span className="text-commit">✓</span>
                  <span>{c.statement}</span>
                </div>
                {c.command && (
                  <div className="ml-5 mt-1">
                    <CopyCommand command={c.command} />
                  </div>
                )}
              </li>
            ))}
          </ul>
        </Panel>
        <Panel title="Not available here">
          {unavailable.length === 0 ? (
            <p className="text-muted">Everything listed is available.</p>
          ) : (
            <ul className="space-y-2">
              {unavailable.map((c) => (
                <li key={c.id}>
                  <div className="flex items-baseline gap-2">
                    <span className="text-muted">–</span>
                    <span>{c.statement}</span>
                  </div>
                  <p className="ml-5 text-[12px] text-muted">{c.reason}</p>
                </li>
              ))}
            </ul>
          )}
        </Panel>
      </div>

      <Panel title={`Scenario library (${scenarios.length})`} subtitle="Each scenario is a YAML file; every run of it is deterministic given its seed.">
        <Table head={["scenario", "description", "agents", "ticks", "faults", "judgment"]}>
          {scenarios.map((s) => (
            <tr key={s.id}>
              <td className="px-2 py-1 font-mono">{s.id}</td>
              <td className="px-2 py-1 text-text/85">{s.description}</td>
              <td className="px-2 py-1 tabular-nums">{s.agents}</td>
              <td className="px-2 py-1 tabular-nums">{s.ticks}</td>
              <td className="px-2 py-1">
                <div className="flex flex-wrap gap-1">
                  {s.faults.length ? s.faults.map((f) => <Badge key={f} tone="withhold">{f}</Badge>) : <span className="text-muted">none</span>}
                </div>
              </td>
              <td className="px-2 py-1 text-muted">{s.judgment}</td>
            </tr>
          ))}
        </Table>
      </Panel>
    </div>
  );
}
