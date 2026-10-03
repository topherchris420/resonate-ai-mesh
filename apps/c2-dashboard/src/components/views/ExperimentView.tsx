"use client";

import React, { useEffect, useMemo, useState } from "react";
import { Badge, Button, CopyCommand, Empty, ErrorNote, Loading, Panel, Select, Table, cx, downloadText, type Tone } from "@/components/ui";
import { fmtDelta, fmtInterval, fmtValue, titleCase } from "@/lib/format";
import type { MeshSource } from "@/lib/source";
import type { ConditionSummary, ExperimentSummary, ManifestInfo, MetricDefinition } from "@/lib/types";
import { useAsync } from "@/lib/useRun";
import { useWidth } from "@/components/useWidth";

export const VERDICT_TONE: Record<string, Tone> = {
  supported: "commit",
  contradicted: "reject",
  inconclusive: "withhold",
  not_evaluable: "neutral",
};

function ForestPlot({ conditions, metric }: { conditions: ConditionSummary[]; metric: string }) {
  const [ref, width] = useWidth<HTMLDivElement>(640);
  const rows = conditions.filter((c) => c.status === "ran" && c.metrics[metric]?.mean !== null && c.metrics[metric] !== undefined);
  if (rows.length === 0) return <div ref={ref} className="text-muted">No condition recorded {metric}.</div>;
  const values = rows.flatMap((c) => {
    const s = c.metrics[metric];
    return [s.mean ?? 0, ...(s.ci95 ?? [])];
  });
  let lo = Math.min(...values);
  let hi = Math.max(...values);
  if (lo === hi) {
    lo -= 1;
    hi += 1;
  }
  const pad = (hi - lo) * 0.08;
  lo -= pad;
  hi += pad;
  const W = width;
  const labelW = 190;
  const valueW = 120;
  const rowH = 26;
  const sx = (v: number) => labelW + ((v - lo) / (hi - lo)) * (W - labelW - valueW);
  return (
    <div ref={ref} className="w-full">
    <svg width={W} height={rows.length * rowH + 22} className="block" role="img" aria-label={`${metric} by condition with 95% confidence intervals`}>
      {[lo + pad, (lo + hi) / 2, hi - pad].map((v) => (
        <g key={v}>
          <line x1={sx(v)} x2={sx(v)} y1={0} y2={rows.length * rowH} stroke="rgb(var(--line))" strokeDasharray="2 3" />
          <text x={sx(v)} y={rows.length * rowH + 14} textAnchor="middle" fontSize={10} fill="rgb(var(--muted))">
            {fmtValue(v)}
          </text>
        </g>
      ))}
      {rows.map((c, i) => {
        const s = c.metrics[metric];
        const y = i * rowH + rowH / 2;
        return (
          <g key={c.id}>
            <text x={0} y={y + 4} fontSize={11} fill="rgb(var(--text) / 0.9)">
              {c.id}
            </text>
            {s.ci95 && <line x1={sx(s.ci95[0])} x2={sx(s.ci95[1])} y1={y} y2={y} stroke="rgb(var(--accent))" strokeWidth={2} />}
            <circle cx={sx(s.mean ?? 0)} cy={y} r={4} fill="rgb(var(--accent))" />
            <text x={W - valueW + 12} y={y + 4} fontSize={11} fill="rgb(var(--muted))">
              {fmtValue(s.mean)} (n={s.n})
            </text>
          </g>
        );
      })}
    </svg>
    </div>
  );
}

