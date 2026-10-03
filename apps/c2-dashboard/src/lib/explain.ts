// "Why did this happen?": a port of crates/mesh-lab/src/explain.rs so the
// static cockpit can walk causal chains without a server. explain.test.ts
// checks it against the Rust output recorded in the fixture.

import type { ChainLink, EventEnvelope, Explanation } from "./types";

type Payload = Record<string, unknown>;
const obj = (value: unknown): Payload => (value && typeof value === "object" ? (value as Payload) : {});
const text = (value: unknown, fallback = "?"): string => (typeof value === "string" ? value : fallback);
const json = (value: unknown): string => (value === undefined ? "null" : JSON.stringify(value));
const list = (value: unknown): string =>
  Array.isArray(value) ? value.filter((v): v is string => typeof v === "string").join(", ") : "";
const short = (hash: unknown): string => text(hash, "").replace(/^sha256:/, "").slice(0, 10);

export function summarize(event: EventEnvelope): string {
  const p = obj(event.payload);
  switch (event.event_type) {
    case "observation": {
      const o = obj(p.observation);
      const fault = typeof p.fault_effect === "string" ? ` (fault: ${p.fault_effect})` : "";
      return `sensor reading for ${event.subject_id} observed_at=${json(o.observed_at)} quality=${text(o.quality)}${fault}`;
    }
    case "proposal": {
      const proposal = obj(p.proposal);
      return `${event.subject_id} proposes ${text(proposal.action_type)} to ${json(proposal.target)} priority ${json(proposal.priority)}: "${text(p.rationale, "")}"`;
    }
    case "validation":
      if (p.accepted === true) {
        const checks = Array.isArray(p.checks) ? p.checks.length : 0;
        return `PASS: ${checks} checks; observation age ${json(p.observation_age_ms)} ms; deterministic confidence ${json(p.confidence)}`;
      }
      return `FAIL: ${list(p.reasons)} (${Array.isArray(p.contradictions) ? p.contradictions.filter((c) => typeof c === "string").join("; ") : ""})`;
    case "judgment":
      return `${text(p.provider)} / ${text(p.provider_model_version)} -> ${text(p.disposition)} [${list(p.reason_codes)}] status ${text(p.provider_status)}`;
    case "commitment":
      return `policy ${text(p.policy_version)} -> ${text(p.outcome)} [${list(p.reason_codes)}] at operator level ${text(p.adaptive_level)}`;
    case "state_transition":
      return `${text(p.mutation)} ${text(p.subject_id)} revision ${json(p.revision)}: ${short(p.before_hash)} -> ${short(p.after_hash)}`;
    case "human_resolution":
      return `${text(p.operator_ref)} decided ${text(p.decision)} (${text(p.note, "")})`;
    case "human_state": {
      const d = obj(p.datum);
      return `${text(d.metric)} = ${json(d.value)} ${text(d.unit, "")} (${text(d.mode)}, quality ${text(d.quality)})`;
    }
    case "adaptive_level":
      return `operator level ${text(p.previous_level)} -> ${text(p.level)} (load ${json(p.operator_load_index)}, signal ${text(p.signal_quality, "none")})`;
    case "resonance_sample": {
      const s = obj(p.sample);
      return `last ${json(s.window_ticks)} ticks: commit rate ${json(s.commit_rate)}, rejection rate ${json(s.rejection_rate)}, level ${text(p.adaptive_level)}`;
    }
    case "fault_injected":
      return `fault injected: ${text(p.label)}`;
    case "fault_cleared":
      return `fault cleared: ${text(p.label)}`;
    case "fault_effect":
      return `${text(p.effect)} affected ${text(p.agent_id)}`;
    case "environment_change":
      return `${text(p.change)}: ${text(obj(p.hazard).id)}`;
    case "operator_command":
      return `operator command: ${text(p.command)}`;
    case "run_started":
      return `run started: scenario ${text(p.scenario)}, seed ${json(p.seed)}`;
    case "run_completed":
      return `run completed: ${text(p.termination)} after ${json(p.ticks_run)} ticks`;
    default:
      return `${event.event_type} event`;
  }
}

/** Explain a proposal id, correlation id, or event id. */
export function explain(events: EventEnvelope[], target: string): Explanation | null {
  const byId = new Map(events.map((e) => [e.event_id, e]));
  let correlation: string;
  const direct = byId.get(target);
  const proposal = events.find((e) => e.event_type === "proposal" && obj(obj(e.payload).proposal).proposal_id === target);
  if (direct) correlation = direct.correlation_id;
  else if (proposal) correlation = proposal.correlation_id;
  else if (events.some((e) => e.correlation_id === target)) correlation = target;
  else return null;

  const selected = new Set(events.filter((e) => e.correlation_id === correlation).map((e) => e.seq));
  // Causal ancestors outside the correlation (the observation a proposal used).
  const frontier = events.filter((e) => selected.has(e.seq)).map((e) => e.causation_id);
  while (frontier.length > 0) {
    const cause = byId.get(frontier.pop() as string);
    if (!cause || cause.event_type === "run_started") continue;
    if (!selected.has(cause.seq)) {
      selected.add(cause.seq);
      frontier.push(cause.causation_id);
    }
  }
  const chain: ChainLink[] = events
    .filter((e) => selected.has(e.seq))
    .map((e) => ({
      seq: e.seq,
      event_id: e.event_id,
      event_type: e.event_type,
      tick: e.tick,
      source: e.source,
      causation_id: e.causation_id,
      summary: summarize(e),
      ai_involved: e.ai_involved,
      mode: e.mode,
      policy_version: e.policy_version,
    }));
  return {
    target,
    correlation_id: correlation,
    chain,
    state_changed: chain.some((l) => l.event_type === "state_transition"),
    ai_involved: chain.some((l) => l.ai_involved),
  };
}
