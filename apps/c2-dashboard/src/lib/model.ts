// The cockpit's view of a run, derived from its event log.
//
// Decisions are reconstructed from events (proposal or human_resolution, then
// validation, optional judgment, commitment, and an optional commit), so a
// recorded bundle and a live stream go through the same code. For recorded
// runs, the independent oracle's verdict is merged in from decisions.jsonl;
// model.test.ts checks that the reconstruction agrees with decisions.jsonl.

import type { DecisionRecord, EventEnvelope, Hazard, RunManifest, Scenario, Vec2 } from "./types";

export type Outcome = "committed" | "withheld" | "rejected_deterministic" | "awaiting_human_review" | "pending";

export interface CheckView {
  check: string;
  status: string;
  detail?: string;
  measured?: number;
  limit?: number;
}

export interface DecisionView {
  key: string;
  tick: number;
  kind: "proposal" | "human_resolution";
  proposalId: string;
  agentId: string;
  correlationId: string;
  triggerEventId: string;
  actionType: string;
  target: (number | null)[] | null;
  from: Vec2 | null;
  priority: number | null;
  rationale: string | null;
  observationId: string | null;
  observationAgeMs: number | null;
  observationQuality: string | null;
  validation: {
    eventId: string;
    accepted: boolean;
    reasons: string[];
    failedChecks: string[];
    checks: CheckView[];
    confidence: number | null;
  } | null;
  judgment: {
    eventId: string;
    provider: string;
    model: string;
    disposition: string;
    reasonCodes: string[];
    providerStatus: string;
    latencyMs: number;
    aiInvolved: boolean;
    evaluationMode: string;
  } | null;
  judgmentSkipReason: string | null;
  /** For a human resolution: the judgment that routed the proposal to a person. */
  priorJudgment: DecisionView["judgment"];
  policy: { eventId: string; outcome: Outcome; reasonCodes: string[]; basis: string | null; adaptiveLevel: string } | null;
  transition: { eventId: string; revision: number; afterHash: string } | null;
  committed: boolean;
  humanDecision: string | null;
  duplicate: boolean;
  /** Independent oracle verdict, when the bundle records one. */
  oracle: { known: boolean; unsafe: string | null };
  eventIds: string[];
}

export interface FaultSpan {
  label: string;
  kind: string;
  target: string | null;
  start: number;
  end: number;
}

export interface RunModel {
  runId: string;
  scenario: Scenario | null;
  ticks: number;
  agentIds: string[];
  /** positions[agent][tick] after the tick's commits. */
  positions: Record<string, (Vec2 | null)[]>;
  hazards: (Hazard & { activeFrom: number; activeUntil: number | null })[];
  decisions: DecisionView[];
  decisionsByTick: DecisionView[][];
  faults: FaultSpan[];
  load: (number | null)[];
  levels: (string | null)[];
  events: EventEnvelope[];
  eventsById: Map<string, EventEnvelope>;
}

type Payload = Record<string, unknown>;

const obj = (value: unknown): Payload => (value && typeof value === "object" ? (value as Payload) : {});
const str = (value: unknown): string | null => (typeof value === "string" ? value : null);
const num = (value: unknown): number | null => (typeof value === "number" && Number.isFinite(value) ? value : null);
const strs = (value: unknown): string[] => (Array.isArray(value) ? value.filter((v): v is string => typeof v === "string") : []);

function vec2(value: unknown): Vec2 | null {
  if (Array.isArray(value)) {
    const [x, y] = value;
    return typeof x === "number" && typeof y === "number" ? [x, y] : null;
  }
  const v = obj(value);
  return typeof v.x === "number" && typeof v.y === "number" ? [v.x, v.y] : null;
}