function Detail({ manifest, summary, definitions, source, onRan }: { manifest: ManifestInfo; summary: ExperimentSummary | null; definitions: MetricDefinition[]; source: MeshSource; onRan: () => void }) {
  const metrics = summary?.dependent_variables ?? [];
  const [metric, setMetric] = useState<string>(summary?.prediction?.metric ?? metrics[0] ?? "");
  const [reps, setReps] = useState(manifest.repetitions);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);
  useEffect(() => {
    setMetric(summary?.prediction?.metric ?? summary?.dependent_variables[0] ?? "");
  }, [summary]);
  const comparisons = useMemo(() => summary?.comparisons.filter((c) => c.metric === metric) ?? [], [summary, metric]);
  const definition = definitions.find((d) => d.id === metric);
  const prediction = summary?.prediction ?? null;
  const reproduce = `mesh experiment run ${manifest.path ?? `experiments/${manifest.id}/manifest.yaml`}`;

  const run = async () => {
    if (!source.server) return;
    setBusy(true);
    setError(null);
    try {
      await source.server.runExperiment(manifest.id, reps);
      onRan();
    } catch (e) {
      setError(e);
    } finally {
      setBusy(false);
    }
  };

  const download = async (file: "runs.csv" | "report.md" | "summary.json") => {
    const text = await source.experimentText(manifest.id, file);
    if (text) downloadText(`${manifest.id}-${file}`, text, file.endsWith(".csv") ? "text/csv" : file.endsWith(".md") ? "text/markdown" : "application/json");
  };

  return (
    <div className="space-y-4">
      <Panel title={manifest.title}>
        <dl className="space-y-3">
          <div>
            <dt className="text-[11px] uppercase tracking-wide text-muted">Question</dt>
            <dd className="text-[14px]">{manifest.question}</dd>
          </div>
          <div>
            <dt className="text-[11px] uppercase tracking-wide text-muted">Hypothesis</dt>
            <dd>{manifest.hypothesis}</dd>
          </div>
          {manifest.prediction && (
            <div>
              <dt className="text-[11px] uppercase tracking-wide text-muted">Pre-registered prediction</dt>
              <dd className="flex flex-wrap items-center gap-2">
                <span>
                  <span className="font-mono">{manifest.prediction.metric}</span> will {manifest.prediction.direction} from <span className="font-mono">{manifest.prediction.baseline}</span> to{" "}
                  <span className="font-mono">{manifest.prediction.treatment}</span>
                </span>
                {prediction ? <Badge tone={VERDICT_TONE[prediction.status] ?? "neutral"}>{prediction.status.replace("_", " ")}</Badge> : <Badge>not run</Badge>}
              </dd>
              {prediction && <dd className="mt-1 text-muted">{prediction.detail}</dd>}
            </div>
          )}
        </dl>
        <div className="mt-4 flex flex-wrap items-center gap-2">
          {source.server ? (
            <>
              <label className="flex items-center gap-2 text-[12px] text-muted">
                repetitions
                <input type="number" min={1} max={1000} value={reps} onChange={(e) => setReps(Math.max(1, Math.min(1000, Number(e.target.value) || 1)))} className="w-20 rounded border border-line bg-raised px-2 py-1 text-text" />
              </label>
              <Button tone="primary" onClick={run} disabled={busy}>
                {busy ? "Running…" : summary ? "Run again" : "Run experiment"}
              </Button>
            </>
          ) : null}
          {summary && (
            <>
              <Button onClick={() => void download("runs.csv")}>Raw data (runs.csv)</Button>
              <Button onClick={() => void download("report.md")}>Report</Button>
              <Button onClick={() => void download("summary.json")}>summary.json</Button>
            </>
          )}
        </div>
        <div className="mt-3">
          <CopyCommand command={reproduce} />
        </div>
        <ErrorNote error={error} />
      </Panel>

      <Panel title="Conditions" subtitle={summary ? `${summary.repetitions} paired repetitions per condition; repetition r uses the same seed in every condition (common random numbers).` : undefined}>
        <Table head={["condition", "description", "changes", "status"]}>
          {(summary?.conditions ?? manifest.conditions.map((c) => ({ id: c.id, description: c.description, overrides: c.set, status: "not run", skip_reason: null, runs: 0 }))).map((c) => (
            <tr key={c.id}>
              <td className="px-2 py-1 font-mono">{c.id}</td>
              <td className="px-2 py-1 text-text/85">{c.description}</td>
              <td className="px-2 py-1">
                <div className="flex flex-wrap gap-1">
                  {Object.entries(c.overrides).map(([k, v]) => (
                    <Badge key={k}>
                      {k}={JSON.stringify(v)}
                    </Badge>
                  ))}
                </div>
              </td>
              <td className="px-2 py-1">
                {c.status === "ran" ? <Badge tone="commit">{c.runs} runs</Badge> : <Badge tone="withhold" title={c.skip_reason ?? undefined}>{c.status === "skipped" ? "skipped" : c.status}</Badge>}
                {c.skip_reason && <div className="text-[11px] text-muted">{c.skip_reason}</div>}
              </td>
            </tr>
          ))}
        </Table>
      </Panel>

      {summary && (
        <>
          <Panel
            title="Effect"
            subtitle={definition ? `${definition.definition} (${definition.unit})` : undefined}
            actions={<Select label="metric" value={metric} onChange={setMetric} options={metrics.map((m) => ({ value: m, label: m }))} />}
          >
            <ForestPlot conditions={summary.conditions} metric={metric} />
            <p className="mb-3 mt-1 text-[11px] text-muted">Mean per condition with 95% t-interval.</p>
            {comparisons.length > 0 && (
              <Table head={["baseline → treatment", "pairs", "mean Δ", "95% CI (t)", "95% CI (bootstrap)", "d_z", "g", "↑ / ↓ / ="]}>
                {comparisons.map((c) => (
                  <tr key={`${c.baseline}-${c.treatment}`}>
                    <td className="px-2 py-1 font-mono">
                      {c.baseline} → {c.treatment}
                    </td>
                    <td className="px-2 py-1 tabular-nums">{c.n_pairs}</td>
                    <td className="px-2 py-1 tabular-nums">{fmtDelta(c.mean_difference)}</td>
                    <td className="px-2 py-1 tabular-nums">{fmtInterval(c.ci95_t)}</td>
                    <td className="px-2 py-1 tabular-nums">{fmtInterval(c.ci95_bootstrap)}</td>
                    <td className="px-2 py-1 tabular-nums">{fmtValue(c.cohens_dz)}</td>
                    <td className="px-2 py-1 tabular-nums">{fmtValue(c.hedges_g)}</td>
                    <td className="px-2 py-1 tabular-nums text-muted">
                      {c.pairs_increased} / {c.pairs_decreased} / {c.pairs_equal}
                    </td>
                  </tr>
                ))}
              </Table>
            )}
          </Panel>

          <div className="grid gap-4 xl:grid-cols-2">
            <Panel title="Invariants across every run">
              <Table head={["invariant", "expected", "violated"]}>
                {summary.invariants.map((inv) => (
                  <tr key={inv.name}>
                    <td className="px-2 py-1">{inv.name}</td>
                    <td className="px-2 py-1 text-muted">{inv.expected ? "always holds" : "may not hold"}</td>
                    <td className={cx("px-2 py-1 tabular-nums", inv.runs_violated > 0 && inv.expected ? "text-reject" : "")}>
                      {inv.runs_violated} of {inv.runs_checked}
                    </td>
                  </tr>
                ))}
              </Table>
            </Panel>
            <Panel title="Notes">
              {summary.unexpected.length > 0 && (
                <>
                  <h3 className="text-[11px] uppercase tracking-wide text-muted">Flagged</h3>
                  <ul className="mb-3 list-disc pl-4 text-withhold">
                    {summary.unexpected.map((u) => (
                      <li key={u}>{u}</li>
                    ))}
                  </ul>
                </>
              )}
              <h3 className="text-[11px] uppercase tracking-wide text-muted">Limitations</h3>
              <ul className="list-disc pl-4 text-text/85">
                {summary.limitations.map((l) => (
                  <li key={l}>{l}</li>
                ))}
              </ul>
              <p className="mt-3 text-[11px] text-muted">
                Manifest hash <span className="font-mono">{summary.manifest_hash.slice(7, 19)}</span> · base seed {summary.base_seed} · {summary.software_version}
              </p>
            </Panel>
          </div>
        </>
      )}
    </div>
  );
}

