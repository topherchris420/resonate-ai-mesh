"use client";

import { useEffect, useState } from "react";
import { buildModel, parseJsonl, type RunModel } from "./model";
import type { MeshSource } from "./source";
import type { ChainedEvent, DecisionRecord, MetricsDoc, Provenance, ReplayInfo, RunManifest, TimelineComparison } from "./types";

export interface LoadedRun {
  id: string;
  manifest: RunManifest | null;
  manifestText: string | null;
  eventsText: string | null;
  replay: ReplayInfo | null;
  metrics: MetricsDoc | null;
  provenance: Provenance | null;
  divergence: TimelineComparison | null;
  model: RunModel;
}

const parse = <T,>(text: string | null): T | null => (text ? (JSON.parse(text) as T) : null);

export async function loadRun(source: MeshSource, id: string, withDivergence = false): Promise<LoadedRun> {
  const files = ["manifest.json", "events.jsonl", "decisions.jsonl", "replay.json", "metrics.json", "provenance.json"];
  // Only counterfactual branches carry divergence.json; asking others would just 404.
  if (withDivergence) files.push("divergence.json");
  const [manifestText, eventsText, decisionsText, replayText, metricsText, provenanceText, divergenceText = null] = await Promise.all(
    files.map((file) => source.runText(id, file).catch(() => null)),
  );
  if (!eventsText) throw new Error(`run ${id} has no events.jsonl`);
  const manifest = parse<RunManifest>(manifestText);
  const events = parseJsonl<ChainedEvent>(eventsText);
  const records = decisionsText ? parseJsonl<DecisionRecord>(decisionsText) : null;
  return {
    id,
    manifest,
    manifestText,
    eventsText,
    replay: parse<ReplayInfo>(replayText),
    metrics: parse<MetricsDoc>(metricsText),
    provenance: parse<Provenance>(provenanceText),
    divergence: parse<TimelineComparison>(divergenceText),
    model: buildModel(id, manifest, null, events, records),
  };
}

export function useRun(source: MeshSource | null, id: string | null, withDivergence = false) {
  const [state, setState] = useState<{ run: LoadedRun | null; error: unknown; loading: boolean }>({ run: null, error: null, loading: false });
  useEffect(() => {
    if (!source || !id) return;
    let cancelled = false;
    setState((s) => ({ ...s, loading: true, error: null }));
    loadRun(source, id, withDivergence)
      .then((run) => !cancelled && setState({ run, error: null, loading: false }))
      .catch((error) => !cancelled && setState({ run: null, error, loading: false }));
    return () => {
      cancelled = true;
    };
  }, [source, id, withDivergence]);
  return state;
}

/** Load any async value tied to a source; re-runs when deps change. */
export function useAsync<T>(fn: (() => Promise<T>) | null, deps: unknown[]) {
  const [state, setState] = useState<{ value: T | null; error: unknown; loading: boolean }>({ value: null, error: null, loading: true });
  useEffect(() => {
    if (!fn) return;
    let cancelled = false;
    setState((s) => ({ ...s, loading: true, error: null }));
    fn()
      .then((value) => !cancelled && setState({ value, error: null, loading: false }))
      .catch((error) => !cancelled && setState({ value: null, error, loading: false }));
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, deps);
  return state;
}
