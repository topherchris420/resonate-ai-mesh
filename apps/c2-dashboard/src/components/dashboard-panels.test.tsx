import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { disabledJudgmentEnvelope, type CanonicalEventEnvelope, type JudgmentEnvelope } from "@pordenone/shared-types";
import OperatorPanel from "./OperatorPanel";
import ValidationInspector from "./ValidationInspector";
import EventFeed from "./EventFeed";
import AgentPanel from "./AgentPanel";
import { deterministicRows, formatProbability } from "./validation-display";
import { selectDisplayEvents } from "./event-feed-model";
import { judgmentEnvelopeFixture } from "../judgment-schema-check";

function event(id: string, eventType: string): CanonicalEventEnvelope {
  return {
    event_id: id,
    event_type: eventType,
    schema_version: "1.0.0",
    timestamp: 1700000000000,
    source: "sim",
    subject_id: "subject",
    correlation_id: `corr_${id}`,
    causation_id: id,
    provenance: "test",
  };
}

describe("shared judgment fixture", () => {
  it("satisfies the TypeScript envelope contract used by the dashboard", () => {
    expect(judgmentEnvelopeFixture.disposition).toBe("PASS");
    expect(judgmentEnvelopeFixture.simulation_label).toBe("SIMULATED");
    expect(judgmentEnvelopeFixture.answers.map((answer) => answer.question_id)).toContain("proposal_support");
  });
});

describe("validation display", () => {
  it("marks coordinate contradictions and formats missing probabilities", () => {
    const rows = deterministicRows({
      proposal_id: "p",
      agent_id: "a",
      accepted: false,
      feasibility: 0,
      contradictions: ["Target position exceeds boundary"],
      confidence: 0.2,
      reasons: ["bounds"],
      provenance: "validator",
      timestamp: 1,
    });
    expect(rows.find((row) => row.label === "Coordinates valid")?.ok).toBe(false);
    expect(rows.find((row) => row.label === "Action permitted")?.ok).toBe(true);
    expect(rows.find((row) => row.label === "State consistent")?.ok).toBe(false);
    expect(formatProbability(undefined)).toBe("—");
    expect(formatProbability(0.5)).toBe("0.50");
  });

  it("keeps non-telemetry events when the feed is aggregated", () => {
    const events = [
      event("t1", "telemetry"),
      event("v1", "validation"),
      event("t2", "telemetry"),
      event("t3", "telemetry"),
      event("t4", "telemetry"),
      event("j1", "judgment"),
    ];
    const shown = selectDisplayEvents(events, true).map((item) => item.event_id);
    expect(shown).toEqual(["t1", "v1", "t2", "t3", "j1"]);
    expect(selectDisplayEvents(events, false)).toHaveLength(6);
  });
});

describe("human-state panel", () => {
  it("labels missing telemetry as simulated and toggles the flag", () => {
    const onToggle = vi.fn();
    render(<OperatorPanel telemetry={null} adaptiveState={null} onToggleSimulated={onToggle} />);
    expect(screen.getByText("Human-State Telemetry")).toBeTruthy();
    expect(screen.getByText("[SIMULATED DATA]")).toBeTruthy();
    expect(screen.getByText("NORMAL")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: /switch to live/i }));
    expect(onToggle).toHaveBeenCalledWith(false);
  });

  it("shows a live sensor label only when the sample says it is not simulated", () => {
    render(
      <OperatorPanel
        telemetry={{
          heart_rate: 80,
          hrv: 55,
          arousal: 0.5,
          attention: 0.7,
          stress: 0.3,
          confidence: 0.6,
          cognitive_load: 0.9,
          sensor_provenance: "sensor:test",
          is_simulated: false,
          timestamp: 1,
        }}
        adaptiveState={{
          state: "CRITICAL",
          stability: 0.2,
          resonance: 1,
          adaptation_rate: 0.05,
          confidence: 0.6,
          timestamp: 1,
        }}
        onToggleSimulated={() => undefined}
      />
    );
    expect(screen.getByText("[LIVE SENSOR]")).toBeTruthy();
    expect(screen.getByText("CRITICAL")).toBeTruthy();
    expect(screen.getByText("90.0%")).toBeTruthy();
  });
});