/** Group events into decisions. Exported for tests. */
export function deriveDecisions(events: EventEnvelope[]): DecisionView[] {
  const open = new Map<string, DecisionView>();
  const decisions: DecisionView[] = [];
  const observationQuality = new Map<string, string>();
  const proposals = new Map<string, DecisionView>();
  // The runner's order within a tick: due human reviews are resolved against
  // the tick-start snapshot, then a new snapshot is taken and every agent
  // proposes from it. Proposals therefore see review commits from the same
  // tick, but not each other's commits.
  let atTickStart = new Map<string, Vec2>();
  let beforeAgents: Map<string, Vec2> | null = null;
  const position = new Map<string, Vec2>();
  let currentTick = -1;

  for (const event of events) {
    const p = obj(event.payload);
    const tick = event.tick ?? 0;
    if (tick !== currentTick) {
      currentTick = tick;
      atTickStart = new Map(position);
      beforeAgents = null;
    }
    if (event.event_type === "proposal" && !event.source.includes("operator") && beforeAgents === null) {
      beforeAgents = new Map(position);
    }
    switch (event.event_type) {
      case "observation": {
        const o = obj(p.observation);
        const id = str(o.observation_id);
        if (id) observationQuality.set(id, str(o.quality) ?? "UNKNOWN");
        break;
      }
      case "proposal": {
        const proposal = obj(p.proposal);
        const proposalId = str(proposal.proposal_id) ?? event.event_id;
        const observationId = str(proposal.source_observation);
        const decision: DecisionView = {
          key: event.event_id,
          tick,
          kind: "proposal",
          proposalId,
          agentId: str(proposal.agent_id) ?? event.subject_id,
          correlationId: event.correlation_id,
          triggerEventId: event.event_id,
          actionType: str(proposal.action_type) ?? "?",
          target: Array.isArray(proposal.target) ? (proposal.target as (number | null)[]) : null,
          from: (beforeAgents ?? atTickStart).get(event.subject_id) ?? null,
          priority: num(proposal.priority),
          rationale: str(p.rationale),
          observationId,
          observationAgeMs: null,
          observationQuality: observationId ? observationQuality.get(observationId) ?? "MISSING" : "MISSING",
          validation: null,
          judgment: null,
          judgmentSkipReason: null,
          priorJudgment: null,
          policy: null,
          transition: null,
          committed: false,
          humanDecision: null,
          duplicate: p.duplicate_submission === true,
          oracle: { known: false, unsafe: null },
          eventIds: [event.event_id],
        };
        decisions.push(decision);
        open.set(event.correlation_id, decision);
        if (!proposals.has(proposalId)) proposals.set(proposalId, decision);
        break;
      }
      case "human_resolution": {
        const proposalId = str(p.proposal_id) ?? "?";
        const original = proposals.get(proposalId);
        const decision: DecisionView = {
          ...(original ?? ({} as DecisionView)),
          key: event.event_id,
          tick,
          kind: "human_resolution",
          proposalId,
          agentId: original?.agentId ?? event.subject_id,
          correlationId: event.correlation_id,
          triggerEventId: event.event_id,
          actionType: original?.actionType ?? "?",
          target: original?.target ?? null,
          from: original ? atTickStart.get(original.agentId) ?? null : null,
          priority: original?.priority ?? null,
          rationale: original?.rationale ?? null,
          observationId: original?.observationId ?? null,
          observationAgeMs: null,
          observationQuality: original?.observationQuality ?? null,
          validation: null,
          judgment: null,
          judgmentSkipReason: null,
          priorJudgment: original?.judgment ?? null,
          policy: null,
          transition: null,
          committed: false,
          humanDecision: str(p.decision),
          duplicate: false,
          oracle: { known: false, unsafe: null },
          eventIds: [event.event_id],
        };
        decisions.push(decision);
        open.set(event.correlation_id, decision);
        break;
      }
      case "validation": {
        const decision = open.get(event.correlation_id);
        if (!decision) break;
        const checks = (Array.isArray(p.checks) ? p.checks : []).map((c) => obj(c) as unknown as CheckView);
        decision.validation = {
          eventId: event.event_id,
          accepted: p.accepted === true,
          reasons: strs(p.reasons),
          failedChecks: checks.filter((c) => c.status === "FAIL").map((c) => c.check),
          checks,
          confidence: num(p.confidence),
        };
        decision.observationAgeMs = Math.max(0, num(p.observation_age_ms) ?? 0);
        decision.eventIds.push(event.event_id);
        break;
      }
      case "judgment": {
        const decision = open.get(event.correlation_id);
        if (!decision) break;
        decision.judgment = {
          eventId: event.event_id,
          provider: str(p.provider) ?? "?",
          model: str(p.model) ?? "?",
          disposition: str(p.disposition) ?? "?",
          reasonCodes: strs(p.reason_codes),
          providerStatus: str(p.provider_status) ?? "?",
          latencyMs: num(p.latency_ms) ?? 0,
          aiInvolved: event.ai_involved,
          evaluationMode: str(p.evaluation_mode) ?? "?",
        };
        decision.eventIds.push(event.event_id);
        break;
      }
      case "commitment": {
        const decision = open.get(event.correlation_id);
        if (!decision) break;
        const judgment = obj(p.judgment);
        decision.policy = {
          eventId: event.event_id,
          outcome: (str(p.outcome) ?? "pending") as Outcome,
          reasonCodes: strs(p.reason_codes),
          basis: str(p.basis),
          adaptiveLevel: str(p.adaptive_level) ?? "?",
        };
        decision.judgmentSkipReason = str(judgment.skip_reason);
        decision.eventIds.push(event.event_id);
        break;
      }
      case "state_transition": {
        const mutation = str(p.mutation);
        const subject = str(p.subject_id) ?? event.subject_id;
        const change = obj(p.change);
        if (mutation === "register_agent") {
          const at = vec2(change.position);
          if (at) {
            position.set(subject, at);
            atTickStart.set(subject, at);
            beforeAgents?.set(subject, at);
          }
        }
        if (mutation === "commit") {
          const to = vec2(change.to);
          if (to) position.set(subject, to);
          const decision = open.get(event.correlation_id);
          if (decision) {
            decision.transition = { eventId: event.event_id, revision: num(p.revision) ?? 0, afterHash: str(p.after_hash) ?? "" };
            decision.committed = true;
            decision.eventIds.push(event.event_id);
          }
        }
        break;
      }
      default:
        break;
    }
  }
  return decisions;
}

