import { readFileSync } from "node:fs";
import { join } from "node:path";
import { render, screen, waitFor } from "@testing-library/react";
import React from "react";
import { beforeAll, describe, expect, it, vi } from "vitest";
import { staticSource, type MeshSource } from "@/lib/source";
import { DEMO } from "@/lib/testdata";
import type { RunListing, ScenarioInfo } from "@/lib/types";
import CapabilitiesView from "./CapabilitiesView";
import CompareView from "./CompareView";
import EvidenceView from "./EvidenceView";
import ExperimentView from "./ExperimentView";
import LiveView from "./LiveView";
import ReplayView from "./ReplayView";
import TopologyView from "./TopologyView";

// The views render the real static export (public/demo) through a fetch that
// reads from disk, so these tests fail if the export and the UI drift apart.
let source: MeshSource;
let runs: RunListing[];
let scenarios: ScenarioInfo[];

beforeAll(async () => {
  vi.stubGlobal("fetch", async (url: string) => {
    const path = join(DEMO, decodeURIComponent(String(url).replace(/^\/demo\//, "")));
    try {
      const text = readFileSync(path, "utf8");
      return { ok: true, status: 200, statusText: "OK", text: async () => text, json: async () => JSON.parse(text) };
    } catch {
      return { ok: false, status: 404, statusText: "Not Found", text: async () => "", json: async () => ({}) };
    }
  });
  const index = JSON.parse(readFileSync(join(DEMO, "index.json"), "utf8"));
  source = staticSource(index);
  runs = await source.runs();
  scenarios = await source.scenarios();
});

describe("cockpit views on the static export", () => {
  it("replay shows a real decision, its checks, and why it happened", async () => {
    render(<ReplayView source={source} runs={runs} definitions={await source.metricDefinitions()} initialRun={null} />);
    await screen.findByText(/Arena · tick/);
    expect(await screen.findByText("Why did this happen?")).toBeTruthy();
    expect(screen.getByText("Deterministic checks")).toBeTruthy();
    expect(screen.getByText("no_unsafe_commits")).toBeTruthy();
    expect(screen.getByText("Verify hash chain in this browser")).toBeTruthy();
    // The run's own description, not invented copy.
    expect(screen.getByText(/a hazard that appears mid-run/)).toBeTruthy();
  });

  it("evidence lists every claim with its declared status and no errors", async () => {
    render(<EvidenceView source={source} />);
    expect(await screen.findByText("validator-blocks-unsafe-commits")).toBeTruthy();
    expect(screen.getByText("0 errors")).toBeTruthy();
    expect(screen.getAllByText(/declared: /).length).toBeGreaterThanOrEqual(10);
  });

  it("experiment shows the pre-registered prediction and its verdict", async () => {
    render(<ExperimentView source={source} definitions={await source.metricDefinitions()} />);
    expect(await screen.findByText("Pre-registered prediction")).toBeTruthy();
    expect(await screen.findByText("Raw data (runs.csv)")).toBeTruthy();
    expect(screen.getByText("Invariants across every run")).toBeTruthy();
  });

  it("compare locates where the counterfactual diverges", async () => {
    render(<CompareView source={source} runs={runs} definitions={[]} onRunsChanged={() => undefined} onOpenRun={() => undefined} />);
    expect(await screen.findByText("first decision difference")).toBeTruthy();
    expect(await screen.findByText(/kernel.judgment.provider = "disabled"/)).toBeTruthy();
    expect(screen.getByText(/Creating branches needs/)).toBeTruthy();
  });

  it("capabilities describe the export, not the machine that made it", async () => {
    render(<CapabilitiesView source={source} report={await source.capabilities()} scenarios={scenarios} />);
    expect(screen.getByText(/This page is a static export/)).toBeTruthy();
    expect(document.body.textContent).not.toMatch(/credentials present/);
    expect(document.body.textContent).not.toMatch(/\/tmp\/|\/home\//);
  });

  it("topology shows that only the policy may mutate state", async () => {
    render(<TopologyView source={source} runs={runs} scenarios={scenarios} />);
    expect(await screen.findByText(/may mutate state: pordenone.policy$/)).toBeTruthy();
    expect(screen.getByText("networked: none")).toBeTruthy();
  });

  it("live says plainly that it needs a server instead of simulating one", async () => {
    render(<LiveView source={source} scenarios={scenarios} onFinished={() => undefined} />);
    expect(screen.getByText("Live sessions need a running mesh server")).toBeTruthy();
    await waitFor(() => expect(document.querySelector("svg")).toBeNull());
  });
});
