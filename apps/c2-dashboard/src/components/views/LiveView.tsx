"use client";

import React, { useCallback, useEffect, useMemo, useRef, useState } from "react";
import Arena from "@/components/Arena";
import DecisionInspector from "@/components/DecisionInspector";
import Timeline from "@/components/Timeline";
import { Badge, Button, CopyCommand, Empty, ErrorNote, Mono, Panel, Select } from "@/components/ui";
import { summarize } from "@/lib/explain";
import { buildModel, type DecisionView } from "@/lib/model";
import type { ControlCommand, LiveStatus, MeshSource } from "@/lib/source";
import type { EventEnvelope, Scenario, ScenarioInfo } from "@/lib/types";

export const FAULT_KINDS = [
  "sensor_dropout",
  "observation_delay",
  "stale_telemetry",
  "corrupt_observation",
  "invalidate_observation",
  "duplicate_proposal",
  "clock_skew",
  "freeze_agent",
  "disable_judge",
  "judge_timeout",
  "network_unavailable",
  "judge_latency",
  "human_state_dropout",
  "human_state_degraded",
] as const;

const ACTIONS = ["MOVE", "PATROL", "INSPECT", "HOLD", "STANDBY"] as const;

function Offline() {
  return (
    <Empty title="Live sessions need a running mesh server">
      <p>This page is showing recorded data. Nothing here is simulated in the browser, so there is no live view without a server.</p>
      <div className="mx-auto max-w-md space-y-2 text-left">
        <CopyCommand command="cargo run --release -p mesh-lab --bin mesh -- serve" />
        <CopyCommand command="pnpm --filter c2-dashboard dev" />
      </div>
      <p>Then open http://localhost:3000. Live sessions are recorded like any other run and can be replayed exactly afterwards.</p>
    </Empty>
  );
}

