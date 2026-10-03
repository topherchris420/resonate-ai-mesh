"use client";

import React, { useCallback, useEffect, useState } from "react";
import Orientation, { type View } from "@/components/Orientation";
import { Badge, Button, Empty, ErrorNote, Loading, cx } from "@/components/ui";
import CapabilitiesView from "@/components/views/CapabilitiesView";
import CompareView from "@/components/views/CompareView";
import EvidenceView from "@/components/views/EvidenceView";
import ExperimentView from "@/components/views/ExperimentView";
import LiveView from "@/components/views/LiveView";
import ReplayView from "@/components/views/ReplayView";
import TopologyView from "@/components/views/TopologyView";
import { connect, storeToken, type Connection } from "@/lib/source";
import type { CapabilityReport, MetricDefinition, RunListing, ScenarioInfo } from "@/lib/types";

const TABS: { id: View; label: string; hint: string }[] = [
  { id: "live", label: "Live", hint: "Run a session and decide human reviews (needs mesh serve)" },
  { id: "experiment", label: "Experiment", hint: "Hypotheses, conditions, paired statistics" },
  { id: "evidence", label: "Evidence", hint: "Claims checked against recorded evidence" },
  { id: "replay", label: "Replay", hint: "Scrub a recorded run and inspect any decision" },
  { id: "compare", label: "Compare", hint: "Counterfactual branches and where they diverge" },
  { id: "topology", label: "Topology", hint: "Components, trust boundaries, and who may mutate state" },
  { id: "capabilities", label: "Capabilities", hint: "What this installation can do right now" },
];

const ORIENTED_KEY = "mesh-cockpit-oriented";

function Mark() {
  return (
    <svg width="26" height="26" viewBox="0 0 32 32" aria-hidden="true">
      <circle cx="16" cy="16" r="14" fill="none" stroke="rgb(var(--accent) / 0.35)" strokeWidth="1.5" />
      <circle cx="16" cy="16" r="9" fill="none" stroke="rgb(var(--accent) / 0.65)" strokeWidth="1.5" />
      <circle cx="16" cy="16" r="3.2" fill="rgb(var(--accent))" />
      <circle cx="27.5" cy="9" r="1.8" fill="rgb(var(--text))" />
      <circle cx="5" cy="21" r="1.8" fill="rgb(var(--text))" />
    </svg>
  );
}

function TokenPrompt({ url, onSubmit }: { url: string; onSubmit: (token: string) => void }) {
  const [token, setToken] = useState("");
  return (
    <form
      className="mx-auto mt-16 max-w-md space-y-3 rounded-lg border border-line bg-panel p-5"
      onSubmit={(e) => {
        e.preventDefault();
        onSubmit(token.trim());
      }}
    >
      <p>
        <span className="font-mono">{url}</span> requires an API token (the server&apos;s <span className="font-mono">MESH_API_TOKEN</span>). It is kept in this tab&apos;s session storage only.
      </p>
      <input type="password" autoComplete="off" value={token} onChange={(e) => setToken(e.target.value)} className="w-full rounded border border-line bg-raised px-2 py-1.5" aria-label="API token" />
      <Button type="submit" tone="primary">
        Connect
      </Button>
    </form>
  );
}

