// Where the cockpit's data comes from.
//
// * server: a running `mesh serve` (NEXT_PUBLIC_MESH_URL, `?server=`, or
//   http://localhost:7878 when the page itself is served from localhost).
//   Everything is available, including live sessions and re-execution.
// * static: the files `mesh export-web` wrote to public/demo. Recorded runs,
//   experiment summaries, and claims only; nothing is simulated in the browser.

import type {
  CapabilityReport,
  ClaimResult,
  ExperimentSummary,
  ManifestInfo,
  MetricDefinition,
  ReplayReport,
  RunListing,
  ScenarioInfo,
  Scenario,
  TimelineComparison,
  Topology,
} from "./types";

export interface LiveStatus {
  running: boolean;
  run_id?: string;
  tick?: number;
  elapsed_s?: number;
  human_state_source?: string;
  scenario?: Scenario;
}

export interface Health {
  status: string;
  software_version: string;
  uptime_s: number;
  components: { judgment: string };
}

export type ControlCommand =
  | { command: "inject_fault"; fault: { kind: string; target: string | null; at_tick: number; duration_ticks: number; magnitude: number } }
  | { command: "operator_proposal"; agent_id: string; intent: { action_type: string; target: { x: number; y: number; z: number }; priority: number; rationale: string } }
  | { command: "human_decision"; proposal_id: string; approve: boolean; note: string };

export interface ServerOps {
  url: string;
  health(): Promise<Health>;
  replay(runId: string): Promise<{ report: ReplayReport; text: string }>;
  counterfactual(runId: string, set: Record<string, unknown>, label: string): Promise<{ run_id: string; comparison: TimelineComparison }>;
  runExperiment(id: string, repetitions: number): Promise<ExperimentSummary>;
  liveStatus(): Promise<LiveStatus>;
  liveStart(request: { scenario: string; seed?: number; tick_ms?: number; human_state?: string }): Promise<{ run_id: string; tick_ms: number; scenario: Scenario }>;
  liveStop(): Promise<unknown>;
  liveCommand(command: ControlCommand): Promise<{ queued: boolean; applies_at_tick: number }>;
  streamUrl(): string;
}

export interface MeshSource {
  kind: "server" | "static";
  label: string;
  capabilities(): Promise<CapabilityReport>;
  runs(): Promise<RunListing[]>;
  runText(runId: string, file: string): Promise<string | null>;
  experiments(): Promise<ManifestInfo[]>;
  experimentSummary(id: string): Promise<ExperimentSummary | null>;
  experimentText(id: string, file: "report.md" | "runs.csv" | "summary.json"): Promise<string | null>;
  claims(): Promise<ClaimResult[]>;
  scenarios(): Promise<ScenarioInfo[]>;
  topology(scenario: string | null, runId: string | null): Promise<Topology | null>;
  metricDefinitions(): Promise<MetricDefinition[]>;
  server: ServerOps | null;
}

export class HttpError extends Error {
  constructor(
    readonly status: number,
    message: string,
  ) {
    super(message);
  }
}

async function readError(response: Response): Promise<string> {
  try {
    const body = (await response.json()) as { error?: string };
    return body.error ?? response.statusText;
  } catch {
    return response.statusText;
  }
}

const TOKEN_KEY = "mesh-api-token";

export function storedToken(): string | null {
  try {
    return sessionStorage.getItem(TOKEN_KEY);
  } catch {
    return null;
  }
}

export function storeToken(token: string | null): void {
  try {
    if (token) sessionStorage.setItem(TOKEN_KEY, token);
    else sessionStorage.removeItem(TOKEN_KEY);
  } catch {
    // Storage may be unavailable (private mode); the token then lasts for this page only.
  }
}

