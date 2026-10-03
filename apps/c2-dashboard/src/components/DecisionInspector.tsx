"use client";

import React, { useMemo } from "react";
import { explain } from "@/lib/explain";
import { fmtValue, shortHash } from "@/lib/format";
import { OUTCOME_LABEL, type DecisionView, type RunModel } from "@/lib/model";
import { Badge, CopyCommand, Mono, outcomeTone, type Tone } from "./ui";

export const LAW = [
  { id: "observe", label: "Observe", text: "An agent reads a timestamped observation." },
  { id: "propose", label: "Propose", text: "It proposes an action. Proposals carry no authority." },
  { id: "validate", label: "Validate", text: "Deterministic checks: schema, freshness, bounds, step, hazards, separation." },
  { id: "judge", label: "Judge", text: "A bounded judge may advise on proposals that passed. It cannot overturn a failure." },
  { id: "policy", label: "Policy", text: "Versioned policy decides: commit, withhold, or route to a person." },
  { id: "commit", label: "Commit", text: "Only an authorized commit mutates authoritative state." },
  { id: "record", label: "Record", text: "Every step is an event in a hash chain, so the run can be replayed." },
] as const;

type StageState = { tone: Tone; value: string; detail?: string };

function stages(d: DecisionView, maxStaleMs: number | null): Record<(typeof LAW)[number]["id"], StageState> {
  const outcome = d.policy?.outcome ?? "pending";
  const degraded = d.observationQuality !== "GOOD" || (maxStaleMs !== null && (d.observationAgeMs ?? 0) > maxStaleMs / 2);
  return {
    observe:
      d.kind === "human_resolution"
        ? { tone: "review", value: "re-checked", detail: "the original observation is re-validated against current state" }
        : { tone: degraded ? "withhold" : "commit", value: d.observationQuality ?? "—", detail: `${d.observationId ?? "no observation"} · age ${fmtValue(d.observationAgeMs)} ms` },
    propose:
      d.kind === "human_resolution"
        ? { tone: "review", value: d.humanDecision ?? "?", detail: `a person decided ${d.humanDecision ?? "?"} on ${d.proposalId}` }
        : { tone: "neutral", value: d.actionType, detail: `${d.agentId} → ${d.target ? d.target.map((v) => (v === null ? "NaN" : fmtValue(v))).join(", ") : "—"}` },
    validate: d.validation
      ? { tone: d.validation.accepted ? "commit" : "reject", value: d.validation.accepted ? "PASS" : "FAIL", detail: d.validation.reasons.join(", ") }
      : { tone: "neutral", value: "—" },
    judge: d.kind === "human_resolution" && d.priorJudgment
      ? {
          tone: "review",
          value: `earlier ${d.priorJudgment.disposition}`,
          detail: `${d.priorJudgment.provider}: the judgment that routed this to a person; not consulted again`,
        }
      : d.judgment
      ? {
          tone: d.judgment.disposition === "PASS" ? "commit" : d.judgment.disposition === "HUMAN_REVIEW" ? "review" : "withhold",
          value: d.judgment.disposition,
          detail: `${d.judgment.provider} · ${d.judgment.model}${d.judgment.aiInvolved ? " · AI-involved" : " · deterministic stand-in"}`,
        }
      : {
          tone: "neutral",
          value: d.validation && !d.validation.accepted ? "not consulted" : "skipped",
          detail: d.validation && !d.validation.accepted ? "validation failed, so judgment is never asked" : d.judgmentSkipReason ?? "judgment disabled",
        },
    policy: d.policy
      ? { tone: outcomeTone(outcome), value: OUTCOME_LABEL[outcome] ?? outcome, detail: [d.policy.basis, ...d.policy.reasonCodes].filter(Boolean).join(" · ") }
      : { tone: "neutral", value: "pending" },
    commit: d.committed
      ? { tone: "commit", value: `revision ${d.transition?.revision}`, detail: `state ${shortHash(d.transition?.afterHash, 10)}` }
      : { tone: outcome === "rejected_deterministic" ? "reject" : "withhold", value: "unchanged", detail: "authoritative state did not change" },
    record: { tone: "accent", value: `${d.eventIds.length} events`, detail: "hash-chained" },
  };
}