export default function LiveView({ source, scenarios, onFinished }: { source: MeshSource; scenarios: ScenarioInfo[]; onFinished: (runId: string) => void }) {
  const server = source.server;
  const [status, setStatus] = useState<LiveStatus>({ running: false });
  const [scenarioId, setScenarioId] = useState("perturbed-mesh");
  const [seed, setSeed] = useState(42);
  const [tickMs, setTickMs] = useState(400);
  const [humanState, setHumanState] = useState<"simulated" | "ingest">("simulated");
  const [runId, setRunId] = useState<string | null>(null);
  const [scenario, setScenario] = useState<Scenario | null>(null);
  const [events, setEvents] = useState<EventEnvelope[]>([]);
  const [ingested, setIngested] = useState<EventEnvelope[]>([]);
  const [finished, setFinished] = useState<string | null>(null);
  const [lagged, setLagged] = useState(0);
  const [selected, setSelected] = useState<DecisionView | null>(null);
  const [error, setError] = useState<unknown>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const buffer = useRef<EventEnvelope[]>([]);
  const runRef = useRef<string | null>(null);
  // The first events of a session can arrive before the start request returns
  // its run id; hold them per run until the id is known.
  const early = useRef(new Map<string, EventEnvelope[]>());

  const adopt = useCallback((id: string) => {
    runRef.current = id;
    buffer.current.push(...(early.current.get(id) ?? []));
    early.current.clear();
  }, []);

  const refresh = useCallback(async () => {
    if (!server) return;
    try {
      const s = await server.liveStatus();
      setStatus(s);
      if (s.running && s.run_id && runRef.current !== s.run_id) {
        adopt(s.run_id);
        setRunId(s.run_id);
        setScenario(s.scenario ?? null);
      }
    } catch (e) {
      setError(e);
    }
  }, [server, adopt]);

  useEffect(() => {
    void refresh();
    const timer = setInterval(() => void refresh(), 2000);
    return () => clearInterval(timer);
  }, [refresh]);

  // Stream events; batch them so a fast session does not re-render per event.
  useEffect(() => {
    if (!server) return;
    const socket = new WebSocket(server.streamUrl());
    socket.onmessage = (message) => {
      try {
        const data = JSON.parse(String(message.data)) as { type: string; event?: EventEnvelope; skipped?: number };
        if (data.type === "lagged") setLagged((n) => n + (data.skipped ?? 0));
        if (data.type !== "event" || !data.event) return;
        const event = data.event;
        if (event.event_type === "session_status") {
          const payload = event.payload as { running?: boolean; bundle?: string };
          if (payload.running === false && event.run_id === runRef.current) setFinished(event.run_id);
          return;
        }
        if (event.source.startsWith("ingest:")) {
          setIngested((list) => [event, ...list].slice(0, 20));
          return;
        }
        if (!event.run_id) return;
        if (event.run_id === runRef.current) {
          buffer.current.push(event);
        } else {
          const held = early.current.get(event.run_id) ?? [];
          if (held.length < 20000) held.push(event);
          early.current.set(event.run_id, held);
        }
      } catch {
        // Ignore frames that are not JSON.
      }
    };
    socket.onerror = () => setNotice("The event stream disconnected. Refresh to reconnect.");
    const flush = setInterval(() => {
      if (buffer.current.length === 0) return;
      const batch = buffer.current;
      buffer.current = [];
      setEvents((list) => [...list, ...batch]);
    }, 200);
    return () => {
      clearInterval(flush);
      socket.close();
    };
  }, [server]);

  const model = useMemo(() => (runId ? buildModel(runId, null, scenario, events, null) : null), [runId, scenario, events]);
  const tick = model ? model.ticks - 1 : 0;
  const joinedLate = events.length > 0 && (events[0].tick ?? 0) > 0;
  const pending = useMemo(() => {
    if (!model) return [];
    const resolved = new Set(model.decisions.filter((d) => d.kind === "human_resolution").map((d) => d.proposalId));
    return model.decisions.filter((d) => d.kind === "proposal" && d.policy?.outcome === "awaiting_human_review" && !resolved.has(d.proposalId));
  }, [model]);

  if (!server) return <Offline />;

  const start = async () => {
    setError(null);
    setFinished(null);
    setEvents([]);
    setSelected(null);
    buffer.current = [];
    try {
      const started = await server.liveStart({ scenario: scenarioId, seed, tick_ms: tickMs, human_state: humanState });
      adopt(started.run_id);
      setRunId(started.run_id);
      setScenario(started.scenario);
      await refresh();
    } catch (e) {
      setError(e);
    }
  };

  const send = async (command: ControlCommand, done: string) => {
    setError(null);
    try {
      const result = await server.liveCommand(command);
      setNotice(`${done}; applies at tick ${result.applies_at_tick}.`);
    } catch (e) {
      setError(e);
    }
  };

  return (
    <div className="space-y-4">
      <Panel title="Live session" subtitle="A person at the console decides human reviews. Every command is recorded as an event, so the session replays exactly.">
        <div className="flex flex-wrap items-end gap-3">
          <Select label="scenario" value={scenarioId} onChange={setScenarioId} options={scenarios.map((s) => ({ value: s.id, label: s.id }))} />
          <label className="flex items-center gap-2 text-[12px] text-muted">
            seed
            <input type="number" value={seed} onChange={(e) => setSeed(Number(e.target.value) || 0)} className="w-20 rounded border border-line bg-raised px-2 py-1 text-text" />
          </label>
          <label className="flex items-center gap-2 text-[12px] text-muted">
            ms per tick
            <input type="number" min={10} max={5000} value={tickMs} onChange={(e) => setTickMs(Number(e.target.value) || 250)} className="w-20 rounded border border-line bg-raised px-2 py-1 text-text" />
          </label>
          <Select
            label="human state"
            value={humanState}
            onChange={setHumanState}
            options={[
              { value: "simulated", label: "simulated model" },
              { value: "ingest", label: "from /ingest (falls back to loss handling)" },
            ]}
          />
          {status.running ? (
            <Button tone="danger" onClick={() => void server.liveStop().then(refresh)}>
              Stop
            </Button>
          ) : (
            <Button tone="primary" onClick={() => void start()}>
              Start
            </Button>
          )}
          {status.running && (
            <span className="flex items-center gap-2 text-[12px]">
              <span className="h-2 w-2 animate-pulse rounded-full bg-commit" /> running · tick {Math.max(status.tick ?? 0, tick)} · <Mono>{status.run_id}</Mono>
            </span>
          )}
        </div>
        {finished && (
          <div className="mt-3 flex flex-wrap items-center gap-3 rounded border border-commit/40 bg-commit/5 px-3 py-2">
            <span>Session ended and was saved as a run bundle.</span>
            <Button tone="primary" onClick={() => onFinished(finished)}>
              Open {finished} in Replay
            </Button>
          </div>
        )}
        {joinedLate && (
          <p className="mt-2 text-[12px] text-muted">
            This view joined the session at tick {events[0].tick}; earlier events are in the recording and appear in Replay when the session ends.
          </p>
        )}
        {lagged > 0 && <p className="mt-2 text-[12px] text-withhold">{lagged} events were dropped from this view because the browser fell behind; the recording is complete.</p>}
        {notice && <p className="mt-2 text-[12px] text-muted">{notice}</p>}
        <div className="mt-2">
          <ErrorNote error={error} />
        </div>
      </Panel>

      {model && (
        <>
          <div className="grid gap-4 xl:grid-cols-[minmax(0,5fr)_minmax(0,6fr)]">
            <Panel title={`Arena · tick ${tick}`}>
              <div className="mx-auto aspect-square max-h-[460px]">
                <Arena model={model} tick={tick} selected={selected} onSelect={setSelected} />
              </div>
            </Panel>
            <div className="space-y-4">
              <Panel title={`Awaiting your review (${pending.length})`} subtitle="Judgment routed these valid proposals to a person. Approval is re-validated against the current state before anything commits.">
                {pending.length === 0 ? (
                  <p className="text-muted">Nothing is waiting.</p>
                ) : (
                  <ul className="space-y-2">
                    {pending.map((d) => (
                      <li key={d.key} className="flex flex-wrap items-center gap-2 rounded border border-review/30 px-2 py-1.5">
                        <Mono>{d.proposalId}</Mono>
                        <span className="text-muted">{d.judgment?.reasonCodes.join(", ")}</span>
                        <span className="ml-auto flex gap-1">
                          <Button onClick={() => setSelected(d)}>Inspect</Button>
                          <Button tone="primary" onClick={() => void send({ command: "human_decision", proposal_id: d.proposalId, approve: true, note: "approved at the console" }, "Approval queued")}>
                            Approve
                          </Button>
                          <Button tone="danger" onClick={() => void send({ command: "human_decision", proposal_id: d.proposalId, approve: false, note: "rejected at the console" }, "Rejection queued")}>
                            Reject
                          </Button>
                        </span>
                      </li>
                    ))}
                  </ul>
                )}
              </Panel>
              <Interventions agents={model.agentIds} tick={tick} send={send} />
            </div>
          </div>
          {selected && (
            <Panel title="Decision">
              <DecisionInspector model={model} decision={selected} runId={model.runId} />
            </Panel>
          )}
          <Panel title="Causal timeline">
            <Timeline model={model} tick={tick} onTick={() => undefined} selected={selected} onSelect={setSelected} />
          </Panel>
          <Panel title="Event stream" subtitle={`${events.length} events recorded in this session`}>
            <ol className="max-h-72 space-y-0.5 overflow-y-auto font-mono text-[11.5px]">
              {events
                .slice(-120)
                .reverse()
                .map((e) => (
                  <li key={e.event_id} className="flex gap-2">
                    <span className="w-10 shrink-0 text-muted">t{e.tick ?? "–"}</span>
                    <span className="w-32 shrink-0 text-accent">{e.event_type}</span>
                    <span className="truncate text-text/80">{summarize(e)}</span>
                    {e.ai_involved && <Badge tone="ai">AI</Badge>}
                  </li>
                ))}
            </ol>
          </Panel>
        </>
      )}

      {ingested.length > 0 && (
        <Panel title="Admitted on /ingest" subtitle="External data that passed the ingest gate (schema, size, rate, and LIVE-source registration).">
          <ol className="space-y-0.5 font-mono text-[11.5px]">
            {ingested.map((e) => (
              <li key={e.event_id} className="flex gap-2">
                <Badge tone={e.mode === "LIVE" ? "ai" : "accent"}>{e.mode}</Badge>
                <span className="text-text/80">{summarize(e)}</span>
                <span className="text-muted">{e.source}</span>
              </li>
            ))}
          </ol>
        </Panel>
      )}
    </div>
  );
}