export function serverSource(url: string, token: string | null): MeshSource {
  const base = url.replace(/\/+$/, "");
  const headers = (json = false): HeadersInit => ({
    ...(token ? { Authorization: `Bearer ${token}` } : {}),
    ...(json ? { "Content-Type": "application/json" } : {}),
  });
  const get = async <T>(path: string): Promise<T> => {
    const response = await fetch(`${base}${path}`, { headers: headers() });
    if (!response.ok) throw new HttpError(response.status, await readError(response));
    return (await response.json()) as T;
  };
  const text = async (path: string): Promise<string | null> => {
    const response = await fetch(`${base}${path}`, { headers: headers() });
    if (response.status === 404) return null;
    if (!response.ok) throw new HttpError(response.status, await readError(response));
    return response.text();
  };
  const post = async <T>(path: string, body: unknown): Promise<T> => {
    const response = await fetch(`${base}${path}`, { method: "POST", headers: headers(true), body: JSON.stringify(body) });
    if (!response.ok) throw new HttpError(response.status, await readError(response));
    return (await response.json()) as T;
  };
  const run = (id: string) => encodeURIComponent(id);
  return {
    kind: "server",
    label: `mesh serve · ${base}`,
    capabilities: () => get("/api/capabilities"),
    runs: async () => {
      const rows = await get<{ id: string; events: number; parent: { run_id: string } | null; has_divergence: boolean }[]>("/api/runs");
      return rows.map((r) => ({
        id: r.id,
        label: r.id,
        kind: r.parent ? "counterfactual" : r.id.startsWith("live.") ? "live" : "recorded",
        parent: r.parent?.run_id ?? null,
        hasDivergence: r.has_divergence,
        events: r.events,
      }));
    },
    runText: (id, file) => text(`/api/runs/${run(id)}/files/${file}`),
    experiments: () => get("/api/experiments"),
    experimentSummary: async (id) => {
      try {
        return await get<ExperimentSummary>(`/api/experiments/${id}/summary`);
      } catch (error) {
        if (error instanceof HttpError && error.status === 404) return null;
        throw error;
      }
    },
    experimentText: (id, file) => text(`/api/experiments/${id}/files/${file}`),
    claims: () => get("/api/claims"),
    scenarios: () => get("/api/scenarios"),
    topology: (scenario) => get(`/api/topology?scenario=${encodeURIComponent(scenario ?? "perturbed-mesh")}`),
    metricDefinitions: () => get("/api/metrics/definitions"),
    server: {
      url: base,
      health: () => get("/health"),
      replay: (id) => post(`/api/runs/${run(id)}/replay`, {}),
      counterfactual: (id, set, label) => post(`/api/runs/${run(id)}/counterfactual`, { set, label }),
      runExperiment: (id, repetitions) => post(`/api/experiments/${id}/run`, { repetitions }),
      liveStatus: () => get("/api/live"),
      liveStart: (request) => post("/api/live/start", request),
      liveStop: () => post("/api/live/stop", {}),
      liveCommand: (command) => post("/api/live/command", command),
      streamUrl: () => `${base.replace(/^http/, "ws")}/ws${token ? `?token=${encodeURIComponent(token)}` : ""}`,
    },
  };
}

interface StaticIndex {
  format: string;
  note: string;
  runs: { id: string; label: string; kind: "recorded" | "counterfactual"; parent?: string; path: string; divergence?: string }[];
  experiments: { id: string; title: string; summary: string; report: string; raw: string }[];
  scenarios: ScenarioInfo[];
  manifests: ManifestInfo[];
  capabilities: string;
  claims: string;
  metric_definitions: MetricDefinition[];
}

export const STATIC_BASE = "/demo";

