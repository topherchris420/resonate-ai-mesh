"use client";

import React, { useEffect, useMemo, useState } from "react";
import Arena from "@/components/Arena";
import DecisionInspector, { LawStrip } from "@/components/DecisionInspector";
import IntegrityPanel from "@/components/IntegrityPanel";
import Timeline from "@/components/Timeline";
import { Badge, Button, Empty, ErrorNote, Loading, Mono, Panel, Select, Stat, Table, outcomeTone } from "@/components/ui";
import { fmtValue } from "@/lib/format";
import { OUTCOME_LABEL, type DecisionView } from "@/lib/model";
import type { MeshSource } from "@/lib/source";
import type { MetricDefinition, RunListing } from "@/lib/types";
import { useRun, type LoadedRun } from "@/lib/useRun";

function interesting(run: LoadedRun): DecisionView | null {
  const d = run.model.decisions;
  return (
    d.find((x) => x.kind === "human_resolution" && !x.committed) ??
    d.find((x) => x.validation && !x.validation.accepted && x.kind === "proposal") ??
    d[0] ??
    null
  );
}

export function cliTarget(source: MeshSource, runId: string): string {
  return source.kind === "static" ? `apps/c2-dashboard/public/demo/runs/${runId}` : runId;
}

export function RunPicker({ runs, value, onChange }: { runs: RunListing[]; value: string | null; onChange: (id: string) => void }) {
  return (
    <Select
      label="Run"
      value={value ?? ""}
      onChange={onChange}
      options={runs.map((r) => ({ value: r.id, label: `${r.label}${r.kind === "counterfactual" ? " (branch)" : ""}` }))}
    />
  );
}

function Metrics({ run, definitions }: { run: LoadedRun; definitions: MetricDefinition[] }) {
  const m = run.metrics;
  if (!m) return <p className="text-muted">metrics.json is not part of this bundle.</p>;
  const def = (id: string) => definitions.find((d) => d.id === id);
  const v = (id: string) => m.values[id] ?? null;
  return (
    <div className="space-y-4">
      <div className="grid grid-cols-3 gap-3">
        <Stat label="committed" value={fmtValue(v("committed"))} tone="commit" hint={def("committed")?.definition} />
        <Stat label="withheld" value={fmtValue(v("withheld"))} tone="withhold" hint={def("withheld")?.definition} />
        <Stat label="rejected" value={fmtValue(v("rejected_proposals"))} tone="reject" hint={def("rejected_proposals")?.definition} />
        <Stat label="human reviews" value={fmtValue(v("human_reviews"))} tone="review" hint={def("human_reviews")?.definition} />
        <Stat label="unsafe commits" value={fmtValue(v("unsafe_commits"))} hint={def("unsafe_commits")?.definition} />
        <Stat label="network calls" value={fmtValue(v("network_calls"))} hint="Calls to a networked judgment provider during this run." />
      </div>
      <div>
        <h3 className="mb-1 text-[11px] uppercase tracking-wide text-muted">Resonance vector</h3>
        <p className="mb-2 text-[11px] text-muted">Operational measures computed from recorded decisions. Hover for the definition; “basis” is how many observations each value rests on.</p>
        <ul className="space-y-1">
          {Object.entries(m.resonance).map(([dim, r]) => (
            <li key={dim} className="grid grid-cols-[92px_1fr_88px] items-center gap-2" title={def(`resonance.${dim}`)?.definition}>
              <span className="text-[12px]">{dim}</span>
              <div className="h-1.5 rounded bg-line">
                {r.value !== null && <div className="h-1.5 rounded bg-accent" style={{ width: `${Math.max(0, Math.min(1, r.value)) * 100}%` }} />}
              </div>
              <span className="text-right font-mono text-[11px] text-muted">{r.value === null ? (r.note ?? "n/a") : `${fmtValue(r.value)} · n=${r.basis}`}</span>
            </li>
          ))}
        </ul>
      </div>
      <div>
        <h3 className="mb-1 text-[11px] uppercase tracking-wide text-muted">Invariants</h3>
        <ul className="space-y-0.5">
          {m.invariants.map((inv) => (
            <li key={inv.name} className="flex items-baseline gap-2 text-[12px]" title={inv.description}>
              <span className={inv.holds ? "text-commit" : "text-reject"}>{inv.holds ? "✓" : "✗"}</span>
              <span>{inv.name}</span>
              <span className="ml-auto font-mono text-[11px] text-muted">{inv.checked} checked</span>
            </li>
          ))}
        </ul>
      </div>
    </div>
  );
}