function Interventions({ agents, tick, send }: { agents: string[]; tick: number; send: (command: ControlCommand, done: string) => Promise<void> }) {
  const [kind, setKind] = useState<string>("sensor_dropout");
  const [target, setTarget] = useState("all");
  const [duration, setDuration] = useState(8);
  const [magnitude, setMagnitude] = useState(0);
  const [agent, setAgent] = useState(agents[0] ?? "");
  const [action, setAction] = useState<string>("MOVE");
  const [x, setX] = useState(0);
  const [y, setY] = useState(0);
  const input = "w-20 rounded border border-line bg-raised px-2 py-1 text-text";
  return (
    <Panel title="Interventions" subtitle="Both are recorded. Neither bypasses validation.">
      <div className="space-y-3">
        <div className="flex flex-wrap items-end gap-2">
          <Select label="fault" value={kind} onChange={setKind} options={FAULT_KINDS.map((k) => ({ value: k, label: k }))} />
          <Select label="target" value={target} onChange={setTarget} options={[{ value: "all", label: "all" }, ...agents.map((a) => ({ value: a, label: a }))]} />
          <label className="flex items-center gap-1 text-[12px] text-muted">
            ticks <input className={input} type="number" min={1} max={1000} value={duration} onChange={(e) => setDuration(Number(e.target.value) || 1)} />
          </label>
          <label className="flex items-center gap-1 text-[12px] text-muted">
            magnitude <input className={input} type="number" value={magnitude} onChange={(e) => setMagnitude(Number(e.target.value) || 0)} />
          </label>
          <Button
            onClick={() =>
              void send({ command: "inject_fault", fault: { kind, target: target === "all" ? "all" : target, at_tick: tick + 1, duration_ticks: duration, magnitude } }, `Fault ${kind} queued`)
            }
          >
            Inject
          </Button>
        </div>
        <div className="flex flex-wrap items-end gap-2">
          <Select label="propose as" value={agent} onChange={setAgent} options={agents.map((a) => ({ value: a, label: a }))} />
          <Select label="action" value={action} onChange={setAction} options={ACTIONS.map((a) => ({ value: a, label: a }))} />
          <label className="flex items-center gap-1 text-[12px] text-muted">
            x <input className={input} type="number" value={x} onChange={(e) => setX(Number(e.target.value) || 0)} />
          </label>
          <label className="flex items-center gap-1 text-[12px] text-muted">
            y <input className={input} type="number" value={y} onChange={(e) => setY(Number(e.target.value) || 0)} />
          </label>
          <Button
            onClick={() =>
              void send(
                { command: "operator_proposal", agent_id: agent, intent: { action_type: action, target: { x, y, z: 0 }, priority: 5, rationale: "operator console" } },
                "Operator proposal queued for validation",
              )
            }
          >
            Propose
          </Button>
        </div>
        <p className="text-[11px] text-muted">An operator proposal goes through the same deterministic validation as an agent&apos;s. A step that is too long or crosses a hazard is rejected.</p>
      </div>
    </Panel>
  );
}
