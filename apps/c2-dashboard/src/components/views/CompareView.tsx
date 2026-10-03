"use client";

import React, { useEffect, useMemo, useState } from "react";
import { Badge, Button, CopyCommand, Empty, ErrorNote, Loading, Mono, OUTCOME_COLOR, Panel, Select, Stat, Table, outcomeTone } from "@/components/ui";
import { fmtDelta, fmtValue } from "@/lib/format";
import { OUTCOME_LABEL, type RunModel } from "@/lib/model";
import type { MeshSource } from "@/lib/source";
import type { MetricDefinition, RunListing, TimelineComparison } from "@/lib/types";
import { useRun } from "@/lib/useRun";
import { useWidth } from "@/components/useWidth";

export const COUNTERFACTUAL_PRESETS: { id: string; label: string; set: Record<string, unknown> }[] = [
  { id: "no-judgment", label: "Judgment disabled", set: { "kernel.judgment.provider": "disabled" } },
  { id: "contrarian", label: "Contrarian judge", set: { "kernel.judgment.provider": "mock", "kernel.judgment.profile": "contrarian" } },
  { id: "no-hazard-check", label: "Hazard check disabled", set: { "kernel.validator.disabled_checks": ["hazard_clearance"] } },
  { id: "no-gating", label: "Operator-load gating off", set: { "kernel.policy.adaptive.enabled": false } },
  { id: "hold-last", label: "Signal loss keeps last level", set: { "kernel.policy.adaptive.on_signal_loss": "hold_last" } },
  { id: "separation-4", label: "Minimum separation 4", set: { "kernel.validator.min_separation": 4 } },
];

function StateStrip({ comparison }: { comparison: TimelineComparison }) {
  const n = comparison.state_by_tick.length;
  return (
    <div>
      <div className="flex h-5 w-full overflow-hidden rounded border border-line">
        {comparison.state_by_tick.map((s) => {
          const same = s.left !== null && s.left === s.right;
          return (
            <div
              key={s.tick}
              className="h-full flex-1"
              style={{ background: same ? "rgb(var(--commit) / 0.25)" : s.left && s.right ? "rgb(var(--withhold) / 0.55)" : "rgb(var(--line))" }}
              title={`t${s.tick}: ${same ? "identical authoritative state" : "states differ"}`}
            />
          );
        })}
      </div>
      <div className="mt-1 flex justify-between text-[11px] text-muted">
        <span>t0</span>
        <span>authoritative state hash per tick: green identical, amber different</span>
        <span>t{n - 1}</span>
      </div>
    </div>
  );
}

function DualTimeline({ left, right, comparison }: { left: RunModel; right: RunModel; comparison: TimelineComparison }) {
  const ticks = Math.max(left.ticks, right.ticks);
  const changed = new Set(comparison.decision_changes.map((c) => c.proposal_id));
  const agents = [...new Set([...left.agentIds, ...right.agentIds])].sort();
  const [ref, W] = useWidth<HTMLDivElement>(1000);
  const labelW = 90;
  const rowH = 30;
  const x = (t: number) => labelW + ((t + 0.5) / ticks) * (W - labelW - 10);
  const firstDecision = comparison.first_decision_divergence_tick;
  return (
    <div ref={ref} className="w-full">
    <svg width={W} height={agents.length * rowH + 26} className="block" role="img" aria-label="Decisions in both timelines">
      {firstDecision !== null && (
        <g>
          <line x1={x(firstDecision)} x2={x(firstDecision)} y1={0} y2={agents.length * rowH + 8} stroke="rgb(var(--withhold))" strokeDasharray="4 3" />
          <text x={x(firstDecision) + 4} y={agents.length * rowH + 20} fontSize={11} fill="rgb(var(--withhold))">
            first decision divergence t{firstDecision}
          </text>
        </g>
      )}
      {agents.map((agent, i) => {
        const y0 = i * rowH + 8;
        return (
          <g key={agent}>
            <text x={0} y={y0 + 12} fontSize={11} fill="rgb(var(--text) / 0.85)">
              {agent}
            </text>
            <line x1={labelW} x2={W - 10} y1={y0 + 5} y2={y0 + 5} stroke="rgb(var(--line) / 0.6)" />
            <line x1={labelW} x2={W - 10} y1={y0 + 15} y2={y0 + 15} stroke="rgb(var(--line) / 0.6)" />
            {[left, right].map((model, side) =>
              model.decisions
                .filter((d) => d.agentId === agent)
                .map((d) => {
                  const outcome = d.policy?.outcome ?? "pending";
                  const highlight = changed.has(d.proposalId);
                  return (
                    <rect
                      key={`${side}-${d.key}`}
                      x={x(d.tick) - 2}
                      y={y0 + (side === 0 ? 1 : 11)}
                      width={4}
                      height={8}
                      rx={1}
                      fill={OUTCOME_COLOR[outcome] ?? "rgb(var(--muted))"}
                      opacity={highlight ? 1 : 0.35}
                      stroke={highlight ? "rgb(var(--text))" : "none"}
                      strokeWidth={0.8}
                    >
                      <title>{`${side === 0 ? "original" : "branch"} t${d.tick} ${d.proposalId} → ${outcome}`}</title>
                    </rect>
                  );
                }),
            )}
          </g>
        );
      })}
    </svg>
    </div>
  );
}