export function LawStrip({ decision, maxStaleMs }: { decision: DecisionView | null; maxStaleMs: number | null }) {
  const states = decision ? stages(decision, maxStaleMs) : null;
  return (
    <ol className="grid grid-cols-2 gap-1 sm:grid-cols-4 2xl:grid-cols-7" aria-label="Execution law">
      {LAW.map((stage, i) => {
        const state = states?.[stage.id];
        return (
          <li key={stage.id} className="relative min-w-0 rounded border border-line bg-raised/60 px-1.5 py-1.5" title={state?.detail ?? stage.text}>
            <div className="flex items-center gap-1 text-[10px] uppercase tracking-wide text-muted">
              <span className="text-muted/60">{i + 1}</span>
              <span className="truncate">{stage.label}</span>
            </div>
            <div className="mt-0.5 break-words">{state ? <Badge tone={state.tone} wrap>{state.value}</Badge> : <span className="text-[11px] text-muted/70">{stage.text.split(".")[0]}</span>}</div>
          </li>
        );
      })}
    </ol>
  );
}

export default function DecisionInspector({ model, decision, runId }: { model: RunModel; decision: DecisionView; runId: string }) {
  const why = useMemo(() => explain(model.events, decision.triggerEventId), [model.events, decision.triggerEventId]);
  const maxStale = model.scenario?.kernel.validator.max_stale_ms ?? null;
  const outcome = decision.policy?.outcome ?? "pending";
  return (
    <div className="space-y-4">
      <div className="flex flex-wrap items-center gap-2">
        <Mono className="text-text">{decision.proposalId}</Mono>
        <Badge tone={outcomeTone(outcome)}>{OUTCOME_LABEL[outcome] ?? outcome}</Badge>
        {decision.kind === "human_resolution" && <Badge tone="review">human resolution</Badge>}
        {decision.judgment?.aiInvolved ? <Badge tone="ai">AI involved</Badge> : <Badge>no model involved</Badge>}
        {decision.oracle.known ? (
          decision.oracle.unsafe ? (
            <Badge tone="reject" title="Independent geometric oracle">oracle: unsafe ({decision.oracle.unsafe})</Badge>
          ) : (
            <Badge tone="commit" title="Independent geometric oracle">oracle: safe</Badge>
          )
        ) : (
          <Badge title="The oracle verdict is recorded in decisions.jsonl when the run is saved">oracle: not yet recorded</Badge>
        )}
      </div>

      <LawStrip decision={decision} maxStaleMs={maxStale} />

      {decision.rationale && (
        <p className="text-[12px] text-muted">
          Agent rationale (recorded, never used for authority): <span className="text-text">“{decision.rationale}”</span>
        </p>
      )}

      {decision.validation && (
        <div>
          <h3 className="mb-1 text-[11px] uppercase tracking-wide text-muted">Deterministic checks</h3>
          <ul className="grid grid-cols-1 gap-x-4 gap-y-0.5 sm:grid-cols-2">
            {decision.validation.checks.map((c) => (
              <li key={c.check} className="flex items-baseline justify-between gap-2 text-[12px]">
                <span className={c.status === "FAIL" ? "text-reject" : c.status === "PASS" ? "text-text/85" : "text-muted"}>
                  {c.status === "PASS" ? "✓" : c.status === "FAIL" ? "✗" : "–"} {c.check}
                </span>
                <span className="font-mono text-[11px] text-muted">
                  {c.measured !== undefined ? `${fmtValue(c.measured)} / ${fmtValue(c.limit)}` : c.detail ?? c.status.toLowerCase()}
                </span>
              </li>
            ))}
          </ul>
        </div>
      )}

      {why && (
        <div>
          <h3 className="mb-1 text-[11px] uppercase tracking-wide text-muted">Why did this happen?</h3>
          <ol className="space-y-1 border-l border-line pl-3">
            {why.chain.map((link) => (
              <li key={link.event_id} className="text-[12px]">
                <div className="flex flex-wrap items-baseline gap-x-2">
                  <Mono className="text-muted">t{link.tick ?? "–"}</Mono>
                  <span className="font-medium text-text">{link.event_type}</span>
                  {link.ai_involved && <Badge tone="ai">AI</Badge>}
                  <Mono className="text-muted/70" title={`caused by ${link.causation_id}`}>
                    {link.event_id}
                  </Mono>
                </div>
                <div className="text-text/80">{link.summary}</div>
              </li>
            ))}
          </ol>
          <p className="mt-2 text-[12px] text-muted">
            Authoritative state changed: <span className="text-text">{why.state_changed ? "yes" : "no"}</span>. A probabilistic model was involved:{" "}
            <span className="text-text">{why.ai_involved ? "yes" : "no"}</span>.
          </p>
          <div className="mt-2">
            <CopyCommand command={`mesh explain ${runId} ${decision.proposalId}`} />
          </div>
        </div>
      )}
    </div>
  );
}