export default function Cockpit() {
  const [connection, setConnection] = useState<Connection | null>(null);
  const [view, setView] = useState<View>("replay");
  const [capabilities, setCapabilities] = useState<CapabilityReport | null>(null);
  const [runs, setRuns] = useState<RunListing[]>([]);
  const [scenarios, setScenarios] = useState<ScenarioInfo[]>([]);
  const [definitions, setDefinitions] = useState<MetricDefinition[]>([]);
  const [openRun, setOpenRun] = useState<string | null>(null);
  const [orientation, setOrientation] = useState(false);
  const [loadError, setLoadError] = useState<unknown>(null);
  const source = connection?.source ?? null;

  useEffect(() => {
    void connect().then(setConnection);
    const fromHash = () => {
      const id = window.location.hash.replace("#", "") as View;
      if (TABS.some((t) => t.id === id)) setView(id);
    };
    fromHash();
    window.addEventListener("hashchange", fromHash);
    try {
      if (!localStorage.getItem(ORIENTED_KEY)) setOrientation(true);
    } catch {
      setOrientation(true);
    }
    return () => window.removeEventListener("hashchange", fromHash);
  }, []);

  const reloadRuns = useCallback(async () => {
    if (!source) return;
    setRuns(await source.runs());
  }, [source]);

  useEffect(() => {
    if (!source) return;
    setLoadError(null);
    Promise.all([source.capabilities(), source.runs(), source.scenarios(), source.metricDefinitions()])
      .then(([c, r, s, d]) => {
        setCapabilities(c);
        setRuns(r);
        setScenarios(s);
        setDefinitions(d);
      })
      .catch(setLoadError);
  }, [source]);

  const go = (next: View) => {
    setView(next);
    window.history.replaceState(null, "", `#${next}`);
  };
  const closeOrientation = () => {
    setOrientation(false);
    try {
      localStorage.setItem(ORIENTED_KEY, "1");
    } catch {
      // Without storage the orientation simply shows again next time.
    }
  };
  const openInReplay = (id: string) => {
    void reloadRuns().then(() => {
      setOpenRun(id);
      go("replay");
    });
  };

  return (
    <div className="flex min-h-screen flex-col">
      <header className="sticky top-0 z-40 border-b border-line bg-ink/90 backdrop-blur">
        <div className="mx-auto flex max-w-[1680px] flex-wrap items-center gap-x-5 gap-y-2 px-4 py-2">
          <div className="flex items-center gap-2.5">
            <Mark />
            <div className="leading-tight">
              <div className="text-[14px] font-semibold tracking-wide">Resonate AI Mesh</div>
              <div className="text-[11px] text-muted">Pordenone kernel · research cockpit</div>
            </div>
          </div>
          <nav className="-mx-1 flex overflow-x-auto" aria-label="Views">
            {TABS.map((tab) => (
              <button
                key={tab.id}
                type="button"
                title={tab.hint}
                onClick={() => go(tab.id)}
                aria-current={view === tab.id ? "page" : undefined}
                className={cx(
                  "mx-0.5 whitespace-nowrap rounded px-2.5 py-1.5 text-[11.5px] font-semibold uppercase tracking-[0.07em]",
                  view === tab.id ? "bg-accent/10 text-accent" : "text-muted hover:text-text",
                )}
              >
                {tab.label}
              </button>
            ))}
          </nav>
          <div className="ml-auto flex items-center gap-2">
            {source ? (
              <Badge tone={source.kind === "server" ? "commit" : "withhold"} title={connection?.notes.join(" ")}>
                {source.kind === "server" ? `● ${source.label}` : "static export · recorded runs"}
              </Badge>
            ) : (
              connection && <Badge tone="reject">no data source</Badge>
            )}
            <Badge tone="accent" title="All human-state data in these runs is simulated">
              {capabilities?.status.data_mode ?? "SIMULATED"}
            </Badge>
            <Button tone="quiet" onClick={() => setOrientation(true)} title="What is this?">
              ?
            </Button>
          </div>
        </div>
      </header>

      <main className="mx-auto w-full max-w-[1680px] flex-1 px-4 py-4">
        {!connection && <Loading what="data source" />}
        {connection?.needsToken && connection.serverUrl && (
          <TokenPrompt
            url={connection.serverUrl}
            onSubmit={(token) => {
              storeToken(token);
              void connect().then(setConnection);
            }}
          />
        )}
        {connection && !source && !connection.needsToken && (
          <Empty title="No data to show">
            {connection.notes.map((n) => (
              <p key={n}>{n}</p>
            ))}
          </Empty>
        )}
        <ErrorNote error={loadError} />
        {source && (
          <>
            {view === "live" && <LiveView source={source} scenarios={scenarios} onFinished={openInReplay} />}
            {view === "experiment" && <ExperimentView source={source} definitions={definitions} />}
            {view === "evidence" && <EvidenceView source={source} />}
            {view === "replay" && (runs.length > 0 ? <ReplayView source={source} runs={runs} definitions={definitions} initialRun={openRun} /> : <Loading what="runs" />)}
            {view === "compare" && (
              <CompareView
                source={source}
                runs={runs}
                definitions={definitions}
                onRunsChanged={(id) => void reloadRuns().then(() => setOpenRun(id))}
                onOpenRun={openInReplay}
              />
            )}
            {view === "topology" && <TopologyView source={source} runs={runs} scenarios={scenarios} />}
            {view === "capabilities" && capabilities && <CapabilitiesView source={source} report={capabilities} scenarios={scenarios} />}
          </>
        )}
      </main>

      <footer className="border-t border-line px-4 py-3 text-[11px] text-muted">
        <div className="mx-auto flex max-w-[1680px] flex-wrap gap-x-4 gap-y-1">
          <span>Every figure here is read from recorded files; nothing is generated for display.</span>
          <span>Human-state values are simulated operational indices, not clinical measurements.</span>
          <span>Nothing here controls physical hardware.</span>
        </div>
      </footer>

      {orientation && (
        <Orientation
          report={capabilities}
          onClose={closeOrientation}
          onGo={(next) => {
            closeOrientation();
            go(next);
          }}
        />
      )}
    </div>
  );
}