describe("validation inspector", () => {
  it("does not request judgment after a deterministic rejection", () => {
    render(
      <ValidationInspector
        latestValidation={{
          proposal_id: "p",
          agent_id: "agent_alpha",
          accepted: false,
          feasibility: 0,
          contradictions: ["Target position exceeds boundary"],
          confidence: 0.4,
          reasons: ["bounds"],
          provenance: "LocalFallbackValidator",
          timestamp: 1,
        }}
        latestJudgment={null}
        humanDecision={null}
        onDispatchProposal={() => undefined}
        onResolveHumanReview={() => undefined}
      />
    );
    expect(screen.getByText("REJECTED")).toBeTruthy();
    expect(screen.getByText(/no remote judgment was called/i)).toBeTruthy();
    expect(screen.getByText(/policy was not applied/i)).toBeTruthy();
  });

  it("dispatches a simulated proposal and an explicit out-of-bounds move", () => {
    const onDispatch = vi.fn();
    render(
      <ValidationInspector
        latestValidation={null}
        latestJudgment={null}
        humanDecision={null}
        onDispatchProposal={onDispatch}
        onResolveHumanReview={() => undefined}
      />
    );
    fireEvent.click(screen.getByRole("button", { name: /dispatch proposal to kernel/i }));
    expect(onDispatch).toHaveBeenCalledWith("PATROL", 200, 150);

    fireEvent.change(screen.getByRole("combobox"), { target: { value: "INVALID_OUT_OF_BOUNDS" } });
    fireEvent.click(screen.getByRole("button", { name: /dispatch proposal to kernel/i }));
    expect(onDispatch).toHaveBeenLastCalledWith("MOVE", 999999, 150);
  });

  it("offers human review actions only while the disposition is HUMAN_REVIEW", () => {
    const onResolve = vi.fn();
    const judgment: JudgmentEnvelope = {
      ...disabledJudgmentEnvelope({
        proposal_id: "prop_review",
        correlation_id: "corr",
        causation_id: "cause",
        timestamp: 1,
      }),
      disposition: "HUMAN_REVIEW",
    };
    const view = render(
      <ValidationInspector
        latestValidation={{
          proposal_id: "prop_review",
          agent_id: "agent_alpha",
          accepted: true,
          feasibility: 1,
          contradictions: [],
          confidence: 1,
          reasons: ["ok"],
          provenance: "validator",
          timestamp: 1,
        }}
        latestJudgment={judgment}
        humanDecision={null}
        onDispatchProposal={() => undefined}
        onResolveHumanReview={onResolve}
      />
    );
    fireEvent.click(screen.getByRole("button", { name: "Approve" }));
    expect(onResolve).toHaveBeenCalledWith("prop_review", "approve");
    view.rerender(
      <ValidationInspector
        latestValidation={{
          proposal_id: "prop_review",
          agent_id: "agent_alpha",
          accepted: true,
          feasibility: 1,
          contradictions: [],
          confidence: 1,
          reasons: ["ok"],
          provenance: "validator",
          timestamp: 1,
        }}
        latestJudgment={judgment}
        humanDecision="approve"
        onDispatchProposal={() => undefined}
        onResolveHumanReview={onResolve}
      />
    );
    expect(screen.queryByRole("button", { name: "Approve" })).toBeNull();
    expect(screen.getByText(/human decision recorded: APPROVE/i)).toBeTruthy();
    expect(screen.getByText("HUMAN_REVIEW")).toBeTruthy();
  });
});

describe("event feed and agent list", () => {
  it("shows the aggregation badge and hides telemetry past the first three", () => {
    const events = [event("t1", "telemetry"), event("t2", "telemetry"), event("t3", "telemetry"), event("t4", "telemetry")];
    render(<EventFeed events={events} aggregateAlerts />);
    expect(screen.getByText("Alert Aggregation Active")).toBeTruthy();
    expect(screen.getAllByText("[TELEMETRY]")).toHaveLength(3);
  });

  it("selects an agent from the list", () => {
    const onSelect = vi.fn();
    render(
      <AgentPanel
        agents={[
          {
            agent_id: "agent_alpha",
            state: "EXECUTING",
            capabilities: [],
            task_assignments: ["PATROL_SECTOR_0"],
            priority: 1,
            position: { x: 12.4, y: 8.2, z: 0 },
            velocity: { x: 0, y: 0, z: 0 },
            confidence: 0.95,
            timestamp: 1,
          },
        ]}
        selectedAgentId={null}
        onSelectAgent={onSelect}
      />
    );
    fireEvent.click(screen.getByText("agent_alpha"));
    expect(onSelect).toHaveBeenCalledWith("agent_alpha");
    expect(screen.getByText("Active Agents (1)")).toBeTruthy();
  });
});