export default function ExperimentView({ source, definitions }: { source: MeshSource; definitions: MetricDefinition[] }) {
  const [version, setVersion] = useState(0);
  const manifests = useAsync(() => source.experiments(), [source, version]);
  const [selected, setSelected] = useState<string | null>(null);
  const list = manifests.value ?? [];
  // Default to the flagship ablation, then to any experiment with results and a prediction.
  const current =
    list.find((m) => m.id === selected) ??
    list.find((m) => m.id === "judgment-ablation" && m.has_summary) ??
    list.find((m) => m.has_summary && m.prediction) ??
    list[0] ??
    null;
  const summary = useAsync(current ? () => source.experimentSummary(current.id) : null, [source, current?.id, version]);

  if (manifests.loading && !manifests.value) return <Loading what="experiments" />;
  if (list.length === 0) return <Empty title="No experiment manifests found" />;
  return (
    <div className="grid gap-4 lg:grid-cols-[260px_minmax(0,1fr)]">
      <nav className="space-y-1" aria-label="Experiments">
        {list.map((m) => (
          <button
            key={m.id}
            type="button"
            onClick={() => setSelected(m.id)}
            className={cx("block w-full rounded border px-3 py-2 text-left", current?.id === m.id ? "border-accent/50 bg-accent/5" : "border-line bg-panel hover:border-muted/50")}
          >
            <div className="text-[12px] font-medium">{titleCase(m.id)}</div>
            <div className="text-[11px] text-muted">{m.has_summary ? "results available" : "not run here"}</div>
          </button>
        ))}
        <ErrorNote error={manifests.error} />
      </nav>
      <div>
        {current && (
          <>
            {summary.loading && !summary.value ? (
              <Loading what="summary" />
            ) : (
              <Detail key={current.id} manifest={current} summary={summary.value} definitions={definitions} source={source} onRan={() => setVersion((v) => v + 1)} />
            )}
            <ErrorNote error={summary.error} />
          </>
        )}
      </div>
    </div>
  );
}