/** Merge the oracle verdicts recorded in decisions.jsonl. */
export function attachOracle(decisions: DecisionView[], records: DecisionRecord[]): void {
  const byKey = new Map(records.map((r) => [`${r.kind}|${r.proposal_id}|${r.tick}|${r.trigger_event_id}`, r]));
  for (const decision of decisions) {
    const record = byKey.get(`${decision.kind}|${decision.proposalId}|${decision.tick}|${decision.triggerEventId}`);
    if (record) decision.oracle = { known: true, unsafe: record.oracle_unsafe };
  }
}

export function buildModel(
  runId: string,
  manifest: RunManifest | null,
  scenarioOverride: Scenario | null,
  events: EventEnvelope[],
  records: DecisionRecord[] | null,
): RunModel {
  const scenario = scenarioOverride ?? manifest?.scenario ?? null;
  const decisions = deriveDecisions(events);
  if (records) attachOracle(decisions, records);
  const maxTick = events.reduce((m, e) => Math.max(m, e.tick ?? 0), 0);
  const ticks = Math.max(maxTick + 1, 1);

  const agentIds = new Set<string>(scenario?.agents.map((a) => a.id) ?? []);
  const positions: Record<string, (Vec2 | null)[]> = {};
  const current = new Map<string, Vec2>();
  const hazards = new Map<string, Hazard & { activeFrom: number; activeUntil: number | null }>();
  const faultsOpen = new Map<string, FaultSpan>();
  const faults: FaultSpan[] = [];
  const load: (number | null)[] = new Array(ticks).fill(null);
  const levels: (string | null)[] = new Array(ticks).fill(null);
  const byTick: EventEnvelope[][] = Array.from({ length: ticks }, () => []);
  for (const event of events) byTick[event.tick ?? 0]?.push(event);

  let level: string | null = null;
  for (let tick = 0; tick < ticks; tick++) {
    for (const event of byTick[tick]) {
      const p = obj(event.payload);
      if (event.event_type === "state_transition") {
        const change = obj(p.change);
        const subject = str(p.subject_id) ?? event.subject_id;
        const at = p.mutation === "register_agent" ? vec2(change.position) : p.mutation === "commit" ? vec2(change.to) : null;
        if (at) {
          current.set(subject, at);
          agentIds.add(subject);
        }
      } else if (event.event_type === "environment_change") {
        const hazard = obj(p.hazard);
        const id = str(hazard.id);
        if (id && p.change === "hazard_appeared") {
          hazards.set(id, { ...(hazard as unknown as Hazard), activeFrom: tick, activeUntil: null });
        } else if (id && hazards.has(id)) {
          hazards.get(id)!.activeUntil = tick;
        }
      } else if (event.event_type === "fault_injected") {
        const fault = obj(p.fault);
        const label = str(p.label) ?? "fault";
        const span: FaultSpan = { label, kind: str(fault.kind) ?? "?", target: str(fault.target), start: tick, end: ticks };
        faultsOpen.set(label, span);
        faults.push(span);
      } else if (event.event_type === "fault_cleared") {
        const span = faultsOpen.get(str(p.label) ?? "");
        if (span) span.end = tick;
      } else if (event.event_type === "human_state") {
        const datum = obj(p.datum);
        if (datum.metric === "operator_load_index") load[tick] = datum.quality === "GOOD" || datum.quality === "DEGRADED" ? num(datum.value) : null;
      } else if (event.event_type === "adaptive_level") {
        level = str(p.level);
      }
    }
    levels[tick] = level;
    for (const id of agentIds) {
      if (!positions[id]) positions[id] = new Array(ticks).fill(null);
      positions[id][tick] = current.get(id) ?? null;
    }
  }

  const decisionsByTick: DecisionView[][] = Array.from({ length: ticks }, () => []);
  for (const decision of decisions) decisionsByTick[decision.tick]?.push(decision);

  return {
    runId,
    scenario,
    ticks,
    agentIds: [...agentIds].sort(),
    positions,
    hazards: [...hazards.values()],
    decisions,
    decisionsByTick,
    faults,
    load,
    levels,
    events,
    eventsById: new Map(events.map((e) => [e.event_id, e])),
  };
}

export function parseJsonl<T>(text: string): T[] {
  return text
    .split("\n")
    .filter((line) => line.trim().length > 0)
    .map((line) => JSON.parse(line) as T);
}

export const OUTCOME_LABEL: Record<string, string> = {
  committed: "committed",
  withheld: "withheld",
  rejected_deterministic: "rejected",
  awaiting_human_review: "awaiting review",
  pending: "pending",
};