export function staticSource(index: StaticIndex, base = STATIC_BASE): MeshSource {
  const fetchText = async (path: string): Promise<string | null> => {
    const response = await fetch(`${base}/${path}`);
    if (response.status === 404) return null;
    if (!response.ok) throw new HttpError(response.status, `${path}: ${response.statusText}`);
    return response.text();
  };
  const fetchJson = async <T>(path: string): Promise<T | null> => {
    const body = await fetchText(path);
    return body === null ? null : (JSON.parse(body) as T);
  };
  const runPath = (id: string) => index.runs.find((r) => r.id === id)?.path ?? null;
  const experimentPath = (id: string) => index.experiments.find((e) => e.id === id);
  return {
    kind: "static",
    label: "Static export",
    capabilities: async () => (await fetchJson<CapabilityReport>(index.capabilities)) as CapabilityReport,
    runs: async () =>
      index.runs.map((r) => ({
        id: r.id,
        label: r.label,
        kind: r.kind,
        parent: r.parent ?? null,
        hasDivergence: Boolean(r.divergence),
      })),
    runText: async (id, file) => {
      const path = runPath(id);
      return path ? fetchText(`${path}/${file}`) : null;
    },
    experiments: async () => index.manifests.map((m) => ({ ...m, has_summary: Boolean(experimentPath(m.id)) })),
    experimentSummary: async (id) => {
      const entry = experimentPath(id);
      return entry ? fetchJson<ExperimentSummary>(entry.summary) : null;
    },
    experimentText: async (id, file) => {
      const entry = experimentPath(id);
      if (!entry) return null;
      return fetchText(file === "report.md" ? entry.report : file === "runs.csv" ? entry.raw : entry.summary);
    },
    claims: async () => (await fetchJson<ClaimResult[]>(index.claims)) ?? [],
    scenarios: async () => index.scenarios,
    topology: async (_scenario, runId) => {
      const id = runId ?? index.runs[0]?.id;
      const path = id ? runPath(id) : null;
      return path ? fetchJson<Topology>(`${path}/topology.json`) : null;
    },
    metricDefinitions: async () => index.metric_definitions ?? [],
    server: null,
  };
}

async function withTimeout<T>(promise: Promise<T>, ms: number): Promise<T> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  try {
    return await Promise.race([
      promise,
      new Promise<T>((_, reject) => {
        timer = setTimeout(() => reject(new Error("timeout")), ms);
      }),
    ]);
  } finally {
    clearTimeout(timer);
  }
}

export interface Connection {
  source: MeshSource | null;
  /** Why the server was not used, or why nothing could be loaded. */
  notes: string[];
  needsToken: boolean;
  serverUrl: string | null;
}

/** Pick a data source: a reachable server first, then the static export. */
export async function connect(options: { forceStatic?: boolean; serverUrl?: string | null } = {}): Promise<Connection> {
  const notes: string[] = [];
  const params = typeof window === "undefined" ? new URLSearchParams() : new URLSearchParams(window.location.search);
  const host = typeof window === "undefined" ? "" : window.location.hostname;
  const local = ["localhost", "127.0.0.1", "[::1]"].includes(host);
  const configured = options.serverUrl ?? params.get("server") ?? process.env.NEXT_PUBLIC_MESH_URL ?? null;
  const candidate = configured ?? (local ? "http://localhost:7878" : null);
  const forceStatic = options.forceStatic || params.get("source") === "static";
  if (candidate && !forceStatic) {
    try {
      const response = await withTimeout(fetch(`${candidate.replace(/\/+$/, "")}/health`), 1500);
      if (response.ok) {
        const source = serverSource(candidate, storedToken());
        try {
          await source.capabilities();
          return { source, notes, needsToken: false, serverUrl: candidate };
        } catch (error) {
          if (error instanceof HttpError && error.status === 401) {
            return { source: null, notes: [`${candidate} requires an API token (MESH_API_TOKEN).`], needsToken: true, serverUrl: candidate };
          }
          notes.push(`${candidate} answered /health but not the API: ${(error as Error).message}`);
        }
      } else {
        notes.push(`${candidate} answered ${response.status}.`);
      }
    } catch {
      notes.push(`No mesh server at ${candidate}. Start one with \`mesh serve\` for live sessions.`);
    }
  }
  try {
    const response = await fetch(`${STATIC_BASE}/index.json`);
    if (!response.ok) throw new Error(`${response.status}`);
    const index = (await response.json()) as StaticIndex;
    return { source: staticSource(index), notes, needsToken: false, serverUrl: candidate };
  } catch {
    notes.push("No static export found at /demo/index.json. Generate one with `mesh export-web`.");
    return { source: null, notes, needsToken: false, serverUrl: candidate };
  }
}