export default function ReplayView({
  source,
  runs,
  definitions,
  initialRun,
}: {
  source: MeshSource;
  runs: RunListing[];
  definitions: MetricDefinition[];
  initialRun: string | null;
}) {
  const [runId, setRunId] = useState<string | null>(initialRun ?? runs.find((r) => r.kind === "recorded")?.id ?? runs[0]?.id ?? null);
  const { run, error, loading } = useRun(source, runId, runs.find((r) => r.id === runId)?.hasDivergence ?? false);
  const [tick, setTick] = useState(0);
  const [selected, setSelected] = useState<DecisionView | null>(null);
  const [playing, setPlaying] = useState(false);

  useEffect(() => {
    if (initialRun) setRunId(initialRun);
  }, [initialRun]);

  useEffect(() => {
    if (!run) return;
    const first = interesting(run);
    setSelected(first);
    setTick(first?.tick ?? 0);
    setPlaying(false);
  }, [run]);

  useEffect(() => {
    if (!playing || !run) return;
    const timer = setInterval(() => {
      setTick((t) => {
        if (t + 1 >= run.model.ticks) {
          setPlaying(false);
          return t;
        }
        return t + 1;
      });
    }, 220);
    return () => clearInterval(timer);
  }, [playing, run]);

  const atTick = useMemo(() => run?.model.decisionsByTick[tick] ?? [], [run, tick]);

  if (!runId) return <Empty title="No recorded runs">Record one with <Mono>mesh run scenarios/perturbed-mesh.yaml</Mono>.</Empty>;

  return (
    <div className="space-y-4">
      <div className="flex flex-wrap items-center gap-x-4 gap-y-2">
        <RunPicker runs={runs} value={runId} onChange={setRunId} />
        {run?.manifest && (
          <span className="text-[12px] text-muted">
            scenario <span className="text-text">{run.manifest.scenario.id}</span> · seed <span className="text-text">{run.manifest.seed}</span> · {run.model.ticks} ticks · judge{" "}
            <span className="text-text">{run.provenance?.judge ? `${run.provenance.judge.name} (${run.provenance.judge.kind})` : "none"}</span>
          </span>
        )}
        <Badge tone="accent">{run?.model.events[0]?.mode ?? "SIMULATED"}</Badge>
        {run?.provenance?.parent && <Badge tone="review">branch of {run.provenance.parent.run_id}</Badge>}
      </div>
      {run?.manifest?.scenario.description && <p className="-mt-2 max-w-4xl text-muted">{run.manifest.scenario.description}</p>}
      <ErrorNote error={error} />
      {loading && !run && <Loading what="run bundle" />}

      {run && (
        <>
          <div className="grid gap-4 xl:grid-cols-[minmax(0,5fr)_minmax(0,6fr)]">
            <Panel
              title={`Arena · tick ${tick}`}
              actions={
                <div className="flex items-center gap-1">
                  <Button tone="quiet" onClick={() => setTick((t) => Math.max(0, t - 1))} title="Previous tick">
                    ◀
                  </Button>
                  <Button tone="quiet" onClick={() => setPlaying((p) => !p)}>
                    {playing ? "Pause" : "Play"}
                  </Button>
                  <Button tone="quiet" onClick={() => setTick((t) => Math.min(run.model.ticks - 1, t + 1))} title="Next tick">
                    ▶
                  </Button>
                </div>
              }
            >
              <div className="mx-auto aspect-square max-h-[480px]">
                <Arena model={run.model} tick={tick} selected={selected} onSelect={setSelected} />
              </div>
              <input
                type="range"
                min={0}
                max={run.model.ticks - 1}
                value={tick}
                onChange={(e) => setTick(Number(e.target.value))}
                className="mt-3 w-full accent-[rgb(var(--accent))]"
                aria-label="Tick"
              />
              <div className="mt-1 flex flex-wrap gap-3 text-[11px] text-muted">
                <span><span className="text-commit">━</span> committed</span>
                <span><span className="text-withhold">━</span> withheld</span>
                <span><span className="text-reject">┅</span> rejected</span>
                <span><span className="text-review">◆</span> human review</span>
                <span><span className="text-hazard">◯</span> hazard (dashed: not yet active)</span>
              </div>
            </Panel>
            <Panel title="Decision" subtitle={selected ? `tick ${selected.tick} · ${selected.agentId}` : "Select a decision on the arena, the timeline, or the table."}>
              {selected ? (
                <DecisionInspector model={run.model} decision={selected} runId={cliTarget(source, run.id)} />
              ) : (
                <LawStrip decision={null} maxStaleMs={null} />
              )}
            </Panel>
          </div>

          <Panel title="Causal timeline" subtitle="Click a marker to inspect it; click anywhere else to move to that tick. Linked markers share a proposal.">
            <Timeline model={run.model} tick={tick} onTick={setTick} selected={selected} onSelect={setSelected} />
          </Panel>

          <div className="grid gap-4 xl:grid-cols-3">
            <Panel title={`Decisions at tick ${tick}`}>
              {atTick.length === 0 ? (
                <p className="text-muted">No decisions at this tick.</p>
              ) : (
                <Table head={["agent", "action", "outcome", "reasons"]}>
                  {atTick.map((d) => (
                    <tr key={d.key} className={`cursor-pointer hover:bg-raised ${selected?.key === d.key ? "bg-raised" : ""}`} onClick={() => setSelected(d)}>
                      <td className="px-2 py-1">{d.agentId}</td>
                      <td className="px-2 py-1">{d.kind === "human_resolution" ? `human ${d.humanDecision}` : d.actionType}</td>
                      <td className="px-2 py-1">
                        <Badge tone={outcomeTone(d.policy?.outcome)}>{OUTCOME_LABEL[d.policy?.outcome ?? "pending"] ?? d.policy?.outcome}</Badge>
                      </td>
                      <td className="px-2 py-1 text-muted">{(d.validation && !d.validation.accepted ? d.validation.reasons : d.policy?.reasonCodes ?? []).join(", ")}</td>
                    </tr>
                  ))}
                </Table>
              )}
            </Panel>
            <Panel title="Integrity" subtitle="Is this the record that was made?">
              <IntegrityPanel run={run} source={source} cliTarget={cliTarget(source, run.id)} />
            </Panel>
            <Panel title="Measurements">
              <Metrics run={run} definitions={definitions} />
            </Panel>
          </div>
        </>
      )}
    </div>
  );
}
