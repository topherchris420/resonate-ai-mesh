// @vitest-environment node
import { describe, expect, it } from "vitest";
import { isJudgmentEnvelope } from "@pordenone/shared-types";
import { buildModel, deriveDecisions, parseJsonl } from "./model";
import { goldenBundles, read, REPO } from "./testdata";
import type { ChainedEvent, DecisionRecord, RunManifest } from "./types";

describe("decisions reconstructed from events", () => {
  it.each(goldenBundles())("agree with the recorded decisions.jsonl in %s", (dir) => {
    const events = parseJsonl<ChainedEvent>(read(dir, "events.jsonl"));
    const records = parseJsonl<DecisionRecord>(read(dir, "decisions.jsonl"));
    const derived = deriveDecisions(events);
    expect(derived.length).toBe(records.length);
    derived.forEach((d, i) => {
      const r = records[i];
      const where = `${r.proposal_id} @t${r.tick}`;
      expect(d.kind, where).toBe(r.kind);
      expect(d.proposalId, where).toBe(r.proposal_id);
      expect(d.tick, where).toBe(r.tick);
      expect(d.triggerEventId, where).toBe(r.trigger_event_id);
      expect(d.validation?.eventId, where).toBe(r.validation.event_id);
      expect(d.validation?.accepted, where).toBe(r.validation.accepted);
      expect(d.validation?.reasons, where).toEqual(r.validation.reasons);
      expect(d.validation?.failedChecks, where).toEqual(r.validation.failed_checks);
      expect(d.judgment?.disposition ?? null, where).toBe(r.judgment?.disposition ?? null);
      expect(d.judgmentSkipReason, where).toBe(r.judgment_skip_reason);
      expect(d.policy?.outcome, where).toBe(r.policy.outcome);
      expect(d.policy?.basis ?? null, where).toBe(r.policy.basis);
      expect(d.committed, where).toBe(r.committed);
      expect(d.transition?.revision ?? null, where).toBe(r.transition?.revision ?? null);
      if (r.kind === "proposal") {
        expect(d.observationAgeMs, where).toBe(r.observation_age_ms);
        expect(d.observationQuality, where).toBe(r.observation_quality);
        expect(d.from, where).toEqual(r.from.slice(0, 2));
      }
    });
  });

  it("merges the oracle verdict and positions agents from commits only", () => {
    const dir = goldenBundles().find((d) => d.endsWith("perturbed-mesh"))!;
    const manifest = JSON.parse(read(dir, "manifest.json")) as RunManifest;
    const events = parseJsonl<ChainedEvent>(read(dir, "events.jsonl"));
    const records = parseJsonl<DecisionRecord>(read(dir, "decisions.jsonl"));
    const model = buildModel(manifest.run_id, manifest, null, events, records);
    expect(model.decisions.every((d) => d.oracle.known)).toBe(true);
    expect(model.decisions.filter((d) => d.oracle.unsafe !== null && d.committed)).toEqual([]);
    expect(model.ticks).toBe(manifest.scenario.ticks);
    for (const agent of manifest.scenario.agents) {
      const first = model.decisions.find((d) => d.agentId === agent.id && d.kind === "proposal")!;
      expect(first.from).toEqual(agent.start);
    }
    // A rejected proposal never moves its agent.
    const rejected = model.decisions.find((d) => d.validation && !d.validation.accepted && d.kind === "proposal")!;
    expect(model.positions[rejected.agentId][rejected.tick]).toEqual(model.positions[rejected.agentId][rejected.tick - 1] ?? rejected.from);
    expect(model.hazards.map((h) => h.id).sort()).toEqual(manifest.scenario.hazards.map((h) => h.id).sort());
    expect(model.faults.map((f) => f.label)).toEqual(["sensor_dropout:runner_02@20", "corrupt_observation:tuner_03@52"]);
  });

  it("recorded judgment payloads satisfy the shared TypeScript contract", () => {
    const sample = JSON.parse(read(REPO, "schemas/fixtures/judgment-envelope.sample.json"));
    expect(isJudgmentEnvelope(sample)).toBe(true);
    let checked = 0;
    for (const dir of goldenBundles()) {
      for (const event of parseJsonl<ChainedEvent>(read(dir, "events.jsonl"))) {
        if (event.event_type !== "judgment") continue;
        expect(isJudgmentEnvelope(event.payload), event.event_id).toBe(true);
        checked++;
      }
    }
    expect(checked).toBeGreaterThan(100);
  });
});