export default function CompareView({
  source,
  runs,
  definitions,
  onRunsChanged,
  onOpenRun,
}: {
  source: MeshSource;
  runs: RunListing[];
  definitions: MetricDefinition[];
  onRunsChanged: (select: string) => void;
  onOpenRun: (id: string) => void;
}) {
  const branches = runs.filter((r) => r.kind === "counterfactual" && r.hasDivergence);
  const originals = runs.filter((r) => r.kind !== "counterfactual");
  const [branchId, setBranchId] = useState<string | null>(branches[0]?.id ?? null);
  const branch = useRun(source, branchId, true);
  const parentId = branch.run?.provenance?.parent?.run_id ?? branch.run?.divergence?.left_run ?? null;
  const parent = useRun(source, parentId);
  const comparison = branch.run?.divergence ?? null;
  const [baseRun, setBaseRun] = useState<string>(originals[0]?.id ?? "");
  const [preset, setPreset] = useState(COUNTERFACTUAL_PRESETS[0].id);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);

  useEffect(() => {
    if (!branchId && branches[0]) setBranchId(branches[0].id);
  }, [branches, branchId]);

  const nonZero = useMemo(() => comparison?.metric_deltas.filter((d) => d.delta !== null && d.delta !== 0) ?? [], [comparison]);
  const definition = (id: string) => definitions.find((d) => d.id === id)?.definition;

  const create = async () => {
    if (!source.server || !baseRun) return;
    const chosen = COUNTERFACTUAL_PRESETS.find((p) => p.id === preset)!;
    setBusy(true);
    setError(null);
    try {
      const result = await source.server.counterfactual(baseRun, chosen.set, chosen.id);
      onRunsChanged(result.run_id);
      setBranchId(result.run_id);
    } catch (e) {
      setError(e);
    } finally {
      setBusy(false);
    }
  };

  const chosenPreset = COUNTERFACTUAL_PRESETS.find((p) => p.id === preset)!;
  const setArgs = Object.entries(chosenPreset.set)
    .map(([k, v]) => (typeof v === "string" ? `--set ${k}=${v}` : `--set '${k}=${JSON.stringify(v)}'`))
    .join(" ");

  return (
    <div className="space-y-4">
      <Panel
        title="Counterfactual replay"
        subtitle="Re-run a recording with one change, same seed and same inputs, and see where the timelines part. The original recording is never modified."
      >
        <div className="flex flex-wrap items-end gap-3">
          <Select label="Original run" value={baseRun} onChange={setBaseRun} options={originals.map((r) => ({ value: r.id, label: r.label }))} />
          <Select label="Change" value={preset} onChange={setPreset} options={COUNTERFACTUAL_PRESETS.map((p) => ({ value: p.id, label: p.label }))} />
          {source.server ? (
            <Button tone="primary" onClick={create} disabled={busy || !baseRun}>
              {busy ? "Running branch…" : "Run counterfactual"}
            </Button>
          ) : (
            <span className="text-[12px] text-muted">Creating branches needs <Mono>mesh serve</Mono>; the export includes one recorded branch.</span>
          )}
        </div>
        {!source.server && baseRun && (
          <div className="mt-3">
            <CopyCommand command={`mesh counterfactual ${baseRun} ${setArgs} --label ${chosenPreset.id}`} />
          </div>
        )}
        <div className="mt-3">
          <ErrorNote error={error} />
        </div>
      </Panel>

      {branches.length === 0 ? (
        <Empty title="No counterfactual branches yet">Run one above, or with <Mono>mesh counterfactual &lt;run&gt; --set key=value</Mono>.</Empty>
      ) : (
        <div className="flex flex-wrap items-center gap-3">
          <Select label="Branch" value={branchId ?? ""} onChange={setBranchId} options={branches.map((b) => ({ value: b.id, label: b.label }))} />
          {parentId && (
            <Button tone="quiet" onClick={() => onOpenRun(parentId)}>
              Open original in Replay
            </Button>
          )}
          {branchId && (
            <Button tone="quiet" onClick={() => onOpenRun(branchId)}>
              Open branch in Replay
            </Button>
          )}
        </div>
      )}
      <ErrorNote error={branch.error} />
      {branch.loading && <Loading what="branch" />}

      {comparison && (
        <>
          <Panel title="What changed" subtitle={`${comparison.left_run}  →  ${comparison.right_run}`}>
            <div className="mb-4 flex flex-wrap gap-2">
              {Object.entries(comparison.overrides).map(([k, v]) => (
                <Badge key={k} tone="withhold">
                  {k} = {JSON.stringify(v)}
                </Badge>
              ))}
            </div>
            <div className="grid grid-cols-2 gap-4 md:grid-cols-4">
              <Stat label="first event difference" value={comparison.first_divergent_event ? `#${comparison.first_divergent_event.index}` : "none"} hint="Index of the first event whose content differs" />
              <Stat label="first decision difference" value={comparison.first_decision_divergence_tick !== null ? `t${comparison.first_decision_divergence_tick}` : "none"} />
              <Stat label="first state difference" value={comparison.first_state_divergence_tick !== null ? `t${comparison.first_state_divergence_tick}` : "none"} />
              <Stat label="events" value={`${comparison.left_events} → ${comparison.right_events}`} />
            </div>
            {comparison.first_divergent_event && (
              <p className="mt-3 text-[12px] text-muted">
                Event #{comparison.first_divergent_event.index}: original <span className="text-text">{comparison.first_divergent_event.left?.event_type ?? "—"}</span> (t
                {comparison.first_divergent_event.left?.tick ?? "–"}, {comparison.first_divergent_event.left?.subject_id}), branch{" "}
                <span className="text-text">{comparison.first_divergent_event.right?.event_type ?? "—"}</span>. The record diverges as soon as judgment is no longer consulted; behavior diverges
                later, when a decision first comes out differently.
              </p>
            )}
            <div className="mt-4">
              <StateStrip comparison={comparison} />
            </div>
          </Panel>

          {parent.run && branch.run && (
            <Panel title="Decisions in both timelines" subtitle="Top lane: original. Bottom lane: branch. Outlined marks are decisions whose final outcome changed.">
              <DualTimeline left={parent.run.model} right={branch.run.model} comparison={comparison} />
            </Panel>
          )}

          <div className="grid gap-4 xl:grid-cols-2">
            <Panel title={`Decisions that changed (${comparison.decision_changes.length})`}>
              <Table head={["tick", "proposal", "original", "branch"]}>
                {comparison.decision_changes.map((c) => (
                  <tr key={c.proposal_id}>
                    <td className="px-2 py-1 font-mono text-muted">t{c.tick}</td>
                    <td className="px-2 py-1 font-mono">{c.proposal_id}</td>
                    <td className="px-2 py-1">
                      {c.left ? <Badge tone={outcomeTone(c.left)}>{OUTCOME_LABEL[c.left] ?? c.left}</Badge> : <span className="text-muted">not proposed</span>}
                      {c.left_decided_at !== null && c.left_decided_at !== c.tick && <span className="ml-1 text-[11px] text-muted">at t{c.left_decided_at}</span>}
                    </td>
                    <td className="px-2 py-1">
                      {c.right ? <Badge tone={outcomeTone(c.right)}>{OUTCOME_LABEL[c.right] ?? c.right}</Badge> : <span className="text-muted">not proposed</span>}
                      {c.right_decided_at !== null && c.right_decided_at !== c.tick && <span className="ml-1 text-[11px] text-muted">at t{c.right_decided_at}</span>}
                    </td>
                  </tr>
                ))}
              </Table>
            </Panel>
            <Panel title="Measured differences" subtitle={`${nonZero.length} of ${comparison.metric_deltas.length} metrics changed; one seed, so these are observations, not effect estimates. Experiments estimate effects.`}>
              <Table head={["metric", <span key="o" className="block text-right">original</span>, <span key="b" className="block text-right">branch</span>, <span key="d" className="block text-right">Δ</span>]}>
                {comparison.metric_deltas.map((d) => (
                  <tr key={d.metric} className={d.delta ? "" : "text-muted"} title={definition(d.metric)}>
                    <td className="px-2 py-1">{d.metric}</td>
                    <td className="px-2 py-1 text-right tabular-nums">{fmtValue(d.left)}</td>
                    <td className="px-2 py-1 text-right tabular-nums">{fmtValue(d.right)}</td>
                    <td className={`px-2 py-1 text-right tabular-nums ${d.delta ? "text-text" : ""}`}>{fmtDelta(d.delta)}</td>
                  </tr>
                ))}
              </Table>
            </Panel>
          </div>
        </>
      )}
    </div>
  );
}
